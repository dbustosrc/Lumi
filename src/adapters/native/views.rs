//! Scene construction shares every control rectangle with native input and UIA.
use super::model::*;
use super::scene::*;
use crate::application::events::UserIntent;
use crate::application::presenter::{LyricsAvailability, VoiceState};
use crate::application::reminders::FocusPhase;
use crate::domain::media::MediaCommand;
use crate::domain::notch::{NotchPage, NotchView};
use chrono::{Datelike, Timelike};
use std::time::Instant;

pub const MUSIC: &str = "\u{e189}";
pub const TIMER: &str = "\u{e916}";
pub const DROP: &str = "water-drop";
pub const SHELF: &str = "\u{e7c3}";
pub const TODO: &str = "\u{e9d5}";
pub const PLAY: &str = "\u{e768}";
pub const PAUSE: &str = "\u{e769}";
pub const PLUS: &str = "\u{e710}";
pub const EDIT: &str = "\u{e70f}";
pub const MIC: &str = "\u{e720}";
const WHITE: u32 = 0xffffff;
const DIM: u32 = 0x8a8a8a;
const GREEN: u32 = 0x32cd65;
const BLUE: u32 = 0x29a8ff;
const RED: u32 = 0xff4b55;
fn full(key: &str) -> String {
    crate::i18n::translate(key)
}
fn icon_button(s: &mut Scene, r: Rect, glyph: &str, action: Action, label: &str, color: u32) {
    s.round(r, r.h / 2., color);
    let size = match glyph {
        "\u{e76b}" | "\u{e76c}" => 9.,
        "\u{e787}" => 15.,
        "\u{e777}" => 14.,
        "\u{e711}" => 8.,
        _ => 12.,
    };
    s.icon(
        Rect::new(r.x + (r.w - size) / 2., r.y + (r.h - size) / 2., size, size),
        glyph,
        WHITE,
    );
    s.hit(r, action, label);
}
fn title(s: &mut Scene, b: Rect, text: &str) {
    s.text(
        Rect::new(b.x + 15., 10., 111., 18.),
        text,
        14.,
        WHITE,
        true,
        false,
    );
}
fn percent(s: &mut Scene, r: Rect, value: f32, color: u32) {
    s.round(r, r.h / 2., 0x242426);
    s.round(
        Rect::new(r.x, r.y, r.w * value.clamp(0., 1.), r.h),
        r.h / 2.,
        color,
    );
}
fn toggle(s: &mut Scene, r: Rect, checked: bool, action: Action, label: &str, color: u32) {
    s.round(r, r.h / 2., if checked { color } else { 0x414146 });
    s.round(
        Rect::new(
            r.x + if checked { r.w - r.h + 3. } else { 3. },
            r.y + 3.,
            r.h - 6.,
            r.h - 6.,
        ),
        (r.h - 6.) / 2.,
        WHITE,
    );
    s.hit(r, action, label);
}

pub fn notch_scene(m: &UiModel, view: NotchView, now: Instant) -> Scene {
    let mut s = Scene::default();
    let b = Shell::for_view(view, m.nav).body();
    match view {
        NotchView::Minimal => {}
        NotchView::Resting => mascot(&mut s, m, b, now),
        NotchView::Closed => {
            if m.mascot
                && (m.wake_progress(now).is_some()
                    || m.headphones_progress(now).is_some()
                    || m.stalk_amount(now) > 0. && !m.focus_running()
                    || m.bell_progress(now).is_some() && !m.focus_running())
            {
                mascot(&mut s, m, b, now);
            } else {
                battery(&mut s, Rect::new(b.x + 12.5, 7.5, 14., 14.), m);
                s.text(
                    Rect::new(b.x + 34., 0., 82., 30.),
                    m.clock.format("%H:%M").to_string(),
                    15.5,
                    WHITE,
                    700u16,
                    true,
                );
                if m.focus_running() {
                    timer_badge(&mut s, Rect::new(b.x + 118., 5., 20., 20.), m);
                } else {
                    equalizer(
                        &mut s,
                        Rect::new(b.x + 120., 8., 19., 13.),
                        m.playing,
                        now.duration_since(m.start).as_secs_f32(),
                    );
                }
                mascot_bell(&mut s, m, Rect::new(b.x + 101., 8., 12., 15.), now);
            }
        }
        NotchView::Hud => hud(&mut s, m, b),
        NotchView::ChargingPeek | NotchView::HeadphonesPeek => {
            device_page(&mut s, m, b, view, true)
        }
        _ => {
            header(&mut s, m, b);
            match view {
                NotchView::Music | NotchView::Lyrics => {
                    music(&mut s, m, b, view == NotchView::Lyrics)
                }
                NotchView::Pomodoro => focus(&mut s, m, b),
                NotchView::Water => water(&mut s, m, b, now),
                NotchView::Todo => todo(&mut s, m, b),
                NotchView::Shelf => shelf(&mut s, m, b, now),
                NotchView::Charging | NotchView::Headphones => {
                    device_page(&mut s, m, b, view, false)
                }
                _ => {}
            }
            if !(view == NotchView::Todo && m.calendar_open)
                && !(view == NotchView::Water && m.water_settings)
            {
                icon_button(
                    &mut s,
                    Rect::new(b.x + 10., 59., 22., 22.),
                    "\u{e76b}",
                    Action::Intent(UserIntent::PreviousPage),
                    "Previous",
                    0x424242,
                );
                icon_button(
                    &mut s,
                    Rect::new(b.x + b.w - 32., 59., 22., 22.),
                    "\u{e76c}",
                    Action::Intent(UserIntent::NextPage),
                    "Next",
                    0x424242,
                );
            }
            navigation(&mut s, m, b, now);
        }
    }
    for hit in &mut s.hits {
        let voice_busy = matches!(m.voice, VoiceState::Recording | VoiceState::Transcribing);
        hit.enabled &= match hit.action {
            Action::AddTask | Action::EditTasks | Action::EditTask(_) => {
                !voice_busy && !m.draft.as_ref().is_some_and(|draft| draft.saving)
            }
            Action::Intent(UserIntent::ToggleVoiceCapture) => {
                m.voice != VoiceState::Transcribing && m.draft.is_none()
            }
            Action::SaveTask => m
                .draft
                .as_ref()
                .is_some_and(|draft| !draft.saving && !draft.title.trim().is_empty()),
            Action::CancelTask | Action::Palette | Action::Color(_) | Action::Daily => {
                !m.draft.as_ref().is_some_and(|draft| draft.saving)
            }
            _ => true,
        };
    }
    s.hover_controls(
        m.mouse,
        m.previous_hover,
        (now.duration_since(m.hover_changed).as_secs_f32() / 0.15).clamp(0., 1.),
    );
    s
}
fn battery(s: &mut Scene, r: Rect, m: &UiModel) {
    s.ring(
        r,
        f32::from(m.power.percent) / 100.,
        if m.power.is_plugged_in {
            0x5eff9c
        } else {
            0x33e07a
        },
        2.3,
    );
}
fn timer_badge(s: &mut Scene, r: Rect, m: &UiModel) {
    s.progress_ring(r, m.remaining(), 0xff5c64, false);
    s.icon(r.inset(r.w * 0.24), TIMER, RED);
}
fn equalizer(s: &mut Scene, r: Rect, playing: bool, elapsed: f32) {
    for i in 0..5 {
        let height = if playing {
            3.8 + 8.6 * (elapsed / 0.14 + i as f32 * 1.35).sin().abs()
        } else {
            3.8
        };
        s.round(
            Rect::new(r.x + i as f32 * 4.2, r.y + (r.h - height) / 2., 2.2, height),
            1.1,
            WHITE,
        );
    }
}

fn header(s: &mut Scene, m: &UiModel, b: Rect) {
    let x = b.x + ((b.w - 144.) / 2.).max(120.);
    battery(s, Rect::new(x, 10., 14., 14.), m);
    s.text_run(
        Rect::new(x + 21., 9., b.w - (x - b.x) - 21., 18.),
        vec![
            TextSpan::new(
                crate::i18n::format_header_date(m.clock),
                11.5,
                0xeeeeef,
                400,
            ),
            TextSpan::new(m.clock.format("%H:%M").to_string(), 12., WHITE, 700),
        ],
        7.,
    );
    if m.focus_running() {
        timer_badge(s, Rect::new(b.x + b.w - 35., 7., 19., 19.), m);
    }
    if m.playing && !m.focus_running() {
        equalizer(
            s,
            Rect::new(b.x + b.w - 29., 11., 19., 13.),
            true,
            Instant::now().duration_since(m.start).as_secs_f32(),
        );
    }
}
fn navigation(s: &mut Scene, m: &UiModel, b: Rect, now: Instant) {
    let selected = match m.view {
        NotchView::Music | NotchView::Lyrics => 0,
        NotchView::Pomodoro => 1,
        NotchView::Water => 2,
        NotchView::Shelf => 3,
        NotchView::Todo => 4,
        NotchView::Headphones => 5,
        _ => 6,
    };
    let glyphs = [MUSIC, TIMER, DROP, SHELF, TODO, "\u{e7f6}", "\u{e850}"];
    let amount = m.nav_amount(now);
    let width = 82. + 158. * amount;
    let x = b.x + (b.w - width) / 2.;
    let y = 132. + 6. * amount;
    let height = 6. + 18. * amount;
    if amount > 0.01 {
        s.paint.push(Paint::Opacity(amount));
        s.round(Rect::new(x, y, width, height), height / 2., 0x171719);
        s.clip(Rect::new(x, y, width, height));
        for i in 0..8 {
            let slot = (width - 16.) / 8.;
            let cx = x + 8. + (i as f32 + 0.5) * slot;
            let tint = if i == selected {
                [
                    0xff3d68, 0xe6e6ea, 0x36baff, 0xffb026, 0x38d36f, 0x66aaff, 0x3be666, 0x99999e,
                ][i]
            } else {
                0x99999e
            };
            if i == 1 && m.focus_running() {
                timer_badge(s, Rect::new(cx - 9., y + (height - 18.) / 2., 18., 18.), m);
            } else {
                s.icon(
                    Rect::new(cx - 7., y + (height - 14.) / 2., 14., 14.),
                    if i == 7 { "page-settings" } else { glyphs[i] },
                    tint,
                );
            }
            if m.nav {
                s.hit(
                    Rect::new(x + 8. + i as f32 * slot, y, slot, height),
                    if i == 7 {
                        Action::Settings
                    } else {
                        Action::Intent(UserIntent::ShowPage(
                            NotchPage::from_index(i).expect("page index"),
                        ))
                    },
                    tr([
                        "nav-music",
                        "nav-pomodoro",
                        "nav-water",
                        "nav-shelf",
                        "nav-todo",
                        "headset",
                        "battery",
                        "nav-general",
                    ][i]),
                );
            }
        }
        s.unclip();
        s.paint.push(Paint::Opacity(1.));
    }
    if amount < 0.99 {
        s.paint.push(Paint::Opacity(1. - amount));
        let mut dx = b.x + b.w / 2. - 31.;
        for i in 0..7 {
            let w = if i == selected { 14. } else { 4. };
            s.round(
                Rect::new(dx, 133., w, 4.),
                2.,
                if i == selected { WHITE } else { 0x68686c },
            );
            if !m.nav {
                s.hit(
                    Rect::new(dx - 2., 127., w + 4., 16.),
                    Action::Intent(UserIntent::ShowPage(
                        NotchPage::from_index(i).expect("page index"),
                    )),
                    tr([
                        "nav-music",
                        "nav-pomodoro",
                        "nav-water",
                        "nav-shelf",
                        "nav-todo",
                        "headset",
                        "battery",
                    ][i]),
                );
            }
            dx += w + 4.;
        }
        s.paint.push(Paint::Opacity(1.));
    }
}

fn music(s: &mut Scene, m: &UiModel, b: Rect, lyrics: bool) {
    let left = b.x + 42.;
    let width = if lyrics { 204. } else { 336. };
    let art_size = if lyrics { 48. } else { 62. };
    let art_y = if lyrics { 26. } else { 12. };
    let cover = Rect::new(left, art_y, art_size, art_size);
    s.round(cover, 10., 0x1b1b1d);
    if let Some(art) = &m.artwork {
        s.paint.push(Paint::RoundedBitmap(
            cover,
            art.width,
            art.height,
            art.rgba.clone(),
            9.,
        ));
    } else {
        s.icon(
            Rect::new(
                cover.x + (art_size - 28.) / 2.,
                cover.y + (art_size - 28.) / 2.,
                28.,
                28.,
            ),
            MUSIC,
            0x77777b,
        );
    }
    let title = m.track.as_ref().map_or_else(
        || full("notch.music.idleTitle"),
        |track| track.title.clone(),
    );
    let subtitle = m.track.as_ref().map_or_else(
        || {
            full(match m.connection {
                crate::domain::media::MediaConnection::Connecting => {
                    "notch.music.connectingSubtitle"
                }
                crate::domain::media::MediaConnection::AuthenticationFailed => {
                    "notch.music.authFailedSubtitle"
                }
                crate::domain::media::MediaConnection::Unavailable => {
                    "notch.music.unavailableSubtitle"
                }
                _ => "notch.music.notConfiguredSubtitle",
            })
        },
        |track| track.artist.clone(),
    );
    let tx = left + art_size + 16.;
    let ty = art_y + if lyrics { 4. } else { 27. };
    let tw = if lyrics { 142. } else { 246. };
    s.text(Rect::new(tx, ty, tw, 20.), title, 14.5, WHITE, true, false);
    s.text(
        Rect::new(tx, ty + 21., tw, 17.),
        subtitle,
        11.5,
        DIM,
        false,
        false,
    );
    let y = if lyrics { 77. } else { 83. };
    let fs = if lyrics { 10.5 } else { 11. };
    s.text(
        Rect::new(left, y, if lyrics { 36. } else { 40. }, 14.),
        format_duration(m.position),
        fs,
        DIM,
        false,
        false,
    );
    let bar = Rect::new(
        left + if lyrics { 38. } else { 44. },
        y + 4.,
        if lyrics { 124. } else { 246. },
        4.,
    );
    percent(
        s,
        bar,
        if m.duration.is_zero() {
            0.
        } else {
            m.position.as_secs_f32() / m.duration.as_secs_f32()
        },
        WHITE,
    );
    s.hit(Rect::new(bar.x, y, bar.w, 12.), Action::Seek, "Seek");
    s.text(
        Rect::new(
            left + if lyrics { 164. } else { 294. },
            y,
            if lyrics { 40. } else { 42. },
            14.,
        ),
        format!(
            "-{}",
            format_duration(m.duration.saturating_sub(m.position))
        ),
        fs,
        DIM,
        false,
        true,
    );
    let transport_y = if lyrics { 93. } else { 104. };
    for (bx, glyph, command, label) in [
        (
            if lyrics { 62. } else { width / 2. - 59. },
            "\u{e892}",
            MediaCommand::Previous,
            "Previous track",
        ),
        (
            if lyrics { 104. } else { width / 2. - 13. },
            if m.playing {
                "transport-pause"
            } else {
                "transport-play"
            },
            MediaCommand::PlayPause,
            "Play/Pause",
        ),
        (
            if lyrics { 146. } else { width / 2. + 33. },
            "\u{e893}",
            MediaCommand::Next,
            "Next track",
        ),
    ] {
        let r = Rect::new(left + bx, transport_y, 26., 24.);
        s.icon(Rect::new(r.x + 4.5, r.y + 3.5, 17., 17.), glyph, WHITE);
        s.hit(r, Action::Intent(UserIntent::Media(command)), label);
    }
    let bubble = Rect::new(
        left + if lyrics { 6.5 } else { 8.5 },
        transport_y + 4.5,
        17.,
        15.,
    );
    s.icon(
        bubble,
        if lyrics { "lyrics-on" } else { "lyrics-off" },
        WHITE,
    );
    s.hit(
        Rect::new(left + if lyrics { 2. } else { 4. }, transport_y, 26., 24.),
        Action::Intent(UserIntent::ToggleLyrics),
        "Lyrics",
    );
    if !lyrics {
        let vx = left + width - 102.;
        let vy = transport_y - 1.;
        s.icon(Rect::new(vx + 82., vy + 5., 20., 18.), "\u{e767}", WHITE);
        s.text(
            Rect::new(vx + 94., vy + 4., 10., 20.),
            if m.muted || m.volume == 0. { "×" } else { ")" },
            12.,
            DIM,
            false,
            false,
        );
        s.hit(
            Rect::new(vx, vy, 102., 26.),
            Action::ShowVolume,
            tr("volume"),
        );
        let alpha = m.volume_amount(Instant::now());
        if alpha > 0.01 {
            s.paint.push(Paint::Opacity(alpha));
            let popup = Rect::new(vx + (1. - alpha) * 10., vy + 1., 76., 24.);
            s.round(popup, 12., 0x202020);
            let slider = Rect::new(popup.x + 6., popup.y, 65., popup.h);
            let value = if m.muted { 0. } else { m.volume };
            let rail = Rect::new(
                slider.x + VOLUME_THUMB_SIZE / 2.,
                slider.y + (slider.h - 4.) / 2.,
                slider.w - VOLUME_THUMB_SIZE,
                4.,
            );
            percent_bar(s, rail, value, 0x60cdff, 0x999999);
            let thumb = Rect::new(
                slider.x + rail.w * value,
                slider.y + (slider.h - VOLUME_THUMB_SIZE) / 2.,
                VOLUME_THUMB_SIZE,
                VOLUME_THUMB_SIZE,
            );
            s.round(thumb, VOLUME_THUMB_SIZE / 2., 0x454545);
            s.border(thumb, VOLUME_THUMB_SIZE / 2., 0x555555, 0.5);
            s.round(thumb.inset(4.), 6., 0x60cdff);
            s.paint.push(Paint::Opacity(1.));
            if m.volume_shown {
                s.hit(slider, Action::Volume, tr("volume"));
            }
        }
    }
    if lyrics {
        s.line(b.x + 256., 28., b.x + 256., 116., 0x1f1f1f, 1.);
        let r = Rect::new(b.x + 268., 26., 210., 91.);
        s.clip(r);
        match &m.lyrics {
            LyricsAvailability::Ready(lines) => {
                let active = m.active_lyric.unwrap_or(0);
                let position = m.lyric_position(Instant::now());
                for (i, line) in lines
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| (*i as f32 - position).abs() < 4.)
                {
                    let row = Rect::new(r.x, r.y + 33. + (i as f32 - position) * 24., r.w, 24.);
                    s.text(
                        row,
                        line,
                        if i == active { 14. } else { 13. },
                        if i == active {
                            WHITE
                        } else if row.contains(m.mouse.0, m.mouse.1) {
                            0xb8b8b8
                        } else {
                            0x575757
                        },
                        700u16,
                        false,
                    );
                    s.hit(row, Action::Lyric(i), line);
                }
            }
            other => s.paint.push(Paint::Paragraph(
                r,
                full(match other {
                    LyricsAvailability::Loading => "notch.lyrics.loading",
                    LyricsAvailability::NotFound => "notch.lyrics.notFound",
                    _ => "notch.lyrics.unavailable",
                }),
                12.5,
                DIM,
            )),
        }
        s.unclip();
    }
}

fn focus(s: &mut Scene, m: &UiModel, b: Rect) {
    title(s, b, &tr("nav-pomodoro"));
    let x = b.x;
    let (seconds, phase) = m.utility.as_ref().map_or(
        (
            u64::from(m.settings.reminders.focus_minutes) * 60,
            FocusPhase::Focus,
        ),
        |u| (u.focus_seconds, u.phase),
    );
    let is_break = phase != FocusPhase::Focus;
    s.icon(
        Rect::new(x + 32., 23., 108., 108.),
        "focus-glow",
        if is_break { 0x154c5a } else { 0x772430 },
    );
    s.round(
        Rect::new(x + 42., 33., 88., 88.),
        44.,
        if is_break { 0x0c3039 } else { 0x3c1119 },
    );
    s.progress_ring(
        Rect::new(x + 50., 41., 72., 72.),
        1. - m.remaining(),
        if is_break { 0x2bc9da } else { 0xff4857 },
        false,
    );
    s.icon(Rect::new(x + 65., 55., 42., 42.), TIMER, WHITE);
    s.text(
        Rect::new(x + 194., 33., 128., 35.),
        format_duration(std::time::Duration::from_secs(seconds)),
        31.,
        WHITE,
        700u16,
        true,
    );
    s.text(
        Rect::new(x + 198., 69., 120., 17.),
        tr(match phase {
            FocusPhase::Focus => "focus-time",
            FocusPhase::ShortBreak => "short-break-action",
            FocusPhase::LongBreak => "long-break",
        }),
        11.,
        0xaaaaaf,
        false,
        true,
    );
    icon_button(
        s,
        Rect::new(x + 328., 37., 24., 24.),
        "\u{e777}",
        Action::Intent(UserIntent::ResetFocus),
        &tr("reset"),
        0x1d1d20,
    );
    s.button(
        Rect::new(x + 138., 92., 88., 24.),
        tr("short-break-action"),
        TIMER,
        Action::Intent(UserIntent::SelectFocusPhase(FocusPhase::ShortBreak)),
        0x212121,
    );
    s.button(
        Rect::new(x + 232., 92., 52., 24.),
        "",
        if m.focus_running() { PAUSE } else { PLAY },
        Action::Intent(UserIntent::ToggleFocus),
        RED,
    );
    s.button(
        Rect::new(x + 290., 92., 88., 24.),
        tr("focus-time"),
        TIMER,
        Action::Intent(UserIntent::SelectFocusPhase(FocusPhase::Focus)),
        0x212121,
    );
}

fn water(s: &mut Scene, m: &UiModel, b: Rect, now: Instant) {
    let x = b.x;
    if m.water_settings {
        water_settings(s, m, b);
        return;
    }
    s.text_run(
        Rect::new(x + 15., 10., 121., 18.),
        vec![
            TextSpan::new(tr("nav-water"), 14., WHITE, 600),
            TextSpan::new(tr("water-reminder-suffix"), 14., 0x3cc2ff, 600),
        ],
        3.,
    );
    let goal = m
        .water
        .as_ref()
        .map_or(m.settings.water.goal_ml, |w| w.goal_ml);
    let current = m.water.as_ref().map_or(0, |w| w.consumed_ml);
    let t = (now.duration_since(m.water_change).as_secs_f32() / 0.8).clamp(0., 1.);
    let value = m.previous_water as f32
        + (current as f32 - m.previous_water as f32) * (1. - (1. - t).powi(3));
    let progress = value / f32::from(goal.max(1));
    let mut goal_span = TextSpan::new(
        format!("/ {:.1} L", f32::from(goal) / 1000.),
        11.,
        0xa7a7ac,
        400,
    );
    goal_span.bottom = true;
    s.text_run(
        Rect::new(x + 42., 31., 118., 31.),
        vec![
            TextSpan::new(format!("{:.1}", current as f64 / 1000.), 29., WHITE, 700),
            goal_span,
        ],
        4.,
    );
    toggle(
        s,
        Rect::new(x + 160., 36., 34., 20.),
        m.settings.water.enabled,
        Action::ToggleWater,
        &tr("water-enable"),
        BLUE,
    );
    let next = m.water.as_ref().and_then(|w| w.next_at).map_or_else(
        || tr("water-goal-reached"),
        |at| format!("{} {}", tr("water-next"), at.format("%H:%M")),
    );
    s.text(
        Rect::new(x + 42., 65., 155., 15.),
        format!(
            "{} · {}% · {}",
            tr("today"),
            (current as f32 / f32::from(goal.max(1)) * 100.).round() as u32,
            next
        ),
        9.,
        DIM,
        false,
        false,
    );
    percent(s, Rect::new(x + 42., 83., 152., 4.), progress, BLUE);
    s.line(x + 210., 33., x + 210., 121., 0x161619, 1.);
    s.button(
        Rect::new(x + 46., 97., 60., 24.),
        tr("drink-short"),
        DROP,
        Action::Intent(UserIntent::DrinkWater),
        0x075c9a,
    );
    s.button(
        Rect::new(x + 112., 97., 78., 24.),
        tr("water-settings"),
        "page-settings",
        Action::WaterSettings,
        0x232326,
    );
    let r = Rect::new(x + 227., 52., 151., 68.);
    s.round(
        Rect::new(x + 356., 38., 4., 4.),
        2.,
        if !m.water_graph { WHITE } else { DIM },
    );
    s.round(
        Rect::new(x + 365., 38., 4., 4.),
        2.,
        if m.water_graph { WHITE } else { DIM },
    );
    s.hit(
        Rect::new(x + 350., 32., 28., 18.),
        Action::WaterGraph,
        "Chart",
    );
    if m.water_graph {
        let hours = m.water.as_ref().map_or([0; 24], |water| water.hourly_ml);
        let current = m.clock.hour() as usize;
        let first = (usize::from(m.settings.water.active_from_minute) / 60)
            .min(current)
            .min(hours.iter().position(|ml| *ml > 0).unwrap_or(current));
        let last = usize::from(m.settings.water.active_until_minute)
            .div_ceil(60)
            .max(current + 1)
            .max(
                hours
                    .iter()
                    .rposition(|ml| *ml > 0)
                    .map_or(0, |hour| hour + 1),
            );
        let count = (last - first).max(1);
        let max = hours
            .iter()
            .copied()
            .max()
            .unwrap_or(0)
            .div_ceil(200)
            .saturating_mul(200)
            .max(400) as f32;
        s.line(r.x, r.y + 53., r.x + 122., r.y + 53., 0x15384b, 1.);
        for (hour, amount) in hours.iter().enumerate().take(last).skip(first) {
            let column = 122. / count as f32;
            let bx = r.x + (hour - first) as f32 * column;
            let bw = (column * 0.7).min(6.);
            let height = (m.previous_hours[hour] as f32
                + (*amount as f32 - m.previous_hours[hour] as f32) * (1. - (1. - t).powi(3)))
                / max
                * 50.;
            s.round(
                Rect::new(bx + (column - bw) / 2., r.y + 1., bw, 50.),
                3.,
                0x151518,
            );
            s.paint.push(Paint::GradientRound(
                Rect::new(bx + (column - bw) / 2., r.y + 51. - height, bw, height),
                3.,
                0x5bceff,
                0x2372d1,
            ));
            if hour.is_multiple_of(3) {
                s.text(
                    Rect::new(bx, r.y + 56., column, 12.),
                    format!("{hour:02}"),
                    8.,
                    0x69696f,
                    false,
                    true,
                );
            }
        }
        for (dy, text) in [
            (-2., format!("{max:.0}")),
            (23., format!("{:.0}", max / 2.)),
            (48., "0 ml".into()),
        ] {
            s.text(
                Rect::new(r.x + 128., r.y + dy, 23., 12.),
                text,
                8.,
                0x8c8c93,
                false,
                false,
            );
        }
    } else {
        let gauge = Rect::new(x + 263., 42., 78., 78.);
        s.progress_ring(gauge, progress, 0x35c9ff, true);
        s.text(
            Rect::new(gauge.x, gauge.y + 19., 78., 31.),
            format!("{:.0}", progress * 100.),
            26.,
            WHITE,
            700u16,
            true,
        );
        s.icon(
            Rect::new(gauge.x + 29., gauge.y + 52., 10., 10.),
            DROP,
            0x3acaff,
        );
        s.text(
            Rect::new(gauge.x + 41., gauge.y + 50., 10., 14.),
            "%",
            10.,
            0xb5b5ba,
            false,
            false,
        );
        if t < 1. && current > m.previous_water {
            s.ring(
                Rect::new(
                    gauge.x - 6. * t,
                    gauge.y - 6. * t,
                    gauge.w + 12. * t,
                    gauge.h + 12. * t,
                ),
                1.,
                0x123846,
                1.,
            );
        }
    }
}
fn water_settings(s: &mut Scene, m: &UiModel, b: Rect) {
    let x = b.x + 42.;
    let y = 34.;
    let p = &m.settings.water;
    icon_button(
        s,
        Rect::new(b.x + 15., 10., 20., 20.),
        "\u{e76b}",
        Action::WaterSettings,
        "Back",
        0x2e3032,
    );
    s.text(
        Rect::new(b.x + 39., 10., 90., 20.),
        tr("water-settings"),
        13.,
        WHITE,
        true,
        false,
    );
    for (i, label) in [
        "water-bottle-short",
        "water-goal-short",
        "water-hours-short",
    ]
    .iter()
    .enumerate()
    {
        s.text(
            Rect::new(x, y + i as f32 * 30., 45., 22.),
            tr(label),
            11.,
            0xd3d5d6,
            true,
            false,
        );
    }
    let step = |s: &mut Scene,
                sx: f32,
                sy: f32,
                key: Preference,
                value: i32,
                plus: bool,
                enabled: bool| {
        let r = Rect::new(x + sx, y + sy, 20., 20.);
        s.round(r, 10., 0x25282a);
        s.text(r, if plus { "+" } else { "−" }, 16., WHITE, false, true);
        s.hit(
            r,
            Action::Preference(key, value),
            if plus { "+" } else { "−" },
        );
        if let Some(hit) = s.hits.last_mut() {
            hit.enabled = enabled;
        }
    };
    step(
        s,
        50.,
        1.,
        Preference::Bottle,
        (i32::from(p.bottle_ml) - 50).max(1),
        false,
        p.bottle_ml > 1,
    );
    step(
        s,
        136.,
        1.,
        Preference::Bottle,
        (i32::from(p.bottle_ml) + 50).min(65535),
        true,
        p.bottle_ml < 65535,
    );
    s.text(
        Rect::new(x + 72., y, 62., 22.),
        format!("{} ml", p.bottle_ml),
        11.,
        WHITE,
        true,
        true,
    );
    s.paint.push(Paint::Paragraph(
        Rect::new(x + 162., y + 1., 72., 22.),
        crate::i18n::translate_with(
            "desktopUi.water-dose-detail",
            &[
                ("ml", &p.dose_ml().to_string()),
                ("sips", &p.sips_per_bottle().to_string()),
            ],
        ),
        8.,
        0x858b90,
    ));

    for (i, goal) in [1500, 2000, 2500, 3000].iter().enumerate() {
        let r = Rect::new(x + 49. + i as f32 * 47., y + 31., 44., 22.);
        let selected = *goal == i32::from(p.goal_ml);
        s.round(r, 11., if selected { 0x35baff } else { 0x202426 });
        s.text(
            r,
            format!("{} L", *goal as f32 / 1000.),
            10.,
            if selected { 0x061e30 } else { 0xb2b7ba },
            false,
            true,
        );
        s.hit(
            r,
            Action::Preference(Preference::Goal, *goal),
            format!("{goal} ml"),
        );
    }
    let from = i32::from(p.active_from_minute);
    let until = i32::from(p.active_until_minute);
    step(
        s,
        50.,
        61.,
        Preference::From,
        (from - 15).max(0),
        false,
        from > 0,
    );
    step(
        s,
        117.,
        61.,
        Preference::From,
        from + 15,
        true,
        from + 15 < until,
    );
    step(
        s,
        152.,
        61.,
        Preference::Until,
        until - 15,
        false,
        until - 15 > from,
    );
    step(
        s,
        219.,
        61.,
        Preference::Until,
        (until + 15).min(1440),
        true,
        until < 1440,
    );
    for (offset, value) in [(72., from), (174., until)] {
        s.text(
            Rect::new(x + offset, y + 60., 43., 22.),
            format!("{:02}:{:02}", value / 60, value % 60),
            11.,
            WHITE,
            true,
            true,
        );
    }
    s.text(
        Rect::new(x + 139., y + 60., 12., 22.),
        "–",
        10.,
        0x90979a,
        false,
        true,
    );
    s.round(Rect::new(x + 246., y, 90., 60.), 14., 0x0d1c29);
    s.text(
        Rect::new(x + 246., y + 3., 90., 25.),
        p.reminder_count().to_string(),
        23.,
        0x65d6ff,
        700u16,
        true,
    );
    s.text(
        Rect::new(x + 246., y + 28., 90., 12.),
        tr("water-reminders-short"),
        8.,
        0x8f9aa2,
        false,
        true,
    );
    s.text(
        Rect::new(x + 246., y + 43., 90., 15.),
        format!("{} {}", p.refills(), tr("water-refills-short")),
        10.,
        0x65d6ff,
        false,
        true,
    );
    s.button(
        Rect::new(x + 246., y + 66., 90., 20.),
        tr("reset"),
        "",
        Action::Intent(UserIntent::ResetWater),
        0x212121,
    );
    if let Some(error) = m.water_error {
        s.text(
            Rect::new(x, y + 81., 336., 9.),
            full(error),
            7.,
            0xff8f8f,
            false,
            false,
        );
    }
}

fn todo(s: &mut Scene, m: &UiModel, b: Rect) {
    if m.calendar_open {
        calendar(s, m, b);
        return;
    }
    let x = b.x;
    s.text_run(
        Rect::new(b.x + 15., 10., 111., 18.),
        vec![
            TextSpan::new(full("notch.todo.titlePrefix"), 14., WHITE, 600),
            TextSpan::new(full("notch.todo.titleAccent"), 14., 0x38d36f, 600),
        ],
        0.,
    );
    let summary = !m.list_expanded || m.draft.is_some();
    let voice_color = match m.voice {
        VoiceState::Recording => 0xff4b55,
        VoiceState::Transcribing => 0x2d5bff,
        _ => 0x212121,
    };
    let expansion = if m.draft.is_some() {
        0.
    } else {
        m.list_amount(Instant::now())
    };
    let list = Rect::new(
        x + 198. - 116. * expansion,
        33.,
        180. + 116. * expansion,
        90.,
    );
    s.round(list, 21., 0x121214);
    if summary {
        s.text(
            Rect::new(x + 42., 34., 100., 38.),
            m.utility
                .as_ref()
                .filter(|u| u.task_id.is_some())
                .map_or_else(
                    || format!("{}/{}", m.progress.done, m.progress.total),
                    |u| format_duration(std::time::Duration::from_secs(u.task_seconds)),
                ),
            32.,
            WHITE,
            700u16,
            false,
        );
        icon_button(
            s,
            Rect::new(x + 104., 37., 27., 27.),
            "\u{e787}",
            Action::Calendar,
            &tr("nav-calendar"),
            0x252527,
        );
        s.text(
            Rect::new(x + 42., 73., 140., 17.),
            m.editor_error
                .or(m.voice_error)
                .map(full)
                .unwrap_or_else(|| {
                    m.draft
                        .as_ref()
                        .map_or_else(|| full("notch.todo.completedToday"), |d| d.date.clone())
                }),
            10.,
            if m.editor_error.is_some() { RED } else { DIM },
            false,
            false,
        );
        s.button(
            Rect::new(x + 42., 94., 74., 24.),
            full("notch.todo.addButton"),
            "plus-circle",
            Action::AddTask,
            0x212121,
        );
        icon_button(
            s,
            Rect::new(x + 122., 94., 26., 24.),
            EDIT,
            Action::EditTasks,
            &full("notch.todo.editButton"),
            0x212121,
        );
        icon_button(
            s,
            Rect::new(x + 154., 94., 28., 24.),
            MIC,
            Action::Intent(UserIntent::ToggleVoiceCapture),
            &full("notch.todo.voiceButton"),
            voice_color,
        );
        if m.utility.as_ref().is_some_and(|u| u.can_snooze) {
            s.button(
                Rect::new(x + 151., 74., 33., 17.),
                tr("snooze-action"),
                "",
                Action::Intent(UserIntent::SnoozeReminder),
                0x303034,
            );
        }
    } else {
        icon_button(
            s,
            Rect::new(x + 44., 35., 28., 24.),
            "plus-circle",
            Action::AddTask,
            &full("notch.todo.addButton"),
            0x212121,
        );
        icon_button(
            s,
            Rect::new(x + 45., 66., 26., 24.),
            EDIT,
            Action::EditTasks,
            &full("notch.todo.editButton"),
            0x212121,
        );
        icon_button(
            s,
            Rect::new(x + 44., 97., 28., 24.),
            MIC,
            Action::Intent(UserIntent::ToggleVoiceCapture),
            &full("notch.todo.voiceButton"),
            voice_color,
        );
    }
    if let Some(d) = &m.draft {
        task_editor(s, m, list, d);
        return;
    }
    if m.tasks.is_empty() {
        s.icon(
            Rect::new(list.x + list.w / 2. - 16., 48., 32., 32.),
            TODO,
            0x48484d,
        );
        s.text(
            Rect::new(list.x, 91., list.w, 16.),
            tr("todo-empty-short"),
            11.,
            DIM,
            false,
            true,
        );
        s.hit(list, Action::ExpandTasks, "Tasks");
        return;
    }
    task_rows(s, m, list, false);
}
fn task_rows(s: &mut Scene, m: &UiModel, r: Rect, agenda: bool) {
    let row_height = if agenda { 34. } else { 29. };
    let scroll = m
        .task_scroll
        .min((m.tasks.len() as f32 * row_height - r.h).max(0.));
    s.hit(r, Action::ExpandTasks, "Tasks");
    s.clip(r);
    for (index, task) in m.tasks.iter().enumerate() {
        let y = r.y + index as f32 * row_height - scroll;
        if y + row_height <= r.y || y >= r.y + r.h {
            continue;
        }
        let edit = m.edit_mode && !agenda;
        let row = if agenda {
            let card = Rect::new(r.x, y + 2., r.w - 3., 30.);
            let selected = task.schedule.date.to_string() == m.selected_date;
            s.round(card, 10., if selected { 0x1d3025 } else { 0x17191b });
            s.border(card, 10., if selected { 0x356548 } else { 0x25282a }, 1.);
            Rect::new(card.x + 3., y + 4., card.w - 6., 26.)
        } else {
            let row = Rect::new(r.x, y, r.w, 26.);
            s.alpha_round(
                row,
                8.,
                if edit {
                    0x38d36f
                } else if task.is_done {
                    0x28783c
                } else {
                    WHITE
                },
                if edit {
                    0.14
                } else if task.is_done {
                    0.38
                } else {
                    0.09
                },
            );
            if edit {
                s.alpha_border(row, 8., 0x38d36f, 1., 0.58);
            }
            row
        };
        let color = TASK_COLORS[task.color.index() as usize];
        let dot = Rect::new(row.x + 6., row.y + 5., 16., 16.);
        if edit {
            s.alpha_round(dot, 8., 0x38d36f, 0.25);
            s.icon(dot.inset(2.), EDIT, 0x38d36f);
        } else if task.is_done {
            s.round(dot, 8., color);
            s.icon(
                Rect::new(dot.x + 3.5, dot.y + 3.5, 9., 9.),
                "\u{e73e}",
                WHITE,
            );
        } else {
            s.border(dot, 8., color, 1.5);
        }
        if !edit {
            s.hit(
                Rect::new(row.x + 2., row.y, 24., 26.),
                Action::Intent(UserIntent::ToggleTask(task.id)),
                &task.title,
            );
        }
        let schedule = if agenda {
            format!(
                "{}{}",
                if task.schedule.daily {
                    full("notch.todo.daily")
                } else {
                    task.schedule.date.format("%d/%m/%Y").to_string()
                },
                task.schedule
                    .time
                    .map_or_else(String::new, |time| format!(" · {}", time.format("%H:%M")))
            )
        } else {
            task.schedule
                .time
                .map_or_else(String::new, |time| time.format("%H:%M").to_string())
        };
        let title = Rect::new(
            row.x + 27.,
            row.y,
            row.w - 60.,
            if !schedule.is_empty() && !edit {
                15.
            } else {
                26.
            },
        );
        let ink = if task.is_done && !edit {
            0xb2b2b2
        } else {
            WHITE
        };
        s.text(title, &task.title, 11.5, ink, false, false);
        if task.is_done && !edit {
            s.paint
                .push(Paint::Strike(title, task.title.clone(), 11.5, ink));
        }
        if !schedule.is_empty() && !edit {
            s.text(
                Rect::new(title.x, row.y + 15., title.w, 10.),
                schedule,
                8.,
                DIM,
                false,
                false,
            );
        }
        s.hit(
            Rect::new(
                row.x + if edit { 0. } else { 27. },
                row.y,
                row.w - if edit { 27. } else { 54. },
                26.,
            ),
            if edit {
                Action::EditTask(task.id)
            } else {
                Action::ExpandTasks
            },
            &task.title,
        );
        if edit {
            let del = Rect::new(row.x + row.w - 25., row.y + 3., 20., 20.);
            s.round(del, 6., 0x290c0e);
            s.icon(del.inset(4.), "\u{e74d}", 0xff7b82);
            s.hit(
                del,
                Action::Intent(UserIntent::DeleteTask(task.id)),
                "Delete",
            );
        } else if !task.is_done {
            let play = Rect::new(row.x + row.w - 25., row.y + 3., 22., 20.);
            s.text(play, "▶", 10., 0x8ae5a2, false, true);
            s.hit(
                play,
                Action::Intent(UserIntent::StartTask(task.id)),
                tr("start"),
            );
        }
    }
    s.unclip();
}

fn task_editor(s: &mut Scene, m: &UiModel, r: Rect, d: &TaskDraft) {
    s.round(r, 18., 0x1b1c1e);
    let title_r = Rect::new(r.x + 6., r.y + 7., r.w - 12., 28.);
    s.round(title_r, 11., 0x080909);
    s.border(title_r, 11., 0xd6d9d7, 1.);
    s.round(
        Rect::new(r.x + 15., r.y + 17., 8., 8.),
        4.,
        TASK_COLORS[d.color],
    );
    s.hit(
        Rect::new(r.x + 10., r.y + 10., 20., 23.),
        Action::Palette,
        "Task color",
    );
    s.input(
        Field::TaskTitle,
        Rect::new(r.x + 31., r.y + 10., r.w - 44., 22.),
        &d.title,
        false,
        false,
        &full("notch.todo.inputPlaceholder"),
    );
    if d.id.is_none() {
        let row = Rect::new(r.x + 8., r.y + 38., r.w - 16., 20.);
        s.round(row, 7., 0x242729);
        s.icon(Rect::new(row.x + 2., row.y + 3., 14., 14.), TIMER, WHITE);
        s.input(
            Field::TaskTime,
            Rect::new(row.x + 23., row.y + 2., 55., 16.),
            &d.time,
            false,
            false,
            "HH:MM",
        );
        s.button(
            Rect::new(row.x + row.w - 62., row.y + 2., 60., 16.),
            full(if d.daily {
                "notch.todo.daily"
            } else {
                "notch.todo.once"
            }),
            "",
            Action::Daily,
            if d.daily { 0x356546 } else { 0x484c4a },
        );
    }
    s.button(
        Rect::new(r.x + 6., r.y + 65., (r.w - 18.) / 2., 20.),
        full("notch.todo.cancelButton"),
        "",
        Action::CancelTask,
        0x343639,
    );
    s.button(
        Rect::new(r.x + (r.w + 6.) / 2., r.y + 65., (r.w - 18.) / 2., 20.),
        full("notch.todo.saveButton"),
        "",
        Action::SaveTask,
        0x2fd06b,
    );
    if let Some(hit) = s.hits.last_mut() {
        hit.enabled = !d.saving && !d.title.trim().is_empty();
    }
    if m.palette {
        s.round(Rect::new(r.x + 6., r.y + 36., 126., 26.), 10., 0x383b3a);
        for (index, color) in TASK_COLORS.iter().enumerate() {
            let dot = Rect::new(r.x + 12. + index as f32 * 23., r.y + 40., 18., 18.);
            s.round(dot, 9., *color);
            if index == d.color {
                s.border(dot, 9., WHITE, 2.);
            }
            s.hit(dot, Action::Color(index), format!("Color {}", index + 1));
        }
    }
}
pub fn draw_calendar(s: &mut Scene, m: &UiModel, r: Rect, compact: bool) {
    let Some(grid) = &m.calendar else {
        return;
    };
    let header = if compact { 16. } else { 24. };
    let weekdays = if compact {
        9.
    } else if m.calendar.as_ref().is_some_and(|g| g.week_view) {
        16.
    } else {
        14.
    };
    let week = grid.week_view;
    let months = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    s.text(
        Rect::new(r.x + 17., r.y, r.w - 34., header),
        format!(
            "{} {}",
            full(&format!(
                "calendar.months.{}",
                months[grid.month as usize - 1]
            )),
            grid.year
        ),
        if compact { 9. } else { 12. },
        WHITE,
        true,
        true,
    );
    for (i, offset, glyph) in [(0, -1, "\u{e76b}"), (1, 1, "\u{e76c}")] {
        let ar = Rect::new(if i == 0 { r.x } else { r.x + r.w - 16. }, r.y, 16., header);
        s.icon(ar.inset(3.), glyph, WHITE);
        s.hit(
            ar,
            Action::Intent(UserIntent::CalendarMove(offset)),
            "Move calendar",
        );
    }
    let names = if grid.monday_first {
        ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]
    } else {
        ["sun", "mon", "tue", "wed", "thu", "fri", "sat"]
    };
    let cw = (r.w - 8.) / 7.;
    let ch = (r.h - header - weekdays) / if week { 1. } else { 6. };
    for (i, name) in names.iter().enumerate() {
        s.text(
            Rect::new(r.x + 4. + i as f32 * cw, r.y + header, cw, weekdays),
            full(&format!("calendar.weekdays.{name}")),
            if compact { 6.5 } else { 9. },
            DIM,
            false,
            true,
        );
    }
    for (i, day) in grid.days.iter().enumerate() {
        let cell = Rect::new(
            r.x + 4. + (i % 7) as f32 * cw,
            r.y + header + weekdays + (i / 7) as f32 * ch,
            cw,
            ch,
        );
        let selected = day.date.to_string() == m.selected_date;
        if selected || day.is_today || cell.contains(m.mouse.0, m.mouse.1) {
            s.round(
                Rect::new(cell.x + 2., cell.y + 1., cell.w - 4., cell.h - 2.),
                7.,
                if selected {
                    0xdcf3e3
                } else if cell.contains(m.mouse.0, m.mouse.1) {
                    0x303635
                } else {
                    0x183222
                },
            );
        }
        s.text(
            Rect::new(
                cell.x,
                cell.y,
                cell.w,
                cell.h - if compact && !week { 2. } else { 4. },
            ),
            day.date.day().to_string(),
            if compact {
                if week { 11. } else { 7.5 }
            } else if week {
                13.
            } else {
                10.
            },
            if selected {
                0x142219
            } else if day.is_today {
                GREEN
            } else if day.is_current_month || week {
                WHITE
            } else {
                0x62666a
            },
            if selected || day.is_today {
                700u16
            } else {
                400u16
            },
            true,
        );
        if m.tasks.iter().any(|t| t.schedule.occurs_on(day.date)) {
            let d = if compact { 2. } else { 3. };
            s.round(
                Rect::new(
                    cell.x + cell.w / 2. - d / 2.,
                    cell.y + cell.h - if compact && !week { 2. } else { 5. },
                    d,
                    d,
                ),
                d / 2.,
                if selected { 0x22623a } else { GREEN },
            );
        }
        s.hit(
            cell,
            Action::SelectDate(day.date.to_string()),
            day.date.to_string(),
        );
    }
}
fn calendar(s: &mut Scene, m: &UiModel, b: Rect) {
    icon_button(
        s,
        Rect::new(b.x + 15., 10., 20., 20.),
        "\u{e76b}",
        Action::Calendar,
        "Back",
        0x252a27,
    );
    s.text(
        Rect::new(b.x + 39., 10., 88., 20.),
        tr("nav-calendar"),
        12.,
        WHITE,
        true,
        false,
    );
    let week = m.calendar.as_ref().is_some_and(|grid| grid.week_view);
    draw_calendar(
        s,
        m,
        Rect::new(b.x + 42., 33., 140., if week { 60. } else { 90. }),
        true,
    );
    if week {
        s.text(
            Rect::new(b.x + 42., 98., 48., 22.),
            format!("{}/{}", m.progress.done, m.progress.total),
            18.,
            WHITE,
            true,
            false,
        );
        s.text(
            Rect::new(b.x + 94., 98., 88., 22.),
            full("notch.todo.completedToday"),
            8.,
            DIM,
            false,
            false,
        );
    }
    let r = Rect::new(b.x + 198., 33., 180., 90.);
    s.text(
        Rect::new(r.x, r.y, r.w - 30., 17.),
        if m.selected_date.is_empty() {
            tr("nav-todo")
        } else {
            m.selected_date.clone()
        },
        9.,
        DIM,
        false,
        false,
    );
    icon_button(
        s,
        Rect::new(r.x + r.w - 24., r.y, 24., 17.),
        PLUS,
        Action::AddTask,
        &full("notch.todo.addButton"),
        0x252729,
    );
    let list = Rect::new(r.x, r.y + 20., r.w, 70.);
    if m.tasks.is_empty() {
        s.text(list, full("notch.todo.emptyHint"), 10., DIM, false, true);
    } else {
        task_rows(s, m, list, true);
    }
}
fn shelf(s: &mut Scene, m: &UiModel, b: Rect, now: Instant) {
    title(s, b, &tr("nav-shelf"));
    if !m.shelf.is_empty() {
        s.button(
            Rect::new(b.x + 300., 11., 55., 16.),
            full("notch.shelf.clearButton"),
            "",
            Action::Intent(UserIntent::ClearShelf),
            0x212121,
        );
    }
    s.button(
        Rect::new(b.x + 360., 10., 22., 18.),
        "+",
        "",
        Action::ShelfMenu,
        0x212121,
    );
    let r = Rect::new(b.x + 42., 33., 336., 90.);
    if m.shelf_text_open {
        s.round(r, 12., 0x171719);
        s.round(Rect::new(r.x + 7., r.y + 8., r.w - 14., 32.), 6., 0x212124);
        s.input(
            Field::ShelfText,
            Rect::new(r.x + 14., r.y + 14., r.w - 28., 20.),
            &m.shelf_text,
            false,
            false,
            &tr("shelf-text-placeholder"),
        );
        s.button(
            Rect::new(r.x + r.w - 155., r.y + 56., 70., 24.),
            full("notch.todo.cancelButton"),
            "",
            Action::CancelShelfText,
            0x212121,
        );
        s.button(
            Rect::new(r.x + r.w - 78., r.y + 56., 71., 24.),
            full("notch.todo.saveButton"),
            "",
            Action::SaveShelfText,
            0x212121,
        );
        return;
    }
    let clearing = m.shelf.is_empty()
        && !m.previous_shelf.is_empty()
        && now.duration_since(m.shelf_change).as_secs_f32() < 0.3;
    let t = (now.duration_since(m.shelf_change).as_secs_f32() / 0.28).clamp(0., 1.);
    if m.shelf.is_empty() && !clearing || m.shelf_drag {
        s.round(r, 16., if m.shelf_drag { 0x101420 } else { 0x090909 });
        s.icon(
            r,
            "drop-zone-border",
            if m.shelf_drag { 0xededf0 } else { 0x515154 },
        );
        s.icon(
            Rect::new(
                r.x + 154.,
                r.y + if m.shelf_drag { 19. } else { 13. },
                28.,
                28.,
            ),
            "\u{e896}",
            if m.shelf_drag { WHITE } else { 0xa1a1a5 },
        );
        s.text(
            Rect::new(r.x + 8., r.y + 51., r.w - 16., 17.),
            full(if m.shelf_drag {
                "notch.shelf.releaseTitle"
            } else {
                "notch.shelf.dropTitle"
            }),
            12.,
            WHITE,
            true,
            true,
        );
        if !m.shelf_drag {
            s.text(
                Rect::new(r.x + 8., r.y + 69., r.w - 16., 13.),
                full("notch.shelf.dropSubtitle"),
                9.5,
                0x8b8b90,
                false,
                true,
            );
        }
        s.hit(
            r,
            Action::Intent(UserIntent::BrowseShelfFiles),
            full("notch.shelf.addButton"),
        );
    }
    let entries = if clearing {
        &m.previous_shelf
    } else {
        &m.shelf
    };
    s.clip(r);
    if clearing {
        s.paint.push(Paint::Opacity(1. - t));
    }
    for (i, entry) in entries.iter().enumerate() {
        let card = Rect::new(
            r.x + i as f32 * 68. - m.shelf_scroll,
            r.y + 4. + if clearing { t * t * 90. } else { 0. },
            64.,
            74.,
        );
        if card.x + card.w < r.x || card.x > r.x + r.w {
            continue;
        }
        let hovered = card.contains(m.mouse.0, m.mouse.1);
        if hovered {
            s.alpha_round(card, 10., WHITE, 0.08);
        }
        let tile = Rect::new(card.x + 12., card.y + 4., 40., 40.);
        s.alpha_round(tile, 9., WHITE, 0.05);
        if let Some(image) = &entry.thumbnail {
            s.paint.push(Paint::RoundedBitmap(
                tile,
                image.width,
                image.height,
                image.rgba.clone(),
                9.,
            ));
        } else if let Some(text) = entry.item.text() {
            s.paint.push(Paint::Paragraph(
                Rect::new(tile.x + 4., tile.y + 4., 32., 32.),
                text.to_owned(),
                8.,
                WHITE,
            ));
        } else if entry.item.is_directory {
            s.icon(
                Rect::new(tile.x + 2., tile.y + 4., 36., 32.),
                "folder",
                0xffca43,
            );
        } else {
            let color = match entry.item.kind {
                crate::domain::shelf::ShelfItemKind::Audio => 0xec4899,
                crate::domain::shelf::ShelfItemKind::Video => 0x8b5cf6,
                crate::domain::shelf::ShelfItemKind::Archive => 0xf59e0b,
                crate::domain::shelf::ShelfItemKind::Code => 0x10b981,
                _ => 0x2d5bff,
            };
            let badge = Rect::new(tile.x + 3., tile.y + 1., 34., 38.);
            s.alpha_round(badge, 7., WHITE, 0.08);
            s.alpha_border(badge, 7., WHITE, 1., 0.16);
            s.round(Rect::new(badge.x + 24., badge.y, 10., 10.), 2., color);
            s.round(Rect::new(badge.x + 3., badge.y + 19., 28., 14.), 4., color);
            s.text(
                Rect::new(badge.x + 3., badge.y + 19., 28., 14.),
                &entry.badge,
                8.5,
                WHITE,
                700u16,
                true,
            );
        }
        s.text(
            Rect::new(card.x + 2., card.y + 46., 60., 14.),
            &entry.item.display_name,
            10.,
            WHITE,
            true,
            true,
        );
        s.text(
            Rect::new(card.x + 2., card.y + 59., 60., 12.),
            &entry.size_label,
            8.8,
            DIM,
            500u16,
            true,
        );
        if !clearing {
            s.hit(
                card,
                Action::DragShelf(entry.item.id),
                &entry.item.display_name,
            );
            if hovered {
                icon_button(
                    s,
                    Rect::new(card.x + 47., card.y + 1., 16., 16.),
                    "\u{e711}",
                    Action::Intent(UserIntent::RemoveShelfItem(entry.item.id)),
                    "Remove",
                    0x000000,
                );
            }
        }
    }
    s.paint.push(Paint::Opacity(1.));
    s.unclip();
    if m.shelf_menu {
        let menu = Rect::new(b.x + 55., 30., 142., 91.);
        s.round(menu, 10., 0x25252a);
        for (i, (label, action)) in [
            (
                full("notch.shelf.addButton"),
                Action::Intent(UserIntent::BrowseShelfFiles),
            ),
            (tr("shelf-text-button"), Action::ShelfText),
            (tr("shelf-paste-button"), Action::PasteShelf),
        ]
        .into_iter()
        .enumerate()
        {
            s.button(
                Rect::new(menu.x + 5., menu.y + 5. + i as f32 * 28., 132., 24.),
                label,
                "",
                action,
                0x212121,
            );
        }
    }
}

fn hud(s: &mut Scene, m: &UiModel, b: Rect) {
    if let Some((phase, seconds)) = m.focus_notice {
        let focus = phase == FocusPhase::Focus;
        s.progress_ring(Rect::new(b.x + 13., 8., 44., 44.), 1., 0xff5c64, false);
        s.icon(
            Rect::new(b.x + 23.56, 18.56, 22.88, 22.88),
            TIMER,
            if focus { 0xff6870 } else { 0xffb7bc },
        );
        s.text(
            Rect::new(b.x + 69., 12., 126., 19.),
            tr(if focus {
                "focus-ready-title"
            } else {
                "break-ready-title"
            }),
            13.,
            WHITE,
            700u16,
            false,
        );
        s.text(
            Rect::new(b.x + 69., 32., 126., 16.),
            crate::i18n::translate_with(
                if focus {
                    "desktopUi.focus-ready-detail"
                } else {
                    "desktopUi.break-ready-detail"
                },
                &[("minutes", &(seconds / 60).to_string())],
            ),
            10.,
            DIM,
            false,
            false,
        );
        let close = Rect::new(b.x + 202., 18., 27., 27.);
        s.round(close, 14., 0x431c1b);
        s.text(close, "×", 23., 0xff6c64, false, true);
        s.hit(close, Action::Intent(UserIntent::DismissAlert), "Dismiss");
        s.button(
            Rect::new(b.x + 236., 17., 62., 29.),
            tr(if focus {
                "focus-action"
            } else {
                "break-action"
            }),
            "",
            Action::Intent(UserIntent::StartFocusFromReminder),
            0x087c31,
        );
        return;
    }
    s.text(
        Rect::new(b.x + 16., 11., if m.hud_alert { 194. } else { 270. }, 18.),
        &m.hud_title,
        12.,
        WHITE,
        true,
        false,
    );
    s.text(
        Rect::new(b.x + 16., 31., 204., 16.),
        &m.hud_detail,
        10.,
        DIM,
        false,
        false,
    );
    if let Some(level) = m.hud_level {
        percent(s, Rect::new(b.x + 16., 51., 278., 3.), level, WHITE);
    }
    if m.hud_reminder {
        s.button(
            Rect::new(b.x + 222., 27., 75., 24.),
            tr("snooze-action"),
            "",
            Action::Intent(UserIntent::SnoozeReminder),
            0x303032,
        );
    }
}
fn device_page(s: &mut Scene, m: &UiModel, b: Rect, view: NotchView, peek: bool) {
    let charging = matches!(view, NotchView::Charging | NotchView::ChargingPeek);
    let connected = m.headphones.is_some();
    let color = if charging { 0x33e07a } else { 0x5ec2ff };
    let (name, detail) = if charging {
        (
            full(if m.power.is_plugged_in {
                "notch.charging.title"
            } else {
                "notch.charging.onBatteryTitle"
            }),
            full(if m.power.is_plugged_in {
                "notch.charging.subtitle"
            } else {
                "notch.charging.onBatterySubtitle"
            }),
        )
    } else {
        m.headphones.as_ref().map_or_else(
            || {
                (
                    full("notch.headphones.disconnectedTitle"),
                    full("notch.headphones.disconnectedSubtitle"),
                )
            },
            |status| {
                (
                    if status.name.trim().is_empty() {
                        full("notch.headphones.defaultName")
                    } else {
                        status.name.clone()
                    },
                    status.battery_percent.map_or_else(
                        || full("notch.headphones.connectedNoBattery"),
                        |percent| {
                            crate::i18n::translate_with(
                                "notch.headphones.connectedWithBattery",
                                &[("percent", &percent.to_string())],
                            )
                        },
                    ),
                )
            },
        )
    };
    let percent = if charging {
        f32::from(m.power.percent)
    } else {
        m.headphones.as_ref().map_or(0., |headphones| {
            f32::from(headphones.battery_percent.unwrap_or(100))
        })
    };
    let card = if peek {
        b
    } else {
        Rect::new(b.x + 44., 34., 332., 76.)
    };
    if !peek {
        s.round(card, 16., 0x0f0f0f);
        s.border(
            card,
            16.,
            if charging && m.power.is_plugged_in {
                0x114c29
            } else if !charging && connected {
                0x1a3647
            } else {
                0x1f1f1f
            },
            1.,
        );
    }
    let ring = Rect::new(
        card.x + if peek { 20. } else { 16. },
        if peek { 15. } else { card.y + 23. },
        30.,
        30.,
    );
    s.ring_track(
        ring,
        percent / 100.,
        color,
        4.5,
        if charging {
            if peek { 0x0e3e22 } else { 0x194a2d }
        } else if peek {
            0x383838
        } else {
            0x3a3a3a
        },
    );
    s.icon(
        Rect::new(
            ring.x + if charging { 9.5 } else { 9. },
            ring.y + if charging { 8.5 } else { 9. },
            if charging { 11. } else { 12. },
            if charging { 13. } else { 12. },
        ),
        if charging { "\u{e945}" } else { "bluetooth" },
        if peek || charging && m.power.is_plugged_in || !charging && connected {
            color
        } else {
            DIM
        },
    );
    let tx = card.x + if peek { 64. } else { 58. };
    let ty = if peek {
        13.
    } else {
        card.y + if charging { 13. } else { 18. }
    };
    let text_width = if peek {
        if charging { 165. } else { 182. }
    } else if charging {
        184.
    } else {
        206.
    };
    s.text(
        Rect::new(tx, ty, text_width, 20.),
        name,
        15.,
        WHITE,
        if peek { 600u16 } else { 700u16 },
        false,
    );
    s.text(
        Rect::new(
            tx,
            ty + if peek || charging { 20. } else { 23. },
            text_width,
            17.,
        ),
        detail,
        if charging && !peek { 11. } else { 11.5 },
        if !peek && (charging && m.power.is_plugged_in || !charging && connected) {
            color
        } else {
            DIM
        },
        500u16,
        false,
    );
    if charging {
        if !peek {
            percent_bar(
                s,
                Rect::new(card.x + 58., card.y + 55., 180., 6.),
                percent / 100.,
                0x33e07a,
                0x0e3e22,
            );
        }
        s.text(
            Rect::new(
                card.x + if peek { 220. } else { 246. },
                card.y,
                if peek { 68. } else { 70. },
                card.h,
            ),
            format!("{}%", m.power.percent),
            if peek { 24. } else { 25. },
            WHITE,
            if peek { 400u16 } else { 700u16 },
            true,
        );
    } else {
        let outer = Rect::new(
            card.x + if peek { 255. } else { 274. },
            card.y + if peek { 10. } else { 16. },
            if peek { 40. } else { 44. },
            if peek { 40. } else { 44. },
        );
        s.round(
            outer,
            outer.w / 2.,
            if connected { 0x08131a } else { 0x0a0a0a },
        );
        s.border(
            outer,
            outer.w / 2.,
            if connected { 0x214458 } else { 0x1f1f1f },
            1.5,
        );
        let inner = outer.inset(if peek { 4.5 } else { 5. });
        s.round(
            inner,
            inner.w / 2.,
            if connected { 0x0d1a26 } else { 0x0f0f0f },
        );
        s.border(
            inner,
            inner.w / 2.,
            if connected { 0x366f91 } else { 0x292929 },
            1.5,
        );
        s.icon(
            Rect::new(
                inner.x + (inner.w - 16.) / 2.,
                inner.y + (inner.h - 16.) / 2.,
                16.,
                16.,
            ),
            "headphones",
            if connected { WHITE } else { DIM },
        );
    }
}
fn percent_bar(s: &mut Scene, r: Rect, value: f32, color: u32, track: u32) {
    s.round(r, r.h / 2., track);
    s.round(
        Rect::new(r.x, r.y, r.w * value.clamp(0., 1.), r.h),
        r.h / 2.,
        color,
    );
}

fn mascot(s: &mut Scene, m: &UiModel, b: Rect, now: Instant) {
    // Outgoing compact scenes are also painted while the shell expands.
    if !m.compact_mascot(now) {
        return;
    }
    let c = b.x + b.w / 2.;
    let elapsed = now.duration_since(m.start).as_secs_f32();
    let waking = m.wake_progress(now);
    let headphones = m.headphones_progress(now);
    let stalk = if headphones.is_none() && m.bell_progress(now).is_none() {
        m.stalk_amount(now)
    } else {
        0.
    };
    let crouch = (stalk * std::f32::consts::PI).sin();
    let smile = headphones.map_or(0., |p| {
        smooth_step((p - 0.30) / 0.10) * smooth_step((1. - p) / 0.13)
    });
    let reach = headphones.map_or(0., |p| {
        smooth_step(p / 0.13) * (1. - smooth_step((p - 0.22) / 0.12))
    });
    if m.mascot {
        if b.w > 100. {
            s.paint
                .push(Paint::Opacity(if m.dozing { 0.6 } else { 1. }));
            s.paint.push(Paint::Bitmap(
                Rect::new(b.x, 6., b.w, 24.),
                16,
                16,
                mascot_glow(),
            ));
            s.paint.push(Paint::Opacity(1.));
        }
        let phase = elapsed % 3.4;
        let blink = if m.dozing {
            1.
        } else if elapsed > 3.4 && phase < 0.15 {
            (phase / 0.12).clamp(0., 1.)
        } else if elapsed > 3.4 && phase < 0.27 {
            1. - ((phase - 0.15) / 0.12).clamp(0., 1.)
        } else {
            0.
        };
        let glance_step = (elapsed / 2.6).floor() as usize % 4;
        let offsets = [0., -2.4, 0., 2.4];
        let t = ((elapsed % 2.6) / 0.28).clamp(0., 1.);
        let idle_glance = if m.dozing || m.cursor_near {
            0.
        } else {
            offsets[(glance_step + 3) % 4]
                + (offsets[glance_step] - offsets[(glance_step + 3) % 4]) * (1. - (1. - t).powi(3))
        };
        let gaze = m.gaze(now);
        let glance = idle_glance * (1. - stalk) + gaze.0 * (1. - smile) * (1. + stalk - crouch);
        let greeting = waking.is_some_and(|p| p > 0.7)
            && !m.cursor_near
            && m.bell_progress(now).is_none()
            && headphones.is_none();
        let greeting_alpha = if greeting {
            waking.map_or(0., |p| {
                smooth_step((p - 0.7) / 0.08) * smooth_step((1. - p) / 0.08)
            })
        } else {
            0.
        };
        if m.dozing {
            s.icon(Rect::new(c - 20., 9., 40., 12.), "sleep-eyes", 0xf7f4f0);
        } else {
            for side in [-1., 1.] {
                let eye = c + side * (13. - 2. * crouch) + glance;
                let openness = waking.map_or(1. - blink, |p| match m.wake_style {
                    WakeStyle::Gentle => smooth_step(p / 0.65),
                    WakeStyle::OneEye => smooth_step((p - if side < 0. { 0. } else { 0.3 }) / 0.4),
                    WakeStyle::Surprise => smooth_step(p / 0.22),
                });
                let bounce = waking
                    .filter(|_| m.wake_style == WakeStyle::Surprise)
                    .map_or(0., |p| {
                        if p < 0.45 {
                            (p / 0.45 * std::f32::consts::PI).sin() * -1.5
                        } else {
                            0.
                        }
                    });
                let y = 15. + (gaze.1 + bounce) * (1. - smile) - reach * 3.;
                mascot_eye(
                    s,
                    eye,
                    y,
                    openness.max(reach),
                    (1. - greeting_alpha) * (1. - stalk) * (1. - smile),
                );
                if stalk > 0. {
                    s.paint.push(Paint::Opacity(stalk));
                    s.icon(
                        Rect::new(eye - 7., y - 4. + crouch * 3., 14., 8.),
                        if side < 0. {
                            "stalk-eye-left"
                        } else {
                            "stalk-eye-right"
                        },
                        WHITE,
                    );
                    s.paint.push(Paint::Opacity(1.));
                }
            }
        }
        if greeting {
            s.paint.push(Paint::Opacity(greeting_alpha));
            s.icon(Rect::new(c - 17., 9., 30., 10.), "wake-eyes", WHITE);
            let phase = waking.unwrap_or(0.) * 28.;
            s.icon(
                Rect::new(c + 16. + phase.sin() * 1.5, 10. + phase.cos(), 12., 13.),
                "wake-hand",
                WHITE,
            );
            s.paint.push(Paint::Opacity(1.));
        }
        if let Some(progress) = headphones {
            s.clip(b);
            let arrival = smooth_step(progress / 0.15);
            let placing = smooth_step((progress - 0.13) / 0.24);
            let fade = smooth_step(progress / 0.06) * smooth_step((1. - progress) / 0.13);
            let groove = ((progress - 0.37).max(0.) * 9. * std::f32::consts::PI).sin();
            let sway = groove * 4. * smile;
            let nod = groove.abs() * 3. * smile;
            let headphone_x = c + (1. - arrival) * -9. + sway;
            let headphone_y = -13. + arrival * 9. + placing * 6. + nod;
            s.paint.push(Paint::Opacity(fade));
            s.icon(
                Rect::new(headphone_x - 30., headphone_y, 60., 28.),
                "mascot-headphones",
                WHITE,
            );
            let hands = fade * smooth_step(progress / 0.12) * smooth_step((0.6 - progress) / 0.16);
            if hands > 0. {
                s.paint.push(Paint::Opacity(hands));
                for side in [-1., 1.] {
                    s.icon(
                        Rect::new(
                            headphone_x + side * 27. - 4.,
                            25. - arrival * 14. + placing * 5.,
                            8.,
                            9.,
                        ),
                        "mascot-paw",
                        0xf7f4f0,
                    );
                }
            }
            if smile > 0. {
                s.paint.push(Paint::Opacity(smile));
                s.icon(
                    Rect::new(c + sway - 19., 10. + nod, 38., 12.),
                    "wake-eyes",
                    WHITE,
                );
                let beat = ((progress - 0.3) * 4.).fract();
                s.paint
                    .push(Paint::Opacity(smile * (std::f32::consts::PI * beat).sin()));
                s.text(
                    Rect::new(c - 52. - beat * 5., 13. - beat * 9., 16., 16.),
                    "♪",
                    14.,
                    0xc5bdff,
                    true,
                    true,
                );
            }
            s.paint.push(Paint::Opacity(1.));
            s.unclip();
        }
        if m.dozing && m.bell_progress(now).is_none() && elapsed % 7. < 1.4 {
            let puff = elapsed % 7. / 1.4;
            s.paint.push(Paint::Opacity((1. - puff) * 0.8));
            s.text(
                Rect::new(c + 21. + puff * 4., 9. - puff * 9., 12., 15.),
                "Z",
                10.,
                DIM,
                true,
                true,
            );
            s.paint.push(Paint::Opacity(1.));
        }
    }
    mascot_bell(
        s,
        m,
        Rect::new(
            c + if headphones.is_some() { 34. } else { 24. },
            7.,
            14.,
            16.,
        ),
        now,
    );
}
fn smooth_step(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn mascot_eye(s: &mut Scene, x: f32, y: f32, openness: f32, alpha: f32) {
    let reveal = smooth_step((openness - 0.4) / 0.6);
    if reveal < 0.98 {
        s.paint.push(Paint::Opacity((1. - reveal) * alpha));
        s.icon(Rect::new(x - 7., y - 4., 14., 8.), "sleep-eye", 0xf7f4f0);
        s.paint.push(Paint::Opacity(1.));
    }
    if reveal > 0.02 {
        let width = 8.4 * reveal.sqrt();
        let height = 12.4 * reveal;
        if openness > 0.95 {
            s.paint.push(Paint::Opacity(
                smooth_step((openness - 0.95) / 0.05) * alpha,
            ));
            s.icon(
                Rect::new(x - 9.2, y - height / 2. - 5., 18.4, height + 10.),
                "eye-halo",
                0x78beff,
            );
        }
        s.paint.push(Paint::Opacity(reveal * alpha));
        s.round(
            Rect::new(x - width / 2., y - height / 2., width, height),
            width.min(height) / 2.,
            WHITE,
        );
        s.paint.push(Paint::Opacity(1.));
    }
}
fn mascot_bell(s: &mut Scene, m: &UiModel, mut rect: Rect, now: Instant) {
    if let Some(progress) = m.bell_progress(now) {
        let seconds = progress * MASCOT_ALERT_DURATION.as_secs_f32();
        rect.x += if seconds < 0.96 {
            (seconds * 52.).sin() * 2.
        } else {
            0.
        };
        let alpha = (progress / 0.05).min(1.) * ((1. - progress) / 0.1).min(1.);
        s.paint.push(Paint::Opacity(alpha));
        s.icon(rect, "bell", 0xffcf69);
        s.paint.push(Paint::Opacity(1.));
    }
}
fn mascot_glow() -> std::sync::Arc<Vec<u8>> {
    static GLOW: std::sync::OnceLock<std::sync::Arc<Vec<u8>>> = std::sync::OnceLock::new();
    GLOW.get_or_init(|| {
        let mut pixels = Vec::with_capacity(16 * 16 * 4);
        for y in 0..16 {
            for x in 0..16 {
                let t = x as f32 / 15.;
                let (a, b, u) = if t < 0.55 {
                    ([25., 184., 255.], [45., 91., 255.], t / 0.55)
                } else {
                    ([45., 91., 255.], [123., 75., 255.], (t - 0.55) / 0.45)
                };
                for i in 0..3 {
                    pixels.push((a[i] + (b[i] - a[i]) * u) as u8);
                }
                let vertical = y as f32 / 15.;
                pixels.push((107. * vertical.powf(2.5)) as u8);
            }
        }
        std::sync::Arc::new(pixels)
    })
    .clone()
}
