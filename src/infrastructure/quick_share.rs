use std::collections::{BTreeMap, HashSet, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use rqs_lib::channel::{ChannelAction, ChannelDirection, ChannelMessage, TransferType};
use rqs_lib::{EndpointInfo, OutboundPayload, RQS, SendInfo, State, TextPayloadType, Visibility};
use tokio::sync::{broadcast, mpsc, watch};
use uuid::Uuid;

use super::shelf_platform::NativeShelfPlatform;
use super::shelf_store::JsonShelfRepository;
use crate::application::presenter::{NotchPresenter, ShelfEntry, ViewUpdate};
use crate::application::quick_share::{Command, Device, Request, Snapshot, Tray};
use crate::application::settings::{AppSettings, QuickShareSettings};
use crate::domain::notch::NotchPage;
use crate::domain::ports::{PortError, ShelfPlatform, ShelfRepository};
use crate::domain::shelf::{ShelfItem, is_same_shelf_path};

#[cfg(test)]
#[path = "../../tests/fixtures/quick_share_receive.rs"]
mod tests;

struct Engine {
    service: RQS,
    send: mpsc::Sender<SendInfo>,
    transfers: broadcast::Receiver<ChannelMessage>,
    endpoints: broadcast::Sender<EndpointInfo>,
    discovery: broadcast::Receiver<EndpointInfo>,
}

impl Engine {
    async fn start(directory: &Path, policy: &QuickShareSettings) -> Result<Self, String> {
        let mut service = RQS::new(visibility(policy.visible), None, Some(directory.to_owned()));
        service.set_device_name(Some(policy.device_name.clone()));
        service.set_nearby_alerts(false);
        let transfers = service.message_sender.subscribe();
        let (send, _) = service.run().await.map_err(|error| error.to_string())?;
        let (endpoints, discovery) = broadcast::channel(64);
        Ok(Self {
            service,
            send,
            transfers,
            endpoints,
            discovery,
        })
    }

    fn decide(&self, id: String, accept: bool) -> bool {
        self.service
            .message_sender
            .send(ChannelMessage {
                id,
                direction: ChannelDirection::FrontToLib,
                action: Some(if accept {
                    ChannelAction::AcceptTransfer
                } else {
                    ChannelAction::RejectTransfer
                }),
                ..Default::default()
            })
            .is_ok()
    }
}

fn visibility(value: bool) -> Visibility {
    if value {
        Visibility::Visible
    } else {
        Visibility::Invisible
    }
}

struct Service {
    engine: Option<Engine>,
    directory: PathBuf,
    received: JsonShelfRepository,
    can_save_received: bool,
    policy: QuickShareSettings,
    presenter: Arc<dyn NotchPresenter>,
    state: Snapshot,
    devices: BTreeMap<String, (String, String)>,
    requests: VecDeque<Request>,
    accepted: HashSet<String>,
    active: Option<(String, Vec<Uuid>)>,
    pending: VecDeque<(SendInfo, Vec<Uuid>)>,
    discovering: bool,
    refresh_after_send: bool,
    picker: Option<tokio::task::JoinHandle<Result<Vec<PathBuf>, PortError>>>,
}

pub async fn run(
    root: PathBuf,
    mut settings: watch::Receiver<AppSettings>,
    mut commands: mpsc::UnboundedReceiver<Command>,
    presenter: Arc<dyn NotchPresenter>,
) {
    let received = JsonShelfRepository::new(root.join("received.json"));
    let directory = root.join("received");
    let previous = received.load();
    let can_save_received = previous.is_ok();
    let mut state = Snapshot::default();
    match previous {
        Ok(items) => {
            state.received = items
                .into_iter()
                .filter(|item| {
                    item.text().is_some()
                        || item.path().is_some_and(|path| {
                            path.parent() == Some(directory.as_path()) && path.is_file()
                        })
                })
                .map(|item| ShelfEntry::new(item, None))
                .collect();
        }
        Err(error) => {
            tracing::warn!(error = %error, "Quick Share received list could not be loaded");
            state.received_status = Some("quickShare.storageError");
        }
    }
    let (mut enabled, policy) = share_policy(&settings.borrow_and_update());
    let mut service = Service {
        engine: None,
        directory,
        received,
        can_save_received,
        policy,
        presenter,
        state,
        devices: BTreeMap::new(),
        requests: VecDeque::new(),
        accepted: HashSet::new(),
        active: None,
        pending: VecDeque::new(),
        discovering: false,
        refresh_after_send: false,
        picker: None,
    };
    service.apply_policy().await;
    service.publish();
    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { break };
                if !enabled { continue; }
                service.command(command).await;
            }
            changed = settings.changed() => {
                if changed.is_err() { break; }
                let (next_enabled, policy) = share_policy(&settings.borrow_and_update());
                if policy == service.policy && next_enabled == enabled { continue; }
                enabled = next_enabled;
                service.policy = policy;
                if !enabled {
                    service.command(Command::Discover(false)).await;
                    if let Some(picker) = service.picker.take() { picker.abort(); }
                }
                service.apply_policy().await;
            }
            event = next_event(&mut service.engine) => {
                match event {
                    EngineEvent::Endpoint(endpoint) => service.endpoint(endpoint),
                    EngineEvent::Transfer(transfer) => service.transfer(transfer),
                    EngineEvent::Lost => service.reset().await,
                }
            }
            result = wait_picker(&mut service.picker) => {
                service.picker = None;
                match result {
                    Ok(paths) => service.stage(paths),
                    Err(error) => {
                        tracing::warn!(%error, "Quick Share file picker failed");
                        service.state.status = Some("quickShare.fileError");
                    }
                }
            }
        }
        service.publish();
        service.refresh_receiver_if_idle().await;
        service.stop_if_idle().await;
    }
    if let Some(mut engine) = service.engine {
        engine.service.stop().await;
    }
}

fn share_policy(settings: &AppSettings) -> (bool, QuickShareSettings) {
    let enabled = settings.desktop.pages.contains(NotchPage::QuickShare);
    let mut policy = settings.quick_share.clone();
    policy.visible &= enabled;
    (enabled, policy)
}

enum EngineEvent {
    Endpoint(EndpointInfo),
    Transfer(ChannelMessage),
    Lost,
}

async fn next_event(engine: &mut Option<Engine>) -> EngineEvent {
    let Some(engine) = engine else {
        return std::future::pending().await;
    };
    tokio::select! {
        endpoint = engine.discovery.recv() => endpoint.map_or(EngineEvent::Lost, EngineEvent::Endpoint),
        transfer = engine.transfers.recv() => transfer.map_or(EngineEvent::Lost, EngineEvent::Transfer),
    }
}

async fn wait_picker(
    picker: &mut Option<tokio::task::JoinHandle<Result<Vec<PathBuf>, PortError>>>,
) -> Result<Vec<PathBuf>, PortError> {
    let Some(task) = picker else {
        return std::future::pending().await;
    };
    task.await
        .map_err(|error| PortError::new("quickShare.fileError", error))?
}

impl Service {
    fn publish(&mut self) {
        self.state.request = self.requests.front().cloned();
        self.presenter
            .apply(ViewUpdate::QuickShare(self.state.clone()));
    }

    fn notify_receive(&mut self) {
        self.publish();
        self.presenter.apply(ViewUpdate::QuickShareAttention);
    }

    fn receive_failed(&mut self, status: &'static str) {
        self.state.received_status = Some(status);
        self.notify_receive();
    }

    async fn ensure_engine(&mut self) -> Option<&mut Engine> {
        if self.engine.is_none() {
            let result = match std::fs::create_dir_all(&self.directory) {
                Ok(()) => Engine::start(&self.directory, &self.policy).await,
                Err(error) => Err(error.to_string()),
            };
            match result {
                Ok(engine) => self.engine = Some(engine),
                Err(error) => {
                    tracing::warn!(%error, "Quick Share could not start");
                    self.state.status = Some("quickShare.unavailable");
                }
            }
        }
        self.engine.as_mut()
    }

    async fn apply_policy(&mut self) {
        if self.policy.visible && self.ensure_engine().await.is_none() {
            return;
        }
        if let Some(engine) = &mut self.engine {
            engine
                .service
                .set_device_name(Some(self.policy.device_name.clone()));
            engine
                .service
                .change_visibility(visibility(self.policy.visible));
            if !self.policy.visible {
                for request in self.requests.drain(..) {
                    engine.decide(request.id, false);
                }
            }
        }
        self.state.visible = self.policy.visible;
    }

    async fn stop_if_idle(&mut self) {
        if !self.state.visible
            && !self.discovering
            && self.active.is_none()
            && self.accepted.is_empty()
            && let Some(mut engine) = self.engine.take()
        {
            engine.service.stop().await;
        }
    }

    async fn refresh_receiver_if_idle(&mut self) {
        if !self.refresh_after_send
            || self.active.is_some()
            || !self.requests.is_empty()
            || !self.accepted.is_empty()
        {
            return;
        }
        self.refresh_after_send = false;
        if !self.policy.visible {
            return;
        }

        // Android can reuse its previous receiver role when we keep the
        // endpoint identity from a completed outbound session.
        let discovering = self.discovering;
        self.discovering = false;
        self.devices.clear();
        self.state.devices.clear();
        if let Some(mut engine) = self.engine.take() {
            engine.service.stop().await;
        }
        self.state.visible = false;
        self.apply_policy().await;
        if self.state.visible && discovering {
            self.start_discovery().await;
        }
        self.publish();
    }

    async fn reset(&mut self) {
        if let Some(mut engine) = self.engine.take() {
            engine.service.stop().await;
        }
        self.requests.clear();
        self.accepted.clear();
        self.active = None;
        self.pending.clear();
        self.devices.clear();
        self.discovering = false;
        self.refresh_after_send = false;
        self.state.visible = false;
        self.state.sending = false;
        self.state.pin = None;
        self.state.devices.clear();
        self.state.status = Some("quickShare.interrupted");
    }

    async fn command(&mut self, command: Command) {
        match command {
            Command::Browse => {
                if self.picker.is_none() {
                    self.picker = Some(tokio::spawn(NativeShelfPlatform.pick_files()));
                }
            }
            Command::Stage(paths) => self.stage(paths),
            Command::StageText(text) => self.stage_text(text),
            Command::Remove(tray, id) => self.remove_files(tray, Some(id)),
            Command::Clear(tray) => self.remove_files(tray, None),
            Command::Discover(true) => {
                if self.discovering {
                    return;
                }
                self.state.status = None;
                self.start_discovery().await;
            }
            Command::Discover(false) => {
                self.discovering = false;
                if let Some(engine) = &mut self.engine {
                    engine.service.stop_discovery();
                }
                self.devices.clear();
                self.state.devices.clear();
            }
            Command::Send(device) => self.send(device),
            Command::Decide { id, accept } => {
                if let Some(index) = self.requests.iter().position(|request| request.id == id)
                    && let Some(engine) = &self.engine
                    && engine.decide(id.clone(), accept)
                {
                    self.requests.remove(index);
                    if accept {
                        self.accepted.insert(id);
                        self.state.received_status = Some("quickShare.receiving");
                    }
                }
            }
        }
    }

    fn remove_files(&mut self, tray: Tray, id: Option<Uuid>) {
        let keep = |entry: &ShelfEntry| id.is_some_and(|id| entry.item.id != id);
        match tray {
            Tray::Outgoing => {
                self.state.outgoing.retain(|entry| {
                    keep(entry)
                        || self
                            .active
                            .as_ref()
                            .is_some_and(|(_, ids)| ids.contains(&entry.item.id))
                        || self
                            .pending
                            .iter()
                            .any(|(_, ids)| ids.contains(&entry.item.id))
                });
            }
            Tray::Received => {
                let items: Vec<_> = self
                    .state
                    .received
                    .iter()
                    .filter(|entry| keep(entry))
                    .map(|entry| entry.item.clone())
                    .collect();
                if items.len() < self.state.received.len() && self.save_received(&items) {
                    self.state.received.retain(keep);
                    if self.state.received_status == Some("quickShare.received") {
                        self.state.received_status = None;
                    }
                }
            }
        }
    }

    async fn start_discovery(&mut self) {
        if let Some(engine) = self.ensure_engine().await {
            engine.discovery = engine.endpoints.subscribe();
            match engine.service.discovery(engine.endpoints.clone()) {
                Ok(()) => self.discovering = true,
                Err(error) => {
                    tracing::warn!(%error, "Quick Share discovery failed");
                    self.state.status = Some("quickShare.unavailable");
                }
            }
        }
    }

    fn stage(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        self.state.status = None;
        for path in paths {
            if self.state.outgoing.iter().any(|entry| {
                entry
                    .item
                    .path()
                    .is_some_and(|saved| is_same_shelf_path(saved, &path))
            }) {
                continue;
            }
            if let Ok(Some(metadata)) = NativeShelfPlatform.inspect_path(&path)
                && !metadata.is_directory
                && path.to_str().is_some()
            {
                self.state.outgoing.push(ShelfEntry::new(
                    ShelfItem::new(path, metadata, chrono::Utc::now()),
                    None,
                ));
            } else {
                self.state.status = Some("quickShare.fileError");
            }
        }
    }

    fn stage_text(&mut self, text: String) {
        if let Some(item) = ShelfItem::new_text(text, chrono::Utc::now()) {
            self.state.outgoing.push(ShelfEntry::new(item, None));
            if !self.state.sending {
                self.state.status = None;
            }
        }
    }

    fn endpoint(&mut self, endpoint: EndpointInfo) {
        if !self.discovering {
            return;
        }
        let id = endpoint.id;
        if endpoint.present != Some(true) {
            self.devices.remove(&id);
        } else if let (Some(name), Some(ip), Some(port)) =
            (endpoint.name, endpoint.ip, endpoint.port)
            && let Some(address) = endpoint_address(&ip, &port)
        {
            self.devices.insert(id, (name, address));
        }
        self.state.devices = self
            .devices
            .iter()
            .map(|(id, (name, _))| Device {
                id: id.clone(),
                name: name.clone(),
            })
            .collect();
    }

    fn send(&mut self, device: String) {
        if self.active.is_some() || self.state.outgoing.is_empty() {
            return;
        }
        let (Some(_), Some((name, address))) = (&self.engine, self.devices.get(&device)) else {
            self.state.status = Some("quickShare.unavailable");
            return;
        };
        self.pending = transfer_payloads(&self.state.outgoing)
            .into_iter()
            .map(|(payload, ids)| {
                (
                    SendInfo {
                        id: Uuid::new_v4().to_string(),
                        name: name.clone(),
                        addr: address.clone(),
                        ob: payload,
                    },
                    ids,
                )
            })
            .collect();
        self.send_next();
    }

    fn send_next(&mut self) {
        self.state.pin = None;
        self.state.sending = false;
        let Some((transfer, ids)) = self.pending.pop_front() else {
            return;
        };
        let id = transfer.id.clone();
        if self
            .engine
            .as_ref()
            .is_some_and(|engine| engine.send.try_send(transfer).is_ok())
        {
            self.active = Some((id, ids));
            self.state.sending = true;
            self.state.status = Some("quickShare.sending");
        } else {
            self.pending.clear();
            self.state.status = Some("quickShare.unavailable");
        }
    }

    fn transfer(&mut self, message: ChannelMessage) {
        if message.direction != ChannelDirection::LibToFront {
            return;
        }
        let Some(state) = message.state.as_ref() else {
            return;
        };
        let final_state = matches!(
            state,
            State::Finished | State::Cancelled | State::Rejected | State::Disconnected
        );
        // The inbound listeners use the remote socket address for TCP and "ble" for GATT.
        let listener_disconnect = *state == State::Disconnected
            && (message.id == "ble" || message.id.parse::<SocketAddr>().is_ok());
        let known_inbound = self.accepted.contains(&message.id)
            || self.requests.iter().any(|request| request.id == message.id);
        let untyped_inbound = known_inbound || listener_disconnect;
        match message.rtype {
            Some(TransferType::Inbound) | None if message.rtype.is_some() || untyped_inbound => {
                if *state == State::WaitingForUserConsent {
                    let request = message.meta.and_then(|meta| {
                        Some(Request {
                            id: message.id.clone(),
                            sender: meta.source?.name,
                            pin: meta.pin_code?,
                            files: meta.files.unwrap_or_default(),
                            text_description: meta.text_description,
                            bytes: meta.total_bytes,
                        })
                    });
                    if self.state.visible
                        && let Some(request) = request
                    {
                        if !self.requests.iter().any(|queued| queued.id == request.id) {
                            self.state.received_status = None;
                            self.requests.push_back(request);
                            self.presenter.apply(ViewUpdate::MascotAlert);
                            self.notify_receive();
                        }
                    } else {
                        if let Some(engine) = &self.engine {
                            engine.decide(message.id, false);
                        }
                        if self.state.visible {
                            self.receive_failed("quickShare.unsupportedReceive");
                        }
                    }
                } else if final_state {
                    self.requests.retain(|request| request.id != message.id);
                    if self.accepted.remove(&message.id) {
                        if *state == State::Finished
                            && let Some(meta) = message.meta
                        {
                            let items = self.received_items(meta.files.unwrap_or_default());
                            let text = if matches!(
                                meta.text_type,
                                Some(TextPayloadType::Text | TextPayloadType::Url)
                            ) {
                                meta.text_payload
                                    .and_then(|text| ShelfItem::new_text(text, chrono::Utc::now()))
                            } else {
                                None
                            };
                            self.receive(items.into_iter().chain(text).collect());
                        } else {
                            self.receive_failed("quickShare.transferError");
                        }
                    } else if self.state.visible
                        && *state == State::Disconnected
                        && (message.rtype.is_some()
                            || known_inbound
                            || !matches!(
                                self.state.received_status,
                                Some("quickShare.transferError" | "quickShare.unsupportedReceive")
                            ))
                    {
                        self.receive_failed("quickShare.receiveStartError");
                    }
                }
            }
            Some(TransferType::Outbound) | None
                if self
                    .active
                    .as_ref()
                    .is_some_and(|(id, _)| id == &message.id) =>
            {
                if final_state {
                    if let Some((_, ids)) = self.active.take()
                        && *state == State::Finished
                    {
                        self.state
                            .outgoing
                            .retain(|entry| !ids.contains(&entry.item.id));
                        self.state.status = Some("quickShare.sent");
                        self.refresh_after_send = true;
                        self.send_next();
                    } else {
                        self.pending.clear();
                        self.state.status = Some("quickShare.transferError");
                        self.state.sending = false;
                        self.state.pin = None;
                    }
                } else if let Some(pin) = message.meta.and_then(|meta| meta.pin_code) {
                    self.state.pin = Some(pin);
                }
            }
            _ => {}
        }
    }

    fn received_items(&self, names: Vec<String>) -> Vec<ShelfItem> {
        let mut items = Vec::new();
        for name in names {
            let path = Path::new(&name);
            if !matches!(
                path.components().collect::<Vec<_>>().as_slice(),
                [Component::Normal(_)]
            ) {
                continue;
            }
            let path = self.directory.join(path);
            if let Ok(Some(metadata)) = NativeShelfPlatform.inspect_path(&path)
                && !metadata.is_directory
            {
                items.push(ShelfItem::new(path, metadata, chrono::Utc::now()));
            }
        }
        items
    }

    fn receive(&mut self, items: Vec<ShelfItem>) {
        if items.is_empty() {
            self.receive_failed("quickShare.transferError");
            return;
        }
        let all: Vec<_> = items
            .iter()
            .rev()
            .cloned()
            .chain(self.state.received.iter().map(|entry| entry.item.clone()))
            .collect();
        if self.save_received(&all) {
            self.state.received.splice(
                0..0,
                items
                    .into_iter()
                    .rev()
                    .map(|item| ShelfEntry::new(item, None)),
            );
            self.state.received_status = Some("quickShare.received");
        } else {
            self.notify_receive();
        }
    }

    fn save_received(&mut self, items: &[ShelfItem]) -> bool {
        if self.can_save_received {
            if let Err(error) = self.received.save(items) {
                tracing::warn!(%error, "Quick Share received list could not be saved");
            } else {
                return true;
            }
        }
        self.state.received_status = Some("quickShare.storageError");
        false
    }
}

fn transfer_payloads(entries: &[ShelfEntry]) -> Vec<(OutboundPayload, Vec<Uuid>)> {
    let mut files = Vec::new();
    let mut file_ids = Vec::new();
    let mut payloads = Vec::new();
    for entry in entries {
        if let Some(path) = entry.item.path().and_then(Path::to_str) {
            files.push(path.to_owned());
            file_ids.push(entry.item.id);
        } else if let Some(text) = entry.item.text() {
            payloads.push((OutboundPayload::Text(text.to_owned()), vec![entry.item.id]));
        }
    }
    if !files.is_empty() {
        payloads.insert(0, (OutboundPayload::Files(files), file_ids));
    }
    payloads
}

fn endpoint_address(ip: &str, port: &str) -> Option<String> {
    if ip == "ble" {
        let octets = port.split(':');
        return (octets.clone().count() == 6
            && octets
                .into_iter()
                .all(|octet| octet.len() == 2 && u8::from_str_radix(octet, 16).is_ok()))
        .then(|| format!("ble:{port}"));
    }
    let ip = ip.trim_matches(['[', ']']).parse::<IpAddr>().ok()?;
    let port = port.parse::<u16>().ok().filter(|port| *port != 0)?;
    Some(SocketAddr::new(ip, port).to_string())
}
