//! Native Win32 notification area (`Shell_NotifyIconW`) icon and popup menu
//! supporting real `MF_CHECKED` toggle checkmarks and `TaskbarCreated` recovery.

use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::sync::{Arc, Mutex, OnceLock};

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
    DeleteObject, HGDIOBJ,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFY_ICON_MESSAGE,
    NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon,
    DestroyMenu, DestroyWindow, GetCursorPos, HICON, HMENU, ICONINFO, MENU_ITEM_FLAGS, MF_CHECKED,
    MF_SEPARATOR, MF_STRING, PostMessageW, RegisterClassW, RegisterWindowMessageW,
    SetForegroundWindow, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TrackPopupMenu, WM_APP, WM_CONTEXTMENU, WM_LBUTTONDBLCLK, WM_NULL, WM_RBUTTONUP, WNDCLASSW,
    WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{PCWSTR, w};

const TRAY_ICON_ID: u32 = 1;
const WM_TRAY_CALLBACK: u32 = WM_APP + 0x42;
const CMD_OPEN_SETTINGS: usize = 1001;
const CMD_TOGGLE_MINIMAL: usize = 1002;
const CMD_TOGGLE_MASCOT: usize = 1003;
const CMD_QUIT: usize = 1004;
const CMD_TOGGLE_MOUSE_MOVE: usize = 1005;

/// User action selected from the Windows notification area icon or popup menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayMenuAction {
    OpenSettings,
    ToggleMinimalNotch,
    ToggleMascot,
    ToggleMouseMove,
    Quit,
}

/// Localized labels displayed in the Win32 notification area tooltip and menu.
#[derive(Debug, Clone)]
pub struct TrayMenuLabels {
    pub tooltip: String,
    pub settings: String,
    pub minimal_notch: String,
    pub mascot: String,
    pub mouse_move: String,
    pub quit: String,
}

/// Checked state of toggleable items in the Win32 notification area menu.
#[derive(Debug, Clone, Copy)]
pub struct TrayMenuChecks {
    pub is_minimal_notch: bool,
    pub is_mascot_enabled: bool,
    pub is_mouse_move_enabled: bool,
}

pub type TrayActionHandler = Arc<dyn Fn(TrayMenuAction) + Send + Sync>;

struct TrayWindowEntry {
    hicon_raw: isize,
    labels: TrayMenuLabels,
    checks: TrayMenuChecks,
    taskbar_created_msg: u32,
    on_action: TrayActionHandler,
}

static TRAY_ENTRIES: OnceLock<Mutex<HashMap<isize, TrayWindowEntry>>> = OnceLock::new();
static TRAY_CLASS_READY: OnceLock<()> = OnceLock::new();

fn tray_entries() -> &'static Mutex<HashMap<isize, TrayWindowEntry>> {
    TRAY_ENTRIES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Owns the hidden Win32 receiver window, `HICON`, and `Shell_NotifyIconW` registration.
pub struct Win32SystemTray {
    hwnd_raw: isize,
    hicon_raw: isize,
}

impl Win32SystemTray {
    pub fn new(
        width: u32,
        height: u32,
        rgba: &[u8],
        labels: TrayMenuLabels,
        checks: TrayMenuChecks,
        on_action: TrayActionHandler,
    ) -> Result<Self, String> {
        // SAFETY: Win32 window and icon creation runs on the main UI thread.
        unsafe {
            ensure_tray_window_class()?;
            let hwnd = create_hidden_tray_window()?;
            let hicon = match create_rgba_icon(width, height, rgba) {
                Ok(icon) => icon,
                Err(err) => {
                    let _ = DestroyWindow(hwnd);
                    return Err(err);
                }
            };
            let hwnd_raw = hwnd.0 as isize;
            let hicon_raw = hicon.0 as isize;
            let taskbar_created_msg = RegisterWindowMessageW(w!("TaskbarCreated"));
            let tooltip = labels.tooltip.clone();
            if let Ok(mut map) = tray_entries().lock() {
                map.insert(
                    hwnd_raw,
                    TrayWindowEntry {
                        hicon_raw,
                        labels,
                        checks,
                        taskbar_created_msg,
                        on_action,
                    },
                );
            }
            if !notify_shell_icon(NIM_ADD, hwnd, hicon, &tooltip) {
                tracing::error!(event = "log.tray.initFailed");
            }
            Ok(Self {
                hwnd_raw,
                hicon_raw,
            })
        }
    }

    pub fn update(&self, labels: TrayMenuLabels, checks: TrayMenuChecks) {
        let tooltip = labels.tooltip.clone();
        if let Ok(mut map) = tray_entries().lock()
            && let Some(entry) = map.get_mut(&self.hwnd_raw)
        {
            entry.labels = labels;
            entry.checks = checks;
        }
        let hwnd = HWND(self.hwnd_raw as *mut _);
        let hicon = HICON(self.hicon_raw as *mut _);
        // SAFETY: `hwnd` and `hicon` remain valid for the lifetime of `self`.
        let updated = unsafe { notify_shell_icon(NIM_MODIFY, hwnd, hicon, &tooltip) };
        if !updated {
            tracing::warn!(event = "log.tray.updateFailed");
        }
    }
}

impl Drop for Win32SystemTray {
    fn drop(&mut self) {
        if let Ok(mut map) = tray_entries().lock() {
            map.remove(&self.hwnd_raw);
        }
        let hwnd = HWND(self.hwnd_raw as *mut _);
        let hicon = HICON(self.hicon_raw as *mut _);
        // SAFETY: Releases owned Win32 notification icon, hidden window, and `HICON`.
        unsafe {
            let _ = notify_shell_icon(NIM_DELETE, hwnd, hicon, "");
            let _ = DestroyWindow(hwnd);
            let _ = DestroyIcon(hicon);
        }
    }
}

unsafe fn ensure_tray_window_class() -> Result<(), String> {
    let mut init_err = None;
    TRAY_CLASS_READY.get_or_init(|| {
        let result = unsafe { register_tray_class() };
        if let Err(err) = result {
            init_err = Some(err);
        }
    });
    init_err.map_or(Ok(()), Err)
}

unsafe fn register_tray_class() -> Result<(), String> {
    let hmodule = unsafe { GetModuleHandleW(PCWSTR::null()) }.map_err(|err| err.to_string())?;
    let class = WNDCLASSW {
        lpfnWndProc: Some(tray_wnd_proc),
        hInstance: HINSTANCE::from(hmodule),
        lpszClassName: w!("LumiTrayHost"),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        return Err("RegisterClassW failed for LumiTrayHost".into());
    }
    Ok(())
}

unsafe fn create_hidden_tray_window() -> Result<HWND, String> {
    let hmodule = unsafe { GetModuleHandleW(PCWSTR::null()) }.map_err(|err| err.to_string())?;
    unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("LumiTrayHost"),
            w!(""),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(HINSTANCE::from(hmodule)),
            None,
        )
    }
    .map_err(|err| err.to_string())
}

unsafe fn create_rgba_icon(width: u32, height: u32, rgba: &[u8]) -> Result<HICON, String> {
    let info = build_top_down_dib_header(width, height);
    let mut bits: *mut c_void = std::ptr::null_mut();
    let color = unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) }
        .map_err(|err| err.to_string())?;
    if bits.is_null() {
        let _ = unsafe { DeleteObject(HGDIOBJ(color.0)) };
        return Err("CreateDIBSection returned null pixel buffer".into());
    }
    unsafe { copy_rgba_to_bgra(rgba, bits as *mut u8, (width * height) as usize) };
    let mask = unsafe { CreateBitmap(width as i32, height as i32, 1, 1, None) };
    let icon_info = ICONINFO {
        fIcon: true.into(),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: color,
    };
    let result = unsafe { CreateIconIndirect(&icon_info) }.map_err(|err| err.to_string());
    let _ = unsafe { DeleteObject(HGDIOBJ(mask.0)) };
    let _ = unsafe { DeleteObject(HGDIOBJ(color.0)) };
    result
}

fn build_top_down_dib_header(width: u32, height: u32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

unsafe fn copy_rgba_to_bgra(rgba: &[u8], dst: *mut u8, pixel_count: usize) {
    let target = unsafe { std::slice::from_raw_parts_mut(dst, pixel_count * 4) };
    let (src_pixels, _) = rgba.as_chunks::<4>();
    let (dst_pixels, _) = target.as_chunks_mut::<4>();
    for (&[r, g, b, a], dst_px) in src_pixels.iter().zip(dst_pixels.iter_mut()) {
        *dst_px = [b, g, r, a];
    }
}

unsafe fn notify_shell_icon(
    action: NOTIFY_ICON_MESSAGE,
    hwnd: HWND,
    hicon: HICON,
    tooltip: &str,
) -> bool {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ICON_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAY_CALLBACK,
        hIcon: hicon,
        ..Default::default()
    };
    write_wide_tooltip(&mut data.szTip, tooltip);
    unsafe { Shell_NotifyIconW(action, &data).as_bool() }
}

fn write_wide_tooltip(buffer: &mut [u16; 128], text: &str) {
    for (slot, unit) in buffer.iter_mut().take(127).zip(text.encode_utf16()) {
        *slot = unit;
    }
}

unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_TRAY_CALLBACK {
        unsafe { handle_tray_mouse_message(hwnd, lparam.0 as u32) };
        return LRESULT(0);
    }
    let recreate = tray_entries().lock().ok().and_then(|map| {
        let entry = map.get(&(hwnd.0 as isize))?;
        (entry.taskbar_created_msg != 0 && msg == entry.taskbar_created_msg)
            .then(|| (entry.hicon_raw, entry.labels.tooltip.clone()))
    });
    if let Some((hicon_raw, tooltip)) = recreate {
        let hicon = HICON(hicon_raw as *mut _);
        let _ = unsafe { notify_shell_icon(NIM_ADD, hwnd, hicon, &tooltip) };
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

unsafe fn handle_tray_mouse_message(hwnd: HWND, event: u32) {
    match event {
        WM_RBUTTONUP | WM_CONTEXTMENU => unsafe { show_tray_context_menu(hwnd) },
        WM_LBUTTONDBLCLK => dispatch_tray_action(hwnd, TrayMenuAction::OpenSettings),
        _ => {}
    }
}

unsafe fn show_tray_context_menu(hwnd: HWND) {
    let snapshot = tray_entries().lock().ok().and_then(|map| {
        map.get(&(hwnd.0 as isize))
            .map(|e| (e.labels.clone(), e.checks))
    });
    let Some((labels, checks)) = snapshot else {
        return;
    };
    let Ok(menu) = (unsafe { build_popup_menu(&labels, checks) }) else {
        return;
    };
    let mut cursor = POINT::default();
    let _ = unsafe { GetCursorPos(&mut cursor) };
    let _ = unsafe { SetForegroundWindow(hwnd) };
    let flags = TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_LEFTALIGN | TPM_RETURNCMD;
    let selected = unsafe { TrackPopupMenu(menu, flags, cursor.x, cursor.y, None, hwnd, None) };
    let _ = unsafe { DestroyMenu(menu) };
    let _ = unsafe { PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0)) };
    if let Some(action) = map_command_to_action(selected.0 as usize) {
        dispatch_tray_action(hwnd, action);
    }
}

unsafe fn build_popup_menu(
    labels: &TrayMenuLabels,
    checks: TrayMenuChecks,
) -> Result<HMENU, String> {
    let menu = unsafe { CreatePopupMenu() }.map_err(|err| err.to_string())?;
    unsafe {
        append_text_item(menu, MF_STRING, CMD_OPEN_SETTINGS, &labels.settings);
        append_text_item(
            menu,
            check_item_flags(checks.is_minimal_notch),
            CMD_TOGGLE_MINIMAL,
            &labels.minimal_notch,
        );
        append_text_item(
            menu,
            check_item_flags(checks.is_mascot_enabled),
            CMD_TOGGLE_MASCOT,
            &labels.mascot,
        );
        append_text_item(
            menu,
            check_item_flags(checks.is_mouse_move_enabled),
            CMD_TOGGLE_MOUSE_MOVE,
            &labels.mouse_move,
        );
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        append_text_item(menu, MF_STRING, CMD_QUIT, &labels.quit);
    }
    Ok(menu)
}

unsafe fn append_text_item(menu: HMENU, flags: MENU_ITEM_FLAGS, id: usize, label: &str) {
    let wide: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
    let _ = unsafe { AppendMenuW(menu, flags, id, PCWSTR(wide.as_ptr())) };
}

fn check_item_flags(is_checked: bool) -> MENU_ITEM_FLAGS {
    if is_checked {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING
    }
}

fn map_command_to_action(command_id: usize) -> Option<TrayMenuAction> {
    match command_id {
        CMD_OPEN_SETTINGS => Some(TrayMenuAction::OpenSettings),
        CMD_TOGGLE_MINIMAL => Some(TrayMenuAction::ToggleMinimalNotch),
        CMD_TOGGLE_MASCOT => Some(TrayMenuAction::ToggleMascot),
        CMD_TOGGLE_MOUSE_MOVE => Some(TrayMenuAction::ToggleMouseMove),
        CMD_QUIT => Some(TrayMenuAction::Quit),
        _ => None,
    }
}

fn dispatch_tray_action(hwnd: HWND, action: TrayMenuAction) {
    let handler = tray_entries().lock().ok().and_then(|map| {
        map.get(&(hwnd.0 as isize))
            .map(|entry| Arc::clone(&entry.on_action))
    });
    if let Some(callback) = handler {
        callback(action);
    }
}

#[cfg(test)]
#[path = "../../../tests/fixtures/system_tray.rs"]
mod tests;
