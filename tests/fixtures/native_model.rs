use std::time::{Duration, Instant};

use chrono::{NaiveDate, TimeZone, Utc};
use uuid::Uuid;

use crate::adapters::native::mascot::{
    MASCOT_ALERT_DURATION, MASCOT_GREETING_DURATION, MASCOT_HEADPHONES_DURATION,
    MASCOT_WAKE_DURATION, MascotClip, VARIANTS_PER_CLIP,
};
use crate::adapters::native::model::{
    MASCOT_PROXIMITY_DIPS, Motion, Rect, SURFACE_HEIGHT, SURFACE_WIDTH, Shell, UiModel,
    FLOATING_GAP, place_notch, surface_placement,
};
use crate::adapters::native::quick_share::{self, Pane as SharePane};
use crate::adapters::native::scene::{Action, Field, Paint, Preference, Scene};
use crate::adapters::native::{settings, settings::SettingsScrollbar, settings::SettingsUi, views};
use crate::application::events::{FileDropDestination, UserIntent};
use crate::application::presenter::{ShelfEntry, ViewUpdate, VoiceState};
use crate::application::quick_share::{Command as ShareCommand, Device, Snapshot, Tray};
use crate::application::reminders::{FocusPhase, UtilityState};
use crate::application::settings::AppSettings;
use crate::domain::device::{HeadphoneStatus, PowerStatus};
use crate::domain::notch::NotchView;
use crate::domain::shelf::{ShelfContent, ShelfItem, ShelfItemKind};
use crate::domain::todo::{TaskColor, TaskSchedule, TodoProgress, TodoTask};
use crate::domain::water::WaterState;

#[test]
fn shell_keeps_compact_and_expanded_views_centered_with_navigation_height() {
    let cases = [
        (NotchView::Minimal, 46., 4., 46., 4.),
        (NotchView::Closed, 150., 30., 150., 30.),
        (NotchView::Resting, 150., 30., 150., 30.),
        (NotchView::Hud, 310., 60., 310., 60.),
        (NotchView::ChargingPeek, 310., 60., 310., 60.),
        (NotchView::HeadphonesPeek, 310., 60., 310., 60.),
        (NotchView::Todo, 420., 140., 420., 170.),
        (NotchView::Lyrics, 520., 140., 520., 170.),
    ];

    for (view, width, height, nav_width, nav_height) in cases {
        let shell = Shell::for_view(view, false);
        let body = shell.body();
        assert_eq!((shell.width, shell.height), (width, height));
        assert_eq!((body.w, body.h), (width, height));
        assert_eq!(body.x + body.w / 2., SURFACE_WIDTH / 2.);

        let with_nav = Shell::for_view(view, true);
        assert_eq!((with_nav.width, with_nav.height), (nav_width, nav_height));
        let nav_body = with_nav.body();
        assert_eq!(nav_body.x + nav_body.w / 2., SURFACE_WIDTH / 2.);
    }
}

#[test]
fn quick_share_trays_match_todo_list_transition_when_expanding_and_collapsing() {
    for fast in [false, true] {
        for (pane, direction) in [(SharePane::Send, 1.), (SharePane::Receive, -1.)] {
            let start = Instant::now();
            let mut model = UiModel::new(AppSettings::default());
            model.settings.desktop.fast_animations = fast;
            let mut share = quick_share::Ui::default();
            share.navigate(pane, start, fast);
            model.set_list_expanded(true, start);
            for millis in [0, 25, 60, 100, 180, 220] {
                let now = start + Duration::from_millis(millis);
                assert!(
                    (share.expansion(now, fast) - direction * model.list_amount(now)).abs()
                        < 0.0001,
                    "{pane:?} expansion differs from To-Do at {millis}ms (fast={fast})"
                );
            }

            let collapse_at = start + Duration::from_millis(250);
            share.navigate(SharePane::Compact, collapse_at, fast);
            model.set_list_expanded(false, collapse_at);
            for millis in [0, 25, 60, 100, 180, 220] {
                let now = collapse_at + Duration::from_millis(millis);
                assert!(
                    (share.expansion(now, fast) - direction * model.list_amount(now)).abs()
                        < 0.0001,
                    "{pane:?} collapse differs from To-Do at {millis}ms (fast={fast})"
                );
            }
        }
    }
}

#[test]
fn quick_share_back_restores_the_compact_or_expanded_origin_tray() {
    for (origin, detail) in [
        (SharePane::Compact, SharePane::Radar),
        (SharePane::Send, SharePane::Radar),
        (SharePane::Compact, SharePane::Request),
        (SharePane::Receive, SharePane::Request),
    ] {
        let now = Instant::now();
        let mut share = quick_share::Ui::default();
        share.navigate(origin, now, false);
        share.navigate(detail, now + Duration::from_millis(10), false);
        assert_eq!(share.pane(), detail);

        share.back();

        assert_eq!(share.pane(), origin, "Back from {detail:?} lost its origin");
        let expected = match origin {
            SharePane::Send => 1.,
            SharePane::Receive => -1.,
            SharePane::Compact => 0.,
            _ => unreachable!(),
        };
        assert_eq!(share.expansion(now + Duration::from_millis(11), false), expected);
    }
}

#[test]
fn quick_share_os_file_drop_scene_renders_two_translucent_destination_zones() {
    let mut model = UiModel::new(AppSettings::default());
    let body = Shell::for_view(NotchView::FileDrop, false).body();
    let zones = views::file_drop_zones(body);
    assert_eq!(
        zones.map(|(destination, _)| destination),
        [FileDropDestination::Shelf, FileDropDestination::QuickShare]
    );
    let [(_, shelf), (_, quick_share)] = zones;
    assert!(shelf.w > 0. && quick_share.w > 0.);
    assert!(shelf.x + shelf.w < quick_share.x);
    assert!(shelf.y >= body.y && shelf.y + shelf.h <= body.y + body.h);
    assert!(quick_share.y >= body.y && quick_share.y + quick_share.h <= body.y + body.h);

    for active in [FileDropDestination::Shelf, FileDropDestination::QuickShare] {
        model.file_drop_destination = Some(active);
        let scene = views::notch_scene(&model, NotchView::FileDrop, Instant::now());
        let fills: Vec<_> = scene
            .paint
            .iter()
            .filter_map(|paint| match paint {
                Paint::AlphaRound(rect, _, _, alpha) => Some((*rect, *alpha)),
                _ => None,
            })
            .collect();
        assert_eq!(fills.len(), 2);
        for (destination, rect) in [
            (FileDropDestination::Shelf, shelf),
            (FileDropDestination::QuickShare, quick_share),
        ] {
            let fill = fills.iter().find(|(candidate, _)| *candidate == rect).unwrap();
            assert_eq!(fill.1, if destination == active { 0.25 } else { 0.12 });
        }
        assert!(scene_has_text(&scene, "Quick Share"));
    }
}

#[test]
fn motion_finishes_at_its_target_and_fast_motion_uses_the_shorter_duration() {
    let start = Instant::now();
    let target = Shell::for_view(NotchView::Todo, false);

    let mut normal = Motion::new(NotchView::Closed);
    normal.set(target, false, start);
    assert!(normal.running(start + Duration::from_millis(539)));
    let middle = normal.sample(start + Duration::from_millis(270));
    assert!(middle.width > Shell::for_view(NotchView::Closed, false).width);
    assert!(middle.width.is_finite());
    assert_eq!(normal.sample(start + Duration::from_millis(540)), target);

    let mut fast = Motion::new(NotchView::Closed);
    fast.set(target, true, start);
    assert!(fast.running(start + Duration::from_millis(296)));
    assert!(!fast.running(start + Duration::from_millis(297)));
    assert_eq!(fast.sample(start + Duration::from_millis(297)), target);
}

#[test]
fn mascot_visuals_only_render_in_a_stable_compact_capsule() {
    let mut closed = UiModel::new(AppSettings::default());
    closed.mascot = true;
    let now = Instant::now();
    closed
        .choreographer
        .start_clip(MascotClip::GreetingWave, now - Duration::from_millis(900));
    let waking = views::notch_scene(&closed, NotchView::Closed, now);
    assert!(scene_has_mascot_visual(&waking));
    assert!(!waking.overlay.is_empty());

    let mut resting = UiModel::new(AppSettings::default());
    resting.mascot = true;
    let transition = Instant::now();
    resting.apply(ViewUpdate::View(NotchView::Resting), transition);
    let settled = transition + Duration::from_secs(1);
    resting.dozing = true;
    resting.start = settled - Duration::from_millis(900);
    let sleeping = views::notch_scene(&resting, NotchView::Resting, settled);
    assert!(scene_has_mascot_visual(&sleeping));
    assert!(!scene_has_icon(&sleeping, "eye-halo"));
    assert!(scene_has_text(&sleeping, "Z"));
    assert!(!sleeping.overlay.is_empty());

    let noncompact_views = [
        NotchView::Music,
        NotchView::Lyrics,
        NotchView::Shelf,
        NotchView::Todo,
        NotchView::Pomodoro,
        NotchView::Water,
        NotchView::Headphones,
        NotchView::Charging,
        NotchView::Hud,
        NotchView::HeadphonesPeek,
        NotchView::ChargingPeek,
        NotchView::Minimal,
    ];
    let mascot_states = [
        (Some(Duration::from_millis(250)), false, false),
        (Some(Duration::from_millis(250)), true, false),
        (None, false, true),
    ];
    for view in noncompact_views {
        for (wake_age, alert, dozing) in mascot_states {
            let mut model = UiModel::new(AppSettings::default());
            model.mascot = true;
            let transition = Instant::now();
            model.apply(ViewUpdate::View(view), transition);
            let at = transition + Duration::from_secs(1);
            if let Some(age) = wake_age {
                model
                    .choreographer
                    .start_clip(MascotClip::WakeRubEye, at - age);
            }
            model.hud_alert = alert;
            model.alert_started = at;
            model.dozing = dozing;
            model.start = at;

            let scene = views::notch_scene(&model, view, at);
            assert!(
                !scene_has_mascot_visual(&scene),
                "mascot leaked into {view:?} with wake={wake_age:?}, alert={alert}, dozing={dozing}"
            );
        }
    }

    let mut expanding = UiModel::new(AppSettings::default());
    expanding.mascot = true;
    let transition = Instant::now();
    expanding.apply(ViewUpdate::View(NotchView::Music), transition);
    let at = transition + Duration::from_secs(1);
    expanding
        .choreographer
        .start_clip(MascotClip::WakeRubEye, at - Duration::from_millis(250));
    expanding.dozing = true;
    expanding.start = at;
    for outgoing in [NotchView::Resting, NotchView::Closed] {
        let scene = views::notch_scene(&expanding, outgoing, at);
        assert!(
            !scene_has_mascot_visual(&scene),
            "outgoing {outgoing:?} scene showed mascot while model view is expanded"
        );
    }

    let mut closing = UiModel::new(AppSettings::default());
    closing.mascot = true;
    let opening = Instant::now();
    closing.apply(ViewUpdate::View(NotchView::Music), opening);
    let close_started = opening + Duration::from_secs(1);
    closing.apply(ViewUpdate::View(NotchView::Closed), close_started);
    let during_close = close_started + Duration::from_millis(100);
    assert!(closing.motion.running(during_close));
    closing.choreographer.start_clip(
        MascotClip::WakeRubEye,
        during_close - Duration::from_millis(250),
    );
    closing.dozing = true;
    closing.start = during_close;
    let scene = views::notch_scene(&closing, NotchView::Closed, during_close);
    assert!(!scene_has_mascot_visual(&scene));

    // Unified Peeks render Mimo live inside scene.paint (keeping scene.overlay empty)
    // and consume the matching queued reaction so it does not replay upon closing.
    let mut peek_charging = UiModel::new(AppSettings::default());
    peek_charging.mascot = true;
    let t_peek = Instant::now();
    peek_charging.apply(
        ViewUpdate::Power(PowerStatus {
            has_battery: true,
            is_plugged_in: false,
            percent: 60,
        }),
        t_peek,
    );
    peek_charging.apply(
        ViewUpdate::Power(PowerStatus {
            has_battery: true,
            is_plugged_in: true,
            percent: 61,
        }),
        t_peek + Duration::from_secs(1),
    );
    assert!(peek_charging.choreographer.has_pending(MascotClip::ChargerZap));
    peek_charging.apply(
        ViewUpdate::View(NotchView::ChargingPeek),
        t_peek + Duration::from_secs(1),
    );
    assert!(
        !peek_charging.choreographer.has_pending(MascotClip::ChargerZap),
        "ChargingPeek claims ChargerZap so it does not replay after closing"
    );
    let charging_scene = views::notch_scene(
        &peek_charging,
        NotchView::ChargingPeek,
        t_peek + Duration::from_millis(1500),
    );
    assert!(scene_has_mascot_visual(&charging_scene));
    assert!(charging_scene.overlay.is_empty());
    peek_charging.apply(
        ViewUpdate::View(NotchView::Closed),
        t_peek + Duration::from_secs(4),
    );
    assert!(
        !peek_charging
            .advance_mascot_reactions(t_peek + Duration::from_secs(5))
    );
}

#[test]
fn mascot_alert_waits_for_a_stable_compact_capsule_and_expires_without_mascot() {
    let mut model = UiModel::new(AppSettings::default());
    model.mascot = true;
    model.dozing = true;
    let started = Instant::now();
    model.apply(ViewUpdate::View(NotchView::Music), started);
    let expanded_at = started + Duration::from_secs(1);

    model.apply(ViewUpdate::MascotAlert, expanded_at);
    assert!(model.choreographer.has_pending(MascotClip::BellAlert));
    assert!(
        model
            .choreographer
            .active_started_at(MascotClip::BellAlert, expanded_at)
            .is_none()
    );
    let expanded = views::notch_scene(&model, NotchView::Music, expanded_at);
    assert!(!scene_has_mascot_visual(&expanded));

    let closing_at = expanded_at + Duration::from_millis(1);
    model.apply(ViewUpdate::View(NotchView::Closed), closing_at);
    let while_closing = closing_at + Duration::from_millis(100);
    assert!(!model.advance_mascot_reactions(while_closing));
    assert!(model.choreographer.has_pending(MascotClip::BellAlert));
    assert!(
        model
            .choreographer
            .active_started_at(MascotClip::BellAlert, while_closing)
            .is_none()
    );

    let compact_at = closing_at + Duration::from_secs(1);
    assert!(model.advance_mascot_reactions(compact_at));
    assert!(!model.choreographer.has_pending(MascotClip::BellAlert));
    assert_eq!(
        model
            .choreographer
            .active_started_at(MascotClip::BellAlert, compact_at),
        Some(compact_at)
    );
    assert_eq!(model.bell_progress(compact_at), Some(0.));
    assert!(!model.dozing, "a bell alert wakes the dozing mascot");
    let compact = views::notch_scene(
        &model,
        NotchView::Closed,
        compact_at + Duration::from_millis(100),
    );
    assert!(scene_has_mascot_visual(&compact));
    assert!(scene_has_text(&compact, "!"));
    assert!(
        model
            .bell_progress(compact_at + MASCOT_ALERT_DURATION)
            .is_none()
    );
    let expired = views::notch_scene(
        &model,
        NotchView::Closed,
        compact_at + MASCOT_ALERT_DURATION,
    );
    assert!(!scene_has_mascot_visual(&expired));

    let mut disabled = UiModel::new(AppSettings::default());
    disabled.mascot = false;
    disabled.apply(ViewUpdate::MascotAlert, compact_at);
    assert!(!disabled.choreographer.has_pending(MascotClip::BellAlert));
    assert!(disabled.bell_progress(compact_at).is_none());

    let mut wake_chain = UiModel::new(AppSettings::default());
    wake_chain.mascot = true;
    let wake_at = Instant::now();
    wake_chain.dozing = true;
    wake_chain.wake_mascot(wake_at);
    assert!(!wake_chain.dozing);
    assert_eq!(
        wake_chain.wake_progress(wake_at + Duration::from_millis(400)),
        Some(0.25)
    );
    assert!(
        wake_chain
            .greeting_progress(wake_at + Duration::from_millis(400))
            .is_none()
    );
    let greet_sample = wake_at + MASCOT_WAKE_DURATION + Duration::from_millis(420);
    assert!(wake_chain.wake_progress(greet_sample).is_none());
    assert!(wake_chain.greeting_progress(greet_sample).is_some());
    assert!(
        wake_chain
            .greeting_progress(wake_at + MASCOT_WAKE_DURATION + MASCOT_GREETING_DURATION)
            .is_none()
    );
}

#[test]
fn new_headphones_reaction_runs_once_in_the_compact_capsule_and_disconnect_cancels_it() {
    let mut model = UiModel::new(AppSettings::default());
    model.mascot = true;
    model.dozing = true;
    let connected_at = Instant::now();
    let status = |battery_percent| HeadphoneStatus {
        name: "Fixture headphones".into(),
        battery_percent,
    };

    // The initial scan and later battery refresh only publish current state.
    model.apply(ViewUpdate::Headphones(Some(status(None))), connected_at);
    model.apply(
        ViewUpdate::Headphones(Some(status(Some(92)))),
        connected_at + Duration::from_secs(1),
    );
    assert!(!model.choreographer.has_pending(MascotClip::Headphones));
    assert!(
        model
            .headphones_progress(connected_at + Duration::from_secs(1))
            .is_none()
    );

    // A new-connection reaction is queued until the stable compact capsule tick.
    let reaction_at = connected_at + Duration::from_secs(1);
    model.apply(ViewUpdate::MascotHeadphones, reaction_at);
    assert!(model.choreographer.has_pending(MascotClip::Headphones));
    assert!(model.advance_mascot_reactions(reaction_at));
    assert!(!model.choreographer.has_pending(MascotClip::Headphones));
    assert_eq!(
        model
            .choreographer
            .active_started_at(MascotClip::Headphones, reaction_at),
        Some(reaction_at)
    );
    assert!(!model.dozing, "new headphones wake a sleeping mascot");
    assert_eq!(model.view, NotchView::Closed);

    let middle = reaction_at + Duration::from_millis(600);
    assert!(model.headphones_progress(middle).is_some());
    let scene = views::notch_scene(&model, NotchView::Closed, middle);
    assert!(scene_has_mascot_visual(&scene));
    assert!(!scene.overlay.is_empty());

    model.apply(
        ViewUpdate::Headphones(Some(status(Some(71)))),
        middle + Duration::from_millis(100),
    );
    assert_eq!(
        model.choreographer.active_started_at(
            MascotClip::Headphones,
            middle + Duration::from_millis(100)
        ),
        Some(reaction_at)
    );
    assert!(
        model
            .headphones_progress(middle + Duration::from_millis(100))
            .is_some()
    );
    assert!(model.view == NotchView::Closed && !model.expanded());
    assert!(
        model
            .headphones_progress(
                reaction_at + MASCOT_HEADPHONES_DURATION - Duration::from_millis(1)
            )
            .is_some()
    );
    assert!(
        model
            .headphones_progress(reaction_at + MASCOT_HEADPHONES_DURATION)
            .is_none()
    );

    model.apply(
        ViewUpdate::Headphones(None),
        middle + Duration::from_millis(200),
    );
    assert!(!model.choreographer.has_pending(MascotClip::Headphones));
    assert!(
        model
            .headphones_progress(middle + Duration::from_millis(200))
            .is_none()
    );
}

#[test]
fn mascot_proximity_wakes_and_gazes_at_cursor_within_five_cm_without_expanding() {
    let base_radius = MASCOT_PROXIMITY_DIPS;
    for scale_ratio in [1., 1.5] {
        let radius = base_radius * scale_ratio;
        let mut model = UiModel::new(AppSettings::default());
        model.mascot = true;
        model.dozing = true;
        let now = Instant::now();
        let pill = Shell::for_view(NotchView::Closed, false).body();
        let beyond_right_edge = radius * 0.3;
        let x = pill.x + pill.w + beyond_right_edge;
        let vertical_at_edge = (radius * radius - beyond_right_edge * beyond_right_edge).sqrt();
        let y_near = pill.h + vertical_at_edge - 1.;
        let y_far = pill.h + vertical_at_edge + 1.;

        assert!(model.observe_mascot_cursor(Some((x, y_near)), radius, now));
        assert!(model.cursor_near);
        assert!(!model.dozing, "near cursor wakes the sleeping mascot");
        assert!(
            model.wake_progress(now).is_none() && model.greeting_progress(now).is_none(),
            "cursor proximity enters stalking directly without overlapping wake/greeting"
        );
        assert_eq!(model.view, NotchView::Closed);
        assert!(!model.expanded());

        let gaze = model.gaze(now + Duration::from_millis(200));
        assert!(
            gaze.0 > 1.9 && gaze.0 <= 3.5,
            "mascot should look toward the cursor: {gaze:?}"
        );
        assert!(
            gaze.1 > 1.9,
            "mascot should look down toward the cursor: {gaze:?}"
        );

        assert!(model.observe_mascot_cursor(Some((x, y_far)), radius, now));
        assert!(!model.cursor_near);
        assert_eq!(model.view, NotchView::Closed);
    }

    for (mascot, fullscreen, view) in [
        (false, false, NotchView::Closed),
        (true, true, NotchView::Closed),
        (true, false, NotchView::Music),
    ] {
        let mut model = UiModel::new(AppSettings::default());
        model.mascot = mascot;
        model.fullscreen = fullscreen;
        model.dozing = true;
        let now = Instant::now();
        model.apply(ViewUpdate::View(view), now);
        let at = now + Duration::from_secs(1);
        let pill = Shell::for_view(NotchView::Closed, false).body();
        let x = pill.x + pill.w / 2.;
        let y = pill.h + MASCOT_PROXIMITY_DIPS / 2.;
        assert!(!model.observe_mascot_cursor(Some((x, y)), MASCOT_PROXIMITY_DIPS, at));
        assert!(!model.cursor_near);
        assert!(model.dozing);
        assert!(model.wake_progress(at).is_none());
    }
}

#[test]
fn mascot_choreographer_prevents_overlaps_and_queues_event_reactions() {
    let mut model = UiModel::new(AppSettings::default());
    model.mascot = true;
    model.dozing = true;
    let t0 = Instant::now();

    // 1. Active greeting wave is protected from cursor-proximity interruption; paw swipe waits.
    model.wake_mascot(t0);
    let greet_at = t0 + MASCOT_WAKE_DURATION + Duration::from_millis(300);
    assert!(model.greeting_progress(greet_at).is_some());
    assert!(model.observe_mascot_cursor(
        Some((SURFACE_WIDTH / 2. + 20., 44.)),
        MASCOT_PROXIMITY_DIPS,
        greet_at,
    ));
    assert!(model.greeting_progress(greet_at).is_some());
    assert!(model.choreographer.paw_swipe(greet_at).is_none());
    let after_greet = t0 + MASCOT_WAKE_DURATION + MASCOT_GREETING_DURATION + Duration::from_millis(20);
    assert!(model.observe_mascot_cursor(
        Some((SURFACE_WIDTH / 2. + 24., 44.)),
        MASCOT_PROXIMITY_DIPS,
        after_greet,
    ));
    assert!(model.choreographer.paw_swipe(after_greet).is_some());
    model.observe_mascot_cursor(
        None,
        MASCOT_PROXIMITY_DIPS,
        after_greet + Duration::from_secs(1),
    );

    // 2. Concurrent P3 reactions queue in FIFO order instead of overlapping, and pause during
    // transient non-alert HUDs (e.g. Volume) instead of expiring unseen.
    let q_at = after_greet + Duration::from_secs(2);
    model.apply(
        ViewUpdate::Headphones(Some(HeadphoneStatus {
            name: "Studio".into(),
            battery_percent: Some(90),
        })),
        q_at,
    );
    model.apply(ViewUpdate::MascotHeadphones, q_at);
    model.apply(ViewUpdate::MascotAlert, q_at);
    assert!(model.headphones_progress(q_at + Duration::from_millis(200)).is_some());
    assert!(model.bell_progress(q_at + Duration::from_millis(200)).is_none());
    assert!(model.choreographer.has_pending(MascotClip::BellAlert));

    let after_hp = q_at + MASCOT_HEADPHONES_DURATION + Duration::from_millis(10);
    assert!(model.advance_mascot_reactions(after_hp));
    let before_hud = after_hp + Duration::from_millis(700);
    let progress_before_hud = model.bell_progress(before_hud).unwrap();
    assert!((progress_before_hud - 0.25).abs() < 0.02);

    model.apply(
        ViewUpdate::Hud {
            title_key: "desktopUi.volume",
            detail: "60%".into(),
            level: Some(0.6),
            reminder: false,
            alert: false,
        },
        before_hud,
    );
    model.apply(ViewUpdate::View(NotchView::Hud), before_hud);
    let hud_close = before_hud + Duration::from_millis(1500);
    model.apply(ViewUpdate::View(NotchView::Closed), hud_close);
    let resumed_at = hud_close + Duration::from_millis(600);
    assert!(model.advance_mascot_reactions(resumed_at));
    let progress_resumed = model.bell_progress(resumed_at).unwrap();
    assert!(
        (progress_resumed - progress_before_hud).abs() < 0.02,
        "BellAlert pauses during Volume HUD and resumes from {progress_before_hud}, got {progress_resumed}"
    );

    // 3. Initial state hydration does not fire reactions; subsequent state deltas do.
    let mut events = UiModel::new(AppSettings::default());
    events.mascot = true;
    let e0 = Instant::now();
    let water = |consumed_ml| WaterState {
        enabled: true,
        consumed_ml,
        goal_ml: 2000,
        bottle_ml: 500,
        hourly_ml: [0; 24],
        active_from_minute: 480,
        active_until_minute: 1020,
        next_at: None,
    };
    let utility = |focus_running| UtilityState {
        phase: FocusPhase::Focus,
        focus_seconds: 1500,
        focus_running,
        focus_total_seconds: 1500,
        task_id: None,
        task_title: String::new(),
        task_seconds: 0,
        can_snooze: false,
    };
    let shelf_item = |id| ShelfEntry {
        item: ShelfItem {
            id: Uuid::from_u128(id),
            content: ShelfContent::Text("note".into()),
            display_name: "note".into(),
            size_bytes: 4,
            is_directory: false,
            kind: ShelfItemKind::Text,
            added_at: Utc.with_ymd_and_hms(2026, 11, 3, 12, 0, 0).unwrap(),
        },
        badge: "TXT".into(),
        size_label: "4 B".into(),
        thumbnail: None,
    };

    events.apply(ViewUpdate::Water(water(500)), e0);
    events.apply(ViewUpdate::Utility(utility(false)), e0);
    events.apply(ViewUpdate::ShelfItems(vec![shelf_item(1)]), e0);
    events.apply(
        ViewUpdate::Tasks {
            tasks: vec![task(1, date(2026, 11, 3))],
            progress: TodoProgress { done: 0, total: 1 },
        },
        e0,
    );
    events.apply(
        ViewUpdate::Power(PowerStatus {
            has_battery: true,
            is_plugged_in: false,
            percent: 50,
        }),
        e0,
    );
    assert!(!events.advance_mascot_reactions(e0));

    events.apply(ViewUpdate::Water(water(750)), e0 + Duration::from_secs(1));
    assert!(events.choreographer.has_pending(MascotClip::WaterSip));
    events.choreographer.clear_active();

    events.apply(ViewUpdate::Utility(utility(true)), e0 + Duration::from_secs(2));
    assert!(events.choreographer.has_pending(MascotClip::PomodoroStart));

    events.apply(
        ViewUpdate::ShelfItems(vec![shelf_item(1), shelf_item(2)]),
        e0 + Duration::from_secs(3),
    );
    assert!(events.choreographer.has_pending(MascotClip::FileCatch));

    let mut events2 = UiModel::new(AppSettings::default());
    events2.mascot = true;
    events2.apply(
        ViewUpdate::Tasks {
            tasks: vec![task(1, date(2026, 11, 3))],
            progress: TodoProgress { done: 0, total: 1 },
        },
        e0,
    );
    events2.apply(
        ViewUpdate::Tasks {
            tasks: vec![task(1, date(2026, 11, 3))],
            progress: TodoProgress { done: 1, total: 1 },
        },
        e0 + Duration::from_secs(1),
    );
    assert!(events2.choreographer.has_pending(MascotClip::TaskCheer));

    events2.apply(
        ViewUpdate::Power(PowerStatus {
            has_battery: true,
            is_plugged_in: false,
            percent: 45,
        }),
        e0,
    );
    events2.apply(
        ViewUpdate::Power(PowerStatus {
            has_battery: true,
            is_plugged_in: true,
            percent: 46,
        }),
        e0 + Duration::from_secs(2),
    );
    assert!(events2.choreographer.has_pending(MascotClip::ChargerZap));

    events2.apply(ViewUpdate::Voice(VoiceState::Recording), e0 + Duration::from_secs(3));
    events2.apply(ViewUpdate::Voice(VoiceState::Idle), e0 + Duration::from_secs(4));
    assert!(events2.choreographer.has_pending(MascotClip::VoiceReaction));
}

#[test]
fn mascot_events_cycle_through_three_distinct_animation_variants() {
    assert_eq!(VARIANTS_PER_CLIP, 3);
    let mut model = UiModel::new(AppSettings::default());
    model.mascot = true;
    model.view = NotchView::Resting;
    model.motion = Motion::new(NotchView::Resting);

    let mut at = Instant::now();
    let mut alert_overlays = Vec::new();
    for expected in [0_u8, 1, 2, 0] {
        model.apply(ViewUpdate::MascotAlert, at);
        let sample_at = at + Duration::from_millis(600);
        assert_eq!(
            model.choreographer.active_variant(sample_at, true, false),
            Some(expected)
        );
        if alert_overlays.len() < 3 {
            let scene = views::notch_scene(&model, NotchView::Resting, sample_at);
            assert!(!scene.overlay.is_empty());
            alert_overlays.push((scene.overlay.len(), scene.overlay_bounds()));
        }
        at += MASCOT_ALERT_DURATION + Duration::from_millis(50);
    }
    assert_ne!(alert_overlays[0], alert_overlays[1]);
    assert_ne!(alert_overlays[1], alert_overlays[2]);
    assert_ne!(alert_overlays[0], alert_overlays[2]);

    for expected in [0_u8, 1, 2, 0] {
        model.dozing = true;
        model.wake_mascot(at);
        assert_eq!(
            model
                .choreographer
                .active_variant(at + Duration::from_millis(500), true, false),
            Some(expected)
        );
        assert_eq!(
            model.choreographer.active_variant(
                at + MASCOT_WAKE_DURATION + Duration::from_millis(500),
                true,
                false
            ),
            Some(expected)
        );
        at += MASCOT_WAKE_DURATION + MASCOT_GREETING_DURATION + Duration::from_millis(50);
    }

    for expected in [0_u8, 1, 2, 0] {
        assert_eq!(model.choreographer.voice_live_variant(), expected);
        model.apply(ViewUpdate::Voice(VoiceState::Recording), at);
        model.apply(
            ViewUpdate::Voice(VoiceState::Idle),
            at + Duration::from_millis(200),
        );
        let react_at = at + Duration::from_millis(250);
        assert!(model.advance_mascot_reactions(react_at));
        assert_eq!(
            model
                .choreographer
                .active_variant(react_at + Duration::from_millis(400), true, false),
            Some(expected)
        );
        at += Duration::from_secs(4);
        let _ = model.advance_mascot_reactions(at);
    }
}

#[test]
fn navigation_stays_open_between_expanded_pages_and_closes_on_compact_view() {
    let mut model = UiModel::new(AppSettings::default());
    let now = Instant::now();
    model.apply(ViewUpdate::View(NotchView::Music), now);
    model.set_nav(true, now + Duration::from_millis(1));

    model.apply(
        ViewUpdate::View(NotchView::Todo),
        now + Duration::from_millis(2),
    );
    assert!(model.nav);

    model.apply(
        ViewUpdate::View(NotchView::Closed),
        now + Duration::from_millis(3),
    );
    assert!(!model.nav);
}

#[test]
fn quick_share_is_a_navigable_page_with_its_own_view() {
    use crate::domain::notch::{NotchContext, NotchPage, resolve_view};

    assert_eq!(NotchPage::Shelf.step(1, true), NotchPage::QuickShare);
    assert_eq!(NotchPage::QuickShare.step(-1, true), NotchPage::Shelf);
    assert_eq!(NotchPage::QuickShare.step(1, true), NotchPage::Todo);
    assert_eq!(NotchPage::QuickShare.view(), NotchView::QuickShare);
    assert_eq!(
        resolve_view(&NotchContext {
            is_hovered: true,
            page: NotchPage::QuickShare,
            ..NotchContext::default()
        }),
        NotchView::QuickShare
    );
}

#[test]
fn quick_share_scene_exposes_send_receive_and_request_actions() {
    use crate::adapters::native::quick_share::Pane;
    use crate::application::quick_share::{Command, Request, Snapshot};

    let mut model = UiModel::new(AppSettings::default());
    let now = Instant::now();

    let compact = views::notch_scene(&model, NotchView::QuickShare, now);
    assert!(compact.hits.iter().any(|hit| {
        hit.action == Action::SharePane(Pane::Send)
    }));
    assert!(compact.hits.iter().any(|hit| {
        hit.action == Action::SharePane(Pane::Receive)
    }));
    assert!(compact.hits.iter().any(|hit| {
        hit.action == Action::Share(Command::Browse)
    }));

    model.quick_share_ui.navigate(Pane::Send, now, false);
    let send = views::notch_scene(&model, NotchView::QuickShare, now);
    assert!(send.hits.iter().any(|hit| {
        hit.action == Action::SharePane(Pane::Radar)
    }));
    model.quick_share_ui.navigate(Pane::Radar, now, false);
    let radar = views::notch_scene(&model, NotchView::QuickShare, now);
    assert!(radar.hits.iter().any(|hit| hit.action == Action::ShareBack));

    model.quick_share_ui.navigate(Pane::Receive, now, false);
    model.quick_share_ui.navigate(Pane::Request, now, false);
    model.quick_share.request = Some(Request {
        id: "request-id".into(),
        sender: "test device".into(),
        pin: "123456".into(),
        files: vec!["test.txt".into()],
        bytes: 4,
    });
    let request = views::notch_scene(&model, NotchView::QuickShare, now);
    assert!(request.hits.iter().any(|hit| hit.action == Action::ShareBack));
    for accept in [false, true] {
        assert!(request.hits.iter().any(|hit| {
            hit.action == Action::Share(Command::Decide {
                id: "request-id".into(),
                accept,
            })
        }));
    }

    model.apply(ViewUpdate::QuickShare(Snapshot::default()), now);
    assert_eq!(model.quick_share_ui.pane(), Pane::Receive);
}

#[test]
fn shelf_and_quick_share_removals_fade_for_280ms_and_keep_survivors_clickable() {
    let removed = shelf_entry(41, "removed-file.txt");
    let survivor = shelf_entry(42, "kept-file.txt");

    let shelf_start = Instant::now();
    let mut shelf = UiModel::new(AppSettings::default());
    shelf.apply(
        ViewUpdate::ShelfItems(vec![removed.clone(), survivor.clone()]),
        shelf_start,
    );
    let before_clear = views::notch_scene(&shelf, NotchView::Shelf, shelf_start);
    assert!(before_clear.hits.iter().any(|hit| {
        hit.action == Action::Intent(UserIntent::ClearShelf)
    }));
    let removed_card = before_clear
        .hits
        .iter()
        .find(|hit| hit.action == Action::DragShelf(removed.item.id))
        .unwrap();
    shelf.mouse = (
        removed_card.rect.x + removed_card.rect.w / 2.,
        removed_card.rect.y + removed_card.rect.h / 2.,
    );
    let hovered = views::notch_scene(&shelf, NotchView::Shelf, shelf_start);
    assert!(hovered.hits.iter().any(|hit| {
        hit.action == Action::Intent(UserIntent::RemoveShelfItem(removed.item.id))
    }));

    let shelf_removed_at = shelf_start + Duration::from_millis(500);
    shelf.apply(
        ViewUpdate::ShelfItems(vec![survivor.clone()]),
        shelf_removed_at,
    );
    for (elapsed, opacity, ghost) in [
        (0, Some(1.), true),
        (140, Some(0.5), true),
        (310, None, false),
    ] {
        assert_removal_frame(
            &mut shelf,
            NotchView::Shelf,
            shelf_removed_at + Duration::from_millis(elapsed),
            &removed,
            &survivor,
            None,
            opacity,
            ghost,
        );
    }

    let share_start = Instant::now();
    let mut share = UiModel::new(AppSettings::default());
    share.apply(
        ViewUpdate::View(NotchView::QuickShare),
        share_start,
    );
    share
        .quick_share_ui
        .navigate(SharePane::Receive, share_start, false);
    share.apply(
        ViewUpdate::QuickShare(Snapshot {
            received: vec![removed.clone(), survivor.clone()],
            ..Snapshot::default()
        }),
        share_start,
    );
    let before_clear_at = share_start + Duration::from_millis(500);
    let before_clear = views::notch_scene(&share, NotchView::QuickShare, before_clear_at);
    assert!(before_clear.hits.iter().any(|hit| {
        hit.action == Action::Share(ShareCommand::Clear(Tray::Received))
    }));
    let removed_card = before_clear
        .hits
        .iter()
        .find(|hit| hit.action == Action::DragSharedFile(removed.item.id))
        .unwrap();
    share.mouse = (
        removed_card.rect.x + removed_card.rect.w / 2.,
        removed_card.rect.y + removed_card.rect.h / 2.,
    );
    let hovered = views::notch_scene(&share, NotchView::QuickShare, before_clear_at);
    assert!(hovered.hits.iter().any(|hit| {
        hit.action == Action::Share(ShareCommand::Remove(
            Tray::Received,
            removed.item.id,
        ))
    }));

    let share_removed_at = before_clear_at;
    share.apply(
        ViewUpdate::QuickShare(Snapshot {
            received: vec![survivor.clone()],
            ..Snapshot::default()
        }),
        share_removed_at,
    );
    for (elapsed, opacity, ghost) in [
        (0, Some(1.), true),
        (140, Some(0.5), true),
        (310, None, false),
    ] {
        assert_removal_frame(
            &mut share,
            NotchView::QuickShare,
            share_removed_at + Duration::from_millis(elapsed),
            &removed,
            &survivor,
            Some(Tray::Received),
            opacity,
            ghost,
        );
    }
}

#[test]
fn quick_share_clear_uses_the_shared_fade_for_all_outgoing_files() {
    let file = shelf_entry(51, "cleared-file.txt");
    let start = Instant::now();
    let mut model = UiModel::new(AppSettings::default());
    model.apply(ViewUpdate::View(NotchView::QuickShare), start);
    model
        .quick_share_ui
        .navigate(SharePane::Send, start, false);
    model.apply(
        ViewUpdate::QuickShare(Snapshot {
            outgoing: vec![file.clone()],
            ..Snapshot::default()
        }),
        start,
    );

    let clear_at = start + Duration::from_millis(500);
    let before_clear = views::notch_scene(&model, NotchView::QuickShare, clear_at);
    assert!(before_clear.hits.iter().any(|hit| {
        hit.action == Action::Share(ShareCommand::Clear(Tray::Outgoing))
    }));
    model.apply(
        ViewUpdate::QuickShare(Snapshot::default()),
        clear_at,
    );

    let halfway = clear_at + Duration::from_millis(140);
    let scene = views::notch_scene(&model, NotchView::QuickShare, halfway);
    assert_eq!(scene_opacity(&scene, "cleared-file.txt"), Some(0.5));
    assert!(scene_has_text(&scene, "cleared-file.txt"));
    assert!(model.animating(halfway));
    assert!(!scene.hits.iter().any(|hit| {
        hit.action == Action::Share(ShareCommand::Remove(Tray::Outgoing, file.item.id))
    }));

    let finished = views::notch_scene(
        &model,
        NotchView::QuickShare,
        clear_at + Duration::from_millis(310),
    );
    assert_eq!(scene_opacity(&finished, "cleared-file.txt"), None);
    assert!(!scene_has_text(&finished, "cleared-file.txt"));
}

#[test]
fn quick_share_radar_rotates_pulses_and_keeps_device_send_clickable() {
    let start = Instant::now();
    let mut model = UiModel::new(AppSettings::default());
    model.start = start;
    model.apply(ViewUpdate::View(NotchView::QuickShare), start);
    model.quick_share_ui.navigate(SharePane::Radar, start, false);
    model.apply(
        ViewUpdate::QuickShare(Snapshot {
            outgoing: vec![shelf_entry(61, "send.txt")],
            devices: vec![Device {
                id: "nearby-id".into(),
                name: "Nearby device".into(),
            }],
            ..Snapshot::default()
        }),
        start,
    );

    let first_at = start + Duration::from_millis(200);
    let second_at = start + Duration::from_millis(800);
    assert!(!model.quick_share.visible);
    assert!(model.animating(first_at));
    assert!(model.animating(second_at));

    let first = views::notch_scene(&model, NotchView::QuickShare, first_at);
    let second = views::notch_scene(&model, NotchView::QuickShare, second_at);
    let leader = |scene: &Scene| {
        scene.paint.iter().find_map(|paint| match paint {
            Paint::Line(_, _, x, y, 0x69b4fa, _) => Some((*x, *y)),
            _ => None,
        })
    };
    assert_ne!(leader(&first), leader(&second));
    let pulse = |scene: &Scene| {
        scene.paint.iter().find_map(|paint| match paint {
            Paint::Ellipse {
                center,
                radius,
                color: 0x73b5ff,
                alpha,
                stroke: Some(1.1),
            } => Some((*center, *radius, *alpha)),
            _ => None,
        })
    };
    let first_pulse = pulse(&first).unwrap();
    let second_pulse = pulse(&second).unwrap();
    assert_eq!(first_pulse.0, second_pulse.0);
    assert_ne!(first_pulse.1, second_pulse.1);
    assert_ne!(first_pulse.2, second_pulse.2);
    for scene in [&first, &second] {
        assert!(scene.hits.iter().any(|hit| {
            hit.action == Action::Share(ShareCommand::Send("nearby-id".into()))
        }));
    }

    let changed_view_at = second_at + Duration::from_millis(1);
    model.apply(ViewUpdate::View(NotchView::Shelf), changed_view_at);
    assert!(!model.animating(changed_view_at + Duration::from_secs(1)));
}

#[test]
fn hud_update_preserves_todo_reminder_and_exposes_snooze_action() {
    let mut model = UiModel::new(AppSettings::default());
    let now = Instant::now();
    model.apply(
        ViewUpdate::Hud {
            title_key: "desktopUi.task-due",
            detail: "Read a book".into(),
            level: None,
            reminder: true,
            alert: true,
        },
        now,
    );

    assert!(model.hud_reminder);
    let scene = views::notch_scene(&model, NotchView::Hud, now);
    assert!(
        scene
            .hits
            .iter()
            .any(|hit| { hit.action == Action::Intent(UserIntent::SnoozeReminder) })
    );

    model.apply(
        ViewUpdate::Hud {
            title_key: "desktopUi.volume",
            detail: "50%".into(),
            level: Some(0.5),
            reminder: false,
            alert: false,
        },
        now + Duration::from_millis(1),
    );
    assert!(!model.hud_reminder);
}

#[test]
fn surface_placement_keeps_monitor_center_through_dpi_scale_changes() {
    let monitor = Rect::new(-1920., -200., 2560., 1440.);
    let center_x = monitor.x + monitor.w / 2.;
    let center_y = monitor.y;

    for percent in [100, 115, 130, 150, 130, 115, 100] {
        let placement = surface_placement(monitor, 1.25, percent).unwrap();
        let expected_scale = 1.25 * f32::from(percent) / 100.;
        let expected_width = (SURFACE_WIDTH * expected_scale).ceil();
        let expected_height = (SURFACE_HEIGHT * expected_scale).ceil();

        assert!((placement.scale - expected_scale).abs() < f32::EPSILON);
        assert_eq!(placement.bounds.w, expected_width);
        assert_eq!(placement.bounds.h, expected_height);
        assert!((placement.bounds.x + placement.bounds.w / 2. - center_x).abs() < 1.);
        assert_eq!(placement.bounds.y, center_y);
    }
}

#[test]
fn floating_placement_adds_the_scaled_top_gap_without_changing_surface_size() {
    let monitor = Rect::new(-1920., -200., 2560., 1440.);
    let scale = 1.25 * 130. / 100.;
    let attached = place_notch(monitor, 1.25, 130, false).unwrap();
    let floating = place_notch(monitor, 1.25, 130, true).unwrap();

    assert_eq!(floating.scale, scale);
    assert_eq!(attached.bounds.y, monitor.y);
    assert_eq!(floating.bounds.y, monitor.y + (FLOATING_GAP * scale).round());
    assert_eq!(floating.bounds.x, attached.bounds.x);
    assert_eq!(floating.bounds.w, attached.bounds.w);
    assert_eq!(floating.bounds.h, attached.bounds.h);
}

#[test]
fn focus_day_accumulates_completions_and_resets_on_date_rollover() {
    use crate::domain::widgets::FocusDay;

    let yesterday = date(2026, 11, 11);
    let today = date(2026, 11, 12);
    let previous = FocusDay {
        date: yesterday,
        minutes: 75,
        sessions: 3,
    };

    assert_eq!(previous.for_date(today), FocusDay::empty(today));
    let first = previous.complete(today, 25);
    assert_eq!(first.minutes, 25);
    assert_eq!(first.sessions, 1);

    let second = first.complete(today, 40);
    assert_eq!(second.minutes, 65);
    assert_eq!(second.sessions, 2);
}

#[test]
fn settings_scrollbar_maps_track_click_drag_offset_and_scroll_endpoints() {
    const WIDTH: f32 = 654.;
    const HEIGHT: f32 = 506.;
    let model = UiModel::new(AppSettings::default());
    let mut ui = SettingsUi::new(&model.settings, true);
    ui.section = 4;
    ui.changed = Instant::now() - Duration::from_secs(1);
    let (scene, max_scroll) =
        settings::settings_scene(&model, &ui, &[], WIDTH, HEIGHT, Instant::now());
    assert!(max_scroll > 0.);
    let track_hit = scene
        .hits
        .iter()
        .find(|hit| hit.action == Action::ScrollSettings)
        .expect("scrollable settings expose a track hit target");
    assert_eq!(track_hit.rect.w, 13.);

    let track = Rect::new(637., 80., 13., 400.);
    let max_scroll = 840.;
    let middle = SettingsScrollbar::new(track, max_scroll, max_scroll / 2.);
    let travel = track.h - middle.thumb.h;
    assert_eq!(middle.thumb.y, track.y + travel / 2.);

    let grab_offset = middle.grab_offset(middle.thumb.y + 17.);
    assert_eq!(grab_offset, 17.);
    assert_eq!(middle.scroll_at(track.y + grab_offset, grab_offset), 0.);
    assert_eq!(
        middle.scroll_at(track.y + travel + grab_offset, grab_offset),
        max_scroll
    );

    let target = middle.scroll_at(track.y + travel * 0.75 + grab_offset, grab_offset);
    assert!((target - max_scroll * 0.75).abs() < 0.01);

    let click_y = track.y + track.h * 0.8;
    let click_offset = middle.grab_offset(click_y);
    assert_eq!(click_offset, middle.thumb.h / 2.);
    let click_scroll = middle.scroll_at(click_y, click_offset);
    assert!(click_scroll > max_scroll * 0.8);

    let top = SettingsScrollbar::new(track, max_scroll, 0.);
    let bottom = SettingsScrollbar::new(track, max_scroll, max_scroll);
    assert_eq!(top.thumb.y, track.y);
    assert_eq!(bottom.thumb.y, track.y + travel);
}

#[test]
fn settings_manual_save_tracks_only_changed_drafts_and_keeps_them_after_auto_preferences() {
    const WIDTH: f32 = 654.;
    const HEIGHT: f32 = 506.;
    let cases = [
        (2, Field::RestSeconds, "75"),
        (3, Field::MaUrl, "https://music.example.test"),
        (4, Field::VoiceHost, "voice.example.test"),
    ];

    for (section, field, draft) in cases {
        let mut model = UiModel::new(AppSettings::default());
        let mut ui = SettingsUi::new(&model.settings, true);
        ui.section = section;
        ui.changed = Instant::now() - Duration::from_secs(1);
        let now = Instant::now();

        let (clean, _) = settings::settings_scene(&model, &ui, &[], WIDTH, HEIGHT, now);
        assert!(!ui.has_unsaved_fields(&model.settings));
        assert!(
            !clean
                .hits
                .iter()
                .any(|hit| hit.action == Action::SaveSettings),
            "SaveSettings should be hidden for unchanged drafts in section {section}"
        );

        ui.fields.insert(field, draft.into());
        assert!(ui.has_unsaved_fields(&model.settings));
        let (dirty, _) = settings::settings_scene(&model, &ui, &[], WIDTH, HEIGHT, now);
        let save = dirty
            .hits
            .iter()
            .find(|hit| hit.action == Action::SaveSettings)
            .expect("dirty manual fields should expose SaveSettings");
        assert!(save.enabled);
        let pending = crate::i18n::translate("settings.status.unsaved");
        assert!(dirty.paint.iter().any(|paint| {
            matches!(paint, Paint::Text(_, text, _, _, _, _) if text == &pending)
        }));

        Preference::OpenClick
            .assign(&mut model.settings, 1)
            .unwrap();
        assert_eq!(ui.fields.get(&field).map(String::as_str), Some(draft));
        assert!(ui.has_unsaved_fields(&model.settings));
        let (after_auto_save, _) = settings::settings_scene(&model, &ui, &[], WIDTH, HEIGHT, now);
        assert!(
            after_auto_save
                .hits
                .iter()
                .any(|hit| hit.action == Action::SaveSettings)
        );
    }
}

#[test]
fn scene_hit_testing_uses_shared_rectangles_and_prefers_the_last_enabled_hit() {
    let mut scene = Scene::default();
    scene.hit(Rect::new(5., 5., 20., 20.), Action::Calendar, "calendar");
    scene.hit(Rect::new(10., 10., 10., 10.), Action::AddTask, "add task");

    let (_, hit) = scene.action_at(12., 12.).unwrap();
    assert_eq!(hit.action, Action::AddTask);
    assert!(scene.action_at(30., 12.).is_none());

    scene.hits[1].enabled = false;
    let (_, hit) = scene.action_at(12., 12.).unwrap();
    assert_eq!(hit.action, Action::Calendar);
}

#[test]
fn scene_clips_partial_hit_targets_to_the_scroll_viewport() {
    let mut scene = Scene::default();
    scene.clip(Rect::new(10., 20., 30., 15.));
    scene.hit(
        Rect::new(2., 25., 15., 15.),
        Action::Calendar,
        "partially visible",
    );
    scene.unclip();

    assert_eq!(scene.hits.len(), 1);
    assert_eq!(scene.hits[0].rect, Rect::new(10., 25., 7., 10.));
    assert!(scene.action_at(9., 25.).is_none());
    assert!(scene.action_at(17., 25.).is_none());
    assert_eq!(
        scene.action_at(10., 25.).unwrap().1.action,
        Action::Calendar
    );
}

#[test]
fn scene_discards_hit_targets_outside_the_scroll_viewport() {
    let mut scene = Scene::default();
    scene.clip(Rect::new(10., 20., 30., 15.));
    scene.hit(
        Rect::new(2., 2., 5., 5.),
        Action::AddTask,
        "outside viewport",
    );
    scene.unclip();

    assert!(scene.hits.is_empty());
    assert!(scene.action_at(3., 3.).is_none());
}

#[test]
fn selecting_a_date_scrolls_to_its_first_task_without_filtering_the_agenda() {
    let mut model = UiModel::new(AppSettings::default());
    model.tasks = vec![
        task(1, date(2025, 3, 3)),
        task(2, date(2025, 3, 5)),
        task(3, date(2025, 3, 5)),
    ];

    model.select_date("2025-03-05".into());
    assert_eq!(model.selected_date, "2025-03-05");
    assert_eq!(model.task_scroll, 34.);
    assert_eq!(model.tasks.len(), 3);

    model.select_date("2025-03-07".into());
    assert_eq!(model.selected_date, "2025-03-07");
    assert_eq!(model.task_scroll, 34.);
    assert_eq!(model.tasks.len(), 3);
}

#[test]
fn todo_task_rows_keep_their_actions_in_list_and_edit_modes() {
    let mut model = UiModel::new(AppSettings::default());
    let id = Uuid::from_u128(1);
    model.tasks = vec![task(id.as_u128(), date(2025, 3, 5))];
    model.list_expanded = true;
    let now = Instant::now();

    let list = views::notch_scene(&model, NotchView::Todo, now);
    assert!(
        list.hits
            .iter()
            .any(|hit| { hit.action == Action::Intent(UserIntent::ToggleTask(id)) })
    );
    assert!(
        list.hits
            .iter()
            .any(|hit| { hit.action == Action::Intent(UserIntent::StartTask(id)) })
    );

    model.edit_mode = true;
    let edit = views::notch_scene(&model, NotchView::Todo, now);
    assert!(
        edit.hits
            .iter()
            .any(|hit| hit.action == Action::EditTask(id))
    );
    assert!(
        edit.hits
            .iter()
            .any(|hit| { hit.action == Action::Intent(UserIntent::DeleteTask(id)) })
    );
    assert!(
        !edit
            .hits
            .iter()
            .any(|hit| { hit.action == Action::Intent(UserIntent::StartTask(id)) })
    );
}

#[test]
fn todo_voice_control_is_available_in_summary_and_expanded_list_but_blocked_transcribing() {
    let id = Uuid::from_u128(11);
    for list_expanded in [false, true] {
        let mut model = UiModel::new(AppSettings::default());
        model.tasks = vec![task(id.as_u128(), date(2025, 3, 5))];
        model.list_expanded = list_expanded;
        let now = Instant::now();

        let summary_or_list = views::notch_scene(&model, NotchView::Todo, now);
        let voice_control = summary_or_list
            .hits
            .iter()
            .find(|hit| hit.action == Action::Intent(UserIntent::ToggleVoiceCapture))
            .expect("ToDo should expose voice capture in summary and expanded list");
        assert!(
            voice_control.enabled,
            "voice capture should be enabled while idle"
        );

        model.voice = VoiceState::Transcribing;
        let transcribing =
            views::notch_scene(&model, NotchView::Todo, now + Duration::from_millis(1));
        let voice_control = transcribing
            .hits
            .iter()
            .find(|hit| hit.action == Action::Intent(UserIntent::ToggleVoiceCapture))
            .expect("voice capture control should remain visible while transcribing");
        assert!(
            !voice_control.enabled,
            "voice capture should be blocked during transcription"
        );
    }
}

#[test]
fn calendar_agenda_keeps_other_dates_visible_after_selecting_an_empty_day() {
    let mut model = UiModel::new(AppSettings::default());
    let first = Uuid::from_u128(1);
    let selected_day = Uuid::from_u128(2);
    let second_selected_day = Uuid::from_u128(3);
    model.tasks = vec![
        task(first.as_u128(), date(2025, 3, 3)),
        task(selected_day.as_u128(), date(2025, 3, 5)),
        task(second_selected_day.as_u128(), date(2025, 3, 5)),
        task(4, date(2025, 3, 9)),
    ];
    model.calendar_open = true;
    model.calendar = crate::domain::calendar::calendar_grid(date(2025, 3, 7), 0, true, false);
    model.select_date("2025-03-05".into());
    model.select_date("2025-03-07".into());

    let scene = views::notch_scene(&model, NotchView::Todo, Instant::now());
    assert_eq!(model.selected_date, "2025-03-07");
    assert_eq!(model.tasks.len(), 4);
    assert!(scene.hits.iter().any(|hit| hit.action == Action::AddTask));
    assert!(
        scene
            .hits
            .iter()
            .any(|hit| { hit.action == Action::Intent(UserIntent::StartTask(selected_day)) })
    );
    assert!(
        scene.hits.iter().any(|hit| {
            hit.action == Action::Intent(UserIntent::StartTask(second_selected_day))
        })
    );
}

#[test]
fn month_and_week_view_choice_is_in_settings_but_not_the_notch() {
    let mut model = UiModel::new(AppSettings::default());
    let now = Instant::now();
    for week in [false, true] {
        model.settings.desktop.calendar_week_view = week;
        model.calendar = crate::domain::calendar::calendar_grid(date(2025, 3, 5), 0, true, week);
        for calendar_open in [false, true] {
            model.calendar_open = calendar_open;
            let scene = views::notch_scene(&model, NotchView::Todo, now);
            assert!(
                !scene
                    .hits
                    .iter()
                    .any(|hit| { matches!(&hit.action, Action::Preference(Preference::Week, _)) })
            );
        }
    }

    let mut ui = SettingsUi::new(&model.settings, true);
    ui.section = 5;
    let (scene, _) = settings::settings_scene(&model, &ui, &[], 800., 600., now);
    assert!(
        scene
            .hits
            .iter()
            .any(|hit| { hit.action == Action::Preference(Preference::Week, 0) })
    );
    assert!(
        scene
            .hits
            .iter()
            .any(|hit| { hit.action == Action::Preference(Preference::Week, 1) })
    );
}

#[test]
fn preference_updates_leave_connection_drafts_uncommitted_until_settings_confirmation() {
    let persisted = AppSettings::default();
    let mut current = persisted.clone();
    let mut ui = SettingsUi::new(&persisted, true);
    ui.fields
        .insert(Field::MaUrl, "  https://media.example.test  ".into());
    ui.fields.insert(Field::RestSeconds, "45".into());

    Preference::Minimal.assign(&mut current, 1).unwrap();
    Preference::Scale.assign(&mut current, 115).unwrap();
    assert!(current.is_minimal_notch);
    assert_eq!(current.desktop.scale_percent, 115);
    assert_eq!(
        current.music_assistant.server_url,
        persisted.music_assistant.server_url
    );

    let confirmed = ui.confirmed_settings(&current).unwrap();
    assert!(confirmed.is_minimal_notch);
    assert_eq!(confirmed.desktop.scale_percent, 115);
    assert_eq!(confirmed.rest_after_secs, 45);
    assert_eq!(
        confirmed.music_assistant.server_url,
        "https://media.example.test"
    );
}

#[test]
fn mouse_move_preference_defaults_off_and_persists_as_a_boolean() {
    let mut settings = AppSettings::default();
    assert!(!settings.is_mouse_move_enabled);
    assert_eq!(Preference::MouseMove.value(&settings), 0);

    Preference::MouseMove.assign(&mut settings, 1).unwrap();
    let encoded = serde_json::to_string(&settings).unwrap();
    let restored: AppSettings = serde_json::from_str(&encoded).unwrap();
    assert!(restored.is_mouse_move_enabled);
    assert_eq!(Preference::MouseMove.value(&restored), 1);

    let mut disabled = restored;
    Preference::MouseMove.assign(&mut disabled, 0).unwrap();
    assert!(!disabled.is_mouse_move_enabled);
}

#[test]
fn appearance_and_status_ring_settings_expose_and_select_the_configured_values() {
    const WIDTH: f32 = 800.;
    const HEIGHT: f32 = 600.;
    let mut model = UiModel::new(AppSettings::default());
    Preference::Floating.assign(&mut model.settings, 1).unwrap();
    Preference::Glass.assign(&mut model.settings, 1).unwrap();
    Preference::GlassTint.assign(&mut model.settings, 55).unwrap();
    Preference::StatusRing.assign(&mut model.settings, 2).unwrap();
    assert_eq!(Preference::Floating.value(&model.settings), 1);
    assert_eq!(Preference::Glass.value(&model.settings), 1);
    assert_eq!(Preference::GlassTint.value(&model.settings), 55);
    assert_eq!(Preference::StatusRing.value(&model.settings), 2);

    let now = Instant::now();
    let mut appearance = SettingsUi::new(&model.settings, true);
    appearance.section = 2;
    appearance.changed = now - Duration::from_secs(1);
    let (appearance_scene, _) =
        settings::settings_scene(&model, &appearance, &[], WIDTH, HEIGHT, now);
    for value in 0..=1 {
        assert!(appearance_scene.hits.iter().any(|hit| {
            hit.action == Action::Preference(Preference::Floating, value)
        }));
    }
    assert!(appearance_scene.hits.iter().any(|hit| {
        hit.action == Action::Preference(Preference::Glass, 0)
    }));

    let mut ring = SettingsUi::new(&model.settings, true);
    ring.section = 12;
    ring.changed = now - Duration::from_secs(1);
    let (ring_scene, _) = settings::settings_scene(&model, &ring, &[], WIDTH, HEIGHT, now);
    for value in 0..4 {
        assert!(ring_scene.hits.iter().any(|hit| {
            hit.action == Action::Preference(Preference::StatusRing, value)
        }));
    }
    let selected = ring_scene
        .hits
        .iter()
        .find(|hit| hit.action == Action::Preference(Preference::StatusRing, 2))
        .unwrap();
    assert!(ring_scene.paint.iter().any(|paint| {
        matches!(paint, Paint::Border(rect, _, 0x168cff, _) if *rect == selected.rect.inset(0.5))
    }));

    let mut widgets = SettingsUi::new(&model.settings, true);
    widgets.section = 13;
    widgets.changed = now - Duration::from_secs(1);
    let (left_widgets, _) = settings::settings_scene(&model, &widgets, &[], WIDTH, HEIGHT, now);
    assert!(left_widgets.hits.iter().any(|hit| {
        hit.action == Action::Preference(Preference::WidgetsEnabled, 1)
    }));
    assert!(left_widgets
        .hits
        .iter()
        .any(|hit| hit.action == Action::WidgetSlot(false)));
    assert!(left_widgets
        .hits
        .iter()
        .any(|hit| hit.action == Action::WidgetSlot(true)));
    for value in 0..7 {
        assert!(left_widgets.hits.iter().any(|hit| {
            hit.action == Action::Preference(Preference::WidgetLeft, value)
        }));
    }

    widgets.widget_right = true;
    let (right_widgets, _) = settings::settings_scene(&model, &widgets, &[], WIDTH, HEIGHT, now);
    for value in 0..7 {
        assert!(right_widgets.hits.iter().any(|hit| {
            hit.action == Action::Preference(Preference::WidgetRight, value)
        }));
    }
}

fn scene_has_icon(scene: &Scene, name: &str) -> bool {
    scene
        .paint
        .iter()
        .chain(&scene.overlay)
        .any(|paint| matches!(paint, Paint::Icon(_, glyph, _) if glyph == name))
}

fn assert_removal_frame(
    model: &mut UiModel,
    view: NotchView,
    now: Instant,
    removed: &ShelfEntry,
    survivor: &ShelfEntry,
    tray: Option<Tray>,
    expected_opacity: Option<f32>,
    ghost_visible: bool,
) {
    let scene = views::notch_scene(model, view, now);
    match expected_opacity {
        Some(expected) => assert!(
            (scene_opacity(&scene, &removed.item.display_name).unwrap() - expected).abs() < 0.0001
        ),
        None => assert_eq!(scene_opacity(&scene, &removed.item.display_name), None),
    }
    assert_eq!(scene_has_text(&scene, &removed.item.display_name), ghost_visible);
    assert!(scene_has_text(&scene, &survivor.item.display_name));

    let (removed_open, removed_remove, survivor_open, survivor_remove) = match tray {
        Some(tray) => (
            Action::DragSharedFile(removed.item.id),
            Action::Share(ShareCommand::Remove(tray, removed.item.id)),
            Action::DragSharedFile(survivor.item.id),
            Action::Share(ShareCommand::Remove(tray, survivor.item.id)),
        ),
        None => (
            Action::DragShelf(removed.item.id),
            Action::Intent(UserIntent::RemoveShelfItem(removed.item.id)),
            Action::DragShelf(survivor.item.id),
            Action::Intent(UserIntent::RemoveShelfItem(survivor.item.id)),
        ),
    };
    assert!(!scene
        .hits
        .iter()
        .any(|hit| hit.action == removed_open || hit.action == removed_remove));
    let survivor_hit = scene
        .hits
        .iter()
        .find(|hit| hit.action == survivor_open)
        .unwrap();
    model.mouse = (
        survivor_hit.rect.x + survivor_hit.rect.w / 2.,
        survivor_hit.rect.y + survivor_hit.rect.h / 2.,
    );
    let hovered = views::notch_scene(model, view, now);
    assert!(hovered.hits.iter().any(|hit| hit.action == survivor_remove));
}

fn scene_opacity(scene: &Scene, text: &str) -> Option<f32> {
    let text_index = scene.paint.iter().position(|paint| {
        matches!(paint, Paint::Text(_, value, _, _, _, _) if value == text)
    })?;
    scene.paint[..text_index]
        .iter()
        .rev()
        .find_map(|paint| match paint {
            Paint::Opacity(opacity) => Some(*opacity),
            _ => None,
        })
}

fn shelf_entry(id: u128, name: &str) -> ShelfEntry {
    ShelfEntry {
        item: ShelfItem {
            id: Uuid::from_u128(id),
            content: ShelfContent::Text(name.into()),
            display_name: name.into(),
            size_bytes: name.len() as u64,
            is_directory: false,
            kind: ShelfItemKind::Text,
            added_at: Utc.with_ymd_and_hms(2026, 11, 3, 12, 0, 0).unwrap(),
        },
        badge: "TXT".into(),
        size_label: format!("{} B", name.len()),
        thumbnail: None,
    }
}

fn scene_has_text(scene: &Scene, text: &str) -> bool {
    scene
        .paint
        .iter()
        .chain(&scene.overlay)
        .any(|paint| matches!(paint, Paint::Text(_, value, _, _, _, _) if value == text))
}

fn scene_has_mascot_visual(scene: &Scene) -> bool {
    !scene.overlay.is_empty()
        || scene.paint.iter().any(|paint| match paint {
            Paint::Icon(_, glyph, _) => glyph == "eye-halo",
            Paint::Bitmap(rect, width, height, _) => {
                *width == 16 && *height == 16 && rect.y == 6. && rect.h == 24.
            }
            Paint::Ellipse { .. } | Paint::VectorPath { .. } => true,
            _ => false,
        })
}

fn task(id: u128, scheduled: NaiveDate) -> TodoTask {
    TodoTask {
        id: Uuid::from_u128(id),
        title: format!("Task {id}"),
        is_done: false,
        created_at: Utc.with_ymd_and_hms(2025, 3, 1, 12, 0, 0).unwrap(),
        completed_at: None,
        schedule: TaskSchedule {
            date: scheduled,
            time: None,
            daily: false,
        },
        color: TaskColor::Green,
    }
}

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}
