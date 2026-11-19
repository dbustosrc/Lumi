use std::sync::Arc;

use crate::application::presenter::{NotchPresenter, ViewUpdate};
use crate::application::quick_share::Snapshot;
use crate::application::settings::AppSettings;
use crate::domain::ports::ShelfRepository;
use crate::domain::shelf::{ShelfItem, ShelfPathMetadata};
use crate::infrastructure::quick_share;
use crate::infrastructure::shelf_store::JsonShelfRepository;

struct RecordingPresenter(tokio::sync::watch::Sender<Option<Snapshot>>);

impl NotchPresenter for RecordingPresenter {
    fn apply(&self, update: ViewUpdate) {
        if let ViewUpdate::QuickShare(snapshot) = update {
            self.0.send_replace(Some(snapshot));
        }
    }
}

#[tokio::test]
async fn quick_share_restores_only_existing_received_files_from_its_storage() {
    let root = std::env::temp_dir().join(format!("lumi-quick-share-{}", uuid::Uuid::new_v4()));
    let received_dir = root.join("received");
    std::fs::create_dir_all(&received_dir).unwrap();
    let saved_path = received_dir.join("saved.txt");
    std::fs::write(&saved_path, "data").unwrap();
    let outside_path = root.join("outside.txt");
    std::fs::write(&outside_path, "data").unwrap();
    let missing_path = received_dir.join("missing.txt");
    let metadata = |path: &std::path::Path| ShelfPathMetadata {
        display_name: path.file_name().unwrap().to_string_lossy().into_owned(),
        size_bytes: 4,
        is_directory: false,
    };
    let saved = ShelfItem::new(saved_path.clone(), metadata(&saved_path), chrono::Utc::now());
    let outside = ShelfItem::new(
        outside_path.clone(),
        metadata(&outside_path),
        chrono::Utc::now(),
    );
    let missing = ShelfItem::new(
        missing_path.clone(),
        metadata(&missing_path),
        chrono::Utc::now(),
    );
    JsonShelfRepository::new(root.join("received.json"))
        .save(&[saved, outside, missing])
        .unwrap();

    let (updates, mut snapshots) = tokio::sync::watch::channel(None);
    let (settings, settings_rx) = tokio::sync::watch::channel(AppSettings::default());
    let (commands, commands_rx) = tokio::sync::mpsc::unbounded_channel();
    let service = tokio::spawn(quick_share::run(
        root.clone(),
        settings_rx,
        commands_rx,
        Arc::new(RecordingPresenter(updates)),
    ));

    snapshots.changed().await.unwrap();
    let snapshot = snapshots.borrow_and_update().clone().unwrap();
    assert_eq!(snapshot.received.len(), 1);
    assert_eq!(snapshot.received[0].item.path().unwrap(), saved_path);
    assert_eq!(snapshot.received_status, None);

    drop(commands);
    drop(settings);
    service.await.unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
