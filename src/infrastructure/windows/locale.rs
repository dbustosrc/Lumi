use windows::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
use windows::core::PWSTR;

/// Buffer capacity in UTF-16 code units for the preferred UI language list.
const UI_LANGUAGES_BUFFER_LEN: usize = 256;

/// Returns the user's primary Windows Display UI language (e.g. `en-US`),
/// rather than the regional date/number format locale.
pub fn get_user_locale() -> Option<String> {
    let mut language_count = 0u32;
    let mut buffer = [0u16; UI_LANGUAGES_BUFFER_LEN];
    let mut buffer_len = u32::try_from(buffer.len()).ok()?;
    // SAFETY: `buffer` is valid for `buffer_len` UTF-16 writes across this call.
    unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut language_count,
            Some(PWSTR(buffer.as_mut_ptr())),
            &mut buffer_len,
        )
        .ok()?;
    }
    if language_count == 0 {
        return None;
    }
    let written = usize::try_from(buffer_len).ok()?.min(buffer.len());
    let first_end = buffer[..written].iter().position(|&unit| unit == 0)?;
    (first_end > 0)
        .then(|| String::from_utf16(&buffer[..first_end]).ok())
        .flatten()
}
