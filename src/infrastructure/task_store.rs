use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::ports::{PortError, TaskRepository};
use crate::domain::todo::{TaskColor, TaskSchedule, TodoTask};

/// Persistence format of a task, kept apart from the domain entity.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskRecord {
    id: Uuid,
    title: String,
    is_done: bool,
    created_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    schedule: TaskSchedule,
    color: TaskColor,
}

impl From<&TodoTask> for TaskRecord {
    fn from(task: &TodoTask) -> Self {
        Self {
            id: task.id,
            title: task.title.clone(),
            is_done: task.is_done,
            created_at: task.created_at,
            completed_at: task.completed_at,
            schedule: task.schedule.clone(),
            color: task.color,
        }
    }
}

impl From<TaskRecord> for TodoTask {
    fn from(record: TaskRecord) -> Self {
        Self {
            id: record.id,
            title: record.title,
            is_done: record.is_done,
            created_at: record.created_at,
            completed_at: record.completed_at,
            schedule: record.schedule,
            color: record.color,
        }
    }
}

pub const TASK_SCHEMA_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskDocument {
    pub schema_version: u32,
    pub tasks: Vec<TaskRecord>,
}

impl TaskDocument {
    pub fn validate(&self) -> Result<(), PortError> {
        if self.schema_version != TASK_SCHEMA_VERSION {
            return Err(PortError::new(
                "todo.error.storageCorrupt",
                "unsupported task schema",
            ));
        }
        Ok(())
    }
}

/// Stores the to-do list as JSON in the user data directory.
pub struct JsonTaskRepository {
    path: PathBuf,
}

impl JsonTaskRepository {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl TaskRepository for JsonTaskRepository {
    fn load(&self) -> Result<Vec<TodoTask>, PortError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read(&self.path)
            .map_err(|error| PortError::new("todo.error.storageRead", error))?;
        let document: TaskDocument = serde_json::from_slice(&content)
            .map_err(|error| PortError::new("todo.error.storageCorrupt", error))?;
        document.validate()?;
        Ok(document.tasks.into_iter().map(TodoTask::from).collect())
    }

    fn save(&self, tasks: &[TodoTask]) -> Result<(), PortError> {
        let document = TaskDocument {
            schema_version: TASK_SCHEMA_VERSION,
            tasks: tasks.iter().map(TaskRecord::from).collect(),
        };
        let content = serde_json::to_vec_pretty(&document)
            .map_err(|error| PortError::new("todo.error.storageSerialize", error))?;
        write_atomically(&self.path, &content)
            .map_err(|error| PortError::new("todo.error.storageWrite", error))
    }
}

#[cfg(test)]
#[path = "../../tests/fixtures/task_store.rs"]
mod tests;

/// Writes to a temporary sibling and renames it, so a crash never leaves a
/// half-written file behind.
pub fn write_atomically(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension("tmp");
    let mut file = std::fs::File::create(&temporary)?;
    file.write_all(content)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temporary, path)
}
