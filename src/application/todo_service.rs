use std::sync::Arc;

use chrono::{Local, Utc};
use uuid::Uuid;

use crate::domain::ports::{PortError, TaskRepository};
use crate::domain::todo::{TaskColor, TaskSchedule, TodoError, TodoProgress, TodoTask};

/// Errors of the to-do use cases.
#[derive(Debug, thiserror::Error)]
pub enum TodoServiceError {
    #[error(transparent)]
    Rule(#[from] TodoError),
    #[error(transparent)]
    Storage(#[from] PortError),
}

impl TodoServiceError {
    pub fn message_key(&self) -> &'static str {
        match self {
            Self::Rule(TodoError::EmptyTitle) => "todo.error.emptyTitle",
            Self::Rule(TodoError::NotFound) => "todo.error.notFound",
            Self::Rule(TodoError::InvalidSchedule) => "todo.error.invalidSchedule",
            Self::Rule(TodoError::InvalidColor) => "todo.error.invalidColor",
            Self::Storage(error) => error.key,
        }
    }
}

/// To-do use cases: every mutation is persisted before being exposed.
pub struct TodoService {
    repository: Arc<dyn TaskRepository>,
    tasks: Vec<TodoTask>,
}

impl TodoService {
    pub fn load(repository: Arc<dyn TaskRepository>) -> Result<Self, PortError> {
        let tasks = repository.load()?;
        Ok(Self { repository, tasks })
    }

    pub fn tasks(&self) -> &[TodoTask] {
        &self.tasks
    }

    pub fn progress(&self) -> TodoProgress {
        TodoProgress::of(&self.tasks)
    }

    pub fn add(&mut self, title: &str) -> Result<(), TodoServiceError> {
        let task = TodoTask::new(title, Utc::now())?;
        self.commit(|tasks| {
            tasks.push(task);
            Ok(())
        })
    }

    pub fn rename(
        &mut self,
        id: Uuid,
        title: &str,
        color_index: i32,
    ) -> Result<(), TodoServiceError> {
        let color = TaskColor::from_index(color_index)?;
        self.commit(|tasks| {
            let task = find_task(tasks, id)?;
            task.rename(title)?;
            task.color = color;
            Ok(())
        })
    }

    pub fn add_scheduled(
        &mut self,
        title: &str,
        date: &str,
        time: &str,
        daily: bool,
        color_index: i32,
    ) -> Result<(), TodoServiceError> {
        let mut task = TodoTask::new(title, Utc::now())?;
        task.schedule = TaskSchedule::from_fields(date, time, daily)?;
        task.color = TaskColor::from_index(color_index)?;
        self.commit(|tasks| {
            tasks.push(task);
            Ok(())
        })
    }

    pub fn advance_daily(&mut self, today: chrono::NaiveDate) -> Result<bool, TodoServiceError> {
        let needs_renewal = |task: &TodoTask| {
            task.schedule.daily
                && task.schedule.occurs_on(today)
                && task.is_done
                && task
                    .completed_at
                    .is_some_and(|completed| completed.with_timezone(&Local).date_naive() < today)
        };
        if !self.tasks.iter().any(needs_renewal) {
            return Ok(false);
        }
        self.commit(|tasks| {
            for task in tasks.iter_mut().filter(|task| needs_renewal(task)) {
                task.is_done = false;
                task.completed_at = None;
            }
            Ok(())
        })?;
        Ok(true)
    }

    pub fn toggle(&mut self, id: Uuid) -> Result<(), TodoServiceError> {
        self.commit(|tasks| {
            find_task(tasks, id)?.toggle(Utc::now());
            Ok(())
        })
    }

    pub fn delete(&mut self, id: Uuid) -> Result<(), TodoServiceError> {
        self.commit(|tasks| {
            let index = tasks
                .iter()
                .position(|task| task.id == id)
                .ok_or(TodoError::NotFound)?;
            tasks.remove(index);
            Ok(())
        })
    }

    /// Applies a mutation on a copy and only keeps it once it is saved, so
    /// memory and disk never diverge.
    fn commit(
        &mut self,
        mutation: impl FnOnce(&mut Vec<TodoTask>) -> Result<(), TodoError>,
    ) -> Result<(), TodoServiceError> {
        let mut next = self.tasks.clone();
        mutation(&mut next)?;
        self.repository.save(&next)?;
        self.tasks = next;
        Ok(())
    }
}

fn find_task(tasks: &mut [TodoTask], id: Uuid) -> Result<&mut TodoTask, TodoError> {
    tasks
        .iter_mut()
        .find(|task| task.id == id)
        .ok_or(TodoError::NotFound)
}

#[cfg(test)]
#[path = "../../tests/fixtures/todo_service.rs"]
mod tests;
