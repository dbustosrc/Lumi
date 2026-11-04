use std::path::PathBuf;

use crate::domain::ports::PortError;

const APP_DIRECTORY: &str = "Lumi";

/// Per-user data directory (`%APPDATA%\Lumi`), created on demand.
pub fn data_directory() -> Result<PathBuf, PortError> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| PortError::new("storage.error.appDataMissing", "APPDATA is not set"))?;
    let directory = base.join(APP_DIRECTORY);
    std::fs::create_dir_all(&directory)
        .map_err(|error| PortError::new("storage.error.createDirectory", error))?;
    Ok(directory)
}
