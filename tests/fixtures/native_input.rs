use crate::adapters::native::graphics::{GraphicsDevice, Surface};
use crate::adapters::native::input::{self, EDIT_BACKGROUNDS, Inputs};
use crate::adapters::native::model::Rect;
use crate::adapters::native::scene::{Field, Input, Preference, Scene};
use crate::application::settings::LanguagePreference;
use std::cell::Cell;
use std::error::Error;
use std::io::Write;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, HBRUSH, HDC, InvalidateRect, PAINTSTRUCT,
    RDW_ALLCHILDREN, RDW_INVALIDATE, RDW_UPDATENOW, RedrawWindow, UpdateWindow,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{CUIAutomation8, IUIAutomation2};
use windows::Win32::UI::Controls::{EM_GETSEL, EM_SETSEL};
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, ES_PASSWORD, GWL_EXSTYLE,
    GWL_STYLE, GetWindowLongW, GetWindowRect, IsWindow, LoadCursorW, RegisterClassW,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SendMessageW, SetWindowPos, UnregisterClassW,
    WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC, WM_ERASEBKGND, WM_PAINT, WNDCLASSW, WS_CLIPCHILDREN,
    WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOREDIRECTIONBITMAP, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};
use windows::core::PCWSTR;

const FIXTURE_TITLE: &str = "Lumi Native Input Composition Fixture";
const FIXTURE_CLASS: &str = "LumiNativeInputFixture";
const FIXTURE_WIDTH: i32 = 900;
const FIXTURE_HEIGHT: i32 = 408;

thread_local! {
    static FIXTURE_BRUSHES: Cell<[isize; 4]> = const { Cell::new([0; 4]) };
}

struct ComApartment;

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

struct LocaleGuard(crate::i18n::Locale);

impl Drop for LocaleGuard {
    fn drop(&mut self) {
        let language = match self.0 {
            crate::i18n::Locale::English => LanguagePreference::English,
            crate::i18n::Locale::Spanish => LanguagePreference::Spanish,
        };
        crate::i18n::select_language(language, None);
    }
}

struct FixtureWindow {
    hwnd: HWND,
    brushes: [HBRUSH; 4],
    instance: windows::Win32::Foundation::HINSTANCE,
    class_name: Vec<u16>,
}

impl FixtureWindow {
    fn new(visible: bool) -> Result<Self, Box<dyn Error>> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let class_name: Vec<u16> = format!("{FIXTURE_CLASS}\0").encode_utf16().collect();
            let class = WNDCLASSW {
                lpfnWndProc: Some(fixture_window_proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, windows::Win32::UI::WindowsAndMessaging::IDC_ARROW)?,
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };
            assert_ne!(
                RegisterClassW(&class),
                0,
                "register isolated input fixture class"
            );

            let brushes = EDIT_BACKGROUNDS.map(|rgb| {
                let bgr = ((rgb & 255) << 16) | (rgb & 0xff00) | ((rgb >> 16) & 255);
                CreateSolidBrush(COLORREF(bgr))
            });
            FIXTURE_BRUSHES.with(|stored| stored.set(brushes.map(|brush| brush.0 as isize)));

            let extended_style = WS_EX_NOREDIRECTIONBITMAP | WS_EX_APPWINDOW;
            let mut style = WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN;
            if visible {
                style |= WS_VISIBLE;
            }
            let mut frame = RECT {
                left: 0,
                top: 0,
                right: FIXTURE_WIDTH,
                bottom: FIXTURE_HEIGHT,
            };
            AdjustWindowRectEx(&mut frame, style, false, extended_style)?;
            let title: Vec<u16> = format!("{FIXTURE_TITLE}\0").encode_utf16().collect();
            let hwnd = CreateWindowExW(
                extended_style,
                PCWSTR(class_name.as_ptr()),
                PCWSTR(title.as_ptr()),
                style,
                20,
                20,
                frame.right - frame.left,
                frame.bottom - frame.top,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            Ok(Self {
                hwnd,
                brushes,
                instance: instance.into(),
                class_name,
            })
        }
    }

    fn resize_client(&self, width: i32, height: i32) -> windows::core::Result<()> {
        unsafe {
            let extended_style = WS_EX_NOREDIRECTIONBITMAP | WS_EX_APPWINDOW;
            let style = WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_VISIBLE;
            let mut frame = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            };
            AdjustWindowRectEx(&mut frame, style, false, extended_style)?;
            SetWindowPos(
                self.hwnd,
                None,
                0,
                0,
                frame.right - frame.left,
                frame.bottom - frame.top,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            )?;
            Ok(())
        }
    }
}

impl Drop for FixtureWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            FIXTURE_BRUSHES.with(|stored| stored.set([0; 4]));
            for brush in self.brushes {
                let _ = DeleteObject(brush.into());
            }
            let _ = UnregisterClassW(PCWSTR(self.class_name.as_ptr()), Some(self.instance));
        }
    }
}

unsafe extern "system" fn fixture_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => {
                let dc = HDC(wparam.0 as *mut _);
                if let Some(index) = input::paint_background(dc, HWND(lparam.0 as *mut _)) {
                    return FIXTURE_BRUSHES.with(|stored| LRESULT(stored.get()[index]));
                }
            }
            WM_ERASEBKGND => return LRESULT(1),
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut paint);
                let _ = EndPaint(hwnd, &paint);
                return LRESULT(0);
            }
            _ => {}
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}

#[test]
fn native_edit_children_compose_over_no_redirection_surface() -> Result<(), Box<dyn Error>> {
    let _locale = LocaleGuard(crate::i18n::active_locale());
    crate::i18n::select_language(LanguagePreference::English, None);
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()? };
    let _com = ComApartment;

    let scene = fixture_scene();
    assert!(scene.inputs.iter().all(|input| {
        input.rect.x >= 0.
            && input.rect.y >= 0.
            && input.rect.x + input.rect.w <= FIXTURE_WIDTH as f32
            && input.rect.y + input.rect.h <= FIXTURE_HEIGHT as f32
    }));
    let visible = sky_capture_enabled();
    let fixture = FixtureWindow::new(visible)?;
    let graphics = GraphicsDevice::new()?;
    let mut surface = Surface::new(
        graphics,
        fixture.hwnd,
        FIXTURE_WIDTH as u32,
        FIXTURE_HEIGHT as u32,
        1.,
    )?;
    let mut native_inputs = Inputs::default();
    native_inputs.sync(fixture.hwnd, &scene.inputs, 1., false)?;
    render(&mut surface, &scene)?;
    update_fixture(&fixture, &native_inputs);
    assert_edits(&native_inputs, &scene.inputs, 1.);

    let automation = unsafe {
        CoCreateInstance::<_, IUIAutomation2>(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?
    };
    let token_hwnd = native_inputs
        .hwnd(Field::MaToken)
        .expect("fake token EDIT exists");
    let token = unsafe { automation.ElementFromHandle(token_hwnd)? };
    assert_eq!(unsafe { token.CurrentName()? }.to_string(), "Access token");
    assert!(unsafe { token.CurrentIsPassword()? }.as_bool());
    let title_hwnd = native_inputs
        .hwnd(Field::TaskTitle)
        .expect("task title EDIT exists");
    let title = unsafe { automation.ElementFromHandle(title_hwnd)? };
    assert_eq!(unsafe { title.CurrentName()? }.to_string(), "Task title");
    assert!(!unsafe { title.CurrentIsPassword()? }.as_bool());
    unsafe {
        let _ = SendMessageW(title_hwnd, EM_SETSEL, Some(WPARAM(2)), Some(LPARAM(6)));
    }
    assert_eq!(selection(title_hwnd), (2, 6));

    if visible {
        println!("NATIVE_INPUT_SKY_READY title={FIXTURE_TITLE}");
        std::io::stdout().flush()?;
        pump_messages_for(Duration::from_secs(25));
    }

    let scale = 1.5;
    let width = (FIXTURE_WIDTH as f32 * scale).round() as u32;
    let height = (FIXTURE_HEIGHT as f32 * scale).round() as u32;
    fixture.resize_client(width as i32, height as i32)?;
    surface.resize(width, height, scale)?;
    native_inputs.sync(fixture.hwnd, &scene.inputs, scale, false)?;
    render(&mut surface, &scene)?;
    update_fixture(&fixture, &native_inputs);
    assert_edits(&native_inputs, &scene.inputs, scale);
    assert_eq!(selection(title_hwnd), (2, 6));

    let child_handles: Vec<_> = scene
        .inputs
        .iter()
        .map(|input| native_inputs.hwnd(input.key).expect("EDIT child exists"))
        .collect();
    native_inputs.clear();
    assert!(
        child_handles
            .iter()
            .all(|hwnd| !unsafe { IsWindow(Some(*hwnd)) }.as_bool())
    );
    Ok(())
}

fn fixture_scene() -> Scene {
    let mut scene = Scene::default();
    scene.round(
        Rect::new(0., 0., FIXTURE_WIDTH as f32, FIXTURE_HEIGHT as f32),
        14.,
        0x0f1012,
    );
    card(
        &mut scene,
        12.,
        12.,
        "Appearance / Pomodoro",
        &[
            (Field::RestSeconds, "Rest after", "45", false),
            (
                Field::Preference(Preference::FocusMinutes),
                "Focus minutes",
                "25",
                false,
            ),
        ],
    );
    card(
        &mut scene,
        458.,
        12.,
        "Music",
        &[
            (
                Field::MaUrl,
                "Server URL",
                "http://fixture.invalid:8095",
                false,
            ),
            (Field::MaToken, "Access token", "fixture-secret-93", true),
            (Field::MaQueue, "Queue", "cola_árbol", false),
        ],
    );
    card(
        &mut scene,
        12.,
        206.,
        "Voice",
        &[
            (Field::VoiceHost, "Host", "fixture.local", false),
            (Field::VoicePort, "Port", "12345", false),
            (Field::VoiceLanguage, "Language", "es-EC", false),
        ],
    );
    card(
        &mut scene,
        458.,
        206.,
        "ToDo editor",
        &[
            (Field::TaskTitle, "Task title", "Café 東京 / tarea", false),
            (Field::TaskTime, "Task time", "08:45", false),
        ],
    );
    scene
}

fn card(scene: &mut Scene, x: f32, y: f32, title: &str, inputs: &[(Field, &str, &str, bool)]) {
    let width = 430.;
    scene.round(Rect::new(x, y, width, 190.), 12., 0x1d1d1f);
    scene.border(Rect::new(x, y, width, 190.), 12., 0x333336, 1.);
    scene.text(
        Rect::new(x + 14., y + 7., width - 28., 18.),
        title,
        13.,
        0xf1f1f3,
        600u16,
        false,
    );
    for (index, (field, label, value, secret)) in inputs.iter().enumerate() {
        let row_y = y + 30. + index as f32 * 38.;
        scene.text(
            Rect::new(x + 14., row_y, width - 28., 12.),
            *label,
            9.,
            0xa3a3a8,
            400u16,
            false,
        );
        let background = Rect::new(x + 14., row_y + 12., width - 28., 31.);
        let style = match field {
            Field::TaskTitle => 0,
            Field::TaskTime => 1,
            Field::ShelfText => 2,
            _ => 3,
        };
        scene.round(background, 7., EDIT_BACKGROUNDS[style]);
        scene.input(
            *field,
            Rect::new(
                background.x + 9.,
                background.y + 5.,
                background.w - 18.,
                21.,
            ),
            *value,
            *secret,
            false,
            *label,
        );
    }
}

fn render(surface: &mut Surface, scene: &Scene) -> windows::core::Result<()> {
    surface.begin(Some(0x0f1012));
    surface.paint(&scene.paint, 1., (0., 0.))?;
    surface.end()
}

fn update_fixture(fixture: &FixtureWindow, inputs: &Inputs) {
    unsafe {
        let _ = InvalidateRect(Some(fixture.hwnd), None, false);
        let _ = UpdateWindow(fixture.hwnd);
        for (key, _) in inputs.collect() {
            if let Some(hwnd) = inputs.hwnd(key) {
                let _ = InvalidateRect(Some(hwnd), None, false);
                let _ = UpdateWindow(hwnd);
            }
        }
        let _ = RedrawWindow(
            Some(fixture.hwnd),
            None,
            None,
            RDW_INVALIDATE | RDW_UPDATENOW | RDW_ALLCHILDREN,
        );
        let _ = DwmFlush();
    }
}

fn assert_edits(inputs: &Inputs, scene_inputs: &[Input], scale: f32) {
    for (key, expected, secret) in [
        (Field::RestSeconds, "45", false),
        (Field::MaUrl, "http://fixture.invalid:8095", false),
        (Field::MaToken, "fixture-secret-93", true),
        (Field::MaQueue, "cola_árbol", false),
        (Field::VoiceHost, "fixture.local", false),
        (Field::VoicePort, "12345", false),
        (Field::VoiceLanguage, "es-EC", false),
        (Field::Preference(Preference::FocusMinutes), "25", false),
        (Field::TaskTitle, "Café 東京 / tarea", false),
        (Field::TaskTime, "08:45", false),
    ] {
        let hwnd = inputs.hwnd(key).expect("native EDIT child exists");
        assert!(
            input::read_text(hwnd) == expected,
            "fixture EDIT value round-trips"
        );
        let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) as u32 };
        let extended_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 };
        assert_ne!(extended_style & WS_EX_LAYERED.0, 0, "EDIT child is layered");
        assert_eq!(
            style & ES_PASSWORD as u32,
            if secret { ES_PASSWORD as u32 } else { 0 },
            "only the fake Music token uses password style"
        );

        let logical = scene_inputs
            .iter()
            .find(|input| input.key == key)
            .expect("fixture scene has the EDIT input");
        let mut rect = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut rect).unwrap() };
        assert_eq!(
            (rect.right - rect.left, rect.bottom - rect.top),
            (
                (logical.rect.w * scale).ceil() as i32,
                (logical.rect.h * scale).ceil() as i32,
            ),
            "EDIT bounds follow the scene at the requested scale"
        );
    }
}

fn pump_messages_for(duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        unsafe {
            let mut message = windows::Win32::UI::WindowsAndMessaging::MSG::default();
            while windows::Win32::UI::WindowsAndMessaging::PeekMessageW(
                &mut message,
                None,
                0,
                0,
                windows::Win32::UI::WindowsAndMessaging::PM_REMOVE,
            )
            .as_bool()
            {
                let _ = windows::Win32::UI::WindowsAndMessaging::TranslateMessage(&message);
                windows::Win32::UI::WindowsAndMessaging::DispatchMessageW(&message);
            }
        }
        std::thread::sleep(Duration::from_millis(8));
    }
}

fn sky_capture_enabled() -> bool {
    std::env::var_os("LUMI_NATIVE_INPUT_SKY_CAPTURE").is_some()
}

fn selection(hwnd: HWND) -> (usize, usize) {
    let packed = unsafe { SendMessageW(hwnd, EM_GETSEL, None, None).0 as u32 };
    ((packed & 0xffff) as usize, (packed >> 16) as usize)
}

#[path = "native_notification_probe.rs"]
mod notification_probe;
