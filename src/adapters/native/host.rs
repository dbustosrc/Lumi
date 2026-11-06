//! The Win32 procedure only queues input. Application/UI state is updated outside
//! callbacks, so synchronous window/control notifications cannot alias Host.
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::mem::{size_of, size_of_val};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Ole::{OleInitialize, OleUninitialize, RevokeDragDrop};
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, Result, w};

use super::accessibility::{self, AccessibleWindow, Command as AccessCommand};
use super::graphics::{GraphicsDevice, Surface};
use super::input::{Inputs, WM_EDIT_CANCEL, WM_EDIT_COMMIT, WM_EDIT_TAB};
use super::model::*;
use super::presenter::{NativePresenter, WM_UPDATES};
use super::scene::*;
use super::settings::{self, Display, SettingsUi};
use super::views;
use crate::application::events::{AppEvent, EventSender, UserIntent};
use crate::application::presenter::ViewUpdate;
use crate::application::settings::SettingsService;
use crate::domain::background::{BackgroundDetection, BackgroundObservation};
use crate::domain::notch::NotchView;
use crate::infrastructure::task_tokens::TaskTokens;
use crate::infrastructure::windows::system_tray::{
    TrayActionHandler, TrayMenuAction, TrayMenuChecks, TrayMenuLabels, Win32SystemTray,
};
use crate::infrastructure::windows::{
    fullscreen,
    notch_background::{BackgroundSampler, MonitorGeometry},
    shelf_drag, shelf_text,
};

const WM_TRAY_ACTION: u32 = WM_APP + 0x11;
pub const WM_ACCESSIBLE_ACTION: u32 = WM_APP + 0x12;
const WATCH_TIMER: usize = 1;
const FRAME_TIMER: usize = 2;
#[derive(Clone)]
enum Event {
    Paint(HWND),
    Resize(HWND),
    Dpi(HWND, RECT),
    Display,
    Setting,
    Timer(usize),
    Move(HWND, f32, f32),
    Leave,
    Down(HWND, f32, f32),
    Up(HWND, f32, f32),
    CaptureLost(HWND),
    Wheel(HWND, f32),
    Command(HWND),
    Commit(HWND, bool),
    Cancel(HWND),
    Key(HWND, usize),
    Close(HWND),
    Tray(usize),
    Update,
    Accessible(HWND),
}
#[derive(Clone, Copy, Default)]
struct Meta {
    notch: isize,
    settings: isize,
    editing: bool,
    fullscreen: bool,
    edit_brushes: [isize; 4],
}
thread_local! {static EVENTS:RefCell<VecDeque<Event>>=const{RefCell::new(VecDeque::new())};static META:Cell<Meta>=Cell::new(Meta::default());}
fn queue(event: Event) {
    EVENTS.with(|events| {
        let mut events = events.borrow_mut();
        if matches!(
            event,
            Event::Timer(_) | Event::Paint(_) | Event::Resize(_) | Event::Update
        ) && events.iter().any(|e| {
            std::mem::discriminant(e) == std::mem::discriminant(&event)
                && match (e, &event) {
                    (Event::Timer(a), Event::Timer(b)) => a == b,
                    (Event::Paint(a), Event::Paint(b)) | (Event::Resize(a), Event::Resize(b)) => {
                        a == b
                    }
                    _ => true,
                }
        }) {
            return;
        }
        events.push_back(event);
    });
}
fn take_events() -> Vec<Event> {
    EVENTS.with(|events| events.borrow_mut().drain(..).collect())
}

pub struct NativeHost {
    pub presenter: Arc<NativePresenter>,
    events: EventSender,
    settings: Arc<SettingsService>,
    task_tokens: Option<Arc<TaskTokens>>,
    system_locale: Option<String>,
    notch: HWND,
    settings_window: HWND,
    access: Arc<AccessibleWindow>,
    settings_access: Option<Arc<AccessibleWindow>>,
    keyboard_focus: Option<(HWND, u64, Rect)>,
    surface: Surface,
    settings_surface: Option<Surface>,
    model: UiModel,
    form: SettingsUi,
    scene: Scene,
    settings_scene: Scene,
    local_transition: Option<(Scene, Instant)>,
    preference_transition: Option<(Scene, Instant)>,
    inputs: Inputs,
    settings_inputs: Inputs,
    displays: Vec<Display>,
    selected: Option<Display>,
    scale: f32,
    settings_scale: f32,
    settings_size: (f32, f32),
    settings_visible: bool,
    notch_visible: bool,
    last_language: Option<String>,
    gesture: (bool, u8, u8),
    settings_scroll_max: f32,
    tray: Win32SystemTray,
    background: BackgroundSampler,
    detection:
        BackgroundDetection<crate::infrastructure::windows::notch_background::BackgroundContext>,
    next_sample: Instant,
    pressed: Option<(HWND, Action, (f32, f32))>,
    scroll_grab: f32,
    pointer_inside: bool,
    region: Option<(i32, i32, i32)>,
    frame_timer: bool,
    dirty: bool,
    settings_dirty: bool,
    edit_brushes: [HBRUSH; 4],
}
impl NativeHost {
    pub fn new(
        settings: Arc<SettingsService>,
        events: EventSender,
        system_locale: Option<String>,
        task_tokens: std::result::Result<Arc<TaskTokens>, &'static str>,
    ) -> Result<Self> {
        unsafe {
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)?;
            OleInitialize(None)?;
            let instance = GetModuleHandleW(None)?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                lpszClassName: w!("LumiNative"),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                hIcon: LoadIconW(
                    Some(instance.into()),
                    windows::core::PCWSTR(1_usize as *const u16),
                )?,
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 {
                return Err(windows::core::Error::from_thread());
            }
            let notch = CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                w!("LumiNative"),
                w!("Lumi"),
                WS_POPUP | WS_CLIPCHILDREN,
                0,
                0,
                SURFACE_WIDTH as i32,
                SURFACE_HEIGHT as i32,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            let edit_brushes = super::input::EDIT_BACKGROUNDS.map(|rgb| {
                CreateSolidBrush(COLORREF(
                    ((rgb & 255) << 16) | (rgb & 0xff00) | ((rgb >> 16) & 255),
                ))
            });
            META.set(Meta {
                notch: notch.0 as isize,
                edit_brushes: edit_brushes.map(|brush| brush.0 as isize),
                ..Default::default()
            });
            let current = settings.current();
            let model = UiModel::new(current.clone());
            let mut form = SettingsUi::new(&current, true);
            match &task_tokens {
                Ok(tokens) => {
                    form.api_error = None;
                    form.api_tokens = tokens.list();
                }
                Err(key) => form.api_error = Some(key),
            }
            let displays = enumerate_displays()?;
            let graphics = GraphicsDevice::new()?;
            let surface = Surface::new(
                graphics,
                notch,
                SURFACE_WIDTH as u32,
                SURFACE_HEIGHT as u32,
                1.,
            )?;
            shelf_text::install_drop_target(notch.0 as isize, events.clone())
                .map_err(|e| windows::core::Error::new(E_FAIL, e.to_string()))?;
            let callback: TrayActionHandler = Arc::new({
                let raw = notch.0 as isize;
                move |action| {
                    let code = match action {
                        TrayMenuAction::OpenSettings => 0,
                        TrayMenuAction::ToggleMinimalNotch => 1,
                        TrayMenuAction::ToggleMascot => 2,
                        TrayMenuAction::ToggleMouseMove => 4,
                        TrayMenuAction::Quit => 3,
                    };
                    let _ = PostMessageW(
                        Some(HWND(raw as *mut _)),
                        WM_TRAY_ACTION,
                        WPARAM(code),
                        LPARAM(0),
                    );
                }
            });
            let icon = crate::adapters::branding::tray_icon();
            let tray = Win32SystemTray::new(
                icon.width,
                icon.height,
                &icon.rgba,
                labels(),
                checks(&current),
                callback,
            )
            .map_err(|e| windows::core::Error::new(E_FAIL, e))?;
            let presenter = NativePresenter::new();
            presenter.attach(notch);
            let mut host = Self {
                presenter,
                events,
                settings,
                task_tokens: task_tokens.ok(),
                system_locale,
                notch,
                settings_window: HWND::default(),
                access: AccessibleWindow::new(notch, "Lumi"),
                settings_access: None,
                keyboard_focus: None,
                surface,
                settings_surface: None,
                model,
                form,
                scene: Scene::default(),
                settings_scene: Scene::default(),
                local_transition: None,
                preference_transition: None,
                inputs: Inputs::default(),
                settings_inputs: Inputs::default(),
                displays,
                selected: None,
                scale: 1.,
                settings_scale: 1.,
                settings_size: (654., 506.),
                settings_visible: false,
                notch_visible: false,
                last_language: None,
                gesture: (false, 255, 255),
                settings_scroll_max: 0.,
                tray,
                background: BackgroundSampler::default(),
                detection: BackgroundDetection::default(),
                next_sample: Instant::now(),
                pressed: None,
                scroll_grab: 0.,
                pointer_inside: false,
                region: None,
                frame_timer: false,
                dirty: true,
                settings_dirty: true,
                edit_brushes,
            };
            host.position_notch()?;
            SetTimer(Some(notch), WATCH_TIMER, 100, None);
            host.paint()?;
            host.watch()?;
            Ok(host)
        }
    }
    pub fn run(mut self) -> Result<()> {
        unsafe {
            let mut message = MSG::default();
            loop {
                let result = GetMessageW(&mut message, None, 0, 0);
                if result.0 == -1 {
                    return Err(windows::core::Error::from_thread());
                }
                if !result.as_bool() {
                    break;
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
                for event in take_events() {
                    self.handle(event)?;
                }
                self.paint()?;
            }
            Ok(())
        }
    }
    fn intent(&self, intent: UserIntent) {
        let _ = self.events.send(AppEvent::Intent(intent));
    }
    fn handle(&mut self, event: Event) -> Result<()> {
        unsafe {
            match event {
                Event::Update => {
                    for update in self.presenter.take() {
                        if matches!(update, ViewUpdate::ReminderSound) {
                            let _ = windows::Win32::System::Diagnostics::Debug::MessageBeep(
                                MB_ICONASTERISK,
                            );
                        }
                        let was_editor = self.model.draft.is_some();
                        self.model.apply(update, Instant::now());
                        if was_editor != self.model.draft.is_some() {
                            self.edit_lock(self.model.draft.is_some())?;
                        }
                    }
                    self.dirty = true;
                    self.settings_dirty = true;
                }
                Event::Paint(hwnd) => {
                    if hwnd == self.notch {
                        self.dirty = true;
                    } else {
                        self.settings_dirty = true;
                    }
                }
                Event::Timer(id) => {
                    if id == WATCH_TIMER {
                        self.watch()?;
                    } else {
                        self.dirty = true;
                        self.settings_dirty = self.settings_visible
                            && (self.form.changed.elapsed() < Duration::from_millis(230)
                                || self.form.hover_changed.elapsed() < Duration::from_millis(155)
                                || self.preference_transition.is_some());
                    }
                }
                Event::Display | Event::Setting => {
                    self.displays = enumerate_displays()?;
                    self.form.dark = true;
                    self.position_notch()?;
                    self.settings_dirty = true;
                }
                Event::Dpi(hwnd, rect) => {
                    if hwnd == self.settings_window {
                        SetWindowPos(
                            hwnd,
                            None,
                            rect.left,
                            rect.top,
                            rect.right - rect.left,
                            rect.bottom - rect.top,
                            SWP_NOACTIVATE | SWP_NOZORDER,
                        )?;
                    } else {
                        self.displays = enumerate_displays()?;
                        self.position_notch()?;
                    }
                }
                Event::Resize(hwnd) => {
                    if hwnd == self.settings_window && !hwnd.is_invalid() {
                        let mut rect = RECT::default();
                        GetClientRect(hwnd, &mut rect)?;
                        self.settings_scale = settings_scale(hwnd)?;
                        self.settings_size = (
                            (rect.right - rect.left) as f32 / self.settings_scale,
                            (rect.bottom - rect.top) as f32 / self.settings_scale,
                        );
                        if let Some(surface) = &mut self.settings_surface {
                            surface.resize(
                                (rect.right - rect.left).max(1) as u32,
                                (rect.bottom - rect.top).max(1) as u32,
                                self.settings_scale,
                            )?;
                        }
                        self.settings_dirty = true;
                    }
                }
                Event::Move(hwnd, x, y) => {
                    let scale = if hwnd == self.notch {
                        self.scale
                    } else {
                        self.settings_scale
                    };
                    let (x, y) = (x / scale, y / scale);
                    if hwnd == self.notch {
                        self.model.mouse = (x, y);
                        let body = self.model.motion.sample(Instant::now()).body();
                        let inside =
                            Rect::new(body.x, body.y, body.w, body.h.max(10.)).contains(x, y);
                        self.set_hover(inside);
                        let nav_width = if self.model.nav { 240. } else { 82. };
                        let nav = self.model.expanded()
                            && self.model.settings.desktop.page_icons
                            && Rect::new(
                                body.x + (body.w - nav_width) / 2.,
                                125.,
                                nav_width,
                                if self.model.nav { 44. } else { 18. },
                            )
                            .contains(x, y);
                        self.model.set_nav(nav, Instant::now());
                        let volume = self.model.view == NotchView::Music
                            && Rect::new(body.x + 276., 103., 102., 26.).contains(x, y);
                        if volume != self.model.volume_shown {
                            self.model.set_volume_shown(volume, Instant::now());
                        }
                        self.model.set_hover_index(
                            self.scene.action_at(x, y).map(|(i, _)| i),
                            Instant::now(),
                        );
                        self.dirty = true;
                    }
                    if hwnd == self.settings_window {
                        self.form.mouse = (x, y);
                        let index = self.settings_scene.action_at(x, y).map(|(i, _)| i);
                        if self.form.hover != index {
                            self.form.previous_hover = self.form.hover;
                            self.form.hover = index;
                            self.form.hover_changed = Instant::now();
                        }
                        self.settings_dirty = true;
                    }
                    let actionable = if hwnd == self.notch {
                        self.scene.action_at(x, y).is_some()
                    } else {
                        self.settings_scene.action_at(x, y).is_some()
                    };
                    SetCursor(Some(LoadCursorW(
                        None,
                        if actionable { IDC_HAND } else { IDC_ARROW },
                    )?));
                    if let Some((owner, action, start)) = self.pressed.clone() {
                        if owner == hwnd {
                            if matches!(action, Action::Seek | Action::Volume) {
                                self.slide(&action, x, y);
                            } else if action == Action::ScrollSettings {
                                self.scroll_settings(y);
                            } else if let Action::DragShelf(id) = action {
                                if (x - start.0).hypot(y - start.1) > 6. {
                                    self.pressed = None;
                                    let _ = ReleaseCapture();
                                    self.drag_shelf(id)?;
                                }
                            }
                        }
                    }
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = TrackMouseEvent(&mut track);
                }
                Event::Leave => {
                    if self.pressed.is_none() {
                        self.set_hover(false);
                        self.model.set_hover_index(None, Instant::now());
                        self.model.mouse = (-1., -1.);
                        self.form.previous_hover = self.form.hover;
                        self.form.hover = None;
                        self.form.mouse = (-1., -1.);
                        self.form.hover_changed = Instant::now();
                        self.settings_dirty = true;
                        self.model.set_nav(false, Instant::now());
                        if self.model.volume_shown {
                            self.model.set_volume_shown(false, Instant::now());
                        }
                        self.dirty = true;
                    }
                }
                Event::Down(hwnd, x, y) => {
                    self.collect_fields(hwnd);
                    let scale = if hwnd == self.notch {
                        self.scale
                    } else {
                        self.settings_scale
                    };
                    let (x, y) = (x / scale, y / scale);
                    let scene = if hwnd == self.notch {
                        &self.scene
                    } else {
                        &self.settings_scene
                    };
                    if hwnd == self.settings_window
                        || (hwnd == self.notch && self.model.input_locked && !self.model.fullscreen)
                    {
                        let _ = SetFocus(Some(hwnd));
                    }
                    if let Some((_, hit)) = scene.action_at(x, y) {
                        let action = hit.action.clone();
                        self.pressed = Some((hwnd, action.clone(), (x, y)));
                        SetCapture(hwnd);
                        if matches!(action, Action::Seek | Action::Volume) {
                            self.slide(&action, x, y);
                        } else if action == Action::ScrollSettings {
                            if let Some(bar) = self.settings_scrollbar() {
                                self.scroll_grab = bar.grab_offset(y);
                                self.scroll_settings(y);
                            }
                        }
                    } else if hwnd == self.notch {
                        self.intent(UserIntent::Activate);
                    }
                }
                Event::Up(hwnd, x, y) => {
                    let _ = ReleaseCapture();
                    let scale = if hwnd == self.notch {
                        self.scale
                    } else {
                        self.settings_scale
                    };
                    let (x, y) = (x / scale, y / scale);
                    if let Some((owner, action, _)) = self.pressed.take() {
                        if owner == hwnd
                            && !matches!(
                                action,
                                Action::Seek | Action::Volume | Action::ScrollSettings
                            )
                        {
                            let scene = if hwnd == self.notch {
                                &self.scene
                            } else {
                                &self.settings_scene
                            };
                            if scene
                                .action_at(x, y)
                                .is_some_and(|(_, h)| h.action == action)
                            {
                                if let Action::DragShelf(id) = action {
                                    self.intent(UserIntent::OpenShelfItem(id));
                                } else {
                                    self.action(action, hwnd)?;
                                }
                            }
                        }
                    }
                    self.dirty = true;
                }
                Event::Wheel(hwnd, delta) => {
                    if hwnd == self.settings_window {
                        self.collect_fields(hwnd);
                        if self.form.section == 1 {
                            self.form.screen_scroll = (self.form.screen_scroll - delta * 80.)
                                .clamp(
                                    0.,
                                    (self.displays.len() as f32 * 174.
                                        - (self.settings_size.0 - 260.))
                                        .max(0.),
                                );
                        } else {
                            self.form.scroll = (self.form.scroll - delta * 35.)
                                .clamp(0., self.settings_scroll_max);
                        }
                        self.settings_dirty = true;
                    } else {
                        match self.model.view {
                            NotchView::Todo => {
                                let h = if self.model.calendar_open { 34. } else { 29. };
                                self.model.task_scroll = (self.model.task_scroll - delta * h * 2.)
                                    .clamp(
                                        0.,
                                        (self.model.tasks.len() as f32 * h
                                            - if self.model.calendar_open { 70. } else { 90. })
                                        .max(0.),
                                    );
                            }
                            NotchView::Shelf => {
                                self.model.shelf_scroll = (self.model.shelf_scroll - delta * 68.)
                                    .clamp(0., (self.model.shelf.len() as f32 * 68. - 336.).max(0.))
                            }
                            _ => {}
                        }
                        self.dirty = true;
                    }
                }
                Event::Command(hwnd) => {
                    let settings_field = self.settings_inputs.field(hwnd).is_some();
                    self.collect_fields(if settings_field {
                        self.settings_window
                    } else {
                        self.notch
                    });
                    if settings_field {
                        self.form.status.clear();
                        self.form.error = false;
                        self.settings_dirty = true;
                    }
                }
                Event::CaptureLost(hwnd) => {
                    if self
                        .pressed
                        .as_ref()
                        .is_some_and(|(owner, _, _)| *owner == hwnd)
                    {
                        self.pressed = None;
                    }
                }
                Event::Commit(hwnd, blur) => {
                    let settings = self.settings_inputs.field(hwnd);
                    self.collect_fields(if settings.is_some() {
                        self.settings_window
                    } else {
                        self.notch
                    });
                    if let Some(field) = settings {
                        if let Field::Preference(key) = field {
                            if let Some(text) = self.form.fields.get(&field) {
                                match key.parse_value(text) {
                                    Ok(value) => self.save_preference(key, value)?,
                                    Err(_) => {
                                        self.form.status =
                                            crate::i18n::translate("settings.status.invalidNumber");
                                        self.form.error = true;
                                        self.settings_dirty = true;
                                    }
                                }
                            }
                        } else if !blur {
                            self.action(Action::SaveSettings, self.settings_window)?;
                        }
                    } else if !blur {
                        self.action(
                            if self.model.draft.is_some() {
                                Action::SaveTask
                            } else {
                                Action::SaveShelfText
                            },
                            self.notch,
                        )?;
                    }
                }
                Event::Cancel(hwnd) => {
                    if self.settings_inputs.field(hwnd).is_some() {
                        self.hide_settings();
                    } else {
                        self.action(
                            if self.model.draft.is_some() {
                                Action::CancelTask
                            } else {
                                Action::CancelShelfText
                            },
                            self.notch,
                        )?;
                    }
                }
                Event::Key(hwnd, key) => {
                    if key == VK_ESCAPE.0 as usize {
                        if hwnd == self.settings_window {
                            self.hide_settings();
                        } else {
                            self.action(Action::CancelTask, hwnd)?;
                        }
                    } else if key == VK_TAB.0 as usize {
                        self.tab(hwnd, GetKeyState(VK_SHIFT.0 as i32) < 0);
                    } else if key == VK_RETURN.0 as usize || key == VK_SPACE.0 as usize {
                        let access = if hwnd == self.notch {
                            Some(self.access.clone())
                        } else {
                            self.settings_access.clone()
                        };
                        if let Some(action) = access.and_then(|a| a.focused_action()) {
                            self.action(action, hwnd)?;
                        }
                    }
                }
                Event::Close(hwnd) => {
                    if hwnd == self.settings_window {
                        self.hide_settings();
                    } else {
                        let _ = DestroyWindow(self.notch);
                    }
                }
                Event::Tray(code) => match code {
                    0 => self.open_settings()?,
                    1 => self.save_preference(
                        Preference::Minimal,
                        i32::from(!self.model.settings.is_minimal_notch),
                    )?,
                    2 => self.save_preference(
                        Preference::Mascot,
                        i32::from(!self.model.settings.is_mascot_enabled),
                    )?,
                    4 => self.save_preference(
                        Preference::MouseMove,
                        i32::from(!self.model.settings.is_mouse_move_enabled),
                    )?,
                    _ => {
                        DestroyWindow(self.notch)?;
                    }
                },
                Event::Accessible(hwnd) => {
                    let access = if hwnd == self.notch {
                        Some(self.access.clone())
                    } else {
                        self.settings_access.clone()
                    };
                    if let Some(access) = access {
                        for command in access.take() {
                            match command {
                                AccessCommand::Invoke(action) => self.action(action, hwnd)?,
                                AccessCommand::Value(action, value) => {
                                    self.intent(if action == Action::Volume {
                                        UserIntent::SetVolume(value)
                                    } else {
                                        UserIntent::SeekToFraction(value)
                                    })
                                }
                                AccessCommand::Focus(id) => {
                                    if !self.model.fullscreen {
                                        if let Some(rect) = access.set_focus(id) {
                                            self.keyboard_focus = Some((hwnd, id, rect));
                                            let _ = SetForegroundWindow(hwnd);
                                            let _ = SetFocus(Some(hwnd));
                                            self.dirty = true;
                                            self.settings_dirty = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(())
        }
    }
    fn set_hover(&mut self, value: bool) {
        if self.pointer_inside != value {
            self.pointer_inside = value;
            self.intent(UserIntent::Hover(value));
        }
    }
    fn tab(&mut self, owner: HWND, back: bool) {
        if self.model.fullscreen {
            return;
        }
        let access = if owner == self.notch {
            Some(self.access.clone())
        } else {
            self.settings_access.clone()
        };
        let Some(access) = access else {
            return;
        };
        let (scene, inputs) = if owner == self.notch {
            (&self.scene, &self.inputs)
        } else {
            (&self.settings_scene, &self.settings_inputs)
        };
        let mut targets: Vec<(Rect, Option<u64>, Option<HWND>)> = access
            .focusable()
            .into_iter()
            .map(|(id, r)| (r, Some(id), None))
            .collect();
        for input in &scene.inputs {
            if let Some(hwnd) = inputs.hwnd(input.key) {
                targets.push((input.rect, None, Some(hwnd)));
            }
        }
        targets.sort_by(|a, b| a.0.y.total_cmp(&b.0.y).then(a.0.x.total_cmp(&b.0.x)));
        if targets.is_empty() {
            return;
        }
        let focused = unsafe { GetFocus() };
        let current = targets.iter().position(|(_, id, hwnd)| {
            hwnd.is_some_and(|h| h == focused)
                || id.is_some_and(|id| {
                    self.keyboard_focus
                        .is_some_and(|(window, key, _)| window == owner && key == id)
                })
        });
        let index = match (current, back) {
            (Some(0), true) | (None, true) => targets.len() - 1,
            (Some(i), true) => i - 1,
            (Some(i), false) => (i + 1) % targets.len(),
            (None, false) => 0,
        };
        let (r, id, hwnd) = targets[index];
        unsafe {
            if let Some(hwnd) = hwnd {
                self.keyboard_focus = None;
                access.clear_focus();
                let _ = SetFocus(Some(hwnd));
            } else if let Some(id) = id {
                access.set_focus(id);
                self.keyboard_focus = Some((owner, id, r));
                let _ = SetFocus(Some(owner));
            }
        }
        self.dirty = true;
        self.settings_dirty = true;
    }
    fn slide(&self, action: &Action, x: f32, _y: f32) {
        let hit = self.scene.hits.iter().find(|h| &h.action == action);
        if let Some(hit) = hit {
            let inset = if *action == Action::Volume {
                VOLUME_THUMB_SIZE / 2.
            } else {
                0.
            };
            let fraction = ((x - hit.rect.x - inset) / (hit.rect.w - 2. * inset)).clamp(0., 1.);
            self.intent(if *action == Action::Seek {
                UserIntent::SeekToFraction(fraction)
            } else {
                UserIntent::SetVolume(fraction)
            });
        }
    }
    fn collect_fields(&mut self, hwnd: HWND) {
        if hwnd == self.settings_window {
            for (key, value) in self.settings_inputs.collect() {
                self.form.fields.insert(key, value);
            }
        } else {
            for (key, value) in self.inputs.collect() {
                match key {
                    Field::TaskTitle => {
                        if let Some(d) = &mut self.model.draft {
                            d.title = value;
                        }
                    }
                    Field::TaskTime => {
                        if let Some(d) = &mut self.model.draft {
                            d.time = value;
                        }
                    }
                    Field::ShelfText => self.model.shelf_text = value,
                    _ => {}
                }
            }
        }
    }
    fn settings_scrollbar(&self) -> Option<settings::SettingsScrollbar> {
        let track = self
            .settings_scene
            .hits
            .iter()
            .find(|hit| hit.action == Action::ScrollSettings)?
            .rect;
        Some(settings::SettingsScrollbar::new(
            track,
            self.settings_scroll_max,
            self.form.scroll,
        ))
    }
    fn scroll_settings(&mut self, y: f32) {
        if let Some(bar) = self.settings_scrollbar() {
            self.collect_fields(self.settings_window);
            self.form.scroll = bar.scroll_at(y, self.scroll_grab);
            self.settings_dirty = true;
        }
    }
    fn action(&mut self, action: Action, owner: HWND) -> Result<()> {
        if (self.model.fullscreen || fullscreen::foreground_is_fullscreen())
            && matches!(
                action,
                Action::AddTask | Action::EditTask(_) | Action::ShelfText
            )
        {
            return Ok(());
        }
        if owner == self.notch
            && matches!(
                action,
                Action::Calendar
                    | Action::AddTask
                    | Action::EditTask(_)
                    | Action::CancelTask
                    | Action::Palette
                    | Action::WaterSettings
                    | Action::WaterGraph
                    | Action::ShelfMenu
                    | Action::ShelfText
                    | Action::CancelShelfText
            )
        {
            self.local_transition = Some((self.scene.clone(), Instant::now()));
        }
        if owner == self.settings_window && matches!(action, Action::Preference(..)) {
            self.preference_transition = Some((self.settings_scene.clone(), Instant::now()));
        }
        match action {
            Action::None => {}
            Action::Intent(intent) => self.intent(intent),
            Action::Settings => self.open_settings()?,
            Action::CloseSettings => self.hide_settings(),
            Action::GenerateTaskToken | Action::CopyTaskToken | Action::RevokeTaskToken(_) => {
                self.task_token_action(action);
            }
            Action::Section(section) => {
                self.collect_fields(self.settings_window);
                self.form.section = section;
                self.form.notices = None;
                self.form.changed = Instant::now();
                self.form.scroll = 0.;
                self.settings_dirty = true;
            }
            Action::OpenNotices | Action::OpenNotice(_) | Action::BackNotices => {
                use settings::NoticesPage;
                (self.form.notices, self.form.scroll) = match (action, self.form.notices) {
                    (Action::OpenNotices, None) => (Some(NoticesPage::Index), 0.),
                    (Action::OpenNotice(index), Some(NoticesPage::Index)) => (
                        Some(NoticesPage::Component {
                            index,
                            index_scroll: self.form.scroll,
                        }),
                        0.,
                    ),
                    (Action::BackNotices, Some(NoticesPage::Component { index_scroll, .. })) => {
                        (Some(NoticesPage::Index), index_scroll)
                    }
                    (Action::BackNotices, Some(NoticesPage::Index)) => (None, 0.),
                    _ => return Ok(()),
                };
                self.form.changed = Instant::now();
                self.settings_dirty = true;
            }
            Action::Preference(key, value) => self.save_preference(key, value)?,
            Action::Display(id) => {
                let mut next = self.settings.current();
                next.desktop.display_id = id;
                self.save_settings(next)?;
            }
            Action::SaveSettings => {
                self.collect_fields(self.settings_window);
                if !self.form.has_unsaved_fields(&self.settings.current()) {
                    return Ok(());
                }
                match self.form.confirmed_settings(&self.settings.current()) {
                    Ok(next) => {
                        self.save_settings(next)?;
                        if !self.form.error {
                            self.form.load_drafts(&self.settings.current());
                        }
                    }
                    Err(e) => self.show_settings_error(e.key),
                }
            }
            Action::ScrollSettings => {
                if let Some(bar) = self.settings_scrollbar() {
                    self.collect_fields(self.settings_window);
                    self.form.scroll = (self.form.scroll + bar.track.h * 0.9).min(bar.max_scroll);
                    self.settings_dirty = true;
                }
            }
            Action::Calendar => {
                self.model.calendar_open = !self.model.calendar_open;
                self.model.draft = None;
                self.edit_lock(false)?;
            }
            Action::SelectDate(date) => {
                if owner == self.settings_window {
                    self.model.selected_date = date;
                } else {
                    self.model.select_date(date);
                }
            }
            Action::ExpandTasks => {
                if self.model.calendar_open {
                    self.model.calendar_open = false;
                    self.model.set_list_expanded(true, Instant::now());
                    self.model.task_scroll = self.model.task_scroll / 34. * 29.;
                } else {
                    self.model
                        .set_list_expanded(!self.model.list_expanded, Instant::now());
                }
            }
            Action::AddTask => {
                self.model.open_task(None);
                self.edit_lock(true)?;
            }
            Action::EditTask(id) => {
                self.model.open_task(Some(id));
                self.edit_lock(true)?;
            }
            Action::EditTasks => {
                self.model.draft = None;
                self.model.edit_mode = !self.model.edit_mode;
                self.edit_lock(false)?;
            }
            Action::SaveTask => {
                self.collect_fields(self.notch);
                if let Some(d) = &mut self.model.draft {
                    if !d.saving && !d.title.trim().is_empty() {
                        d.saving = true;
                        let intent = if let Some(id) = d.id {
                            UserIntent::RenameTask {
                                id,
                                title: d.title.clone(),
                                color_index: d.color as i32,
                            }
                        } else {
                            UserIntent::AddTask {
                                title: d.title.clone(),
                                date: d.date.clone(),
                                time: d.time.clone(),
                                daily: d.daily,
                                color_index: d.color as i32,
                            }
                        };
                        self.intent(intent);
                    }
                }
            }
            Action::CancelTask => {
                if !self.model.draft.as_ref().is_some_and(|d| d.saving) {
                    self.model.draft = None;
                    self.model.palette = false;
                    self.edit_lock(false)?;
                }
            }
            Action::Palette => {
                self.model.palette = !self.model.palette;
            }
            Action::Color(color) => {
                if let Some(d) = &mut self.model.draft {
                    d.color = color;
                }
                self.model.palette = false;
            }
            Action::Daily => {
                if let Some(d) = &mut self.model.draft {
                    d.daily = !d.daily;
                }
            }
            Action::WaterSettings => self.model.water_settings = !self.model.water_settings,
            Action::WaterGraph => self.model.water_graph = !self.model.water_graph,
            Action::ToggleWater => self.save_preference(
                Preference::WaterEnabled,
                i32::from(!self.model.settings.water.enabled),
            )?,
            Action::ShelfMenu => self.model.shelf_menu = !self.model.shelf_menu,
            Action::ShelfText => {
                self.model.shelf_menu = false;
                self.model.shelf_text_open = true;
                self.edit_lock(true)?;
            }
            Action::SaveShelfText => {
                self.collect_fields(self.notch);
                if !self.model.shelf_text.trim().is_empty() {
                    self.intent(UserIntent::AddShelfText(self.model.shelf_text.clone()));
                    self.model.shelf_text_open = false;
                    self.model.shelf_text.clear();
                    self.edit_lock(false)?;
                }
            }
            Action::CancelShelfText => {
                self.model.shelf_text_open = false;
                self.edit_lock(false)?;
            }
            Action::PasteShelf => {
                self.model.shelf_menu = false;
                match shelf_text::clipboard_text() {
                    Ok(text) => self.intent(UserIntent::AddShelfText(text)),
                    Err(e) => {
                        self.model.hud_detail = e.to_string();
                    }
                }
            }
            Action::DragShelf(id) => self.drag_shelf(id)?,
            Action::ShowVolume => {
                self.model.set_volume_shown(true, Instant::now());
            }
            Action::Seek | Action::Volume => {}
            Action::Lyric(index) => self.intent(UserIntent::SeekToLyric(index)),
        }
        self.dirty = true;
        self.settings_dirty = true;
        Ok(())
    }
    fn edit_lock(&mut self, enabled: bool) -> Result<()> {
        if enabled && (self.model.fullscreen || fullscreen::foreground_is_fullscreen()) {
            return Ok(());
        }
        self.model.input_locked = enabled;
        self.intent(UserIntent::LockInteraction(enabled));
        META.with(|meta| {
            let mut value = meta.get();
            value.editing = enabled;
            meta.set(value);
        });
        crate::infrastructure::windows::window_region::configure_overlay_styles(
            self.notch.0 as isize,
            enabled,
            enabled,
        );
        Ok(())
    }
    fn drag_shelf(&mut self, id: uuid::Uuid) -> Result<()> {
        let Some(item) = self
            .model
            .shelf
            .iter()
            .find(|e| e.item.id == id)
            .map(|e| e.item.clone())
        else {
            return Ok(());
        };
        self.intent(UserIntent::Hover(true));
        let _ = self.events.send(AppEvent::OutboundFileDragState {
            is_active: true,
            is_cursor_inside: true,
        });
        let raw = Some(self.notch.0 as isize);
        let result = if let Some(text) = item.text() {
            shelf_drag::start_outbound_text_drag(raw, text)
        } else if let Some(path) = item.path() {
            shelf_drag::start_outbound_file_drag(raw, &[path.to_owned()])
        } else {
            return Ok(());
        };
        if let Err(error) = result {
            tracing::error!(event="log.shelf.dragFailed",error=%error);
        }
        let inside = shelf_drag::sync_cursor_after_drag(raw);
        let _ = self.events.send(AppEvent::OutboundFileDragState {
            is_active: false,
            is_cursor_inside: inside,
        });
        self.set_hover(inside);
        self.dirty = true;
        Ok(())
    }
    fn save_preference(&mut self, key: Preference, value: i32) -> Result<()> {
        let mut next = self.settings.current();
        match key.assign(&mut next, value) {
            Ok(()) => {
                self.form.fields.remove(&Field::Preference(key));
                self.model.water_error = None;
                self.save_settings(next)
            }
            Err(e) => {
                self.show_settings_error(e.key);
                Ok(())
            }
        }
    }
    fn task_token_action(&mut self, action: Action) {
        let Some(tokens) = &self.task_tokens else {
            return;
        };
        let result = match action {
            Action::GenerateTaskToken if self.form.issued_token.is_none() => {
                tokens.generate().map(|token| {
                    self.form.issued_token = Some(token);
                    "taskApi.status.generated"
                })
            }
            Action::CopyTaskToken => {
                let Some(token) = &self.form.issued_token else {
                    return;
                };
                shelf_text::copy_text(&token.secret)
                    .map(|()| {
                        self.form.issued_token = None;
                        "taskApi.status.copied"
                    })
                    .map_err(|_| {
                        crate::domain::ports::PortError::new(
                            "taskApi.error.clipboard",
                            "could not copy token",
                        )
                    })
            }
            Action::RevokeTaskToken(id) => tokens.revoke(id).map(|()| {
                if self
                    .form
                    .issued_token
                    .as_ref()
                    .is_some_and(|token| token.id == id)
                {
                    self.form.issued_token = None;
                }
                "taskApi.status.revoked"
            }),
            _ => return,
        };
        self.form.api_tokens = tokens.list();
        self.form.api_notice = Some(match result {
            Ok(key) => (key, false),
            Err(error) => (error.key, true),
        });
        self.settings_dirty = true;
    }
    fn save_settings(&mut self, next: crate::application::settings::AppSettings) -> Result<()> {
        match self.settings.update(next) {
            Ok(()) => {
                let current = self.settings.current();
                crate::i18n::select_language(current.language, self.system_locale.as_deref());
                self.model.settings = current.clone();
                self.model.mascot = current.is_mascot_enabled;
                self.tray.update(labels(), checks(&current));
                let _ = self.events.send(AppEvent::SettingsChanged(current));
                self.position_notch()?;
                self.form.status = crate::i18n::translate("settings.status.saved");
                self.form.error = false;
                self.dirty = true;
                self.settings_dirty = true;
            }
            Err(e) => self.show_settings_error(e.key),
        }
        Ok(())
    }
    fn show_settings_error(&mut self, key: &'static str) {
        self.form.status = crate::i18n::translate(key);
        self.form.error = true;
        self.model.water_error = Some(key);
        self.settings_dirty = true;
        self.dirty = true;
    }
    fn open_settings(&mut self) -> Result<()> {
        unsafe {
            if fullscreen::foreground_is_fullscreen() {
                return Ok(());
            }
            self.form.load_drafts(&self.settings.current());
            self.form.status.clear();
            self.form.error = false;
            self.form.changed = Instant::now();
            if self.settings_window.is_invalid() {
                let instance = GetModuleHandleW(None)?;
                let monitor = self
                    .displays
                    .iter()
                    .find(|display| display.primary)
                    .ok_or_else(windows::core::Error::from_thread)?;
                self.settings_scale = monitor
                    .scale
                    .min(monitor.work.w / 654.)
                    .min(monitor.work.h / 360.);
                let w = (654. * self.settings_scale).min(monitor.work.w) as i32;
                let h = (506. * self.settings_scale).min(monitor.work.h) as i32;
                self.settings_size = (
                    w as f32 / self.settings_scale,
                    h as f32 / self.settings_scale,
                );
                self.settings_window = CreateWindowExW(
                    WS_EX_NOREDIRECTIONBITMAP | WS_EX_APPWINDOW,
                    w!("LumiNative"),
                    w!("Lumi — Settings"),
                    WS_POPUP | WS_THICKFRAME | WS_SYSMENU | WS_CLIPCHILDREN,
                    monitor.work.x as i32 + (monitor.work.w as i32 - w) / 2,
                    monitor.work.y as i32 + (monitor.work.h as i32 - h) / 2,
                    w,
                    h,
                    None,
                    None,
                    Some(instance.into()),
                    None,
                )?;
                self.settings_access = Some(AccessibleWindow::new(
                    self.settings_window,
                    &crate::i18n::translate("settings.window.title"),
                ));
                META.with(|meta| {
                    let mut value = meta.get();
                    value.settings = self.settings_window.0 as isize;
                    meta.set(value);
                });
                let corner = DWMWCP_ROUND;
                let _ = DwmSetWindowAttribute(
                    self.settings_window,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    (&corner as *const windows::Win32::Graphics::Dwm::DWM_WINDOW_CORNER_PREFERENCE)
                        .cast(),
                    size_of_val(&corner) as u32,
                );
                self.settings_surface = Some(Surface::new(
                    self.surface.device.clone(),
                    self.settings_window,
                    w as u32,
                    h as u32,
                    self.settings_scale,
                )?);
            }
            self.settings_visible = true;
            if let Some(access) = &self.settings_access {
                access.set_visible(true);
            }
            self.settings_dirty = true;
            self.paint()?;
            let _ = ShowWindow(self.settings_window, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(self.settings_window);
            Ok(())
        }
    }
    fn hide_settings(&mut self) {
        self.form.issued_token = None;
        self.form.api_notice = None;
        self.collect_fields(self.settings_window);
        self.settings_visible = false;
        if let Some(access) = &self.settings_access {
            access.set_visible(false);
        }
        unsafe {
            let _ = ShowWindow(self.settings_window, SW_HIDE);
        }
    }
    fn position_notch(&mut self) -> Result<()> {
        unsafe {
            let id = &self.model.settings.desktop.display_id;
            self.selected = self
                .displays
                .iter()
                .find(|m| {
                    if id.is_empty() {
                        m.primary
                    } else {
                        m.id == *id
                    }
                })
                .cloned();
            let Some(monitor) = &self.selected else {
                let _ = ShowWindow(self.notch, SW_HIDE);
                return Ok(());
            };
            let placement = surface_placement(
                monitor.rect,
                monitor.scale,
                self.model.settings.desktop.scale_percent,
            )
            .ok_or_else(|| windows::core::Error::from_hresult(E_INVALIDARG))?;
            self.scale = placement.scale;
            let width = placement.bounds.w as u32;
            let height = placement.bounds.h as u32;
            SetWindowPos(
                self.notch,
                Some(HWND_TOPMOST),
                placement.bounds.x as i32,
                placement.bounds.y as i32,
                width as i32,
                height as i32,
                SWP_NOACTIVATE | SWP_NOCOPYBITS,
            )?;
            self.surface.resize(width, height, self.scale)?;
            self.region = None;
            self.dirty = true;
            Ok(())
        }
    }
    fn watch(&mut self) -> Result<()> {
        unsafe {
            let now = Instant::now();
            let full = fullscreen::foreground_is_fullscreen();
            if self.model.fullscreen != full {
                self.model.fullscreen = full;
                self.inputs.enable(!full);
                let _ = self.events.send(AppEvent::Fullscreen(full));
                self.dirty = true;
            }
            META.with(|meta| {
                let mut value = meta.get();
                value.fullscreen = full;
                meta.set(value);
            });
            let suppressed =
                self.selected.is_none() || (self.model.settings.desktop.hide_in_fullscreen && full);
            if self.notch_visible == suppressed {
                self.access.set_visible(!suppressed);
                let _ = ShowWindow(
                    self.notch,
                    if suppressed {
                        SW_HIDE
                    } else {
                        SW_SHOWNOACTIVATE
                    },
                );
                self.notch_visible = !suppressed;
            }
            if suppressed {
                self.set_hover(false);
            }
            let was_near = self.model.cursor_near;
            let mut cursor = POINT::default();
            let cursor = if !suppressed
                && self.model.compact_mascot(now)
                && GetCursorPos(&mut cursor).is_ok()
                && ScreenToClient(self.notch, &mut cursor).as_bool()
            {
                Some((cursor.x as f32 / self.scale, cursor.y as f32 / self.scale))
            } else {
                None
            };
            if let Some(monitor) = &self.selected {
                let radius = MASCOT_PROXIMITY_DIPS * monitor.scale / self.scale;
                self.dirty |= self.model.observe_mascot_cursor(cursor, radius, now);
            } else {
                self.dirty |= self
                    .model
                    .observe_mascot_cursor(None, MASCOT_PROXIMITY_DIPS, now);
            }
            if was_near != self.model.cursor_near {
                let _ = self
                    .events
                    .send(AppEvent::MascotProximity(self.model.cursor_near));
            }
            if !suppressed {
                if self.model.advance_mascot_reactions(now) {
                    self.dirty = true;
                    let _ = self.events.send(AppEvent::MascotActivity);
                }
                if let Some(monitor) = &self.selected {
                    let geometry = MonitorGeometry {
                        x: monitor.rect.x as i32,
                        y: monitor.rect.y as i32,
                        width: monitor.rect.w as i32,
                        height: monitor.rect.h as i32,
                        scale: f64::from(self.scale),
                    };
                    let observation = self.background.context(geometry);
                    if let Some(value) = self.detection.observe_context(observation) {
                        let _ = self.events.send(AppEvent::BackgroundContent(value));
                    }
                    if let BackgroundObservation::Window(context) = observation {
                        if !self.pointer_inside
                            && !self.model.expanded()
                            && !self.model.motion.running(now)
                            && now >= self.next_sample
                        {
                            self.next_sample = now + Duration::from_secs(1);
                            let sample = self.background.sample(context);
                            if let Some(value) = self.detection.observe_sample(&context, sample) {
                                let _ = self.events.send(AppEvent::BackgroundContent(value));
                            }
                        } else if self.pointer_inside || self.model.motion.running(now) {
                            self.detection.observe_sample(&context, None);
                        }
                    }
                }
            }
            if self.model.compact_mascot(now)
                && (self.model.view == NotchView::Resting || self.model.cursor_near)
            {
                let elapsed = now.duration_since(self.model.start).as_secs_f32();
                let puff = if self.model.dozing && elapsed % 7. < 1.4 {
                    (elapsed % 7. * 10.) as u8
                } else {
                    255
                };
                let gesture = (
                    elapsed > 3.4 && elapsed % 3.4 < 0.28,
                    puff,
                    (elapsed / 2.6) as u8 % 4,
                );
                if gesture != self.gesture {
                    self.gesture = gesture;
                    self.dirty = true;
                }
            }
            if now.duration_since(self.model.shelf_change) > Duration::from_millis(310) {
                self.model.previous_shelf.clear();
            }
            if let Some(language) = crate::infrastructure::windows::desktop_status::input_language()
            {
                if self
                    .last_language
                    .as_ref()
                    .is_some_and(|previous| *previous != language)
                {
                    let _ = self.events.send(AppEvent::InputLanguage(language.clone()));
                }
                self.last_language = Some(language);
            }
            Ok(())
        }
    }
    fn paint(&mut self) -> Result<()> {
        match self.draw_frame() {
            Err(error)
                if matches!(
                    error.code(),
                    windows::Win32::Graphics::Dxgi::DXGI_ERROR_DEVICE_REMOVED
                        | windows::Win32::Graphics::Dxgi::DXGI_ERROR_DEVICE_RESET
                        | windows::Win32::Foundation::D2DERR_RECREATE_TARGET
                ) =>
            {
                let device = GraphicsDevice::new()?;
                self.surface.recreate(device.clone())?;
                if let Some(surface) = &mut self.settings_surface {
                    surface.recreate(device)?;
                }
                self.dirty = true;
                self.settings_dirty = true;
                self.draw_frame()
            }
            result => result,
        }
    }
    fn draw_frame(&mut self) -> Result<()> {
        let now = Instant::now();
        let suppressed = self.selected.is_none()
            || (self.model.settings.desktop.hide_in_fullscreen && self.model.fullscreen);
        if self.dirty && !suppressed {
            let shell = self.model.motion.sample(now);
            let region_w = ((shell.width + shell.ear * 2.) * self.scale).ceil() as i32;
            let region_h = (shell.height.max(10.) * self.scale).ceil() as i32;
            let (left, _) = crate::infrastructure::windows::window_region::centered_region_edges(
                (SURFACE_WIDTH * self.scale).ceil() as i32,
                region_w,
            );
            if self.region != Some((left, region_w, region_h)) {
                unsafe {
                    let region = CreateRectRgn(left, 0, left + region_w, region_h);
                    if SetWindowRgn(self.notch, Some(region), false) == 0 {
                        let _ = DeleteObject(region.into());
                        return Err(windows::core::Error::from_thread());
                    }
                }
                self.region = Some((left, region_w, region_h));
            }
            let elapsed = now.duration_since(self.model.view_changed).as_secs_f32();
            self.scene = views::notch_scene(&self.model, self.model.view, now);
            self.access.update(&self.scene, &self.model, self.scale);
            if let Some((owner, _, rect)) = self.keyboard_focus {
                if owner == self.notch {
                    self.scene.border(rect.inset(-1.), 4., 0xffffff, 1.);
                }
            }
            self.surface.begin(None);
            self.surface.shell(shell)?;
            self.surface.clip_shell()?;
            let page_motion = if self.model.settings.desktop.fast_animations {
                0.143
            } else {
                0.26
            };
            let slide = is_expanded(self.model.previous_view) && self.model.expanded();
            let blend = cubic_bezier((elapsed / page_motion).clamp(0., 1.), 0.42, 0., 0.58, 1.);
            if elapsed < page_motion && self.model.previous_view != self.model.view {
                let old = views::notch_scene(&self.model, self.model.previous_view, now);
                self.surface.paint(
                    &old.paint,
                    1. - blend,
                    (
                        if slide {
                            -self.model.page_direction * blend * 24.
                        } else {
                            0.
                        },
                        0.,
                    ),
                )?;
            }
            let alpha = if self.model.previous_view != self.model.view {
                blend
            } else {
                1.
            };
            let local_alpha = if let Some((old, start)) = &self.local_transition {
                let t = start.elapsed().as_secs_f32() / 0.2;
                if t < 1. {
                    self.surface.paint(&old.paint, (1. - t) * alpha, (0., 0.))?;
                    t
                } else {
                    1.
                }
            } else {
                1.
            };
            self.surface.paint(
                &self.scene.paint,
                alpha * local_alpha,
                (
                    if slide && elapsed < page_motion {
                        self.model.page_direction * (1. - blend) * 24.
                    } else {
                        0.
                    },
                    0.,
                ),
            )?;
            self.surface.unclip_shell();
            self.surface.end()?;
            self.inputs.sync(
                self.notch,
                if elapsed > 0.3 {
                    &self.scene.inputs
                } else {
                    &[]
                },
                self.scale,
                self.model.input_locked && !self.model.fullscreen,
            )?;
            self.inputs.enable(!self.model.fullscreen);
            self.dirty = false;
        }
        if self.settings_visible && self.settings_dirty {
            let (scene, max_scroll) = settings::settings_scene(
                &self.model,
                &self.form,
                &self.displays,
                self.settings_size.0,
                self.settings_size.1,
                now,
            );
            self.settings_scroll_max = max_scroll;
            self.form.scroll = self.form.scroll.min(max_scroll);
            self.settings_scene = scene;
            if let Some(access) = &self.settings_access {
                access.update(&self.settings_scene, &self.model, self.settings_scale);
            }
            if let Some((owner, _, rect)) = self.keyboard_focus {
                if owner == self.settings_window {
                    self.settings_scene.border(
                        rect.inset(-1.),
                        4.,
                        if self.form.dark { 0xffffff } else { 0x222222 },
                        1.,
                    );
                }
            }
            if let Some(surface) = &mut self.settings_surface {
                surface.begin(Some(if self.form.dark { 0x1d1d1f } else { 0xf2f2f5 }));
                let alpha = if let Some((old, start)) = &self.preference_transition {
                    let t = start.elapsed().as_secs_f32() / 0.2;
                    if t < 1. {
                        surface.paint(&old.paint, 1. - t, (0., 0.))?;
                        t
                    } else {
                        1.
                    }
                } else {
                    1.
                };
                surface.paint(&self.settings_scene.paint, alpha, (0., 0.))?;
                surface.end()?;
            }
            self.settings_inputs.sync(
                self.settings_window,
                &self.settings_scene.inputs,
                self.settings_scale,
                false,
            )?;
            self.settings_dirty = false;
        }
        if self
            .local_transition
            .as_ref()
            .is_some_and(|(_, start)| start.elapsed() >= Duration::from_millis(210))
        {
            self.local_transition = None;
        }
        if self
            .preference_transition
            .as_ref()
            .is_some_and(|(_, start)| start.elapsed() >= Duration::from_millis(210))
        {
            self.preference_transition = None;
        }
        let animate = !suppressed && (self.model.animating(now) || self.local_transition.is_some())
            || self.settings_visible
                && (self.form.changed.elapsed() < Duration::from_millis(230)
                    || self.form.hover_changed.elapsed() < Duration::from_millis(155)
                    || self.preference_transition.is_some());
        if animate != self.frame_timer {
            unsafe {
                if animate {
                    SetTimer(Some(self.notch), FRAME_TIMER, 16, None);
                } else {
                    let _ = KillTimer(Some(self.notch), FRAME_TIMER);
                }
            }
            self.frame_timer = animate;
        }
        Ok(())
    }
}
impl Drop for NativeHost {
    fn drop(&mut self) {
        self.access.close();
        if let Some(access) = &self.settings_access {
            access.close();
        }
        self.presenter.detach();
        unsafe {
            let _ = KillTimer(Some(self.notch), WATCH_TIMER);
            let _ = KillTimer(Some(self.notch), FRAME_TIMER);
            let _ = RevokeDragDrop(self.notch);
            self.inputs.clear();
            self.settings_inputs.clear();
            if !self.settings_window.is_invalid() {
                let _ = DestroyWindow(self.settings_window);
            }
            let _ = DestroyWindow(self.notch);
            for brush in self.edit_brushes {
                let _ = DeleteObject(brush.into());
            }
            OleUninitialize();
        }
        META.set(Meta::default());
    }
}
fn labels() -> TrayMenuLabels {
    TrayMenuLabels {
        tooltip: crate::i18n::translate("tray.tooltip"),
        settings: crate::i18n::translate("tray.menu.settings"),
        minimal_notch: crate::i18n::translate("tray.menu.minimalNotch"),
        mascot: crate::i18n::translate("tray.menu.mascot"),
        mouse_move: crate::i18n::translate("tray.menu.mouseMove"),
        quit: crate::i18n::translate("tray.menu.quit"),
    }
}
fn checks(s: &crate::application::settings::AppSettings) -> TrayMenuChecks {
    TrayMenuChecks {
        is_minimal_notch: s.is_minimal_notch,
        is_mascot_enabled: s.is_mascot_enabled,
        is_mouse_move_enabled: s.is_mouse_move_enabled,
    }
}
fn settings_scale(hwnd: HWND) -> Result<f32> {
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return Err(windows::core::Error::from_thread());
        }
        Ok((GetDpiForWindow(hwnd) as f32 / 96.)
            .min((info.rcWork.right - info.rcWork.left) as f32 / 654.)
            .min((info.rcWork.bottom - info.rcWork.top) as f32 / 360.))
    }
}
fn enumerate_displays() -> Result<Vec<Display>> {
    unsafe {
        unsafe extern "system" fn collect(
            monitor: HMONITOR,
            _dc: HDC,
            _rect: *mut RECT,
            data: LPARAM,
        ) -> BOOL {
            unsafe {
                let displays = &mut *(data.0 as *mut Vec<Display>);
                let mut info = MONITORINFOEXW::default();
                info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
                if GetMonitorInfoW(monitor, (&mut info as *mut MONITORINFOEXW).cast()).as_bool() {
                    let end = info
                        .szDevice
                        .iter()
                        .position(|c| *c == 0)
                        .unwrap_or(info.szDevice.len());
                    let id = String::from_utf16_lossy(&info.szDevice[..end]);
                    let mut dx = 96;
                    let mut dy = 96;
                    let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
                    let r = info.monitorInfo.rcMonitor;
                    displays.push(Display {
                        id: id.clone(),
                        label: id,
                        rect: Rect::new(
                            r.left as f32,
                            r.top as f32,
                            (r.right - r.left) as f32,
                            (r.bottom - r.top) as f32,
                        ),
                        work: Rect::new(
                            info.monitorInfo.rcWork.left as f32,
                            info.monitorInfo.rcWork.top as f32,
                            (info.monitorInfo.rcWork.right - info.monitorInfo.rcWork.left) as f32,
                            (info.monitorInfo.rcWork.bottom - info.monitorInfo.rcWork.top) as f32,
                        ),
                        scale: dx as f32 / 96.,
                        primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
                    });
                }
                BOOL(1)
            }
        }
        let mut displays = vec![];
        if !EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM((&mut displays as *mut Vec<Display>) as isize),
        )
        .as_bool()
        {
            return Err(windows::core::Error::from_thread());
        }
        Ok(displays)
    }
}
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let meta = META.get();
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut ps);
                let _ = EndPaint(hwnd, &ps);
                queue(Event::Paint(hwnd));
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_GETOBJECT => accessibility::get_object(hwnd, wparam, lparam)
                .unwrap_or_else(|| DefWindowProcW(hwnd, msg, wparam, lparam)),
            WM_NCCALCSIZE => LRESULT(0),
            WM_GETMINMAXINFO if hwnd.0 as isize == meta.settings => {
                if let Ok(scale) = settings_scale(hwnd) {
                    let info = &mut *(lparam.0 as *mut MINMAXINFO);
                    info.ptMinTrackSize = POINT {
                        x: (654. * scale).ceil() as i32,
                        y: (360. * scale).ceil() as i32,
                    };
                }
                LRESULT(0)
            }
            WM_MOUSEACTIVATE
                if hwnd.0 as isize == meta.notch
                    && (!meta.editing
                        || meta.fullscreen
                        || fullscreen::foreground_is_fullscreen()) =>
            {
                LRESULT(MA_NOACTIVATE as isize)
            }
            WM_NCHITTEST if hwnd.0 as isize == meta.settings => {
                let mut rect = RECT::default();
                let _ = GetWindowRect(hwnd, &mut rect);
                let x = lparam.0 as i16 as i32 - rect.left;
                let y = (lparam.0 >> 16) as i16 as i32 - rect.top;
                let w = rect.right - rect.left;
                let h = rect.bottom - rect.top;
                let dpi = GetDpiForWindow(hwnd) as f32 / 96.;
                let edge = (6. * dpi).ceil() as i32;
                let hit = if x < edge && y < edge {
                    HTTOPLEFT
                } else if x > w - edge && y < edge {
                    HTTOPRIGHT
                } else if x < edge && y > h - edge {
                    HTBOTTOMLEFT
                } else if x > w - edge && y > h - edge {
                    HTBOTTOMRIGHT
                } else if x < edge {
                    HTLEFT
                } else if x > w - edge {
                    HTRIGHT
                } else if y < edge {
                    HTTOP
                } else if y > h - edge {
                    HTBOTTOM
                } else if y < (38. * dpi) as i32 && x > (40. * dpi) as i32 {
                    HTCAPTION
                } else {
                    HTCLIENT
                };
                LRESULT(hit as isize)
            }
            WM_SIZE => {
                queue(Event::Resize(hwnd));
                LRESULT(0)
            }
            WM_DPICHANGED => {
                queue(Event::Dpi(hwnd, *(lparam.0 as *const RECT)));
                LRESULT(0)
            }
            WM_DISPLAYCHANGE => {
                queue(Event::Display);
                LRESULT(0)
            }
            WM_SETTINGCHANGE => {
                queue(Event::Setting);
                LRESULT(0)
            }
            WM_TIMER => {
                queue(Event::Timer(wparam.0));
                LRESULT(0)
            }
            WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP => {
                let x = lparam.0 as i16 as f32;
                let y = (lparam.0 >> 16) as i16 as f32;
                queue(match msg {
                    WM_LBUTTONDOWN => Event::Down(hwnd, x, y),
                    WM_LBUTTONUP => Event::Up(hwnd, x, y),
                    _ => Event::Move(hwnd, x, y),
                });
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                queue(Event::Leave);
                LRESULT(0)
            }
            WM_CAPTURECHANGED => {
                queue(Event::CaptureLost(hwnd));
                LRESULT(0)
            }
            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                queue(Event::Wheel(hwnd, (wparam.0 >> 16) as i16 as f32 / 120.));
                LRESULT(0)
            }
            WM_COMMAND => {
                if (wparam.0 >> 16) as u32 == EN_CHANGE {
                    queue(Event::Command(HWND(lparam.0 as *mut _)));
                }
                LRESULT(0)
            }
            WM_EDIT_COMMIT => {
                queue(Event::Commit(HWND(wparam.0 as *mut _), lparam.0 != 0));
                LRESULT(0)
            }
            WM_EDIT_CANCEL => {
                queue(Event::Cancel(HWND(wparam.0 as *mut _)));
                LRESULT(0)
            }
            WM_EDIT_TAB => {
                queue(Event::Key(hwnd, VK_TAB.0 as usize));
                LRESULT(0)
            }
            WM_KEYDOWN => {
                queue(Event::Key(hwnd, wparam.0));
                LRESULT(0)
            }
            WM_CLOSE => {
                queue(Event::Close(hwnd));
                LRESULT(0)
            }
            WM_TRAY_ACTION => {
                queue(Event::Tray(wparam.0));
                LRESULT(0)
            }
            WM_UPDATES => {
                queue(Event::Update);
                LRESULT(0)
            }
            WM_ACCESSIBLE_ACTION => {
                queue(Event::Accessible(hwnd));
                LRESULT(0)
            }
            WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => {
                let dc = HDC(wparam.0 as *mut _);
                let Some(index) = super::input::paint_background(dc, HWND(lparam.0 as *mut _))
                else {
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                };
                LRESULT(meta.edit_brushes[index])
            }
            WM_DESTROY if hwnd.0 as isize == meta.notch => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
