use crate::domain::fullscreen::covers_monitor;
use std::mem::size_of;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::UI::Shell::{
    QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetClassNameW, GetForegroundWindow, GetWindowLongW, GetWindowRect, IsIconic,
    WS_CAPTION,
};

pub fn foreground_is_fullscreen() -> bool {
    if unsafe { SHQueryUserNotificationState() }
        .is_ok_and(|state| matches!(state, QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE))
    {
        return true;
    }
    unsafe {
        let window = GetForegroundWindow();
        if window.is_invalid() || IsIconic(window).as_bool() {
            return false;
        }
        let mut class = [0; 256];
        let length = GetClassNameW(window, &mut class) as usize;
        if matches!(
            String::from_utf16_lossy(&class[..length]).as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        ) {
            return false;
        }
        let mut monitor = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let mut bounds = RECT::default();
        if !GetMonitorInfoW(
            MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        )
        .as_bool()
            || GetWindowRect(window, &mut bounds).is_err()
        {
            return false;
        }
        let screen = monitor.rcMonitor;
        covers_monitor(
            [bounds.left, bounds.top, bounds.right, bounds.bottom],
            [screen.left, screen.top, screen.right, screen.bottom],
            GetWindowLongW(window, GWL_STYLE) as u32 & WS_CAPTION.0 != 0,
        )
    }
}
