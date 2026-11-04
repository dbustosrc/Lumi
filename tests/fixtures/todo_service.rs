use super::{TodoService, TodoServiceError};

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::domain::ports::{PortError, TaskRepository};
use crate::domain::todo::{TaskColor, TaskSchedule, TodoError, TodoTask};
use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};

#[derive(Default)]
struct MemoryTaskRepository {
    tasks: Mutex<Vec<TodoTask>>,
    save_count: AtomicUsize,
    fail_saves: AtomicBool,
}

impl TaskRepository for MemoryTaskRepository {
    fn load(&self) -> Result<Vec<TodoTask>, PortError> {
        Ok(self.tasks.lock().unwrap().clone())
    }

    fn save(&self, tasks: &[TodoTask]) -> Result<(), PortError> {
        self.save_count.fetch_add(1, Ordering::Relaxed);
        if self.fail_saves.load(Ordering::Relaxed) {
            return Err(PortError::new("todo.error.storageWrite", "test failure"));
        }
        *self.tasks.lock().unwrap() = tasks.to_vec();
        Ok(())
    }
}

fn task(id: u128, date: NaiveDate, time: Option<NaiveTime>, daily: bool) -> TodoTask {
    TodoTask {
        id: uuid::Uuid::from_u128(id),
        title: format!("Task {id}"),
        is_done: true,
        created_at: Utc.with_ymd_and_hms(2025, 1, 1, 12, 0, 0).unwrap(),
        completed_at: Some(Utc.with_ymd_and_hms(2025, 1, 2, 12, 0, 0).unwrap()),
        schedule: TaskSchedule { date, time, daily },
        color: TaskColor::Purple,
    }
}

fn service_with(tasks: Vec<TodoTask>) -> (TodoService, Arc<MemoryTaskRepository>) {
    let repository = Arc::new(MemoryTaskRepository {
        tasks: Mutex::new(tasks),
        ..MemoryTaskRepository::default()
    });
    let service = TodoService::load(repository.clone()).unwrap();
    (service, repository)
}

#[test]
fn scheduled_add_and_rename_persist_the_selected_color_and_schedule() {
    let (mut service, repository) = service_with(Vec::new());

    service
        .add_scheduled("Water plants", "2025-03-05", "09:30", true, 1)
        .unwrap();
    let created = service.tasks()[0].clone();
    assert_eq!(created.color, TaskColor::Blue);
    assert_eq!(created.schedule.date, date(2025, 3, 5));
    assert_eq!(created.schedule.time, Some(time(9, 30)));
    assert!(created.schedule.daily);

    service.rename(created.id, "Water herbs", 4).unwrap();
    assert_eq!(service.tasks()[0].title, "Water herbs");
    assert_eq!(service.tasks()[0].color, TaskColor::Purple);
    assert_eq!(repository.save_count.load(Ordering::Relaxed), 2);
    assert_eq!(repository.tasks.lock().unwrap().as_slice(), service.tasks());
}

#[test]
fn invalid_color_rejects_add_and_rename_without_saving_or_mutating_tasks() {
    let existing = task(1, date(2025, 3, 5), None, false);
    let (mut service, repository) = service_with(vec![existing.clone()]);

    assert!(matches!(
        service.add_scheduled("New task", "2025-03-06", "", false, 5),
        Err(TodoServiceError::Rule(TodoError::InvalidColor))
    ));
    assert!(matches!(
        service.rename(existing.id, "Renamed", -1),
        Err(TodoServiceError::Rule(TodoError::InvalidColor))
    ));

    assert_eq!(service.tasks(), &[existing]);
    assert_eq!(repository.save_count.load(Ordering::Relaxed), 0);
}

#[test]
fn advance_daily_renews_overdue_tasks_once_and_preserves_identity_and_schedule_fields() {
    let mut due = task(1, date(2025, 3, 4), Some(time(8, 15)), true);
    due.completed_at = Some(Utc.with_ymd_and_hms(2025, 3, 4, 12, 0, 0).unwrap());
    let due_id = due.id;
    let due_title = due.title.clone();
    let due_color = due.color;
    let due_schedule_date = due.schedule.date;
    let mut today_daily = task(2, date(2025, 3, 5), Some(time(9, 30)), true);
    today_daily.completed_at = Some(Utc.with_ymd_and_hms(2025, 3, 5, 12, 0, 0).unwrap());
    let one_time = task(3, date(2025, 3, 4), None, false);
    let (mut service, repository) = service_with(vec![due, today_daily.clone(), one_time.clone()]);

    assert!(service.advance_daily(date(2025, 3, 5)).unwrap());

    let advanced = &service.tasks()[0];
    assert_eq!(advanced.id, due_id);
    assert_eq!(advanced.title, due_title);
    assert_eq!(advanced.color, due_color);
    assert_eq!(advanced.schedule.date, due_schedule_date);
    assert_eq!(advanced.schedule.time, Some(time(8, 15)));
    assert!(advanced.schedule.daily);
    assert!(!advanced.is_done);
    assert_eq!(advanced.completed_at, None);
    assert_eq!(service.tasks()[1], today_daily);
    assert_eq!(service.tasks()[2], one_time);
    assert_eq!(repository.save_count.load(Ordering::Relaxed), 1);

    assert!(!service.advance_daily(date(2025, 3, 5)).unwrap());
    assert_eq!(service.tasks().len(), 3);
    assert_eq!(repository.save_count.load(Ordering::Relaxed), 1);
}

#[test]
fn failed_daily_advance_preserves_in_memory_tasks() {
    let overdue = task(1, date(2025, 3, 4), Some(time(8, 15)), true);
    let (mut service, repository) = service_with(vec![overdue]);
    let before = service.tasks().to_vec();
    repository.fail_saves.store(true, Ordering::Relaxed);

    let error = service.advance_daily(date(2025, 3, 5)).unwrap_err();

    assert!(matches!(error, TodoServiceError::Storage(_)));
    assert_eq!(service.tasks(), before);
    assert_eq!(repository.save_count.load(Ordering::Relaxed), 1);
}

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

fn time(hour: u32, minute: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(hour, minute, 0).unwrap()
}
