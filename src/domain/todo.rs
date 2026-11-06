use chrono::{DateTime, Local, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Errors raised by to-do business rules.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TodoError {
    #[error("todo.error.emptyTitle")]
    EmptyTitle,
    #[error("todo.error.notFound")]
    NotFound,
    #[error("todo.error.invalidSchedule")]
    InvalidSchedule,
    #[error("todo.error.invalidColor")]
    InvalidColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskColor {
    Green,
    Blue,
    Orange,
    Red,
    Purple,
}

impl TaskColor {
    pub fn from_index(index: i32) -> Result<Self, TodoError> {
        match index {
            0 => Ok(Self::Green),
            1 => Ok(Self::Blue),
            2 => Ok(Self::Orange),
            3 => Ok(Self::Red),
            4 => Ok(Self::Purple),
            _ => Err(TodoError::InvalidColor),
        }
    }
    pub fn index(self) -> i32 {
        self as i32
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSchedule {
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
    pub daily: bool,
}

impl TaskSchedule {
    pub fn from_fields(date: &str, time: &str, daily: bool) -> Result<Self, TodoError> {
        Ok(Self {
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|_| TodoError::InvalidSchedule)?,
            time: if time.trim().is_empty() {
                None
            } else {
                Some(
                    NaiveTime::parse_from_str(time, "%H:%M")
                        .map_err(|_| TodoError::InvalidSchedule)?,
                )
            },
            daily,
        })
    }

    pub fn occurs_on(&self, date: NaiveDate) -> bool {
        self.date == date || (self.daily && date >= self.date)
    }
}

/// A to-do item owned by the local user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TodoTask {
    pub id: Uuid,
    pub title: String,
    pub is_done: bool,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub schedule: TaskSchedule,
    pub color: TaskColor,
}

impl TodoTask {
    pub fn new(title: &str, now: DateTime<Utc>) -> Result<Self, TodoError> {
        Ok(Self {
            id: Uuid::new_v4(),
            title: normalize_title(title)?,
            is_done: false,
            created_at: now,
            completed_at: None,
            schedule: TaskSchedule {
                date: now.with_timezone(&Local).date_naive(),
                time: None,
                daily: false,
            },
            color: TaskColor::Green,
        })
    }

    pub fn rename(&mut self, title: &str) -> Result<(), TodoError> {
        self.title = normalize_title(title)?;
        Ok(())
    }

    pub fn toggle(&mut self, now: DateTime<Utc>) {
        self.set_done(!self.is_done, now);
    }

    pub fn set_done(&mut self, done: bool, now: DateTime<Utc>) {
        if self.is_done != done {
            self.is_done = done;
            self.completed_at = done.then_some(now);
        }
    }
}

fn normalize_title(title: &str) -> Result<String, TodoError> {
    let collapsed = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return Err(TodoError::EmptyTitle);
    }
    Ok(collapsed)
}

/// Progress shown as "done/total" in the to-do page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TodoProgress {
    pub done: usize,
    pub total: usize,
}

impl TodoProgress {
    pub fn of(tasks: &[TodoTask]) -> Self {
        Self {
            done: tasks.iter().filter(|task| task.is_done).count(),
            total: tasks.len(),
        }
    }
}
