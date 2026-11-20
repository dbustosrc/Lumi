use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rqs_lib::channel::{ChannelDirection, ChannelMessage, TransferType};
use rqs_lib::{OutboundPayload, State};

use super::Service;
use chrono::Utc;

use crate::application::presenter::{NotchPresenter, ShelfEntry, ViewUpdate as PresenterUpdate};
use crate::application::quick_share::{Snapshot, Tray};
use crate::application::settings::QuickShareSettings;
use crate::domain::ports::ShelfRepository;
use crate::domain::shelf::{ShelfContent, ShelfItem, ShelfItemKind};
use crate::infrastructure::shelf_store::JsonShelfRepository;

#[derive(Default)]
struct RecordingPresenter(Mutex<Vec<PresenterUpdate>>);

impl NotchPresenter for RecordingPresenter {
    fn apply(&self, update: PresenterUpdate) {
        self.0.lock().unwrap().push(update);
    }
}

fn service(presenter: Arc<RecordingPresenter>) -> (Service, PathBuf) {
    let root = std::env::temp_dir().join(format!("lumi-quick-share-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let received = JsonShelfRepository::new(root.join("received.json"));
    received.save(&[]).unwrap();
    (
        Service {
            engine: None,
            directory: root.join("received"),
            received,
            can_save_received: true,
            policy: QuickShareSettings::default(),
            presenter,
            state: Snapshot {
                visible: true,
                ..Snapshot::default()
            },
            devices: Default::default(),
            requests: Default::default(),
            accepted: Default::default(),
            active: None,
            pending: Default::default(),
            discovering: false,
            refresh_after_send: false,
            picker: None,
        },
        root,
    )
}

fn disconnected(id: &str, rtype: Option<TransferType>) -> ChannelMessage {
    ChannelMessage {
        id: id.into(),
        direction: ChannelDirection::LibToFront,
        rtype,
        state: Some(State::Disconnected),
        ..ChannelMessage::default()
    }
}

fn inbound_message(
    id: &str,
    state: State,
    files: &[&str],
    text_description: Option<&str>,
    text_type: Option<&str>,
    text_payload: Option<&str>,
) -> ChannelMessage {
    let meta = serde_json::from_value(serde_json::json!({
        "id": id,
        "source": {"name": "Phone", "device_type": "Phone"},
        "pin_code": "123456",
        "files": files,
        "text_description": text_description,
        "text_type": text_type,
        "text_payload": text_payload,
        "total_bytes": 7,
        "ack_bytes": 0
    }))
    .unwrap();
    ChannelMessage {
        id: id.into(),
        direction: ChannelDirection::LibToFront,
        rtype: Some(TransferType::Inbound),
        state: Some(state),
        meta: Some(meta),
        ..ChannelMessage::default()
    }
}

fn has_attention(presenter: &RecordingPresenter) -> bool {
    presenter
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|update| matches!(update, PresenterUpdate::QuickShareAttention))
}

fn last_received_status(presenter: &RecordingPresenter) -> Option<Option<&'static str>> {
    presenter.0.lock().unwrap().iter().rev().find_map(|update| {
        if let PresenterUpdate::QuickShare(snapshot) = update {
            Some(snapshot.received_status)
        } else {
            None
        }
    })
}

fn file_entry(path: PathBuf, id: uuid::Uuid) -> ShelfEntry {
    ShelfEntry::new(
        ShelfItem {
            id,
            content: ShelfContent::File(path.clone()),
            display_name: path.file_name().unwrap().to_string_lossy().into_owned(),
            size_bytes: 4,
            is_directory: false,
            kind: ShelfItemKind::Document,
            added_at: Utc::now(),
        },
        None,
    )
}

#[test]
fn disconnect_before_consent_announces_receive_start_error_for_inbound_ble_and_tcp() {
    for (id, rtype) in [
        ("transfer-id", Some(TransferType::Inbound)),
        ("ble", None),
        ("127.0.0.1:45678", None),
    ] {
        let presenter = Arc::new(RecordingPresenter::default());
        let (mut service, root) = service(presenter.clone());

        service.transfer(disconnected(id, rtype));

        assert_eq!(
            service.state.received_status,
            Some("quickShare.receiveStartError"),
            "unexpected receive status for {id}"
        );
        assert_eq!(
            last_received_status(&presenter),
            Some(Some("quickShare.receiveStartError")),
            "published snapshot was not updated for {id}"
        );
        assert!(has_attention(&presenter), "no native attention for {id}");
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn accepted_transfer_error_survives_duplicate_ble_listener_disconnect() {
    let presenter = Arc::new(RecordingPresenter::default());
    let (mut service, root) = service(presenter.clone());
    service.accepted.insert("transfer-id".into());

    service.transfer(disconnected("transfer-id", Some(TransferType::Inbound)));

    assert_eq!(
        service.state.received_status,
        Some("quickShare.transferError")
    );
    assert!(has_attention(&presenter));
    let attention_count = presenter
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|update| matches!(update, PresenterUpdate::QuickShareAttention))
        .count();

    service.transfer(disconnected("ble", None));

    assert_eq!(
        service.state.received_status,
        Some("quickShare.transferError")
    );
    assert_eq!(
        last_received_status(&presenter),
        Some(Some("quickShare.transferError"))
    );
    assert_eq!(
        presenter
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|update| matches!(update, PresenterUpdate::QuickShareAttention))
            .count(),
        attention_count
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn consent_request_publishes_native_attention() {
    let presenter = Arc::new(RecordingPresenter::default());
    let (mut service, root) = service(presenter.clone());
    let meta = serde_json::from_value(serde_json::json!({
        "id": "transfer-id",
        "source": {"name": "Phone", "device_type": "Phone"},
        "pin_code": "123456",
        "files": ["photo.jpg"],
        "total_bytes": 7,
        "ack_bytes": 0
    }))
    .unwrap();
    let request = ChannelMessage {
        id: "transfer-id".into(),
        direction: ChannelDirection::LibToFront,
        rtype: Some(TransferType::Inbound),
        state: Some(State::WaitingForUserConsent),
        meta: Some(meta),
        ..ChannelMessage::default()
    };

    service.transfer(request);

    assert_eq!(
        service
            .state
            .request
            .as_ref()
            .map(|request| request.sender.as_str()),
        Some("Phone")
    );
    assert!(has_attention(&presenter));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn text_only_consent_request_is_supported_without_saving_received_content() {
    let presenter = Arc::new(RecordingPresenter::default());
    let (mut service, root) = service(presenter.clone());
    service.transfer(inbound_message(
        "text-only",
        State::WaitingForUserConsent,
        &[],
        Some("Shared note"),
        Some("Text"),
        Some("Text to receive"),
    ));

    let request = service.state.request.as_ref().unwrap();
    assert_eq!(request.files, Vec::<String>::new());
    assert_eq!(request.text_description.as_deref(), Some("Shared note"));
    assert_eq!(request.sender, "Phone");
    assert_eq!(service.state.received_status, None);
    assert!(service.state.received.is_empty());
    assert!(service.received.load().unwrap().is_empty());
    assert!(has_attention(&presenter));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn accepted_text_and_url_payloads_are_received_and_persisted() {
    for (id, kind, description, payload) in [
        ("plain-text", "Text", "A note", "Text payload received"),
        (
            "shared-url",
            "Url",
            "A shared link",
            "https://example.test/shared",
        ),
    ] {
        let (mut service, root) = service(Arc::new(RecordingPresenter::default()));
        service.transfer(inbound_message(
            id,
            State::WaitingForUserConsent,
            &[],
            Some(description),
            Some(kind),
            Some(payload),
        ));
        assert_eq!(
            service
                .state
                .request
                .as_ref()
                .unwrap()
                .text_description
                .as_deref(),
            Some(description)
        );
        assert!(service.state.received.is_empty());
        assert!(service.received.load().unwrap().is_empty());

        service.accepted.insert(id.into());
        service.transfer(inbound_message(
            id,
            State::Finished,
            &[],
            Some(description),
            Some(kind),
            Some(payload),
        ));

        assert_eq!(service.state.received.len(), 1);
        assert_eq!(service.state.received[0].item.text(), Some(payload));
        let reloaded = service.received.load().unwrap();
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded[0].text(), Some(payload));
        assert_eq!(service.state.received_status, Some("quickShare.received"));

        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn mixed_file_and_text_shelf_entries_form_local_outbound_payloads() {
    let root = std::env::temp_dir().join(format!(
        "lumi-quick-share-payloads-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let paths = [root.join("first.txt"), root.join("second.txt")];
    for path in &paths {
        std::fs::write(path, "file payload").unwrap();
    }
    let files = [
        file_entry(paths[0].clone(), uuid::Uuid::from_u128(11)),
        file_entry(paths[1].clone(), uuid::Uuid::from_u128(12)),
    ];
    let texts = [
        ShelfEntry::new(
            ShelfItem::new_text("First text payload".into(), Utc::now()).unwrap(),
            None,
        ),
        ShelfEntry::new(
            ShelfItem::new_text("https://example.test/second".into(), Utc::now()).unwrap(),
            None,
        ),
    ];
    let entries = vec![
        files[0].clone(),
        texts[0].clone(),
        files[1].clone(),
        texts[1].clone(),
    ];

    let payloads = super::transfer_payloads(&entries);

    assert_eq!(payloads.len(), 3);
    assert!(matches!(
        &payloads[0].0,
        OutboundPayload::Files(names)
            if names == &paths.iter().map(|path| path.to_string_lossy().into_owned()).collect::<Vec<_>>()
    ));
    assert_eq!(payloads[0].1, vec![files[0].item.id, files[1].item.id]);
    assert!(matches!(
        &payloads[1].0,
        OutboundPayload::Text(text) if text == "First text payload"
    ));
    assert_eq!(payloads[1].1, vec![texts[0].item.id]);
    assert!(matches!(
        &payloads[2].0,
        OutboundPayload::Text(text) if text == "https://example.test/second"
    ));
    assert_eq!(payloads[2].1, vec![texts[1].item.id]);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn quick_share_remove_and_clear_forget_both_trays_without_deleting_files() {
    let presenter = Arc::new(RecordingPresenter::default());
    let (mut service, root) = service(presenter);
    let paths: Vec<_> = [
        "outgoing-remove.txt",
        "outgoing-clear.txt",
        "received-remove.txt",
        "received-clear.txt",
    ]
    .map(|name| {
        let path = root.join(name);
        std::fs::write(&path, name).unwrap();
        path
    })
    .into();
    let entries: Vec<_> = paths
        .iter()
        .enumerate()
        .map(|(index, path)| file_entry(path.clone(), uuid::Uuid::from_u128(index as u128 + 1)))
        .collect();
    service.state.outgoing = entries[..2].to_vec();
    service.state.received = entries[2..].to_vec();
    service
        .received
        .save(
            &service
                .state
                .received
                .iter()
                .map(|entry| entry.item.clone())
                .collect::<Vec<_>>(),
        )
        .unwrap();

    service.remove_files(Tray::Outgoing, Some(entries[0].item.id));
    service.remove_files(Tray::Received, Some(entries[2].item.id));
    assert_eq!(service.state.outgoing, vec![entries[1].clone()]);
    assert_eq!(service.state.received, vec![entries[3].clone()]);
    assert_eq!(
        service.received.load().unwrap(),
        vec![entries[3].item.clone()]
    );

    service.remove_files(Tray::Outgoing, None);
    service.remove_files(Tray::Received, None);
    assert!(service.state.outgoing.is_empty());
    assert!(service.state.received.is_empty());
    assert!(service.received.load().unwrap().is_empty());
    for (path, name) in paths.iter().zip([
        "outgoing-remove.txt",
        "outgoing-clear.txt",
        "received-remove.txt",
        "received-clear.txt",
    ]) {
        assert_eq!(std::fs::read_to_string(path).unwrap(), name);
    }

    std::fs::remove_dir_all(root).unwrap();
}
