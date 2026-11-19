use std::{fs, io, path::Path};

use crate::application::settings::{
    AppSettings, DesktopSettings, QuickShareSettings, SETTINGS_SCHEMA_VERSION,
};
use crate::domain::ports::PortError;

/// Upgrade the stored document before the settings repository reads the current contract.
pub fn migrate(path: &Path) -> Result<(), PortError> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(PortError::new("settings.error.read", error)),
    };
    let mut document: toml::Value = toml::from_str(&content).map_err(parse_error)?;
    let version = document
        .get("schema_version")
        .and_then(toml::Value::as_integer);
    match version {
        Some(version) if version == i64::from(SETTINGS_SCHEMA_VERSION) => return Ok(()),
        Some(4 | 5) => {
            if version == Some(4) {
                add_desktop_fields(&mut document)?;
            }
            add_quick_share(&mut document)?;
        }
        Some(6) => {
            let name = document
                .get_mut("quick_share")
                .and_then(|section| section.get_mut("device_name"))
                .ok_or_else(|| parse_error("missing Quick Share device name"))?;
            if name.as_str() == Some("Lumi") {
                *name = toml::Value::String(QuickShareSettings::default().device_name);
            }
        }
        _ => return Err(parse_error("unsupported settings schema")),
    }
    document["schema_version"] = toml::Value::Integer(i64::from(SETTINGS_SCHEMA_VERSION));
    let settings: AppSettings = document.try_into().map_err(parse_error)?;
    settings.validate()?;
    let upgraded = toml::to_string_pretty(&settings)
        .map_err(|error| PortError::new("settings.error.serialize", error))?;
    super::task_store::write_atomically(path, upgraded.as_bytes())
        .map_err(|error| PortError::new("settings.error.write", error))
}

fn add_quick_share(document: &mut toml::Value) -> Result<(), PortError> {
    let quick_share = toml::Value::try_from(QuickShareSettings::default()).map_err(parse_error)?;
    let root = document
        .as_table_mut()
        .ok_or_else(|| parse_error("missing settings table"))?;
    if root.insert("quick_share".into(), quick_share).is_some() {
        return Err(parse_error(
            "unexpected Quick Share settings in an earlier schema",
        ));
    }
    Ok(())
}

fn add_desktop_fields(document: &mut toml::Value) -> Result<(), PortError> {
    let defaults = toml::Value::try_from(DesktopSettings::default())
        .map_err(|error| PortError::new("settings.error.serialize", error))?;
    let desktop = document
        .get_mut("desktop")
        .and_then(toml::Value::as_table_mut)
        .ok_or_else(|| parse_error("missing desktop settings"))?;
    for key in ["status_ring", "appearance", "widgets"] {
        if desktop.insert(key.into(), defaults[key].clone()).is_some() {
            return Err(parse_error(format!("unexpected schema 4 setting: {key}")));
        }
    }
    Ok(())
}

fn parse_error(error: impl std::fmt::Display) -> PortError {
    PortError::new("settings.error.parse", error)
}
