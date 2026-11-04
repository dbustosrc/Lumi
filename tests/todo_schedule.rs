#[allow(dead_code)]
#[path = "../src/domain/todo.rs"]
mod todo;

use chrono::{Local, NaiveDate, NaiveTime, TimeZone, Utc};
use todo::{TaskColor, TaskSchedule, TodoError, TodoTask};

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

#[test]
fn one_time_schedule_occurs_only_on_its_selected_date() {
    let schedule = TaskSchedule {
        date: date(2025, 3, 5),
        time: Some(NaiveTime::from_hms_opt(9, 30, 0).unwrap()),
        daily: false,
    };

    assert!(schedule.occurs_on(date(2025, 3, 5)));
    assert!(!schedule.occurs_on(date(2025, 3, 4)));
    assert!(!schedule.occurs_on(date(2025, 3, 6)));
}

#[test]
fn daily_schedule_occurs_on_other_dates_and_retains_its_time() {
    let time = NaiveTime::from_hms_opt(8, 15, 0).unwrap();
    let schedule = TaskSchedule {
        date: date(2025, 3, 5),
        time: Some(time),
        daily: true,
    };

    assert!(schedule.occurs_on(date(2025, 3, 5)));
    assert!(schedule.occurs_on(date(2025, 3, 6)));
    assert!(schedule.occurs_on(date(2026, 1, 1)));
    assert!(!schedule.occurs_on(date(2025, 3, 4)));
    assert_eq!(schedule.time, Some(time));
}

#[test]
fn schedule_fields_parse_a_date_optional_time_and_daily_flag() {
    let timed = TaskSchedule::from_fields("2025-03-05", "09:30", true).unwrap();
    let untimed = TaskSchedule::from_fields("2025-03-05", "  ", false).unwrap();

    assert_eq!(timed.date, date(2025, 3, 5));
    assert_eq!(timed.time, Some(NaiveTime::from_hms_opt(9, 30, 0).unwrap()));
    assert!(timed.daily);
    assert_eq!(untimed.date, date(2025, 3, 5));
    assert_eq!(untimed.time, None);
    assert!(!untimed.daily);
}

#[test]
fn schedule_fields_reject_invalid_dates_and_times() {
    assert_eq!(
        TaskSchedule::from_fields("2025-02-30", "09:30", false).unwrap_err(),
        TodoError::InvalidSchedule
    );
    assert_eq!(
        TaskSchedule::from_fields("2025-03-05", "25:00", false).unwrap_err(),
        TodoError::InvalidSchedule
    );
}

#[test]
fn task_colors_round_trip_all_supported_indices_and_reject_unknown_values() {
    for index in 0..=4 {
        assert_eq!(TaskColor::from_index(index).unwrap().index(), index);
    }
    assert_eq!(TaskColor::from_index(-1), Err(TodoError::InvalidColor));
    assert_eq!(TaskColor::from_index(5), Err(TodoError::InvalidColor));
}

#[test]
fn new_task_defaults_to_its_local_creation_date_without_time_or_repetition() {
    let now = Utc.with_ymd_and_hms(2025, 3, 5, 2, 7, 0).unwrap();
    let expected_local_date = now.with_timezone(&Local).date_naive();

    let task = TodoTask::new("Read a book", now).unwrap();

    assert_eq!(task.schedule.date, expected_local_date);
    assert_eq!(task.schedule.time, None);
    assert!(!task.schedule.daily);
    assert_eq!(task.color, TaskColor::Green);
}
