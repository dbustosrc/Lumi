use super::JsonTaskRepository;

use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::ports::TaskRepository;
use crate::domain::todo::{TaskColor, TaskSchedule, TodoTask};
use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
use uuid::Uuid;

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("lumi-tasks-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn corrupt_task_json_reports_error_and_preserves_original_bytes_for_retry() {
    let directory = TempDirectory::new();
    let path = directory.path().join("tasks.json");
    let corrupt = br#"[{"id":"not-a-uuid","title":"keep evidence"}]"#;
    fs::write(&path, corrupt).unwrap();
    let repository = JsonTaskRepository::new(path.clone());

    let error = repository.load().unwrap_err();
    assert_eq!(error.key, "todo.error.storageCorrupt");
    assert_eq!(fs::read(&path).unwrap(), corrupt);

    let retry_error = repository.load().unwrap_err();
    assert_eq!(retry_error.key, "todo.error.storageCorrupt");
    assert_eq!(fs::read(&path).unwrap(), corrupt);

    let remaining_files: Vec<_> = fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(remaining_files, vec![String::from("tasks.json")]);
}

#[test]
fn unsupported_task_schema_is_rejected_without_rewriting_the_document() {
    let directory = TempDirectory::new();
    let path = directory.path().join("tasks.json");
    let unsupported = br#"{"schema_version":2,"tasks":[]}"#;
    fs::write(&path, unsupported).unwrap();
    let repository = JsonTaskRepository::new(path.clone());

    let error = repository.load().unwrap_err();

    assert_eq!(error.key, "todo.error.storageCorrupt");
    assert_eq!(fs::read(path).unwrap(), unsupported);
}

#[test]
fn current_task_document_rejects_records_without_a_schedule() {
    let directory = TempDirectory::new();
    let path = directory.path().join("tasks.json");
    let missing_schedule = br#"{
        "schema_version":1,
        "tasks":[{
            "id":"00000000-0000-0000-0000-000000000123",
            "title":"Missing schedule",
            "is_done":false,
            "created_at":"2025-03-05T02:07:00Z",
            "completed_at":null
        }]
    }"#;
    fs::write(&path, missing_schedule).unwrap();
    let repository = JsonTaskRepository::new(path.clone());

    let error = repository.load().unwrap_err();

    assert_eq!(error.key, "todo.error.storageCorrupt");
    assert_eq!(fs::read(path).unwrap(), missing_schedule);
}

#[test]
fn task_document_round_trip_preserves_schedule() {
    let directory = TempDirectory::new();
    let path = directory.path().join("tasks.json");
    let repository = JsonTaskRepository::new(path.clone());
    let task = TodoTask {
        id: Uuid::from_u128(1),
        title: "Water plants".into(),
        is_done: false,
        created_at: Utc.with_ymd_and_hms(2025, 3, 1, 12, 0, 0).unwrap(),
        completed_at: None,
        schedule: TaskSchedule {
            date: NaiveDate::from_ymd_opt(2025, 3, 5).unwrap(),
            time: Some(NaiveTime::from_hms_opt(9, 30, 0).unwrap()),
            daily: true,
        },
        color: TaskColor::Blue,
    };

    repository.save(std::slice::from_ref(&task)).unwrap();

    let stored: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(stored["schema_version"], 1);
    assert_eq!(stored["tasks"][0]["schedule"]["date"], "2025-03-05");
    assert_eq!(stored["tasks"][0]["schedule"]["time"], "09:30:00");
    assert_eq!(stored["tasks"][0]["schedule"]["daily"], true);
    assert_eq!(stored["tasks"][0]["color"], "blue");
    assert_eq!(repository.load().unwrap(), vec![task]);
}
