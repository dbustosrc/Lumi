//! Bilingual (English / Spanish) internationalization catalog with semantic
//! dot-separated keys and `{placeholder}` variable interpolation.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use chrono::{DateTime, Datelike, Local, Weekday};

use crate::application::settings::LanguagePreference;

const EN_SOURCE: &str = include_str!("../locales/en.toml");
const ES_SOURCE: &str = include_str!("../locales/es.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Locale {
    English = 0,
    Spanish = 1,
}

struct Catalogs {
    english: HashMap<String, String>,
    spanish: HashMap<String, String>,
}

static CATALOGS: OnceLock<Catalogs> = OnceLock::new();
static ACTIVE_LOCALE: AtomicU8 = AtomicU8::new(Locale::English as u8);

fn get_catalogs() -> &'static Catalogs {
    CATALOGS.get_or_init(|| Catalogs {
        english: parse_catalog(EN_SOURCE),
        spanish: parse_catalog(ES_SOURCE),
    })
}

fn parse_catalog(source: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(value) = toml::from_str::<toml::Value>(source) {
        flatten_table("", &value, &mut map);
    }
    map
}

fn flatten_table(prefix: &str, value: &toml::Value, output: &mut HashMap<String, String>) {
    match value {
        toml::Value::Table(table) => {
            for (key, child) in table {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_table(&path, child, output);
            }
        }
        toml::Value::String(text) => {
            output.insert(prefix.to_owned(), text.clone());
        }
        _ => {}
    }
}

/// Resolves and activates the requested language preference.
pub fn select_language(preference: LanguagePreference, system_locale: Option<&str>) -> Locale {
    let locale = match preference {
        LanguagePreference::English => Locale::English,
        LanguagePreference::Spanish => Locale::Spanish,
        LanguagePreference::System => resolve_system_locale(system_locale),
    };
    ACTIVE_LOCALE.store(locale as u8, Ordering::Relaxed);
    locale
}

fn resolve_system_locale(system_locale: Option<&str>) -> Locale {
    let is_spanish = system_locale.is_some_and(|code| code.to_ascii_lowercase().starts_with("es"));
    if is_spanish {
        Locale::Spanish
    } else {
        Locale::English
    }
}

pub fn active_locale() -> Locale {
    match ACTIVE_LOCALE.load(Ordering::Relaxed) {
        1 => Locale::Spanish,
        _ => Locale::English,
    }
}

/// Translates a semantic key in the currently active locale.
pub fn translate(key: &str) -> String {
    translate_for(active_locale(), key)
}

/// Translates a semantic key for an explicit locale.
pub fn translate_for(locale: Locale, key: &str) -> String {
    let catalogs = get_catalogs();
    let table = match locale {
        Locale::English => &catalogs.english,
        Locale::Spanish => &catalogs.spanish,
    };
    table.get(key).cloned().unwrap_or_else(|| key.to_owned())
}

/// Translates a semantic key and interpolates `{name}` placeholders.
pub fn translate_with(key: &str, params: &[(&str, &str)]) -> String {
    let mut text = translate(key);
    for (name, value) in params {
        let token = format!("{{{name}}}");
        text = text.replace(&token, value);
    }
    text
}

/// Formats the header date as `Thu 15 Oct 2026` in the active language.
pub fn format_header_date(now: DateTime<Local>) -> String {
    let weekday_key = match now.weekday() {
        Weekday::Mon => "calendar.weekdays.mon",
        Weekday::Tue => "calendar.weekdays.tue",
        Weekday::Wed => "calendar.weekdays.wed",
        Weekday::Thu => "calendar.weekdays.thu",
        Weekday::Fri => "calendar.weekdays.fri",
        Weekday::Sat => "calendar.weekdays.sat",
        Weekday::Sun => "calendar.weekdays.sun",
    };
    let month_keys = [
        "calendar.months.jan",
        "calendar.months.feb",
        "calendar.months.mar",
        "calendar.months.apr",
        "calendar.months.may",
        "calendar.months.jun",
        "calendar.months.jul",
        "calendar.months.aug",
        "calendar.months.sep",
        "calendar.months.oct",
        "calendar.months.nov",
        "calendar.months.dec",
    ];
    let month_index = (now.month0() as usize).min(11);
    let weekday = translate(weekday_key);
    let month = translate(month_keys[month_index]);
    format!("{weekday} {:02} {month} {}", now.day(), now.year())
}
