use std::sync::OnceLock;

use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc};
use windows::Win32::Foundation::SYSTEMTIME;
use windows::Win32::System::Time::{
    DYNAMIC_TIME_ZONE_INFORMATION, EnumDynamicTimeZoneInformation,
    SystemTimeToTzSpecificLocalTimeEx,
};

use super::model::{Rect, UiModel, tr};
use super::scene::Scene;
use crate::domain::calendar::calendar_grid;
use crate::domain::widgets::{CITIES, StatusRing, Widget, moon_phase};

pub const WIDGETS_ICON: &str = "widgets-grid";
const INK: u32 = 0xf5f5f7;
const DIM: u32 = 0xa4a4aa;
const GREEN: u32 = 0x33e07a;
const BLUE: u32 = 0x35c9ff;

pub fn label(widget: Widget) -> &'static str {
    match widget {
        Widget::Week => "widgets-week",
        Widget::Water => "nav-water",
        Widget::Focus => "widgets-focus",
        Widget::WorldClock => "widgets-clock",
        Widget::Battery => "battery",
        Widget::Moon => "widgets-moon",
        Widget::Countdown => "widgets-countdown",
    }
}

fn glyph(widget: Widget) -> &'static str {
    match widget {
        Widget::Week => "\u{e787}",
        Widget::Water => super::views::DROP,
        Widget::Focus => super::views::TIMER,
        Widget::WorldClock => "world-clock",
        Widget::Battery => "\u{e850}",
        Widget::Moon => "moon-phase",
        Widget::Countdown => "countdown",
    }
}

pub fn ring(s: &mut Scene, r: Rect, mode: StatusRing, value: f32, charging: bool) {
    let (color, icon) = match mode {
        StatusRing::Auto if charging => (0x5eff9c, "\u{e945}"),
        StatusRing::Auto => (GREEN, ""),
        StatusRing::Water => (BLUE, super::views::DROP),
        StatusRing::Todo => (GREEN, "\u{e73e}"),
        StatusRing::Off => return,
    };
    let track = if mode == StatusRing::Water {
        0x123347
    } else {
        0x0e3e22
    };
    s.ring_track(r, value, color, (r.w * 0.16).min(2.3), track);
    if !icon.is_empty() {
        s.icon(r.inset(r.w * 0.27), icon, color);
    }
}

pub fn status_ring(s: &mut Scene, r: Rect, m: &UiModel) -> bool {
    let mode = m.settings.desktop.status_ring;
    let value = match mode {
        StatusRing::Auto if m.power.has_battery => f32::from(m.power.percent) / 100.,
        StatusRing::Water => water_progress(m),
        StatusRing::Todo => m.progress_today.done as f32 / m.progress_today.total.max(1) as f32,
        StatusRing::Auto | StatusRing::Off => return false,
    };
    ring(s, r, mode, value, m.power.is_plugged_in);
    true
}

fn water_progress(m: &UiModel) -> f32 {
    m.water.as_ref().map_or(0., |w| {
        (w.consumed_ml as f32 / f32::from(w.goal_ml.max(1))).min(1.)
    })
}

pub fn page(s: &mut Scene, m: &UiModel, body: Rect) {
    let inner = body.w - 84.;
    let width = (inner - 10.) / 2.;
    let prefs = &m.settings.desktop.widgets;
    for (i, widget) in [prefs.left, prefs.right].into_iter().enumerate() {
        let card = Rect::new(body.x + 42. + i as f32 * (width + 10.), 34., width, 92.);
        s.round(card, 13., 0x19191c);
        draw(s, card, widget, m, false);
    }
}

pub fn preview(s: &mut Scene, r: Rect, widget: Widget, m: &UiModel) {
    draw(s, r, widget, m, true);
}

fn draw(s: &mut Scene, r: Rect, widget: Widget, m: &UiModel, small: bool) {
    let padding = if small { 5. } else { 10. };
    let x = r.x + padding;
    let width = r.w - padding * 2.;
    let title_height = if small { 12. } else { 19. };
    let title = Rect::new(x, r.y + padding, width, title_height);
    let y = title.y + title.h;
    let today = m.clock.date_naive();
    let prefs = &m.settings.desktop.widgets;
    if !small {
        s.icon(Rect::new(x, title.y + 3., 13., 13.), glyph(widget), DIM);
    }
    let text_x = if small { x } else { x + 19. };
    s.text(
        Rect::new(text_x, title.y, title.w - (text_x - x), title.h),
        tr(label(widget)),
        if small { 8. } else { 11. },
        DIM,
        true,
        false,
    );

    let metric = |s: &mut Scene, main: String, detail: String| {
        s.text(
            Rect::new(x, y, width, if small { 15. } else { 27. }),
            main,
            if small { 11. } else { 20. },
            INK,
            true,
            false,
        );
        s.text(
            Rect::new(
                x,
                y + if small { 14. } else { 29. },
                width,
                if small { 11. } else { 15. },
            ),
            detail,
            if small { 7.5 } else { 10. },
            DIM,
            false,
            false,
        );
    };
    match widget {
        Widget::Week => {
            if let Some(week) =
                calendar_grid(today, 0, m.settings.desktop.calendar_starts_monday, true)
            {
                let step = width / 7.;
                for (i, day) in week.days.iter().enumerate() {
                    let cell = Rect::new(
                        x + i as f32 * step,
                        y + if small { 3. } else { 6. },
                        step,
                        if small { 12. } else { 20. },
                    );
                    if day.is_today {
                        s.round(cell.inset(1.), 6., 0x205735);
                    }
                    s.text(
                        cell,
                        day.date.day().to_string(),
                        if small { 7. } else { 10. },
                        INK,
                        day.is_today,
                        true,
                    );
                    if !small && m.tasks.iter().any(|task| task.schedule.occurs_on(day.date)) {
                        s.round(
                            Rect::new(cell.x + cell.w / 2. - 1., cell.y + cell.h, 2., 2.),
                            1.,
                            GREEN,
                        );
                    }
                }
                if !small {
                    let pending = m
                        .tasks
                        .iter()
                        .filter(|t| t.schedule.occurs_on(today) && !t.is_done)
                        .count();
                    s.text(
                        Rect::new(x, y + 35., width, 15.),
                        localized_count("widgets-pending", pending as u64),
                        10.,
                        DIM,
                        false,
                        false,
                    );
                }
            }
        }
        Widget::Water => {
            let amount = m.water.as_ref().map_or(0, |w| w.consumed_ml);
            metric(
                s,
                format!("{amount} ml"),
                format!("{} {} ml", tr("widgets-goal"), m.settings.water.goal_ml),
            );
            if !small {
                let bar = Rect::new(x, y + 48., width, 4.);
                s.round(bar, 2., 0x33333a);
                s.round(
                    Rect::new(bar.x, bar.y, bar.w * water_progress(m), bar.h),
                    2.,
                    BLUE,
                );
            }
        }
        Widget::Focus => {
            let (minutes, sessions) = m.focus_today;
            metric(
                s,
                format!("{minutes} min"),
                localized_count("widgets-sessions", u64::from(sessions)),
            );
        }
        Widget::WorldClock => {
            for (i, city) in [prefs.first_city, prefs.second_city]
                .into_iter()
                .enumerate()
            {
                let city = city as usize;
                let height = if small { 12. } else { 24. };
                let line = y + height * i as f32;
                s.text(
                    Rect::new(x, line, width * 0.61, height),
                    CITIES[city].name,
                    if small { 7. } else { 10.5 },
                    DIM,
                    false,
                    false,
                );
                s.text(
                    Rect::new(x + width * 0.60, line, width * 0.40, height),
                    city_time(city, m.clock.to_utc()).unwrap_or_else(|| "—".into()),
                    if small { 8. } else { 13. },
                    INK,
                    true,
                    true,
                );
            }
        }
        Widget::Battery => {
            if m.power.has_battery {
                metric(
                    s,
                    format!("{}%", m.power.percent),
                    tr(if m.power.is_plugged_in {
                        "widgets-charging"
                    } else {
                        "widgets-on-battery"
                    }),
                );
            } else {
                metric(s, "—".into(), tr("widgets-no-battery"));
            }
        }
        Widget::Moon => {
            let (phase, illumination) = moon_phase(m.clock.to_utc());
            let phases = [
                "moon-new",
                "moon-waxing-crescent",
                "moon-first-quarter",
                "moon-waxing-gibbous",
                "moon-full",
                "moon-waning-gibbous",
                "moon-last-quarter",
                "moon-waning-crescent",
            ];
            let symbols = ["○", "◔", "◑", "◕", "●", "◕", "◐", "◔"];
            metric(
                s,
                format!("{} {illumination}%", symbols[phase]),
                tr(phases[phase]),
            );
        }
        Widget::Countdown => {
            if let Ok(date) = NaiveDate::parse_from_str(&prefs.countdown_date, "%Y-%m-%d") {
                let days = (date - today).num_days();
                metric(
                    s,
                    if days > 0 {
                        localized_count("widgets-days", days as u64)
                    } else if days == 0 {
                        tr("widgets-today")
                    } else {
                        tr("widgets-past")
                    },
                    if prefs.countdown_name.is_empty() {
                        date.to_string()
                    } else {
                        prefs.countdown_name.clone()
                    },
                );
            } else {
                metric(s, "—".into(), tr("widgets-set-date"));
            }
        }
    }
}

fn localized_count(key: &str, count: u64) -> String {
    crate::i18n::translate_with(
        &format!("desktopUi.{key}"),
        &[("count", &count.to_string())],
    )
}

fn city_time(city: usize, utc: DateTime<Utc>) -> Option<String> {
    static ZONES: OnceLock<[Option<DYNAMIC_TIME_ZONE_INFORMATION>; CITIES.len()]> = OnceLock::new();
    let zones = ZONES.get_or_init(|| {
        let mut zones = [None; CITIES.len()];
        for index in 0.. {
            let mut zone = DYNAMIC_TIME_ZONE_INFORMATION::default();
            if unsafe { EnumDynamicTimeZoneInformation(index, &mut zone) } != 0 {
                break;
            }
            let end = zone
                .TimeZoneKeyName
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(zone.TimeZoneKeyName.len());
            let key = String::from_utf16_lossy(&zone.TimeZoneKeyName[..end]);
            if let Some(i) = CITIES.iter().position(|city| city.zone == key) {
                zones[i] = Some(zone);
            }
        }
        zones
    });
    let zone = zones[city].as_ref()?;
    let input = SYSTEMTIME {
        wYear: utc.year().try_into().ok()?,
        wMonth: utc.month() as u16,
        wDay: utc.day() as u16,
        wHour: utc.hour() as u16,
        wMinute: utc.minute() as u16,
        ..Default::default()
    };
    let mut local = SYSTEMTIME::default();
    unsafe { SystemTimeToTzSpecificLocalTimeEx(Some(zone), &input, &mut local) }.ok()?;
    Some(format!("{:02}:{:02}", local.wHour, local.wMinute))
}
