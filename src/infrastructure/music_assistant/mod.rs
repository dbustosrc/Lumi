//! Music Assistant WebSocket client & native Sendspin local audio player:
//! follows playback, streams audio locally to Windows speakers, and forwards
//! transport commands.

mod audio_output;
mod protocol;
mod sendspin_player;
mod tracker;

use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use self::protocol::{MIN_SCHEMA_VERSION, ServerInfo, parse_queue, parse_seconds, websocket_url};
use self::sendspin_player::{local_player_id, supervise_sendspin_player};
use self::tracker::QueueTracker;
use crate::application::events::{AppEvent, EventSender};
use crate::application::settings::{AppSettings, MusicAssistantSettings};
use crate::domain::media::{MediaCommand, MediaConnection, PlaybackSnapshot};
use crate::domain::ports::MediaController;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const QUEUES_REQUEST_ID: &str = "lumi-queues";
const AUTH_REQUEST_ID: &str = "lumi-auth";
const MIN_RETRY_DELAY: Duration = Duration::from_secs(2);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(60);

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug, thiserror::Error)]
enum SessionError {
    #[error("music.error.authentication")]
    Authentication,
    #[error("music.error.connection: {0}")]
    Connection(String),
    #[error("music.error.protocol: {0}")]
    Protocol(&'static str),
}

impl SessionError {
    fn connection(error: impl ToString) -> Self {
        Self::Connection(error.to_string())
    }
}

/// [`MediaController`] backed by the running Music Assistant session.
pub struct MusicAssistantController {
    commands: mpsc::UnboundedSender<MediaCommand>,
}

impl MediaController for MusicAssistantController {
    fn execute(&self, command: MediaCommand) {
        if self.commands.send(command).is_err() {
            tracing::warn!(event = "log.music.supervisorStopped");
        }
    }
}

/// Starts both the control WebSocket supervisor and the native Sendspin local
/// audio player on the current Tokio runtime.
pub fn spawn_music_assistant(
    settings: watch::Receiver<AppSettings>,
    events: EventSender,
) -> MusicAssistantController {
    let (commands, receiver) = mpsc::unbounded_channel();
    tokio::spawn(supervise_sendspin_player(settings.clone()));
    tokio::spawn(supervise(settings, events, receiver));
    MusicAssistantController { commands }
}

/// Keeps one session alive per configuration, reconnecting with backoff and
/// restarting immediately when the Music Assistant settings change.
async fn supervise(
    mut settings: watch::Receiver<AppSettings>,
    events: EventSender,
    mut commands: mpsc::UnboundedReceiver<MediaCommand>,
) {
    let mut retry_delay = MIN_RETRY_DELAY;
    loop {
        let config = settings.borrow_and_update().music_assistant.clone();
        if !config.is_enabled || config.server_url.trim().is_empty() {
            publish_disconnected(&events, MediaConnection::NotConfigured);
            if !wait_for_change(&mut settings, &config).await {
                return;
            }
            continue;
        }
        discard_stale_commands(&mut commands);
        publish(
            &events,
            AppEvent::MediaConnection(MediaConnection::Connecting),
        );
        tokio::select! {
            result = run_session(&config, &events, &mut commands) => {
                let state = report_session_end(result);
                publish_disconnected(&events, state);
            }
            changed = wait_for_change(&mut settings, &config) => {
                publish_disconnected(&events, MediaConnection::NotConfigured);
                if !changed { return; }
                retry_delay = MIN_RETRY_DELAY;
                continue;
            }
        }
        tokio::select! {
            () = tokio::time::sleep(retry_delay) => {
                retry_delay = (retry_delay * 2).min(MAX_RETRY_DELAY);
            }
            changed = wait_for_change(&mut settings, &config) => {
                if !changed { return; }
                retry_delay = MIN_RETRY_DELAY;
            }
        }
    }
}

fn report_session_end(result: Result<(), SessionError>) -> MediaConnection {
    match result {
        Ok(()) => {
            tracing::warn!(event = "log.music.closedByServer");
            MediaConnection::Unavailable
        }
        Err(SessionError::Authentication) => {
            tracing::error!(event = "log.music.authenticationFailed");
            MediaConnection::AuthenticationFailed
        }
        Err(error) => {
            tracing::error!(event = "log.music.sessionFailed", error = %error);
            MediaConnection::Unavailable
        }
    }
}

async fn wait_for_change(
    settings: &mut watch::Receiver<AppSettings>,
    current: &MusicAssistantSettings,
) -> bool {
    while settings.changed().await.is_ok() {
        if settings.borrow().music_assistant != *current {
            return true;
        }
    }
    false
}

fn discard_stale_commands(commands: &mut mpsc::UnboundedReceiver<MediaCommand>) {
    while let Ok(command) = commands.try_recv() {
        tracing::warn!(event = "log.music.commandIgnoredDisconnected", command = ?command);
    }
}

async fn run_session(
    config: &MusicAssistantSettings,
    events: &EventSender,
    commands: &mut mpsc::UnboundedReceiver<MediaCommand>,
) -> Result<(), SessionError> {
    let (mut socket, server) = tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
        let url = websocket_url(&config.server_url);
        let (mut socket, _) = tokio_tungstenite::connect_async(url.as_str())
            .await
            .map_err(SessionError::connection)?;
        let server = read_server_info(&mut socket).await?;
        if server.schema_version < MIN_SCHEMA_VERSION {
            return Err(SessionError::Protocol(
                "Music Assistant schema 31 or newer is required",
            ));
        }
        authenticate(&mut socket, &config.token).await?;
        send_command(
            &mut socket,
            QUEUES_REQUEST_ID,
            "player_queues/all",
            json!({}),
        )
        .await?;
        Ok((socket, server))
    })
    .await
    .map_err(|_| SessionError::Protocol("Music Assistant handshake timed out"))??;
    tracing::info!(
        event = "log.music.sessionConnected",
        schema_version = server.schema_version
    );
    publish(
        events,
        AppEvent::MediaConnection(MediaConnection::Connected),
    );
    let tracker = QueueTracker::new(&config.queue_id, local_player_id());
    let mut session = Session {
        tracker,
        server,
        next_id: 0,
    };
    session.run(&mut socket, events, commands).await
}

async fn read_server_info(socket: &mut Socket) -> Result<ServerInfo, SessionError> {
    let message = next_json(socket)
        .await?
        .ok_or(SessionError::Protocol("closed before server info"))?;
    ServerInfo::parse(&message).ok_or(SessionError::Protocol("invalid server info"))
}

async fn authenticate(socket: &mut Socket, token: &str) -> Result<(), SessionError> {
    if token.trim().is_empty() {
        return Err(SessionError::Authentication);
    }
    send_command(
        socket,
        AUTH_REQUEST_ID,
        "auth",
        json!({ "token": token.trim() }),
    )
    .await?;
    while let Some(message) = next_json(socket).await? {
        if message.get("message_id").and_then(Value::as_str) != Some(AUTH_REQUEST_ID) {
            continue;
        }
        let is_accepted = message.get("error_code").is_none()
            && message
                .get("result")
                .is_some_and(|result| !result.is_null() && result != &json!(false));
        return if is_accepted {
            Ok(())
        } else {
            Err(SessionError::Authentication)
        };
    }
    Err(SessionError::Protocol("closed during authentication"))
}

async fn send_command(
    socket: &mut Socket,
    message_id: &str,
    command: &str,
    args: Value,
) -> Result<(), SessionError> {
    let message = json!({ "message_id": message_id, "command": command, "args": args });
    socket
        .send(Message::text(message.to_string()))
        .await
        .map_err(SessionError::connection)
}

async fn next_json(socket: &mut Socket) -> Result<Option<Value>, SessionError> {
    while let Some(frame) = socket.next().await {
        match frame.map_err(SessionError::connection)? {
            Message::Text(text) => {
                return serde_json::from_str(text.as_str())
                    .map(Some)
                    .map_err(|_| SessionError::Protocol("invalid json"));
            }
            Message::Close(_) => return Ok(None),
            _ => {}
        }
    }
    Ok(None)
}

struct Session {
    tracker: QueueTracker,
    server: ServerInfo,
    next_id: u64,
}

impl Session {
    async fn run(
        &mut self,
        socket: &mut Socket,
        events: &EventSender,
        commands: &mut mpsc::UnboundedReceiver<MediaCommand>,
    ) -> Result<(), SessionError> {
        loop {
            tokio::select! {
                message = next_json(socket) => match message? {
                    Some(message) => self.handle_message(socket, &message, events).await?,
                    None => return Ok(()),
                },
                Some(command) = commands.recv() => {
                    self.send_media_command(socket, command).await?;
                }
            }
        }
    }

    async fn handle_message(
        &mut self,
        socket: &mut Socket,
        message: &Value,
        events: &EventSender,
    ) -> Result<(), SessionError> {
        if message.get("message_id").and_then(Value::as_str) == Some(QUEUES_REQUEST_ID) {
            let queues = message
                .get("result")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            queues.iter().for_each(|queue| self.on_queue(queue, events));
            return self.maybe_transfer_to_local(socket).await;
        }
        if let Some(code) = message.get("error_code") {
            tracing::warn!(event = "log.music.commandRejected", error_code = %code);
            return Ok(());
        }
        match message.get("event").and_then(Value::as_str) {
            Some("queue_added" | "queue_updated") => {
                if let Some(queue) = message.get("data") {
                    self.on_queue(queue, events);
                    self.maybe_transfer_to_local(socket).await?;
                }
            }
            Some("queue_time_updated") => self.on_time(message, events),
            _ => {}
        }
        Ok(())
    }

    fn on_queue(&mut self, value: &Value, events: &EventSender) {
        if let Some(queue) = parse_queue(value, &self.server) {
            publish_snapshot(events, self.tracker.upsert(queue));
        }
    }

    fn on_time(&mut self, message: &Value, events: &EventSender) {
        let Some(queue_id) = message.get("object_id").and_then(Value::as_str) else {
            return;
        };
        let elapsed = parse_seconds(message.get("data"));
        publish_snapshot(events, self.tracker.update_elapsed(queue_id, elapsed));
    }

    async fn maybe_transfer_to_local(&mut self, socket: &mut Socket) -> Result<(), SessionError> {
        if let Some((source, target)) = self.tracker.take_auto_transfer(Instant::now()) {
            self.transfer_queue(socket, &source, &target, true).await?;
        }
        Ok(())
    }

    async fn transfer_queue(
        &mut self,
        socket: &mut Socket,
        source_queue_id: &str,
        target_queue_id: &str,
        auto_play: bool,
    ) -> Result<(), SessionError> {
        self.next_id += 1;
        let id = format!("lumi-transfer-{}", self.next_id);
        let args = json!({
            "source_queue_id": source_queue_id,
            "target_queue_id": target_queue_id,
            "auto_play": auto_play,
        });
        tracing::info!(
            event = "log.music.queueTransferred",
            source = %source_queue_id,
            target = %target_queue_id
        );
        send_command(socket, &id, "player_queues/transfer", args).await
    }

    async fn send_media_command(
        &mut self,
        socket: &mut Socket,
        command: MediaCommand,
    ) -> Result<(), SessionError> {
        if matches!(command, MediaCommand::PlayPause)
            && let Some((source, target)) = self.tracker.take_play_transfer(Instant::now())
        {
            return self.transfer_queue(socket, &source, &target, true).await;
        }
        let Some(queue_id) = self.tracker.followed_queue_id().map(str::to_owned) else {
            tracing::warn!(event = "log.music.noActiveQueue");
            return Ok(());
        };
        let (name, args) = match command {
            MediaCommand::PlayPause => {
                ("player_queues/play_pause", json!({ "queue_id": queue_id }))
            }
            MediaCommand::Next => ("player_queues/next", json!({ "queue_id": queue_id })),
            MediaCommand::Previous => ("player_queues/previous", json!({ "queue_id": queue_id })),
            MediaCommand::Seek(position) => (
                "player_queues/seek",
                json!({ "queue_id": queue_id, "position": position.as_secs() }),
            ),
        };
        self.next_id += 1;
        send_command(socket, &format!("lumi-{}", self.next_id), name, args).await
    }
}

fn publish_snapshot(events: &EventSender, snapshot: Option<PlaybackSnapshot>) {
    if let Some(snapshot) = snapshot {
        publish(events, AppEvent::Playback(snapshot));
    }
}

fn publish_disconnected(events: &EventSender, state: MediaConnection) {
    publish(events, AppEvent::MediaConnection(state));
    publish(events, AppEvent::Playback(PlaybackSnapshot::idle()));
}

fn publish(events: &EventSender, event: AppEvent) {
    if events.send(event).is_err() {
        tracing::warn!(event = "log.events.receiverClosed");
    }
}

#[cfg(test)]
#[path = "../../../tests/fixtures/music_assistant.rs"]
mod tests;
