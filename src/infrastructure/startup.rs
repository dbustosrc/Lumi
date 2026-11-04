use crate::application::settings::StartupRegistration;
use crate::domain::ports::PortError;

pub struct NativeStartupRegistration;

#[cfg(windows)]
impl StartupRegistration for NativeStartupRegistration {
    fn set_enabled(&self, enabled: bool) -> Result<(), PortError> {
        use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
        use windows::Win32::System::Registry::{
            HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
        };
        use windows::core::w;
        let path = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let name = w!("Lumi");
        let result = if enabled {
            let exe = std::env::current_exe()
                .map_err(|error| PortError::new("settings.error.startup", error))?;
            let value: Vec<u16> = format!("\"{}\"", exe.display())
                .encode_utf16()
                .chain(Some(0))
                .collect();
            unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    path,
                    name,
                    REG_SZ.0,
                    Some(value.as_ptr().cast()),
                    (value.len() * 2) as u32,
                )
            }
        } else {
            let result = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, path, name) };
            if result == ERROR_FILE_NOT_FOUND {
                return Ok(());
            }
            result
        };
        result
            .ok()
            .map_err(|error| PortError::new("settings.error.startup", error))
    }
}

#[cfg(not(windows))]
impl StartupRegistration for NativeStartupRegistration {
    fn set_enabled(&self, _enabled: bool) -> Result<(), PortError> {
        Err(PortError::new(
            "settings.error.startup",
            "Windows is required",
        ))
    }
}
