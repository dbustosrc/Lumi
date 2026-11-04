use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::domain::ports::PortError;

pub const SETTINGS_SCHEMA_VERSION: u32 = 4;

/// UI language preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LanguagePreference {
    #[default]
    System,
    English,
    Spanish,
}

#[cfg(test)]
#[path = "../../tests/fixtures/settings.rs"]
mod tests;

/// Connection parameters of the local Music Assistant server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MusicAssistantSettings {
    pub is_enabled: bool,
    /// Base URL, e.g. `http://192.168.1.10:8095`.
    pub server_url: String,
    /// Long-lived token created in Music Assistant (required for schema >= 28).
    pub token: String,
    /// Queue (player) to follow. Empty plays locally on Lumi.
    pub queue_id: String,
}

impl Default for MusicAssistantSettings {
    fn default() -> Self {
        Self {
            is_enabled: false,
            server_url: "http://localhost:8095".into(),
            token: String::new(),
            queue_id: String::new(),
        }
    }
}

/// Connection parameters of the local Wyoming (Whisper) speech-to-text server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WyomingSettings {
    pub is_enabled: bool,
    pub host: String,
    pub port: u16,
    /// Language hint sent to Whisper (ISO 639-1, e.g. `es`, `en`).
    pub language: String,
}

impl Default for WyomingSettings {
    fn default() -> Self {
        Self {
            is_enabled: false,
            host: "localhost".into(),
            port: 10300,
            language: "es".into(),
        }
    }
}

/// All user-configurable parameters, persisted by a [`SettingsRepository`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppSettings {
    pub schema_version: u32,
    pub language: LanguagePreference,
    pub is_minimal_notch: bool,
    pub is_mascot_enabled: bool,
    pub is_mouse_move_enabled: bool,
    pub rest_after_secs: u64,
    pub is_lyrics_enabled: bool,
    pub music_assistant: MusicAssistantSettings,
    pub wyoming: WyomingSettings,
    pub desktop: DesktopSettings,
    pub reminders: ReminderSettings,
    pub water: crate::domain::water::WaterPlan,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            language: LanguagePreference::System,
            is_minimal_notch: false,
            is_mascot_enabled: true,
            is_mouse_move_enabled: false,
            rest_after_secs: 20,
            is_lyrics_enabled: true,
            music_assistant: MusicAssistantSettings::default(),
            wyoming: WyomingSettings::default(),
            desktop: DesktopSettings::default(),
            reminders: ReminderSettings::default(),
            water: crate::domain::water::WaterPlan::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopSettings {
    pub start_with_windows: bool,
    pub hide_in_fullscreen: bool,
    pub display_id: String,
    pub scale_percent: u16,
    pub open_on_click: bool,
    pub page_icons: bool,
    pub fast_animations: bool,
    pub empty_shelf_on_start: bool,
    pub hud_volume: bool,
    pub hud_brightness: bool,
    pub hud_caps_lock: bool,
    pub hud_input_language: bool,
    pub popup_battery: bool,
    pub popup_wifi: bool,
    pub popup_headphones: bool,
    pub calendar_starts_monday: bool,
    pub calendar_week_view: bool,
}
impl Default for DesktopSettings {
    fn default() -> Self {
        Self {
            start_with_windows: false,
            hide_in_fullscreen: true,
            display_id: String::new(),
            scale_percent: 0,
            open_on_click: false,
            page_icons: true,
            fast_animations: false,
            empty_shelf_on_start: false,
            hud_volume: true,
            hud_brightness: true,
            hud_caps_lock: true,
            hud_input_language: true,
            popup_battery: true,
            popup_wifi: true,
            popup_headphones: true,
            calendar_starts_monday: false,
            calendar_week_view: false,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReminderSettings {
    pub sounds: bool,
    pub task_start: bool,
    pub task_end: bool,
    pub snooze_minutes: u16,
    pub task_minutes: u16,
    pub focus_minutes: u16,
    pub short_break_minutes: u16,
    pub long_break_minutes: u16,
    pub focus_cycles: u16,
}
impl Default for ReminderSettings {
    fn default() -> Self {
        Self {
            sounds: true,
            task_start: true,
            task_end: true,
            snooze_minutes: 10,
            task_minutes: 25,
            focus_minutes: 25,
            short_break_minutes: 5,
            long_break_minutes: 15,
            focus_cycles: 4,
        }
    }
}

impl AppSettings {
    pub fn validate(&self) -> Result<(), PortError> {
        if self.schema_version != SETTINGS_SCHEMA_VERSION {
            return Err(PortError::new(
                "settings.error.parse",
                "unsupported settings schema",
            ));
        }
        if self.wyoming.is_enabled
            && (self.wyoming.host.trim().is_empty() || self.wyoming.language.trim().is_empty())
        {
            return Err(PortError::new(
                "settings.status.invalidConnection",
                "Wyoming host and language are required",
            ));
        }
        if self.music_assistant.is_enabled
            && !url::Url::parse(&self.music_assistant.server_url).is_ok_and(|url| {
                matches!(url.scheme(), "http" | "https" | "ws" | "wss") && url.host_str().is_some()
            })
        {
            return Err(PortError::new(
                "settings.status.invalidConnection",
                "Music Assistant requires a valid server URL",
            ));
        }
        if ![0, 100, 115, 130, 150].contains(&self.desktop.scale_percent)
            || self.rest_after_secs == 0
            || [
                self.reminders.snooze_minutes,
                self.reminders.task_minutes,
                self.reminders.focus_minutes,
                self.reminders.short_break_minutes,
                self.reminders.long_break_minutes,
                self.reminders.focus_cycles,
            ]
            .contains(&0)
            || self.wyoming.port == 0
        {
            return Err(PortError::new(
                "settings.status.invalidNumber",
                "invalid size, time or port",
            ));
        }
        if !self.water.is_valid() {
            return Err(PortError::new(
                "settings.status.invalidWater",
                "goal/bottle must be positive and active hours must be ordered within a day",
            ));
        }
        Ok(())
    }
    pub fn rest_after(&self) -> Duration {
        Duration::from_secs(self.rest_after_secs)
    }
}

/// Persistence contract for settings, implemented by infrastructure.
pub trait SettingsRepository: Send + Sync {
    fn load(&self) -> Result<AppSettings, PortError>;
    fn save(&self, settings: &AppSettings) -> Result<(), PortError>;
}

pub trait StartupRegistration: Send + Sync {
    fn set_enabled(&self, enabled: bool) -> Result<(), PortError>;
}

/// Single source of truth for settings; consumers subscribe to changes.
pub struct SettingsService {
    repository: Box<dyn SettingsRepository>,
    sender: watch::Sender<AppSettings>,
    startup: Box<dyn StartupRegistration>,
}

impl SettingsService {
    pub fn new(
        repository: Box<dyn SettingsRepository>,
        startup: Box<dyn StartupRegistration>,
    ) -> Result<Self, PortError> {
        let settings = repository.load()?;
        settings.validate()?;
        if settings.desktop.start_with_windows {
            startup.set_enabled(true)?;
        }
        let (sender, _) = watch::channel(settings);
        Ok(Self {
            repository,
            sender,
            startup,
        })
    }

    pub fn current(&self) -> AppSettings {
        self.sender.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<AppSettings> {
        self.sender.subscribe()
    }

    /// Persists and broadcasts new settings. Nothing is broadcast when
    /// persistence fails, so subscribers never diverge from disk.
    pub fn update(&self, settings: AppSettings) -> Result<(), PortError> {
        settings.validate()?;
        let previous = self.current().desktop.start_with_windows;
        let next = settings.desktop.start_with_windows;
        if previous != next {
            self.startup.set_enabled(next)?;
        }
        if let Err(error) = self.repository.save(&settings) {
            if previous != next
                && let Err(rollback) = self.startup.set_enabled(previous)
            {
                return Err(PortError::new(
                    "settings.error.write",
                    format!("{error}; startup rollback: {rollback}"),
                ));
            }
            return Err(error);
        }
        self.sender.send_replace(settings);
        Ok(())
    }
}
