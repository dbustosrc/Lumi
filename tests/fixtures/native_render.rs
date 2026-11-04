use super::*;

use crate::adapters::native::model::{
    MASCOT_PROXIMITY_DIPS, Motion, Rect, SURFACE_HEIGHT, SURFACE_WIDTH, Shell, UiModel, WakeStyle,
};
use crate::adapters::native::scene::{Action, Field, Paint, Preference, Scene};
use crate::adapters::native::{settings, views};
use crate::application::presenter::ShelfEntry;
use crate::application::reminders::{FocusPhase, UtilityState};
use crate::application::settings::{AppSettings, LanguagePreference};
use crate::domain::calendar::calendar_grid;
use crate::domain::device::{HeadphoneStatus, PowerStatus};
use crate::domain::media::{Artwork, MediaConnection, Track};
use crate::domain::notch::NotchView;
use crate::domain::shelf::{ShelfContent, ShelfItem, ShelfItemKind};
use crate::domain::todo::{TaskColor, TaskSchedule, TodoProgress, TodoTask};
use crate::domain::water::WaterState;
use chrono::{Local, NaiveTime, TimeZone, Utc};
use image::RgbaImage;
use std::error::Error;
use std::path::Path;
use std::time::{Duration, Instant};
use uuid::Uuid;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, IsWindowVisible, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP,
    WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::w;

struct HiddenWindow(HWND);

impl Drop for HiddenWindow {
    fn drop(&mut self) {
        // SAFETY: the fixture owns this hidden HWND and destroys it after its surface is dropped.
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

struct ComApartment;

impl Drop for ComApartment {
    fn drop(&mut self) {
        // SAFETY: balances the successful CoInitializeEx on this test thread.
        unsafe { CoUninitialize() };
    }
}

struct LocaleGuard(crate::i18n::Locale);

impl Drop for LocaleGuard {
    fn drop(&mut self) {
        let preference = match self.0 {
            crate::i18n::Locale::English => LanguagePreference::English,
            crate::i18n::Locale::Spanish => LanguagePreference::Spanish,
        };
        crate::i18n::select_language(preference, None);
    }
}

#[test]
fn native_scenes_render_review_captures_without_showing_a_window()
-> std::result::Result<(), Box<dyn Error>> {
    let _locale = LocaleGuard(crate::i18n::active_locale());
    crate::i18n::select_language(LanguagePreference::English, None);
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()? };
    let _com = ComApartment;

    let module = unsafe { GetModuleHandleW(None)? };
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("Lumi native render fixture"),
            WS_POPUP,
            0,
            0,
            981,
            759,
            None,
            None,
            Some(module.into()),
            None,
        )?
    };
    assert!(!unsafe { IsWindowVisible(hwnd) }.as_bool());
    let window = HiddenWindow(hwnd);
    let graphics = GraphicsDevice::new()?;
    let mut surface = Surface::new(graphics, hwnd, 981, 759, 1.5)?;

    let mut model = fixture_model();
    let now = model.view_changed + Duration::from_secs(1);
    model.water_change = now - Duration::from_secs(1);
    model.shelf_change = now - Duration::from_secs(1);
    model.list_changed = now - Duration::from_secs(1);
    let output = std::env::current_dir()?.join("target/native-render");
    std::fs::create_dir_all(output.join("notch"))?;
    std::fs::create_dir_all(output.join("settings"))?;

    for (name, view, scale) in [
        ("music-100.png", NotchView::Music, 1.0),
        ("todo-150.png", NotchView::Todo, 1.5),
        ("focus-100.png", NotchView::Pomodoro, 1.0),
        ("water-150.png", NotchView::Water, 1.5),
        ("shelf-100.png", NotchView::Shelf, 1.0),
        ("resting-150.png", NotchView::Resting, 1.5),
    ] {
        model.view = view;
        model.nav = view != NotchView::Resting;
        if view == NotchView::Resting {
            let scene = views::notch_scene(&model, view, now);
            assert!(
                scene.paint.iter().any(
                    |paint| matches!(paint, Paint::Icon(_, glyph, _) if glyph == "sleep-eyes")
                )
            );
            assert!(
                !scene
                    .paint
                    .iter()
                    .any(|paint| matches!(paint, Paint::Icon(_, glyph, _) if glyph == "eye-halo"))
            );
        }
        render_notch(
            &mut surface,
            &model,
            view,
            now,
            scale,
            output.join("notch").join(name),
        )?;
    }

    model.view = NotchView::Resting;
    model.nav = false;
    model.motion = Motion::new(NotchView::Resting);
    model.dozing = false;
    model.headphones = None;
    for (name, style, age_ms) in [
        ("mascot-wake-gentle-350ms-100.png", WakeStyle::Gentle, 350),
        ("mascot-wake-one-eye-500ms-100.png", WakeStyle::OneEye, 500),
        (
            "mascot-wake-surprise-220ms-100.png",
            WakeStyle::Surprise,
            220,
        ),
    ] {
        model.wake_style = style;
        model.wake = Some(now - Duration::from_millis(age_ms));
        let scene = views::notch_scene(&model, NotchView::Resting, now);
        if matches!(style, WakeStyle::Gentle | WakeStyle::OneEye) {
            assert!(
                scene
                    .paint
                    .iter()
                    .any(|paint| matches!(paint, Paint::Icon(_, glyph, _) if glyph == "sleep-eye"))
            );
        }
        if matches!(style, WakeStyle::OneEye | WakeStyle::Surprise) {
            assert!(
                scene
                    .paint
                    .iter()
                    .any(|paint| matches!(paint, Paint::Round(_, _, color) if *color == 0xffffff))
            );
        }
        render_notch(
            &mut surface,
            &model,
            NotchView::Resting,
            now,
            1.0,
            output.join("notch").join(name),
        )?;
    }

    model.wake = None;
    model.headphones = Some(HeadphoneStatus {
        name: "Studio headphones".into(),
        battery_percent: Some(82),
    });
    model.headphones_started = Some(now - Duration::from_millis(1400));
    let scene = views::notch_scene(&model, NotchView::Resting, now);
    assert_eq!(
        scene
            .paint
            .iter()
            .filter(|paint| matches!(paint, Paint::Icon(_, glyph, _) if glyph == "mascot-paw"))
            .count(),
        2,
        "mascot uses both paws while placing headphones"
    );
    render_notch(
        &mut surface,
        &model,
        NotchView::Resting,
        now,
        1.0,
        output
            .join("notch")
            .join("mascot-headphones-posed-1400ms-100.png"),
    )?;

    model.headphones_started = Some(now - Duration::from_millis(2600));
    let scene = views::notch_scene(&model, NotchView::Resting, now);
    assert!(
        scene
            .paint
            .iter()
            .any(|paint| matches!(paint, Paint::Icon(_, glyph, _) if glyph == "mascot-headphones"))
    );
    assert!(
        scene
            .paint
            .iter()
            .any(|paint| matches!(paint, Paint::Icon(_, glyph, _) if glyph == "wake-eyes")),
        "late headphone phase shows the mascot smiling"
    );
    assert!(
        scene
            .paint
            .iter()
            .any(|paint| matches!(paint, Paint::Text(_, text, _, _, _, _) if text == "♪")),
        "late headphone phase shows a music note"
    );
    assert!(
        !scene
            .paint
            .iter()
            .any(|paint| matches!(paint, Paint::Icon(_, glyph, _) if glyph == "mascot-paw"))
    );
    render_notch(
        &mut surface,
        &model,
        NotchView::Resting,
        now,
        1.0,
        output
            .join("notch")
            .join("mascot-headphones-on-2600ms-100.png"),
    )?;

    model.headphones = None;
    model.headphones_started = None;
    let gaze_started = now - Duration::from_millis(180);
    assert!(model.observe_mascot_cursor(
        Some((SURFACE_WIDTH / 2., 75.)),
        MASCOT_PROXIMITY_DIPS,
        gaze_started,
    ));
    assert!(model.cursor_near);
    let moved_at = now - Duration::from_millis(80);
    assert!(model.observe_mascot_cursor(
        Some((SURFACE_WIDTH / 2. + 12., 78.)),
        MASCOT_PROXIMITY_DIPS,
        moved_at,
    ));
    assert!(
        model.stalk_amount(now) > 0.35,
        "moving the nearby cursor must not restart the stalking transition"
    );
    assert!(model.gaze(now).1 > 0.5);
    render_notch(
        &mut surface,
        &model,
        NotchView::Resting,
        now,
        1.0,
        output.join("notch").join("mascot-gaze-cursor-100.png"),
    )?;
    let stalk_at = gaze_started + Duration::from_millis(420);
    let scene = views::notch_scene(&model, NotchView::Resting, stalk_at);
    for glyph in ["stalk-eye-left", "stalk-eye-right"] {
        assert!(
            scene
                .paint
                .iter()
                .any(|paint| matches!(paint, Paint::Icon(_, name, _) if name == glyph)),
            "fully stalking mascot renders its own {glyph}"
        );
    }
    render_notch(
        &mut surface,
        &model,
        NotchView::Resting,
        stalk_at,
        1.0,
        output
            .join("notch")
            .join("mascot-stalk-cursor-420ms-100.png"),
    )?;

    model.view = NotchView::Music;
    model.nav = false;
    model.volume = 0.63;
    model.volume_shown = true;
    model.volume_from = 0.;
    model.volume_changed = Instant::now() - Duration::from_secs(1);
    assert_volume_hit_layout(&model, now);
    render_notch(
        &mut surface,
        &model,
        NotchView::Music,
        now,
        1.0,
        output.join("notch").join("music-volume-100.png"),
    )?;

    model.view = NotchView::Shelf;
    model.settings.desktop.fast_animations = false;
    model.nav = true;
    model.nav_from = 0.;
    model.nav_changed = now - Duration::from_millis(130);
    render_notch(
        &mut surface,
        &model,
        NotchView::Shelf,
        now,
        1.0,
        output.join("notch").join("navigation-half-130.png"),
    )?;

    model.nav = false;
    model.nav_from = 0.;
    model.nav_changed = now - Duration::from_secs(1);
    model.previous_shelf = model.shelf.clone();
    model.shelf.clear();
    model.shelf_change = now - Duration::from_millis(140);
    render_notch(
        &mut surface,
        &model,
        NotchView::Shelf,
        now,
        1.0,
        output.join("notch").join("shelf-clearing-140.png"),
    )?;

    model.view = NotchView::Water;
    model.nav = false;
    model.water_settings = true;
    model.settings.water.goal_ml = 1500;
    render_notch(
        &mut surface,
        &model,
        NotchView::Water,
        now,
        1.0,
        output.join("notch").join("water-settings-inline-100.png"),
    )?;
    model.water_settings = false;
    model.settings.water.goal_ml = 2000;

    model.view = NotchView::Todo;
    model.nav = false;
    model.calendar_open = true;
    model.selected_date = "2026-11-03".into();
    model.settings.desktop.calendar_starts_monday = true;
    model.calendar = calendar_grid(model.clock.date_naive(), 0, true, false);
    render_notch(
        &mut surface,
        &model,
        NotchView::Todo,
        now,
        1.5,
        output.join("notch").join("todo-calendar-month-150.png"),
    )?;
    model.settings.desktop.calendar_week_view = true;
    model.calendar = calendar_grid(model.clock.date_naive(), 0, true, true);
    render_notch(
        &mut surface,
        &model,
        NotchView::Todo,
        now,
        1.5,
        output.join("notch").join("todo-calendar-week-150.png"),
    )?;

    model.calendar_open = false;
    model.settings.desktop.calendar_week_view = false;
    model.settings.desktop.calendar_starts_monday = false;
    model.calendar = calendar_grid(model.clock.date_naive(), 0, false, false);
    model.selected_date = "2026-11-02".into();
    model.draft = Some(crate::adapters::native::model::TaskDraft {
        id: None,
        title: "Color fixture".into(),
        date: "2026-11-02".into(),
        time: String::new(),
        daily: true,
        color: 3,
        saving: false,
    });
    model.palette = false;
    render_notch(
        &mut surface,
        &model,
        NotchView::Todo,
        now,
        1.0,
        output
            .join("notch")
            .join("todo-color-selected-editor-100.png"),
    )?;
    model.draft = None;
    model.palette = false;
    model.selected_date = "2026-11-03".into();

    model.view = NotchView::Hud;
    model.focus_notice = Some((FocusPhase::Focus, 5 * 60));
    render_notch(
        &mut surface,
        &model,
        NotchView::Hud,
        now,
        1.0,
        output.join("notch").join("focus-hud-100.png"),
    )?;
    model.focus_notice = None;

    model.power = PowerStatus {
        has_battery: true,
        is_plugged_in: true,
        percent: 68,
    };
    model.headphones = Some(HeadphoneStatus {
        name: "Studio headphones".into(),
        battery_percent: Some(82),
    });
    for (name, view) in [
        ("charging-peek-100.png", NotchView::ChargingPeek),
        ("headphones-peek-100.png", NotchView::HeadphonesPeek),
        ("charging-100.png", NotchView::Charging),
        ("headphones-100.png", NotchView::Headphones),
    ] {
        model.view = view;
        model.nav = false;
        render_notch(
            &mut surface,
            &model,
            view,
            now,
            1.0,
            output.join("notch").join(name),
        )?;
    }

    assert_focus_spinbox_hit_layout(&model, now);
    for (name, section) in [
        ("general-en-100.png", 0),
        ("calendar-en-100.png", 5),
        ("focus-en-100.png", 6),
        ("water-en-100.png", 7),
    ] {
        model.settings.language = LanguagePreference::English;
        if section == 5 {
            model.clock = Local.with_ymd_and_hms(2026, 11, 2, 11, 30, 0).unwrap();
            model.calendar = calendar_grid(model.clock.date_naive(), 0, false, false);
            model.selected_date.clear();
            model.settings.desktop.calendar_week_view = true;
        }
        render_settings(
            &mut surface,
            &model,
            section,
            now,
            1.0,
            output.join("settings").join(name),
        )?;
        if section == 5 {
            model.clock = Local.with_ymd_and_hms(2026, 11, 3, 10, 30, 0).unwrap();
            model.calendar = calendar_grid(model.clock.date_naive(), 0, false, false);
            model.selected_date = "2026-11-03".into();
            model.settings.desktop.calendar_week_view = false;
        }
    }

    crate::i18n::select_language(LanguagePreference::Spanish, None);
    model.settings.language = LanguagePreference::Spanish;
    render_settings(
        &mut surface,
        &model,
        2,
        now,
        1.5,
        output.join("settings").join("appearance-es-150.png"),
    )?;

    crate::i18n::select_language(LanguagePreference::English, None);
    model.settings.language = LanguagePreference::English;
    render_mouse_move_settings(
        &mut surface,
        &model,
        now,
        output.join("settings").join("mouse-move-100.png"),
    )?;

    drop(surface);
    drop(window);
    Ok(())
}

#[test]
fn lumi_branding_settings_and_about_render_in_a_hidden_window()
-> std::result::Result<(), Box<dyn Error>> {
    let _locale = LocaleGuard(crate::i18n::active_locale());
    crate::i18n::select_language(LanguagePreference::English, None);
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()? };
    let _com = ComApartment;

    let module = unsafe { GetModuleHandleW(None)? };
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("Lumi branding render fixture"),
            WS_POPUP,
            0,
            0,
            654,
            506,
            None,
            None,
            Some(module.into()),
            None,
        )?
    };
    assert!(!unsafe { IsWindowVisible(hwnd) }.as_bool());
    let window = HiddenWindow(hwnd);
    let graphics = GraphicsDevice::new()?;
    let mut surface = Surface::new(graphics, hwnd, 654, 506, 1.0)?;
    let model = fixture_model();
    let now = Instant::now();
    let output = std::env::current_dir()?.join("target/native-render/settings");
    std::fs::create_dir_all(&output)?;

    for (section, name) in [
        (0, "branding-general-100.png"),
        (10, "branding-about-100.png"),
    ] {
        render_settings(&mut surface, &model, section, now, 1.0, output.join(name))?;
    }

    drop(surface);
    drop(window);
    Ok(())
}

fn fixture_model() -> UiModel {
    let mut settings = AppSettings::default();
    settings.language = LanguagePreference::English;
    settings.water.enabled = true;
    settings.water.goal_ml = 2000;
    settings.water.bottle_ml = 500;
    let mut model = UiModel::new(settings);
    model.clock = Local.with_ymd_and_hms(2026, 11, 3, 10, 30, 0).unwrap();
    let date = model.clock.date_naive();
    let yesterday = date.pred_opt().unwrap();
    let tomorrow = date.succ_opt().unwrap();
    let later = tomorrow.succ_opt().unwrap();
    let created_at = Utc.with_ymd_and_hms(2026, 11, 3, 17, 30, 0).unwrap();
    model.tasks = vec![
        task(
            1,
            "Write a short summary",
            date,
            Some((9, 0)),
            TaskColor::Purple,
            false,
            created_at,
        ),
        task(
            2,
            "Review tomorrow's plan",
            date,
            Some((14, 0)),
            TaskColor::Green,
            false,
            created_at,
        ),
        task(
            3,
            "Water the plants",
            yesterday,
            Some((16, 30)),
            TaskColor::Orange,
            true,
            created_at,
        ),
        task(
            4,
            "Update the project brief",
            tomorrow,
            None,
            TaskColor::Blue,
            true,
            created_at,
        ),
        task(
            5,
            "Prepare the weekly note",
            later,
            None,
            TaskColor::Blue,
            false,
            created_at,
        ),
    ];
    model.progress = TodoProgress::of(&model.tasks);
    model.list_expanded = true;
    model.calendar = calendar_grid(date, 0, false, false);
    model.selected_date = date.to_string();
    model.utility = Some(UtilityState {
        phase: FocusPhase::Focus,
        focus_seconds: 17 * 60 + 42,
        focus_running: true,
        focus_total_seconds: 25 * 60,
        task_id: None,
        task_title: String::new(),
        task_seconds: 0,
        can_snooze: false,
    });
    model.track = Some(Track {
        id: "fixture-track".into(),
        title: "North Star at Dusk".into(),
        artist: "The Example Quartet".into(),
        album: "Small Hours".into(),
        duration: Duration::from_secs(218),
        artwork_url: None,
    });
    model.playing = true;
    model.connection = MediaConnection::Connected;
    model.position = Duration::from_secs(73);
    model.duration = Duration::from_secs(218);
    model.artwork = Some(Artwork::from_rgba(
        vec![
            20, 52, 82, 255, 60, 113, 142, 255, 13, 30, 49, 255, 150, 116, 73, 255,
        ],
        2,
        2,
    ));
    let mut hourly_ml = [0; 24];
    hourly_ml[8] = 250;
    hourly_ml[10] = 300;
    hourly_ml[12] = 200;
    model.water = Some(WaterState {
        enabled: true,
        consumed_ml: 750,
        goal_ml: 2000,
        bottle_ml: 500,
        hourly_ml,
        active_from_minute: 480,
        active_until_minute: 1020,
        next_at: Some(model.clock + chrono::Duration::minutes(35)),
    });
    model.previous_water = 750;
    model.previous_hours = hourly_ml;
    model.water_graph = true;
    model.shelf = vec![
        shelf_entry(10, "Design notes", "A short fixture note for the shelf."),
        shelf_entry(11, "Focus list", "Draft, review, then take a break."),
    ];
    model.mascot = true;
    model.dozing = true;
    model
}

fn task(
    id: u128,
    title: &str,
    date: chrono::NaiveDate,
    time: Option<(u32, u32)>,
    color: TaskColor,
    is_done: bool,
    created_at: chrono::DateTime<Utc>,
) -> TodoTask {
    TodoTask {
        id: Uuid::from_u128(id),
        title: title.into(),
        is_done,
        created_at,
        completed_at: is_done.then_some(created_at),
        schedule: TaskSchedule {
            date,
            time: time.map(|(hour, minute)| NaiveTime::from_hms_opt(hour, minute, 0).unwrap()),
            daily: false,
        },
        color,
    }
}

fn shelf_entry(id: u128, name: &str, text: &str) -> ShelfEntry {
    ShelfEntry {
        item: ShelfItem {
            id: Uuid::from_u128(id),
            content: ShelfContent::Text(text.into()),
            display_name: name.into(),
            size_bytes: text.len() as u64,
            is_directory: false,
            kind: ShelfItemKind::Text,
            added_at: Utc.with_ymd_and_hms(2026, 11, 3, 17, 30, 0).unwrap(),
        },
        badge: "TXT".into(),
        size_label: "fixture".into(),
        thumbnail: None,
    }
}

fn assert_volume_hit_layout(model: &UiModel, now: Instant) {
    let scene = views::notch_scene(model, NotchView::Music, now);
    let hover = scene
        .hits
        .iter()
        .find(|hit| matches!(&hit.action, Action::ShowVolume))
        .expect("Music exposes its volume hover area");
    assert_eq!((hover.rect.w, hover.rect.h), (102., 26.));

    let slider = scene
        .hits
        .iter()
        .find(|hit| matches!(&hit.action, Action::Volume))
        .expect("visible volume exposes its slider hit area");
    assert_eq!((slider.rect.w, slider.rect.h), (65., 24.));
}

fn assert_focus_spinbox_hit_layout(model: &UiModel, now: Instant) {
    const SETTINGS_WIDTH: f32 = 654.;
    const SETTINGS_HEIGHT: f32 = 506.;
    let mut ui = settings::SettingsUi::new(&model.settings, true);
    ui.section = 6;
    ui.changed = now - Duration::from_secs(1);
    let (scene, _) =
        settings::settings_scene(model, &ui, &[], SETTINGS_WIDTH, SETTINGS_HEIGHT, now);
    assert_scene_fits(&scene, SETTINGS_WIDTH, SETTINGS_HEIGHT);

    let input = scene
        .inputs
        .iter()
        .find(|input| input.key == Field::Preference(Preference::FocusMinutes))
        .expect("Focus settings expose the duration field");
    assert_eq!((input.rect.w, input.rect.h), (50., 20.));
    let control = Rect::new(input.rect.x - 12., input.rect.y - 6., 128., 32.);
    let same = |left: f32, right: f32| (left - right).abs() < 0.01;
    let background = scene.paint.iter().any(|paint| match paint {
        Paint::Round(rect, radius, color) => {
            *color == 0x333336
                && same(*radius, 4.)
                && same(rect.x, control.x)
                && same(rect.y, control.y)
                && same(rect.w, control.w)
                && same(rect.h, control.h)
        }
        _ => false,
    });
    let border = scene.paint.iter().any(|paint| match paint {
        Paint::Border(rect, radius, color, width) => {
            *color == 0x444446
                && same(*radius, 4.)
                && same(*width, 1.)
                && same(rect.x, control.x)
                && same(rect.y, control.y)
                && same(rect.w, control.w)
                && same(rect.h, control.h)
        }
        _ => false,
    });
    assert!(
        background && border,
        "Focus spinbox is 128x32 with its border"
    );

    let spin_hits: Vec<_> = scene
        .hits
        .iter()
        .filter(|hit| matches!(&hit.action, Action::Preference(Preference::FocusMinutes, _)))
        .collect();
    assert_eq!(spin_hits.len(), 2, "Focus spinbox has two adjustment hits");
    for (index, hit) in spin_hits.iter().enumerate() {
        assert_eq!((hit.rect.w, hit.rect.h), (28., 24.));
        assert!(same(hit.rect.x, control.x + 66. + index as f32 * 32.));
        assert!(same(hit.rect.y, control.y + 4.));
    }
}

fn render_notch(
    surface: &mut Surface,
    model: &UiModel,
    view: NotchView,
    now: Instant,
    scale: f32,
    path: impl AsRef<Path>,
) -> std::result::Result<(), Box<dyn Error>> {
    let width = (SURFACE_WIDTH * scale).round() as u32;
    let height = (SURFACE_HEIGHT * scale).round() as u32;
    surface.resize(width, height, scale)?;
    let scene = views::notch_scene(model, view, now);
    assert_scene_fits(&scene, SURFACE_WIDTH, SURFACE_HEIGHT);

    surface.begin(None);
    surface.shell(Shell::for_view(view, model.nav))?;
    surface.clip_shell()?;
    surface.paint(&scene.paint, 1., (0., 0.))?;
    surface.unclip_shell();
    let image = readback(surface)?;
    surface.end()?;

    assert_eq!((image.width(), image.height()), (width, height));
    assert_eq!(image.get_pixel(0, 0).0[3], 0);
    let center = image.get_pixel(
        width / 2,
        (Shell::for_view(view, model.nav).height * scale / 2.) as u32,
    );
    assert!(center.0[3] > 0, "the centered notch shell should render");
    assert!(image.pixels().filter(|pixel| pixel.0[3] > 0).count() > 1_000);
    image.save(path)?;
    Ok(())
}

fn render_settings(
    surface: &mut Surface,
    model: &UiModel,
    section: usize,
    now: Instant,
    scale: f32,
    path: impl AsRef<Path>,
) -> std::result::Result<(), Box<dyn Error>> {
    const LOGICAL_WIDTH: f32 = 654.;
    const LOGICAL_HEIGHT: f32 = 506.;
    let width = (LOGICAL_WIDTH * scale).round() as u32;
    let height = (LOGICAL_HEIGHT * scale).round() as u32;
    surface.resize(width, height, scale)?;
    let mut ui = settings::SettingsUi::new(&model.settings, true);
    ui.section = section;
    ui.changed = now - Duration::from_secs(1);
    let (scene, _) = settings::settings_scene(model, &ui, &[], LOGICAL_WIDTH, LOGICAL_HEIGHT, now);
    assert_scene_fits(&scene, LOGICAL_WIDTH, LOGICAL_HEIGHT);

    surface.begin(Some(0x1d1d1f));
    surface.paint(&scene.paint, 1., (0., 0.))?;
    let image = readback(surface)?;
    surface.end()?;

    assert_eq!((image.width(), image.height()), (width, height));
    assert!(image.pixels().all(|pixel| pixel.0[3] == 255));
    image.save(path)?;
    Ok(())
}

fn render_mouse_move_settings(
    surface: &mut Surface,
    model: &UiModel,
    now: Instant,
    path: impl AsRef<Path>,
) -> std::result::Result<(), Box<dyn Error>> {
    const WIDTH: f32 = 654.;
    const HEIGHT: f32 = 360.;
    surface.resize(WIDTH as u32, HEIGHT as u32, 1.0)?;
    let mut ui = settings::SettingsUi::new(&model.settings, true);
    ui.section = 11;
    ui.changed = now - Duration::from_secs(1);
    let (scene, _) = settings::settings_scene(model, &ui, &[], WIDTH, HEIGHT, now);
    assert_scene_fits(&scene, WIDTH, HEIGHT);

    let mouse_move = scene
        .hits
        .iter()
        .find(|hit| hit.action == Action::Section(11))
        .expect("mouse move section is in the settings sidebar");
    let about = scene
        .hits
        .iter()
        .find(|hit| hit.action == Action::Section(10))
        .expect("About remains in the settings sidebar");
    assert!(
        mouse_move.rect.y + mouse_move.rect.h <= about.rect.y
            || about.rect.y + about.rect.h <= mouse_move.rect.y,
        "Mouse Move and About rows do not overlap at a short window height"
    );
    assert_eq!(
        scene
            .hits
            .iter()
            .filter(|hit| matches!(&hit.action, Action::Preference(Preference::MouseMove, _)))
            .count(),
        1,
        "Mouse Move settings contains one boolean toggle"
    );
    let seconds = crate::application::mouse_mover::MOUSE_MOVE_INTERVAL
        .as_secs()
        .to_string();
    let helper = crate::i18n::translate_with("desktopUi.mouse-move-help", &[("seconds", &seconds)]);
    assert!(
        scene
            .paint
            .iter()
            .any(|paint| matches!(paint, Paint::Text(_, text, _, _, _, _) if text == &helper))
    );
    assert!(
        scene.inputs.is_empty(),
        "Mouse Move settings adds no text inputs"
    );

    surface.begin(Some(0x1d1d1f));
    surface.paint(&scene.paint, 1., (0., 0.))?;
    let image = readback(surface)?;
    surface.end()?;
    assert_eq!(
        (image.width(), image.height()),
        (WIDTH as u32, HEIGHT as u32)
    );
    assert!(image.pixels().all(|pixel| pixel.0[3] == 255));
    image.save(path)?;
    Ok(())
}

fn readback(surface: &Surface) -> std::result::Result<RgbaImage, Box<dyn Error>> {
    let width = surface.width;
    let height = surface.height;
    let description = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_STAGING,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        ..Default::default()
    };
    let mut staging = None;
    unsafe {
        surface
            .device
            .d3d
            .CreateTexture2D(&description, None, Some(&mut staging))?
    };
    let staging = staging.expect("D3D11 creates the requested staging texture");
    let source: ID3D11Texture2D = unsafe { surface.swap.GetBuffer(0)? };
    unsafe { surface.context.Flush(None, None)? };
    let context = unsafe { surface.device.d3d.GetImmediateContext()? };
    unsafe {
        context.CopyResource(&staging, &source);
        context.Flush();
    }
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe { context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))? };
    assert!(!mapped.pData.is_null());
    let row_pitch = mapped.RowPitch as usize;
    assert!(row_pitch >= width as usize * 4);
    let bytes = unsafe {
        std::slice::from_raw_parts(mapped.pData.cast::<u8>(), row_pitch * height as usize)
    };
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for row in bytes.chunks_exact(row_pitch).take(height as usize) {
        for bgra in row.chunks_exact(4).take(width as usize) {
            let alpha = u16::from(bgra[3]);
            let straight = |channel: u8| {
                if alpha == 0 {
                    0
                } else {
                    ((u16::from(channel) * 255 + alpha / 2) / alpha).min(255) as u8
                }
            };
            rgba.extend_from_slice(&[
                straight(bgra[2]),
                straight(bgra[1]),
                straight(bgra[0]),
                bgra[3],
            ]);
        }
    }
    unsafe { context.Unmap(&staging, 0) };
    Ok(RgbaImage::from_raw(width, height, rgba)
        .expect("readback contains one RGBA pixel per texel"))
}

fn assert_scene_fits(scene: &Scene, width: f32, height: f32) {
    let contains = |r: Rect| {
        r.x.is_finite()
            && r.y.is_finite()
            && r.w.is_finite()
            && r.h.is_finite()
            && r.w >= 0.
            && r.h >= 0.
            && r.x >= 0.
            && r.y >= 0.
            && r.x + r.w <= width + 0.01
            && r.y + r.h <= height + 0.01
    };
    let mut clips = Vec::new();
    let clipped_rect_fits = |r: Rect, clips: &[Rect]| {
        if let Some(clip) = clips.last() {
            let visible = r.intersect(*clip);
            visible.w <= 0. || visible.h <= 0. || contains(visible)
        } else {
            contains(r)
        }
    };
    assert!(!scene.paint.is_empty());
    for command in &scene.paint {
        match command {
            Paint::Round(r, _, _)
            | Paint::Border(r, _, _, _)
            | Paint::Text(r, _, _, _, _, _)
            | Paint::Icon(r, _, _)
            | Paint::TextRun(r, _, _)
            | Paint::AlphaRound(r, _, _, _)
            | Paint::AlphaBorder(r, _, _, _, _)
            | Paint::Strike(r, _, _, _)
            | Paint::Paragraph(r, _, _, _)
            | Paint::RoundedBitmap(r, _, _, _, _)
            | Paint::ButtonContent(r, _, _)
            | Paint::Ring(r, _, _, _, _)
            | Paint::ProgressRing(r, _, _, _)
            | Paint::GradientRound(r, _, _, _)
            | Paint::Bitmap(r, _, _, _) => assert!(
                clipped_rect_fits(*r, &clips),
                "paint rectangle exceeds its visible canvas: {r:?}"
            ),
            Paint::Clip(r) => {
                let effective = clips.last().map_or(*r, |current| current.intersect(*r));
                assert!(contains(effective), "clip exceeds its canvas: {r:?}");
                clips.push(effective);
            }
            Paint::Unclip => assert!(clips.pop().is_some(), "unbalanced scene clip"),
            Paint::Line(x, y, x2, y2, _, _) => {
                let bounds = Rect::new(
                    (*x).min(*x2),
                    (*y).min(*y2),
                    (*x - *x2).abs(),
                    (*y - *y2).abs(),
                );
                assert!(
                    clipped_rect_fits(bounds, &clips),
                    "line exceeds its visible canvas"
                );
            }
            Paint::Opacity(value) => assert!((0. ..=1.).contains(value)),
        }
    }
    assert!(clips.is_empty(), "scene left an active clip");
    assert!(scene.hits.iter().all(|hit| contains(hit.rect)));
    assert!(scene.inputs.iter().all(|input| contains(input.rect)));
}
