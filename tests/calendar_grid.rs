#[path = "../src/domain/calendar.rs"]
mod calendar;

use calendar::calendar_grid;
use chrono::{Datelike, NaiveDate, Weekday};

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

#[test]
fn month_grid_has_six_rows_and_marks_leap_month_days() {
    let grid = calendar_grid(date(2024, 2, 29), 0, true, false).unwrap();

    assert_eq!((grid.year, grid.month), (2024, 2));
    assert!(!grid.week_view);
    assert_eq!(grid.days.len(), 42);
    assert_eq!(
        grid.days.iter().filter(|day| day.is_current_month).count(),
        29
    );
    assert_eq!(grid.days.iter().filter(|day| day.is_today).count(), 1);
    assert_eq!(
        grid.days.iter().find(|day| day.is_today).unwrap().date,
        date(2024, 2, 29)
    );
    assert_eq!(grid.days[0].date, date(2024, 1, 29));
    assert_eq!(grid.days[0].date.weekday(), Weekday::Mon);
}

#[test]
fn week_start_preference_changes_the_month_grid_leading_day() {
    let monday_first = calendar_grid(date(2024, 2, 15), 0, true, false).unwrap();
    let sunday_first = calendar_grid(date(2024, 2, 15), 0, false, false).unwrap();

    assert_eq!(monday_first.days[0].date, date(2024, 1, 29));
    assert_eq!(monday_first.days[0].date.weekday(), Weekday::Mon);
    assert_eq!(sunday_first.days[0].date, date(2024, 1, 28));
    assert_eq!(sunday_first.days[0].date.weekday(), Weekday::Sun);
    assert!(monday_first.monday_first);
    assert!(!sunday_first.monday_first);
    assert!(!monday_first.week_view);
    assert!(!sunday_first.week_view);
}

#[test]
fn month_offset_crosses_year_boundaries() {
    let next_month = calendar_grid(date(2024, 12, 31), 1, true, false).unwrap();
    let previous_month = calendar_grid(date(2025, 1, 1), -1, true, false).unwrap();

    assert_eq!((next_month.year, next_month.month), (2025, 1));
    assert_eq!((previous_month.year, previous_month.month), (2024, 12));
    assert!(!next_month.week_view);
    assert!(!previous_month.week_view);
    assert!(
        next_month.days.iter().all(|day| {
            day.is_current_month == (day.date.year() == 2025 && day.date.month() == 1)
        })
    );
}

#[test]
fn week_grid_has_seven_days_and_offsets_by_whole_weeks() {
    let today = date(2025, 3, 5);
    let current = calendar_grid(today, 0, true, true).unwrap();
    let next = calendar_grid(today, 1, true, true).unwrap();

    assert!(current.week_view);
    assert!(next.week_view);
    assert_eq!(current.days.len(), 7);
    assert_eq!(current.days[0].date, date(2025, 3, 3));
    assert_eq!(current.days[6].date, date(2025, 3, 9));
    assert_eq!(current.days.iter().filter(|day| day.is_today).count(), 1);
    assert_eq!(
        current.days.iter().find(|day| day.is_today).unwrap().date,
        today
    );
    assert_eq!(next.days[0].date, date(2025, 3, 10));
}

#[test]
fn week_grid_marks_days_outside_the_anchor_month() {
    let grid = calendar_grid(date(2025, 3, 31), 0, true, true).unwrap();

    assert!(grid.week_view);
    assert_eq!(grid.days[0].date, date(2025, 3, 31));
    assert!(grid.days[..1].iter().all(|day| day.is_current_month));
    assert!(grid.days[1..].iter().all(|day| !day.is_current_month));
    assert_eq!(grid.days[1].date, date(2025, 4, 1));
}
