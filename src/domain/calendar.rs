use chrono::{Datelike, Duration, NaiveDate};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarDay {
    pub date: NaiveDate,
    pub is_today: bool,
    pub is_current_month: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarGrid {
    pub year: i32,
    pub month: u32,
    pub monday_first: bool,
    pub week_view: bool,
    pub days: Vec<CalendarDay>,
}

pub fn calendar_grid(
    today: NaiveDate,
    offset: i32,
    monday_first: bool,
    week: bool,
) -> Option<CalendarGrid> {
    let anchor = if week {
        today.checked_add_signed(Duration::weeks(i64::from(offset)))?
    } else {
        let month = today
            .year()
            .checked_mul(12)?
            .checked_add(today.month0() as i32)?
            .checked_add(offset)?;
        NaiveDate::from_ymd_opt(month.div_euclid(12), month.rem_euclid(12) as u32 + 1, 1)?
    };
    let first = if week { anchor } else { anchor.with_day(1)? };
    let weekday = if monday_first {
        first.weekday().num_days_from_monday()
    } else {
        first.weekday().num_days_from_sunday()
    };
    let first = first.checked_sub_signed(Duration::days(i64::from(weekday)))?;
    let mut days = Vec::new();
    for index in 0..if week { 7 } else { 42 } {
        let date = first.checked_add_signed(Duration::days(index))?;
        days.push(CalendarDay {
            date,
            is_today: date == today,
            is_current_month: date.month() == anchor.month() && date.year() == anchor.year(),
        });
    }
    Some(CalendarGrid {
        year: anchor.year(),
        month: anchor.month(),
        monday_first,
        week_view: week,
        days,
    })
}
