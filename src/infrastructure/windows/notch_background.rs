//! Background control overlap and small pixel samples beside the compact notch.

use std::collections::HashSet;
use std::mem::size_of;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleDC, CreateDIBSection,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GdiFlush, GetDC, HBITMAP, HDC, HGDIOBJ, ReleaseDC,
    SRCCOPY, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GW_HWNDPREV, GetClassNameW, GetForegroundWindow, GetWindow, GetWindowRect,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible,
};

use super::notch_controls::{ControlProbe, ControlQuery};
use crate::domain::background::{BackgroundObservation, ContentSample, nearby_detail};

#[derive(Clone, Copy, PartialEq)]
pub struct MonitorGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
}

#[derive(Clone, Copy, PartialEq)]
pub struct BackgroundContext {
    window: HWND,
    bounds: [i32; 4],
    monitor: MonitorGeometry,
}

#[derive(Default)]
pub struct BackgroundSampler {
    last_external_window: HWND,
    controls: Option<ControlProbe>,
}

impl BackgroundSampler {
    pub fn context(
        &mut self,
        monitor: MonitorGeometry,
    ) -> BackgroundObservation<BackgroundContext> {
        use BackgroundObservation::{Excluded, Unavailable, Window};
        // Editing Lumi may activate its own window; retain the actual background.
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.is_invalid() {
            return Unavailable;
        }
        let mut process = 0;
        if unsafe { GetWindowThreadProcessId(foreground, Some(&mut process)) } == 0 {
            return Unavailable;
        }
        let previous_window = self.last_external_window;
        if process != std::process::id() {
            self.last_external_window = foreground;
        }
        let window = self.last_external_window;
        if window.is_invalid()
            || !unsafe { IsWindowVisible(window) }.as_bool()
            || unsafe { IsIconic(window) }.as_bool()
        {
            return Excluded;
        }
        let mut class = [0; 256];
        let length = unsafe { GetClassNameW(window, &mut class) } as usize;
        let class = String::from_utf16_lossy(&class[..length]);
        if length == 0 {
            return if window != previous_window {
                Excluded
            } else {
                Unavailable
            };
        }
        if matches!(
            class.as_str(),
            "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Progman" | "WorkerW"
        ) {
            return Excluded;
        }
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(window, &mut rect) }.is_err() {
            return if window != previous_window {
                Excluded
            } else {
                Unavailable
            };
        }
        Window(BackgroundContext {
            window,
            bounds: [rect.left, rect.top, rect.right, rect.bottom],
            monitor,
        })
    }

    pub fn sample(&mut self, context: BackgroundContext) -> Option<ContentSample> {
        let monitor = context.monitor;
        let notch = compact_bounds(monitor);
        let [left, top, right, bottom] = context.bounds;
        if !rectangles_overlap(
            notch,
            RECT {
                left,
                top,
                right,
                bottom,
            },
        ) {
            return Some(ContentSample::Clear);
        }
        if self.controls.is_none() {
            match ControlProbe::new() {
                Ok(probe) => self.controls = Some(probe),
                Err(error) => {
                    tracing::warn!(event = "log.notch.controlProbeFailed", error = %error);
                    return None;
                }
            }
        }
        let controls = self.controls.as_mut()?.sample(ControlQuery {
            window: context.window.0 as isize,
            window_bounds: context.bounds,
            notch: [notch.left, notch.top, notch.right, notch.bottom],
        });
        if controls == Some(true) {
            return Some(ContentSample::Detailed);
        }
        let scaled = |value: f64| (value * monitor.scale).round() as i32;
        let center = monitor.x + monitor.width / 2;
        let offset = scaled(75.0 + 18.0);
        let width = scaled(60.0);
        let height = scaled(30.0);
        let mut detail = ContentSample::Clear;
        for left in [center - offset - width, center + offset] {
            let [window_left, window_top, window_right, window_bottom] = context.bounds;
            let rect = RECT {
                left: left.max(window_left + 2).max(monitor.x),
                top: monitor.y.max(window_top + 2),
                right: (left + width)
                    .min(window_right - 2)
                    .min(monitor.x + monitor.width),
                bottom: (monitor.y + height)
                    .min(window_bottom - 2)
                    .min(monitor.y + monitor.height),
            };
            if rect.right - rect.left < scaled(4.0) || rect.bottom - rect.top < scaled(8.0) {
                continue;
            }
            if !is_unobscured(context.window, rect)? {
                return None;
            }
            detail = detail.combine(capture_detail(rect)?);
            if !is_unobscured(context.window, rect)? {
                return None;
            }
        }
        Some(if controls.is_none() && detail == ContentSample::Clear {
            ContentSample::Uncertain
        } else {
            detail
        })
    }
}

fn compact_bounds(monitor: MonitorGeometry) -> RECT {
    let center = monitor.x + monitor.width / 2;
    let half_width = (75.0 * monitor.scale).ceil() as i32;
    RECT {
        left: center - half_width,
        top: monitor.y,
        right: center + half_width,
        bottom: monitor.y + (30.0 * monitor.scale).ceil() as i32,
    }
}

fn rectangles_overlap(a: RECT, b: RECT) -> bool {
    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
}

/// Reject even small popups between ownership probe points, including transparent overlays.
fn is_unobscured(background: HWND, sample: RECT) -> Option<bool> {
    let mut window = background;
    let mut visited = HashSet::from([background.0 as isize]);
    while let Ok(above) = unsafe { GetWindow(window, GW_HWNDPREV) } {
        if !visited.insert(above.0 as isize) {
            return None;
        }
        window = above;
        if !unsafe { IsWindowVisible(window) }.as_bool() || unsafe { IsIconic(window) }.as_bool() {
            continue;
        }
        let mut bounds = RECT::default();
        unsafe { GetWindowRect(window, &mut bounds) }.ok()?;
        if !rectangles_overlap(sample, bounds) {
            continue;
        }
        let mut process = 0;
        if unsafe { GetWindowThreadProcessId(window, Some(&mut process)) } == 0 {
            return None;
        }
        if process == std::process::id() {
            let region = super::window_region::overlay_region_bounds(window.0 as isize)?;
            if !rectangles_overlap(sample, region) {
                continue;
            }
        }
        let mut cloaked = 0u32;
        unsafe {
            DwmGetWindowAttribute(
                window,
                DWMWA_CLOAKED,
                (&mut cloaked as *mut u32).cast(),
                size_of::<u32>() as u32,
            )
        }
        .ok()?;
        if cloaked == 0 {
            return Some(false);
        }
    }
    Some(true)
}

struct Capture {
    screen: HDC,
    memory: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
}

impl Drop for Capture {
    fn drop(&mut self) {
        unsafe {
            if !self.previous.is_invalid() {
                SelectObject(self.memory, self.previous);
            }
            if !self.bitmap.is_invalid() {
                let _ = DeleteObject(self.bitmap.into());
            }
            if !self.memory.is_invalid() {
                let _ = DeleteDC(self.memory);
            }
            if !self.screen.is_invalid() {
                ReleaseDC(None, self.screen);
            }
        }
    }
}

fn capture_detail(rect: RECT) -> Option<ContentSample> {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let mut capture = Capture {
        screen: unsafe { GetDC(None) },
        memory: HDC::default(),
        bitmap: HBITMAP::default(),
        previous: HGDIOBJ::default(),
    };
    if capture.screen.is_invalid() {
        return None;
    }
    capture.memory = unsafe { CreateCompatibleDC(Some(capture.screen)) };
    if capture.memory.is_invalid() {
        return None;
    }
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    capture.bitmap = unsafe {
        CreateDIBSection(
            Some(capture.screen),
            &info,
            DIB_RGB_COLORS,
            &mut bits,
            None,
            0,
        )
    }
    .ok()?;
    capture.previous = unsafe { SelectObject(capture.memory, capture.bitmap.into()) };
    if capture.previous.is_invalid() || bits.is_null() {
        return None;
    }
    unsafe {
        BitBlt(
            capture.memory,
            0,
            0,
            width,
            height,
            Some(capture.screen),
            rect.left,
            rect.top,
            SRCCOPY,
        )
        .ok()?;
        if !GdiFlush().as_bool() {
            return None;
        }
        // The 32-bit top-down DIB has width*height BGRA pixels, alive until Capture drops.
        let pixels = std::slice::from_raw_parts(bits.cast::<[u8; 4]>(), (width * height) as usize);
        Some(nearby_detail(pixels, width as usize))
    }
}

#[cfg(test)]
#[path = "../../../tests/fixtures/native_background.rs"]
mod tests;
