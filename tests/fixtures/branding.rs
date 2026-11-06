use super::*;
use crate::adapters::native::{
    model::UiModel,
    scene::{Action, Field, Paint},
    settings::{self, NoticesPage, SettingsUi},
};
use crate::application::settings::AppSettings;
use crate::i18n::{Locale, translate_for};
use std::time::{Duration, Instant};

#[test]
fn embedded_icons_decode_to_cached_rgba_bitmaps() {
    let app = app_icon();
    assert_eq!((app.width, app.height), (256, 256));
    assert_eq!(app.rgba.len(), (256 * 256 * 4) as usize);
    assert!(app.rgba.chunks_exact(4).any(|pixel| pixel[3] != 0));
    assert!(std::ptr::eq(app, app_icon()));
    assert!(std::sync::Arc::ptr_eq(&app.rgba, &app_icon().rgba));

    let tray = tray_icon();
    assert_eq!((tray.width, tray.height), (32, 32));
    assert_eq!(tray.rgba.len(), (32 * 32 * 4) as usize);
    assert!(tray.rgba.chunks_exact(4).any(|pixel| pixel[3] != 0));
    assert!(std::ptr::eq(tray, tray_icon()));
    assert!(std::sync::Arc::ptr_eq(&tray.rgba, &tray_icon().rgba));
}

#[test]
fn branding_catalogs_use_lumi_and_mimo_in_english_and_spanish() {
    for locale in [Locale::English, Locale::Spanish] {
        assert_eq!(translate_for(locale, "tray.tooltip"), "Lumi");
        assert!(translate_for(locale, "settings.window.title").starts_with("Lumi — "));
        assert_eq!(
            translate_for(locale, "settings.general.mascotLabel"),
            "Mimo"
        );

        let about = translate_for(locale, "desktopUi.about-mascot");
        assert!(about.contains("Mimo"));
        assert!(about.contains("Lumi"));
    }

    assert_eq!(env!("CARGO_PKG_VERSION"), "1.1.0");
}

#[test]
fn branding_general_has_no_logo_and_about_shows_app_identity() {
    let model = UiModel::new(AppSettings::default());
    let now = Instant::now();
    let icon = app_icon();

    let mut general_ui = SettingsUi::new(&model.settings, true);
    general_ui.changed = now - Duration::from_secs(1);
    let (general, _) = settings::settings_scene(&model, &general_ui, &[], 800., 600., now);
    assert!(
        !general
            .paint
            .iter()
            .any(|paint| matches!(paint, Paint::Bitmap(..))),
        "general settings should not show the brand bitmap"
    );

    let mut about_ui = SettingsUi::new(&model.settings, true);
    about_ui.section = 10;
    about_ui.changed = now - Duration::from_secs(1);
    let (about, _) = settings::settings_scene(&model, &about_ui, &[], 800., 600., now);
    assert!(about.paint.iter().any(|paint| match paint {
        Paint::Bitmap(rect, width, height, pixels) => {
            rect.w == 88.
                && rect.h == 88.
                && *width == icon.width
                && *height == icon.height
                && std::sync::Arc::ptr_eq(pixels, &icon.rgba)
        }
        _ => false,
    }));

    let text = about
        .paint
        .iter()
        .filter_map(|paint| match paint {
            Paint::Text(_, text, _, _, _, _) => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(text.iter().any(|value| value.contains("David R. Bustos")));
    assert!(text.contains(&"https://github.com/dbustosrc/Lumi"));
    assert!(text.contains(&"GPL-3.0-only"));
}

#[test]
fn about_opens_embedded_third_party_notice_index_and_full_component_text() {
    let model = UiModel::new(AppSettings::default());
    let now = Instant::now();

    let mut about_ui = SettingsUi::new(&model.settings, true);
    about_ui.section = 10;
    about_ui.changed = now - Duration::from_secs(1);
    let (about, _) = settings::settings_scene(&model, &about_ui, &[], 800., 600., now);
    assert!(
        about
            .hits
            .iter()
            .any(|hit| hit.action == Action::OpenNotices)
    );

    let mut index_ui = SettingsUi::new(&model.settings, true);
    index_ui.section = 10;
    index_ui.notices = Some(NoticesPage::Index);
    index_ui.changed = now - Duration::from_secs(1);
    let (index_scene, _) = settings::settings_scene(&model, &index_ui, &[], 800., 600., now);
    let first_notice = index_scene
        .hits
        .iter()
        .find(|hit| matches!(hit.action, Action::OpenNotice(_)))
        .expect("the embedded notice index shows a component");
    assert!(matches!(first_notice.action, Action::OpenNotice(0)));
    assert!(first_notice.label.starts_with("adler2 2.0.1 · "));

    let component_index = include_str!("../../third-party-licenses/README.md")
        .lines()
        .filter(|line| {
            line.starts_with('|') && !line.starts_with("| Package") && !line.starts_with("| ---")
        })
        .position(|line| line.starts_with("| regex-syntax |"))
        .expect("the locked dependency index includes regex-syntax");
    let expected_nested_text = include_str!(
        "../../third-party-licenses/regex-syntax-0.8.11/src/unicode_tables/LICENSE-UNICODE"
    );
    let mut component_ui = SettingsUi::new(&model.settings, true);
    component_ui.section = 10;
    component_ui.notices = Some(NoticesPage::Component {
        index: component_index,
        index_scroll: 0.,
    });
    component_ui.changed = now - Duration::from_secs(1);
    let (component_scene, _) =
        settings::settings_scene(&model, &component_ui, &[], 800., 600., now);
    assert!(
        component_scene
            .hits
            .iter()
            .any(|hit| hit.action == Action::BackNotices)
    );
    let notice = component_scene
        .inputs
        .iter()
        .find(|input| input.key == Field::ThirdPartyNotice(component_index))
        .expect("the detail page exposes the embedded notice text");
    let embedded_text = notice.text.replace("\r\n", "\n").replace('\\', "/");
    let expected_text = expected_nested_text.replace("\r\n", "\n");
    assert!(notice.multiline);
    assert!(embedded_text.contains("src/unicode_tables/LICENSE-UNICODE"));
    assert!(embedded_text.contains("Copyright © 1991-2018 Unicode, Inc."));
    assert!(embedded_text.contains(expected_text.trim_end()));
}
