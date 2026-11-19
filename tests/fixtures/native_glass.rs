use super::*;

use crate::application::events::create_event_channel;
use crate::application::settings::{
    AppSettings, SettingsRepository, SettingsService, StartupRegistration,
};
use crate::domain::notch::NotchView;
use crate::domain::ports::PortError;
use std::time::Duration;

struct FixtureSettings(AppSettings);

impl SettingsRepository for FixtureSettings {
    fn load(&self) -> std::result::Result<AppSettings, PortError> {
        Ok(self.0.clone())
    }

    fn save(&self, _: &AppSettings) -> std::result::Result<(), PortError> {
        Ok(())
    }
}

struct NoopStartup;

impl StartupRegistration for NoopStartup {
    fn set_enabled(&self, _: bool) -> std::result::Result<(), PortError> {
        Ok(())
    }
}

struct FixtureWindow(HWND);

impl Drop for FixtureWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

const STRIPE_COLORS: [u32; 8] = [
    0x00ff_5656,
    0x00ff_c247,
    0x00eb_e65a,
    0x0047_c878,
    0x0045_c7ff,
    0x006d_72ff,
    0x00c5_62ff,
    0x00ff_5cab,
];

unsafe fn paint_stripes(hwnd: HWND) {
    unsafe {
        let dc = GetDC(Some(hwnd));
        for (index, color) in STRIPE_COLORS.into_iter().enumerate() {
            let brush = CreateSolidBrush(COLORREF(color));
            let rect = RECT {
                left: (index * 650 / STRIPE_COLORS.len()) as i32,
                top: 0,
                right: ((index + 1) * 650 / STRIPE_COLORS.len()) as i32,
                bottom: 240,
            };
            let _ = FillRect(dc, &rect, brush);
            let _ = DeleteObject(brush.into());
        }
        let _ = GdiFlush();
        let _ = ReleaseDC(Some(hwnd), dc);
    }
}

unsafe fn striped_background() -> windows::core::Result<FixtureWindow> {
    let window = FixtureWindow(unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("STATIC"),
            w!(""),
            WS_POPUP,
            100,
            300,
            650,
            240,
            None,
            None,
            Some(GetModuleHandleW(None)?.into()),
            None,
        )?
    });
    unsafe {
        SetWindowPos(
            window.0,
            Some(HWND_TOPMOST),
            100,
            300,
            650,
            240,
            SWP_NOACTIVATE,
        )?;
        let _ = ShowWindow(window.0, SW_SHOWNOACTIVATE);
        paint_stripes(window.0);
    }
    Ok(window)
}

unsafe fn capture_screen() -> windows::core::Result<Vec<u8>> {
    const WIDTH: i32 = 650;
    const HEIGHT: i32 = 240;
    unsafe {
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: WIDTH,
                biHeight: -HEIGHT,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(Some(screen), &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
        let previous = SelectObject(memory, bitmap.into());
        let result = BitBlt(memory, 0, 0, WIDTH, HEIGHT, Some(screen), 100, 300, SRCCOPY);
        let pixels = if result.is_ok() {
            let _ = GdiFlush();
            std::slice::from_raw_parts(bits.cast::<u8>(), (WIDTH * HEIGHT * 4) as usize).to_vec()
        } else {
            Vec::new()
        };
        let _ = SelectObject(memory, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        let _ = ReleaseDC(None, screen);
        result.map(|()| pixels)
    }
}

fn screen_pixel(image: &image::RgbaImage, x: u32, y: u32) -> [u8; 3] {
    let pixel = image.get_pixel(x, y).0;
    [pixel[0], pixel[1], pixel[2]]
}

fn stripe_color(x: u32) -> [u8; 3] {
    let color = STRIPE_COLORS[(x as usize * STRIPE_COLORS.len() / 650).min(7)];
    [color as u8, (color >> 8) as u8, (color >> 16) as u8]
}

fn max_step(image: &image::RgbaImage, y: u32, start: u32, end: u32) -> u8 {
    let mut max = 0;
    for x in start..end {
        let left = screen_pixel(image, x, y);
        let right = screen_pixel(image, x + 1, y);
        for (a, b) in left.into_iter().zip(right) {
            max = max.max(a.abs_diff(b));
        }
    }
    max
}

fn save_capture(name: &str) -> std::result::Result<image::RgbaImage, Box<dyn std::error::Error>> {
    let mut bgra = unsafe { capture_screen()? };
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }
    let image =
        image::RgbaImage::from_raw(650, 240, bgra).expect("capture has the expected dimensions");
    let path = std::env::temp_dir().join(name);
    image.save(&path)?;
    println!("Saved actual desktop composition: {}", path.display());
    Ok(image)
}

unsafe fn present_and_settle(
    host: &mut NativeHost,
    background: &FixtureWindow,
) -> windows::core::Result<()> {
    unsafe {
        host.dirty = true;
        host.draw_frame()?;
        let _ = ShowWindow(host.notch, SW_SHOWNOACTIVATE);
        let started = Instant::now();
        let mut message = MSG::default();
        while started.elapsed() < Duration::from_millis(180) {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            paint_stripes(background.0);
            let _ = windows::Win32::Graphics::Dwm::DwmFlush();
            std::thread::sleep(Duration::from_millis(15));
        }
        let _ = take_events();
        Ok(())
    }
}

fn assert_shell_exposes_stripes(image: &image::RgbaImage, mode: &str) {
    for (x, y) in [(45, 38), (45, 85), (600, 38), (600, 85)] {
        let outside = screen_pixel(image, x, y);
        let expected = stripe_color(x);
        assert!(
            outside
                .into_iter()
                .zip(expected)
                .all(|(actual, source)| actual.abs_diff(source) <= 8),
            "{mode} changed the stripe outside the shell at ({x}, {y}): {outside:?}, expected {expected:?}"
        );
    }
}

fn assert_tinted_blur(image: &image::RgbaImage, mode: &str) {
    let colored = screen_pixel(image, 405, 38);
    let chroma = colored.into_iter().max().unwrap() - colored.into_iter().min().unwrap();
    assert!(
        chroma > 24,
        "{mode} did not show the colored backdrop: {colored:?}"
    );
    assert!(
        screen_pixel(image, 405, 38)
            .into_iter()
            .zip(screen_pixel(image, 480, 38))
            .any(|(a, b)| a.abs_diff(b) > 20),
        "{mode} does not vary with the colored stripes"
    );

    let blurred_step = max_step(image, 38, 395, 415);
    let unblurred_step = max_step(image, 220, 395, 415);
    assert!(
        blurred_step <= 12 && unblurred_step >= 50,
        "{mode} stripe edge should soften inside the shell: glass step={blurred_step}, source step={unblurred_step}"
    );
}

#[test]
#[ignore = "captures real Windows glass modes to %TEMP%/lumi-glass-*.png"]
fn capture_actual_desktop_composited_glass() -> std::result::Result<(), Box<dyn std::error::Error>>
{
    unsafe {
        let mut settings = AppSettings::default();
        settings.is_mascot_enabled = false;
        settings.desktop.appearance.glass = true;
        settings.desktop.appearance.floating = true;
        settings.desktop.appearance.tint_percent = 40;
        let service = Arc::new(SettingsService::new(
            Box::new(FixtureSettings(settings)),
            Box::new(NoopStartup),
        )?);
        let (events, _receiver) = create_event_channel();
        let mut host = NativeHost::new(service, events, None, Err("taskApi.error.listen"))?;
        let background = striped_background()?;
        let _ = KillTimer(Some(host.notch), WATCH_TIMER);
        SetWindowPos(
            host.notch,
            Some(HWND_TOPMOST),
            132,
            320,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE,
        )?;
        host.surface.resize(584, 182, 1.)?;
        host.scale = 1.;
        host.model.view = NotchView::Music;
        host.model.previous_view = NotchView::Music;
        host.model.motion = Motion::new(NotchView::Music);
        host.model.view_changed = Instant::now() - Duration::from_secs(1);
        present_and_settle(&mut host, &background)?;
        let tint40 = save_capture("lumi-glass-fixed-floating.png")?;
        assert_shell_exposes_stripes(&tint40, "floating glass at tint 40");
        assert_tinted_blur(&tint40, "floating glass at tint 40");

        host.model.settings.desktop.appearance.glass = false;
        present_and_settle(&mut host, &background)?;
        let disabled = save_capture("lumi-glass-disabled.png")?;
        assert_shell_exposes_stripes(&disabled, "disabled glass");

        host.model.settings.desktop.appearance.glass = true;
        host.model.settings.desktop.appearance.floating = false;
        present_and_settle(&mut host, &background)?;
        let classic = save_capture("lumi-glass-fixed-classic.png")?;
        assert_shell_exposes_stripes(&classic, "classic glass");
        assert_tinted_blur(&classic, "classic glass");
        drop(host);
        drop(background);
    }
    Ok(())
}
