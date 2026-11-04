//! Native overlay styles and its shared physical interaction bounds.
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::GetWindowRgnBox;
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, WS_EX_NOACTIVATE,
};

pub fn configure_overlay_styles(hwnd: isize, is_activatable: bool, request_focus: bool) {
    unsafe {
        let handle = HWND(hwnd as *mut _);
        let current = GetWindowLongPtrW(handle, GWL_EXSTYLE) as u32;
        let next = if is_activatable {
            current & !WS_EX_NOACTIVATE.0
        } else {
            current | WS_EX_NOACTIVATE.0
        };
        SetWindowLongPtrW(handle, GWL_EXSTYLE, next as isize);
        if let Err(error) = SetWindowPos(
            handle,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        ) {
            tracing::error!(event="log.window.styleFailed",error=%error);
        }
        if request_focus && is_activatable && !super::fullscreen::foreground_is_fullscreen() {
            let _ = SetForegroundWindow(handle);
        }
    }
}

pub fn centered_region_edges(surface_width: i32, region_width: i32) -> (i32, i32) {
    let left = (surface_width - region_width) / 2;
    (left, left + region_width)
}

pub fn overlay_region_bounds(hwnd: isize) -> Option<RECT> {
    let handle = HWND(hwnd as *mut _);
    let mut window = RECT::default();
    let mut region = RECT::default();
    // SAFETY: callers pass a live overlay HWND; both rectangles are writable.
    unsafe {
        GetWindowRect(handle, &mut window).ok()?;
        if GetWindowRgnBox(handle, &mut region).0 == 0 {
            return None;
        }
    }
    Some(RECT {
        left: window.left + region.left,
        top: window.top + region.top,
        right: window.left + region.right,
        bottom: window.top + region.bottom,
    })
}

#[cfg(test)]
#[path = "../../../tests/fixtures/window_region.rs"]
mod tests;
