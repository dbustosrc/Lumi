use super::*;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::domain::ports::PortError;
use crate::infrastructure::settings_store::TomlSettingsRepository;

struct MemoryRepository {
    stored: Arc<Mutex<AppSettings>>,
    fail_save: bool,
    calls: Arc<Mutex<Vec<&'static str>>>,
}

impl SettingsRepository for MemoryRepository {
    fn load(&self) -> Result<AppSettings, PortError> {
        self.calls.lock().unwrap().push("load");
        Ok(self.stored.lock().unwrap().clone())
    }

    fn save(&self, settings: &AppSettings) -> Result<(), PortError> {
        self.calls.lock().unwrap().push("save");
        if self.fail_save {
            return Err(PortError::new("settings.error.write", "mock save failure"));
        }
        *self.stored.lock().unwrap() = settings.clone();
        Ok(())
    }
}

struct MemoryStartupRegistration {
    enabled_calls: Arc<Mutex<Vec<bool>>>,
    calls: Arc<Mutex<Vec<&'static str>>>,
    fail_for: Option<bool>,
}

impl StartupRegistration for MemoryStartupRegistration {
    fn set_enabled(&self, enabled: bool) -> Result<(), PortError> {
        self.calls.lock().unwrap().push("startup");
        self.enabled_calls.lock().unwrap().push(enabled);
        if self.fail_for == Some(enabled) {
            return Err(PortError::new(
                "settings.error.write",
                "mock startup failure",
            ));
        }
        Ok(())
    }
}

struct Setup {
    service: SettingsService,
    stored: Arc<Mutex<AppSettings>>,
    enabled_calls: Arc<Mutex<Vec<bool>>>,
    calls: Arc<Mutex<Vec<&'static str>>>,
}

fn setup(initial: AppSettings, fail_save: bool, fail_startup_for: Option<bool>) -> Setup {
    let stored = Arc::new(Mutex::new(initial.clone()));
    let enabled_calls = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let repository = MemoryRepository {
        stored: Arc::clone(&stored),
        fail_save,
        calls: Arc::clone(&calls),
    };
    let startup = MemoryStartupRegistration {
        enabled_calls: Arc::clone(&enabled_calls),
        calls: Arc::clone(&calls),
        fail_for: fail_startup_for,
    };
    let service = SettingsService::new(Box::new(repository), Box::new(startup)).unwrap();
    Setup {
        service,
        stored,
        enabled_calls,
        calls,
    }
}

#[test]
fn defaults_use_current_schema_with_mouse_move_off_and_validate() {
    let defaults = AppSettings::default();
    assert_eq!(defaults.schema_version, 4);
    assert!(!defaults.is_mouse_move_enabled);
    assert!(defaults.validate().is_ok());
}

#[test]
fn current_schema_requires_the_mouse_move_setting_instead_of_falling_back() {
    let content = toml::to_string_pretty(&AppSettings::default()).unwrap();
    let missing = content.replace("is_mouse_move_enabled = false\n", "");
    assert_ne!(missing, content, "fixture must remove the schema 4 setting");

    let path = std::env::temp_dir().join(format!(
        "lumi-settings-missing-mouse-{}.toml",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(&path, missing).unwrap();
    let result = TomlSettingsRepository::new(path.clone()).load();
    let _ = std::fs::remove_file(path);

    assert_eq!(result.unwrap_err().key, "settings.error.parse");
}

#[test]
fn validation_accepts_supported_display_scales_and_rejects_invalid_values() {
    for scale in [0, 100, 115, 130, 150] {
        let mut settings = AppSettings::default();
        settings.desktop.scale_percent = scale;
        assert!(
            settings.validate().is_ok(),
            "scale {scale} should be accepted"
        );
    }

    let invalid = AppSettings {
        schema_version: 2,
        ..AppSettings::default()
    };
    assert!(invalid.validate().is_err());

    let mut invalid = AppSettings::default();
    invalid.desktop.scale_percent = 125;
    assert!(invalid.validate().is_err());

    let invalid = AppSettings {
        rest_after_secs: 0,
        ..AppSettings::default()
    };
    assert!(invalid.validate().is_err());

    let mut invalid = AppSettings::default();
    invalid.wyoming.port = 0;
    assert!(invalid.validate().is_err());
}

#[test]
fn validation_rejects_zero_reminder_durations_cycles_and_goals() {
    let zero_fields: [fn(&mut ReminderSettings); 6] = [
        |reminders| reminders.snooze_minutes = 0,
        |reminders| reminders.task_minutes = 0,
        |reminders| reminders.focus_minutes = 0,
        |reminders| reminders.short_break_minutes = 0,
        |reminders| reminders.long_break_minutes = 0,
        |reminders| reminders.focus_cycles = 0,
    ];

    for zero_field in zero_fields {
        let mut settings = AppSettings::default();
        zero_field(&mut settings.reminders);
        assert!(settings.validate().is_err());
    }

    let mut settings = AppSettings::default();
    settings.water.goal_ml = 0;
    assert!(settings.validate().is_err());

    let mut settings = AppSettings::default();
    settings.water.bottle_ml = 0;
    assert!(settings.validate().is_err());

    let mut settings = AppSettings::default();
    settings.water.active_until_minute = settings.water.active_from_minute;
    assert!(settings.validate().is_err());

    let mut settings = AppSettings::default();
    settings.water.active_until_minute = 1441;
    assert!(settings.validate().is_err());
}

#[test]
fn startup_registration_runs_at_construction_only_when_enabled() {
    let disabled = setup(AppSettings::default(), false, None);
    assert!(disabled.enabled_calls.lock().unwrap().is_empty());

    let mut initial = AppSettings::default();
    initial.desktop.start_with_windows = true;
    let enabled = setup(initial, false, None);
    assert_eq!(*enabled.enabled_calls.lock().unwrap(), [true]);
    assert_eq!(*enabled.calls.lock().unwrap(), ["load", "startup"]);
}

#[test]
fn enabling_startup_and_mouse_move_persists_before_broadcasting_the_update() {
    let setup = setup(AppSettings::default(), false, None);
    let mut updates = setup.service.subscribe();
    let mut next = setup.service.current();
    next.desktop.start_with_windows = true;
    next.is_mouse_move_enabled = true;

    setup.service.update(next.clone()).unwrap();

    assert_eq!(*setup.calls.lock().unwrap(), ["load", "startup", "save"]);
    assert_eq!(*setup.enabled_calls.lock().unwrap(), [true]);
    assert_eq!(*setup.stored.lock().unwrap(), next);
    assert!(matches!(updates.has_changed(), Ok(true)));
    assert_eq!(*updates.borrow_and_update(), next);
}

#[test]
fn failed_save_rolls_back_startup_and_keeps_the_published_settings_unchanged() {
    let setup = setup(AppSettings::default(), true, None);
    let updates = setup.service.subscribe();
    let initial = setup.service.current();
    let mut next = initial.clone();
    next.desktop.start_with_windows = true;

    assert!(setup.service.update(next).is_err());

    assert_eq!(
        *setup.calls.lock().unwrap(),
        ["load", "startup", "save", "startup"]
    );
    assert_eq!(*setup.enabled_calls.lock().unwrap(), [true, false]);
    assert_eq!(setup.service.current(), initial);
    assert_eq!(*setup.stored.lock().unwrap(), initial);
    assert!(matches!(updates.has_changed(), Ok(false)));
}

#[test]
fn startup_failure_prevents_save_and_broadcast() {
    let setup = setup(AppSettings::default(), false, Some(true));
    let updates = setup.service.subscribe();
    let initial = setup.service.current();
    let mut next = initial.clone();
    next.desktop.start_with_windows = true;

    assert!(setup.service.update(next).is_err());

    assert_eq!(*setup.calls.lock().unwrap(), ["load", "startup"]);
    assert_eq!(setup.service.current(), initial);
    assert_eq!(*setup.stored.lock().unwrap(), initial);
    assert!(matches!(updates.has_changed(), Ok(false)));
}

#[test]
fn toml_repository_round_trips_desktop_and_reminder_settings() {
    let path: PathBuf =
        std::env::temp_dir().join(format!("lumi-settings-{}.toml", uuid::Uuid::new_v4()));
    let repository = TomlSettingsRepository::new(path.clone());
    let mut settings = AppSettings::default();
    settings.desktop.hide_in_fullscreen = false;
    settings.desktop.display_id = "monitor-2".into();
    settings.desktop.scale_percent = 115;
    settings.desktop.open_on_click = true;
    settings.desktop.page_icons = false;
    settings.desktop.fast_animations = true;
    settings.desktop.empty_shelf_on_start = true;
    settings.desktop.hud_caps_lock = false;
    settings.desktop.popup_wifi = false;
    settings.desktop.calendar_starts_monday = true;
    settings.desktop.calendar_week_view = true;
    settings.reminders.sounds = false;
    settings.reminders.task_start = false;
    settings.reminders.snooze_minutes = 12;
    settings.reminders.task_minutes = 35;
    settings.reminders.focus_minutes = 40;
    settings.reminders.short_break_minutes = 7;
    settings.reminders.long_break_minutes = 20;
    settings.reminders.focus_cycles = 3;
    settings.water.enabled = true;
    settings.water.goal_ml = 2500;
    settings.water.bottle_ml = 600;
    settings.water.active_from_minute = 510;
    settings.water.active_until_minute = 1050;

    repository.save(&settings).unwrap();
    let loaded = repository.load().unwrap();
    let _ = std::fs::remove_file(path);

    assert_eq!(loaded, settings);
}

#[test]
fn toml_repository_rejects_unsupported_schema_and_unknown_fields() {
    let path = std::env::temp_dir().join(format!("lumi-settings-{}.toml", uuid::Uuid::new_v4()));
    let repository = TomlSettingsRepository::new(path.clone());
    let unsupported = AppSettings {
        schema_version: 2,
        ..AppSettings::default()
    };
    repository.save(&unsupported).unwrap();
    assert!(repository.load().is_err());

    let current = AppSettings::default();
    repository.save(&current).unwrap();
    let mut content = std::fs::read_to_string(&path).unwrap();
    content.push_str("\nobsolete_setting = true\n");
    std::fs::write(&path, content).unwrap();
    assert!(repository.load().is_err());

    let _ = std::fs::remove_file(path);
}
