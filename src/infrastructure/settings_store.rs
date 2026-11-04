use std::path::PathBuf;

use crate::application::settings::{AppSettings, SettingsRepository};
use crate::domain::ports::PortError;

/// Stores settings as human-editable TOML.
pub struct TomlSettingsRepository {
    path: PathBuf,
}

impl TomlSettingsRepository {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl SettingsRepository for TomlSettingsRepository {
    fn load(&self) -> Result<AppSettings, PortError> {
        if !self.path.exists() {
            let defaults = AppSettings::default();
            self.save(&defaults)?;
            return Ok(defaults);
        }
        let content = std::fs::read_to_string(&self.path)
            .map_err(|error| PortError::new("settings.error.read", error))?;
        let settings: AppSettings = toml::from_str(&content)
            .map_err(|error| PortError::new("settings.error.parse", error))?;
        settings.validate()?;
        Ok(settings)
    }

    fn save(&self, settings: &AppSettings) -> Result<(), PortError> {
        let content = toml::to_string_pretty(settings)
            .map_err(|error| PortError::new("settings.error.serialize", error))?;
        super::task_store::write_atomically(&self.path, content.as_bytes())
            .map_err(|error| PortError::new("settings.error.write", error))
    }
}
