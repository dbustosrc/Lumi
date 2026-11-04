use super::*;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::application::events::create_event_channel;
use crate::domain::ports::{PortError, PortFuture, ShelfPlatform, ShelfRepository};
use crate::domain::shelf::{ShelfItem, ShelfPathMetadata, ShelfThumbnail};
use crate::infrastructure::shelf_store::JsonShelfRepository;
use chrono::Utc;

#[derive(Default)]
struct MemoryShelfRepository {
    items: Mutex<Vec<ShelfItem>>,
    fail_save: AtomicBool,
    save_attempts: AtomicUsize,
}

impl MemoryShelfRepository {
    fn with_items(items: Vec<ShelfItem>) -> Self {
        Self {
            items: Mutex::new(items),
            ..Self::default()
        }
    }
}

impl ShelfRepository for MemoryShelfRepository {
    fn load(&self) -> Result<Vec<ShelfItem>, PortError> {
        Ok(self.items.lock().unwrap().clone())
    }

    fn save(&self, items: &[ShelfItem]) -> Result<(), PortError> {
        self.save_attempts.fetch_add(1, Ordering::SeqCst);
        if self.fail_save.load(Ordering::SeqCst) {
            return Err(PortError::new(
                "shelf.error.storageWrite",
                "mock save failure",
            ));
        }
        *self.items.lock().unwrap() = items.to_vec();
        Ok(())
    }
}

#[derive(Default)]
struct MemoryShelfPlatform {
    copied_text: Mutex<Vec<String>>,
    opened_paths: Mutex<Vec<PathBuf>>,
    fail_copy: AtomicBool,
    inspect_error: Mutex<Option<PortError>>,
    missing_paths: Mutex<Vec<PathBuf>>,
}

impl ShelfPlatform for MemoryShelfPlatform {
    fn inspect_path(&self, path: &Path) -> Result<Option<ShelfPathMetadata>, PortError> {
        if let Some(error) = self.inspect_error.lock().unwrap().clone() {
            return Err(error);
        }
        if self
            .missing_paths
            .lock()
            .unwrap()
            .iter()
            .any(|missing| missing == path)
        {
            return Ok(None);
        }
        Ok(Some(ShelfPathMetadata {
            display_name: path.file_name().unwrap().to_string_lossy().into_owned(),
            size_bytes: 42,
            is_directory: false,
        }))
    }

    fn open_path(&self, path: &Path) -> Result<(), PortError> {
        self.opened_paths.lock().unwrap().push(path.to_path_buf());
        Ok(())
    }

    fn copy_text(&self, text: &str) -> Result<(), PortError> {
        if self.fail_copy.load(Ordering::SeqCst) {
            return Err(PortError::new(
                "shelf.error.copyFailed",
                "mock copy failure",
            ));
        }
        self.copied_text.lock().unwrap().push(text.to_owned());
        Ok(())
    }

    fn pick_files(&self) -> PortFuture<Result<Vec<PathBuf>, PortError>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn load_thumbnail(&self, _path: PathBuf) -> PortFuture<Option<ShelfThumbnail>> {
        Box::pin(async { None })
    }
}

struct ShelfFixture {
    service: ShelfService,
    repository: Arc<MemoryShelfRepository>,
    platform: Arc<MemoryShelfPlatform>,
}

fn fixture(items: Vec<ShelfItem>) -> ShelfFixture {
    fixture_with_platform(items, Arc::new(MemoryShelfPlatform::default()))
}

fn fixture_with_platform(
    items: Vec<ShelfItem>,
    platform: Arc<MemoryShelfPlatform>,
) -> ShelfFixture {
    let repository = Arc::new(MemoryShelfRepository::with_items(items));
    let (events, _receiver) = create_event_channel();
    let repository_port: Arc<dyn ShelfRepository> = repository.clone();
    let platform_port: Arc<dyn ShelfPlatform> = platform.clone();
    let service = ShelfService::load(repository_port, platform_port, events).unwrap();
    ShelfFixture {
        service,
        repository,
        platform,
    }
}

fn text_item(text: &str) -> ShelfItem {
    ShelfItem::new_text(text.to_owned(), Utc::now()).unwrap()
}

fn file_item(path: &str) -> ShelfItem {
    ShelfItem::new(
        PathBuf::from(path),
        ShelfPathMetadata {
            display_name: "report.txt".into(),
            size_bytes: 42,
            is_directory: false,
        },
        Utc::now(),
    )
}

#[test]
fn adding_and_opening_a_text_card_preserves_and_copies_exact_text() {
    let mut fixture = fixture(Vec::new());
    let text = "  first line\nsecond line  \n";

    assert!(fixture.service.add_text(text.to_owned()).unwrap());
    let mut entries = fixture.service.entries();
    let entry = entries.remove(0);
    assert_eq!(entry.item.text(), Some(text));
    assert_eq!(entry.item.display_name, "first line");
    assert_eq!(entry.item.badge(), "TXT");

    let (was_pruned, result) = fixture.service.open_item(entry.item.id);
    assert!(!was_pruned);
    result.unwrap();
    assert_eq!(*fixture.platform.copied_text.lock().unwrap(), [text]);
}

#[test]
fn blank_or_nul_text_is_rejected_without_persistence() {
    let mut fixture = fixture(Vec::new());

    assert!(fixture.service.add_text(" \n\t".into()).is_err());
    assert!(fixture.service.add_text("contains\0nul".into()).is_err());

    assert!(fixture.service.entries().is_empty());
    assert_eq!(fixture.repository.save_attempts.load(Ordering::SeqCst), 0);
}

#[test]
fn shelf_reads_reject_wrong_schema_and_unknown_fields() {
    let path =
        std::env::temp_dir().join(format!("lumi-shelf-schema-{}.json", uuid::Uuid::new_v4()));
    let repository = JsonShelfRepository::new(path.clone());

    for content in [
        r#"{"schema_version":1,"items":[]}"#,
        r#"{"schema_version":2,"items":[],"unexpected":true}"#,
    ] {
        std::fs::write(&path, content).unwrap();
        assert!(repository.load().is_err(), "should reject {content}");
    }

    let _ = std::fs::remove_file(path);
}

#[test]
fn failed_save_leaves_add_remove_clear_and_file_add_state_unchanged() {
    let keep = text_item("keep me");
    let keep_id = keep.id;
    let mut fixture = fixture(vec![keep]);
    fixture.repository.fail_save.store(true, Ordering::SeqCst);

    assert!(fixture.service.add_text("new text".into()).is_err());
    assert!(fixture.service.remove(keep_id).is_err());
    assert!(fixture.service.clear().is_err());
    assert!(
        fixture
            .service
            .add_paths(&[PathBuf::from(r"C:\Temp\report.txt")])
            .is_err()
    );

    let entries = fixture.service.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].item.text(), Some("keep me"));
    assert_eq!(fixture.repository.items.lock().unwrap().len(), 1);
    assert_eq!(fixture.repository.save_attempts.load(Ordering::SeqCst), 4);
}

#[test]
fn successful_clear_persists_an_empty_shelf() {
    let mut fixture = fixture(vec![text_item("clear me")]);

    assert!(fixture.service.clear().unwrap());

    assert!(fixture.service.entries().is_empty());
    assert!(fixture.repository.items.lock().unwrap().is_empty());
    assert_eq!(fixture.repository.save_attempts.load(Ordering::SeqCst), 1);
}

#[test]
fn copying_failure_is_returned_without_removing_the_text_card() {
    let item = text_item("clipboard unavailable");
    let id = item.id;
    let mut fixture = fixture(vec![item]);
    fixture.platform.fail_copy.store(true, Ordering::SeqCst);

    let (was_pruned, result) = fixture.service.open_item(id);

    assert!(!was_pruned);
    assert!(result.is_err());
    assert_eq!(fixture.service.entries().len(), 1);
}

#[test]
fn transient_inspection_error_keeps_the_shelf_reference_for_retry() {
    let item = file_item(r"C:\Temp\report.txt");
    let id = item.id;
    let mut fixture = fixture(vec![item.clone()]);
    *fixture.platform.inspect_error.lock().unwrap() = Some(PortError::new(
        "shelf.error.inspectFailed",
        "temporary filesystem error",
    ));

    let (was_pruned, result) = fixture.service.open_item(id);

    assert!(!was_pruned);
    assert!(result.is_err());
    assert_eq!(fixture.service.entries()[0].item.id, id);
    assert_eq!(fixture.repository.items.lock().unwrap().as_slice(), [item]);
    assert_eq!(fixture.repository.save_attempts.load(Ordering::SeqCst), 0);
}

#[test]
fn confirmed_missing_path_is_pruned_and_persisted() {
    let path = PathBuf::from(r"C:\Temp\missing.txt");
    let item = ShelfItem::new(
        path.clone(),
        ShelfPathMetadata {
            display_name: "missing.txt".into(),
            size_bytes: 42,
            is_directory: false,
        },
        Utc::now(),
    );
    let id = item.id;
    let mut fixture = fixture(vec![item]);
    fixture.platform.missing_paths.lock().unwrap().push(path);

    let (was_pruned, _result) = fixture.service.open_item(id);

    assert!(was_pruned);
    assert!(fixture.service.entries().is_empty());
    assert!(fixture.repository.items.lock().unwrap().is_empty());
    assert_eq!(fixture.repository.save_attempts.load(Ordering::SeqCst), 1);
}

#[test]
fn failed_persistence_keeps_confirmed_missing_reference_visible() {
    let path = PathBuf::from(r"C:\Temp\missing.txt");
    let item = ShelfItem::new(
        path.clone(),
        ShelfPathMetadata {
            display_name: "missing.txt".into(),
            size_bytes: 42,
            is_directory: false,
        },
        Utc::now(),
    );
    let id = item.id;
    let mut fixture = fixture(vec![item.clone()]);
    fixture.platform.missing_paths.lock().unwrap().push(path);
    fixture.repository.fail_save.store(true, Ordering::SeqCst);

    let (was_pruned, result) = fixture.service.open_item(id);

    assert!(!was_pruned);
    assert!(result.is_err());
    assert_eq!(fixture.service.entries()[0].item.id, id);
    assert_eq!(fixture.repository.items.lock().unwrap().as_slice(), [item]);
    assert_eq!(fixture.repository.save_attempts.load(Ordering::SeqCst), 1);
}

#[test]
fn startup_transient_inspection_error_keeps_the_persisted_reference() {
    let item = file_item(r"C:\Temp\report.txt");
    let id = item.id;
    let platform = Arc::new(MemoryShelfPlatform::default());
    *platform.inspect_error.lock().unwrap() = Some(PortError::new(
        "shelf.error.inspectFailed",
        "temporary filesystem error",
    ));

    let fixture = fixture_with_platform(vec![item.clone()], platform);

    assert_eq!(fixture.service.entries()[0].item.id, id);
    assert_eq!(fixture.repository.items.lock().unwrap().as_slice(), [item]);
    assert_eq!(fixture.repository.save_attempts.load(Ordering::SeqCst), 0);
}

#[test]
fn startup_prunes_and_persists_only_a_confirmed_missing_reference() {
    let path = PathBuf::from(r"C:\Temp\missing.txt");
    let item = ShelfItem::new(
        path.clone(),
        ShelfPathMetadata {
            display_name: "missing.txt".into(),
            size_bytes: 42,
            is_directory: false,
        },
        Utc::now(),
    );
    let platform = Arc::new(MemoryShelfPlatform::default());
    platform.missing_paths.lock().unwrap().push(path);

    let fixture = fixture_with_platform(vec![item], platform);

    assert!(fixture.service.entries().is_empty());
    assert!(fixture.repository.items.lock().unwrap().is_empty());
    assert_eq!(fixture.repository.save_attempts.load(Ordering::SeqCst), 1);
}
