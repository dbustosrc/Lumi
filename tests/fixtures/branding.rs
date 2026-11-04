use super::*;
use crate::adapters::native::{
    model::UiModel,
    scene::Paint,
    settings::{self, SettingsUi},
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

    assert_eq!(env!("CARGO_PKG_VERSION"), "1.0.0");
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
    assert!(text.contains(&"GPL-2.0-only"));
}
