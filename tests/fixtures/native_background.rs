use super::*;

use crate::domain::background::ContentSample;
use crate::infrastructure::windows::notch_controls::{
    ControlProbe, ControlQuery, control_overlaps_notch,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{
    UIA_ButtonControlTypeId, UIA_ComboBoxControlTypeId, UIA_EditControlTypeId,
    UIA_PaneControlTypeId, UIA_TextControlTypeId, UIA_TitleBarControlTypeId,
    UIA_WindowControlTypeId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, DispatchMessageW, GetWindowRect, MSG, PM_REMOVE, PeekMessageW,
    TranslateMessage, WS_CHILD, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    WS_VISIBLE,
};
use windows::core::{PCWSTR, w};

#[test]
fn clipped_sample_strips_report_no_nearby_detail_across_monitor_scales() {
    // The window does not intersect the compact notch, so native probes are unnecessary.
    for scale in [1.0, 1.5, 2.0] {
        let context = BackgroundContext {
            window: HWND::default(),
            bounds: [100, 200, 1000, 900],
            monitor: MonitorGeometry {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                scale,
            },
        };

        assert_eq!(
            BackgroundSampler::default().sample(context),
            Some(ContentSample::Clear)
        );
    }
}

#[test]
fn centered_interactive_search_control_overlaps_the_compact_notch_across_scales() {
    for scale in [1.0_f64, 1.15, 1.3, 1.5, 2.0] {
        let monitor = MonitorGeometry {
            x: -3000,
            y: -200,
            width: (1920.0 * scale).round() as i32,
            height: (1080.0 * scale).round() as i32,
            scale,
        };
        let notch = compact_bounds(monitor);
        let center = monitor.x + monitor.width / 2;
        let scaled = |value: f64| (value * scale).round() as i32;
        let half_width = (75.0 * scale).ceil() as i32;

        assert_eq!(notch.left, center - half_width);
        assert_eq!(notch.right, center + half_width);
        assert_eq!(notch.top, monitor.y);
        assert_eq!(notch.bottom, monitor.y + (30.0 * scale).ceil() as i32);

        let search = RECT {
            left: center - scaled(184.0),
            top: monitor.y + scaled(4.0),
            right: center + scaled(184.0),
            bottom: monitor.y + scaled(52.0),
        };

        assert!(notch.left < 0 && notch.top < 0);
        assert!(control_overlaps_notch(
            UIA_EditControlTypeId,
            true,
            false,
            search,
            notch,
        ));
    }
}

#[test]
fn centered_controls_only_count_when_enabled_visible_and_interactive() {
    let notch = RECT {
        left: 885,
        top: 0,
        right: 1035,
        bottom: 30,
    };
    let centered_control = RECT {
        left: 800,
        top: 4,
        right: 1100,
        bottom: 52,
    };

    assert!(control_overlaps_notch(
        UIA_EditControlTypeId,
        true,
        false,
        centered_control,
        notch,
    ));
    assert!(control_overlaps_notch(
        UIA_ComboBoxControlTypeId,
        true,
        false,
        centered_control,
        notch,
    ));
    assert!(control_overlaps_notch(
        UIA_ButtonControlTypeId,
        true,
        false,
        centered_control,
        notch,
    ));
    assert!(!control_overlaps_notch(
        UIA_EditControlTypeId,
        false,
        false,
        centered_control,
        notch,
    ));
    assert!(!control_overlaps_notch(
        UIA_EditControlTypeId,
        true,
        true,
        centered_control,
        notch,
    ));

    for noninteractive in [
        UIA_PaneControlTypeId,
        UIA_TextControlTypeId,
        UIA_TitleBarControlTypeId,
        UIA_WindowControlTypeId,
    ] {
        assert!(!control_overlaps_notch(
            noninteractive,
            true,
            false,
            centered_control,
            notch,
        ));
    }
}

#[test]
fn an_interactive_control_below_the_title_bar_does_not_overlap_the_compact_notch() {
    let monitor = MonitorGeometry {
        x: 0,
        y: -120,
        width: 1920,
        height: 1080,
        scale: 1.0,
    };
    let notch = compact_bounds(monitor);
    let below_notch = RECT {
        left: 900,
        top: notch.bottom,
        right: 1020,
        bottom: notch.bottom + 40,
    };

    assert!(!control_overlaps_notch(
        UIA_EditControlTypeId,
        true,
        false,
        below_notch,
        notch,
    ));
}

#[test]
#[ignore = "requires the interactive Windows UI Automation provider"]
fn native_uia_provider_finds_a_centered_edit_control_and_ignores_the_empty_region_below_it() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    struct TestComApartment;

    impl Drop for TestComApartment {
        fn drop(&mut self) {
            // SAFETY: this balances the successful CoInitializeEx on this test thread.
            unsafe { CoUninitialize() };
        }
    }

    // SAFETY: the window-owning test thread uses a single STA for standard Win32 UIA providers.
    let com_result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    assert!(
        !com_result.is_err(),
        "initialize the fixture window thread as an STA: {com_result:?}"
    );
    let _com_apartment = TestComApartment;

    struct TestWindows(Vec<HWND>);

    impl Drop for TestWindows {
        fn drop(&mut self) {
            for window in self.0.drain(..).rev() {
                // SAFETY: this fixture owns only the HWNDs created in this test.
                unsafe {
                    let _ = DestroyWindow(window);
                }
            }
        }
    }

    fn wait_for_control(probe: &mut ControlProbe, query: ControlQuery) -> Option<bool> {
        use std::time::{Duration, Instant};

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(result) = probe.sample(query) {
                return Some(result);
            }
            let mut message = MSG::default();
            // SAFETY: this isolated test thread pumps its complete queue for UIA/COM marshaling.
            unsafe {
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    let module = unsafe { GetModuleHandleW(PCWSTR::null()) }.expect("current module is available");
    let instance = windows::Win32::Foundation::HINSTANCE::from(module);
    let root = unsafe {
        CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            w!("STATIC"),
            w!(""),
            WS_POPUP | WS_VISIBLE,
            400,
            100,
            360,
            100,
            None,
            None,
            Some(instance),
            None,
        )
    }
    .expect("create the isolated UIA test root");
    let mut windows = TestWindows(vec![root]);
    let edit = unsafe {
        CreateWindowExW(
            WS_EX_NOACTIVATE,
            w!("EDIT"),
            w!(""),
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            360,
            30,
            Some(root),
            None,
            Some(instance),
            None,
        )
    }
    .expect("create the isolated Edit control");
    windows.0.push(edit);

    let mut bounds = RECT::default();
    unsafe { GetWindowRect(root, &mut bounds) }.expect("read the test root bounds");
    let center = (bounds.left + bounds.right) / 2;
    let edit_query = ControlQuery {
        window: root.0 as isize,
        window_bounds: [bounds.left, bounds.top, bounds.right, bounds.bottom],
        notch: [center - 75, bounds.top, center + 75, bounds.top + 30],
    };
    let empty_query = ControlQuery {
        notch: [center - 75, bounds.top + 36, center + 75, bounds.top + 66],
        ..edit_query
    };

    let probe_started = std::time::Instant::now();
    let mut probe = ControlProbe::new().expect("start the read-only UIA worker");
    let edit_result = wait_for_control(&mut probe, edit_query);
    tracing::debug!(
        event = "test.controlProbeResult",
        target = "centered_edit",
        elapsed_ms = probe_started.elapsed().as_millis(),
        result = ?edit_result,
    );
    assert_eq!(edit_result, Some(true));

    let empty_result = wait_for_control(&mut probe, empty_query);
    tracing::debug!(
        event = "test.controlProbeResult",
        target = "empty_region",
        elapsed_ms = probe_started.elapsed().as_millis(),
        result = ?empty_result,
    );
    assert_eq!(empty_result, Some(false));
}

#[test]
fn background_context_changes_when_window_bounds_or_monitor_scale_changes() {
    let context = BackgroundContext {
        window: HWND::default(),
        bounds: [100, 200, 1000, 900],
        monitor: MonitorGeometry {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            scale: 1.0,
        },
    };
    let moved_window = BackgroundContext {
        bounds: [100, 201, 1000, 900],
        ..context
    };
    let changed_scale = BackgroundContext {
        monitor: MonitorGeometry {
            scale: 1.5,
            ..context.monitor
        },
        ..context
    };

    assert!(context != moved_window);
    assert!(context != changed_scale);
}

#[test]
fn a_small_popup_between_probe_points_still_overlaps_the_sample_rect() {
    // The popup misses the old corner/center probes but contributes pixels to the full BitBlt.
    let sample = RECT {
        left: 100,
        top: 100,
        right: 200,
        bottom: 160,
    };
    let popup = RECT {
        left: 145,
        top: 110,
        right: 155,
        bottom: 120,
    };

    assert!(rectangles_overlap(sample, popup));
}

#[test]
fn rectangles_that_only_touch_at_an_edge_do_not_overlap() {
    let sample = RECT {
        left: 10,
        top: 20,
        right: 60,
        bottom: 50,
    };
    let touching_right = RECT {
        left: 60,
        top: 25,
        right: 80,
        bottom: 45,
    };
    let touching_bottom = RECT {
        left: 20,
        top: 50,
        right: 40,
        bottom: 70,
    };

    assert!(!rectangles_overlap(sample, touching_right));
    assert!(!rectangles_overlap(sample, touching_bottom));
}
