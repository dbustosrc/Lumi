#[path = "../src/domain/background.rs"]
mod background;
#[path = "../src/domain/hover.rs"]
mod hover;
#[allow(dead_code)]
#[path = "../src/domain/notch.rs"]
mod notch;

use std::time::{Duration, Instant};

use background::{
    BackgroundDetection, BackgroundObservation, ContentSample, ContentStability, nearby_detail,
};
use hover::{HOVER_LEAVE_GRACE, HOVER_OPEN_DELAY, HoverState};
use notch::{NotchContext, NotchPage, NotchView, Peek, resolve_view};

#[test]
fn hover_opens_after_delay_and_reentry_cancels_leave_grace() {
    assert_eq!(HOVER_OPEN_DELAY, Duration::from_millis(250));
    assert_eq!(HOVER_LEAVE_GRACE, Duration::from_millis(120));

    let start = Instant::now();
    let mut hover = HoverState::default();

    hover.set_inside(true, start);
    assert_eq!(hover.deadline(), Some(start + HOVER_OPEN_DELAY));
    hover.advance(start + HOVER_OPEN_DELAY - Duration::from_millis(1));
    assert!(!hover.is_open());
    hover.advance(start + HOVER_OPEN_DELAY);
    assert!(hover.is_open());

    let leave = start + HOVER_OPEN_DELAY + Duration::from_millis(1);
    hover.set_inside(false, leave);
    let close_at = leave + HOVER_LEAVE_GRACE;
    hover.advance(close_at - Duration::from_millis(1));
    assert!(hover.is_open());

    hover.set_inside(true, close_at - Duration::from_millis(1));
    assert_eq!(hover.deadline(), None);
    hover.advance(close_at);
    assert!(hover.is_open());
}

#[test]
fn brief_hover_is_cancelled_and_reentry_after_close_is_immediate() {
    let start = Instant::now();
    let mut hover = HoverState::default();

    hover.set_inside(true, start);
    hover.set_inside(false, start + Duration::from_millis(100));
    hover.advance(start + HOVER_OPEN_DELAY);
    assert!(!hover.is_open());

    hover.set_inside(true, start + Duration::from_secs(1));
    hover.advance(start + Duration::from_secs(1) + HOVER_OPEN_DELAY);
    assert!(hover.is_open());
    let leave = start + Duration::from_secs(2);
    hover.set_inside(false, leave);
    let closed_at = leave + HOVER_LEAVE_GRACE;
    hover.advance(closed_at);
    assert!(!hover.is_open());

    hover.set_inside(true, closed_at + Duration::from_millis(579));
    assert!(hover.is_open());
    assert_eq!(hover.deadline(), None);
}

#[test]
fn fast_motion_reentry_is_immediate_before_settle_delay_then_uses_hover_delay() {
    fn fast_closed_at(start: Instant) -> (HoverState, Instant) {
        let mut hover = HoverState::default();
        hover.set_fast_motion(true);
        hover.set_inside(true, start);
        hover.advance(start + HOVER_OPEN_DELAY);
        assert!(hover.is_open());

        let leave = start + HOVER_OPEN_DELAY;
        hover.set_inside(false, leave);
        let closed_at = leave + HOVER_LEAVE_GRACE;
        hover.advance(closed_at);
        assert!(!hover.is_open());
        (hover, closed_at)
    }

    let start = Instant::now();
    let (mut early_reentry, closed_at) = fast_closed_at(start);
    early_reentry.set_inside(true, closed_at + Duration::from_millis(339));
    assert!(early_reentry.is_open());
    assert_eq!(early_reentry.deadline(), None);

    let (mut late_reentry, closed_at) = fast_closed_at(start + Duration::from_secs(2));
    let reentered = closed_at + Duration::from_millis(340);
    late_reentry.set_inside(true, reentered);
    assert!(!late_reentry.is_open());
    assert_eq!(late_reentry.deadline(), Some(reentered + HOVER_OPEN_DELAY));
    late_reentry.advance(reentered + HOVER_OPEN_DELAY - Duration::from_millis(1));
    assert!(!late_reentry.is_open());
    late_reentry.advance(reentered + HOVER_OPEN_DELAY);
    assert!(late_reentry.is_open());
}

#[test]
fn forced_reveal_stays_open_while_pointer_is_inside() {
    let start = Instant::now();
    let mut hover = HoverState::default();
    hover.set_inside(true, start);
    hover.reveal_for(start + Duration::from_millis(20), Duration::from_secs(1));
    assert!(hover.is_open());
    assert_eq!(hover.deadline(), None);
    hover.advance(start + Duration::from_secs(2));
    assert!(hover.is_open());

    let mut forced_while_outside = HoverState::default();
    forced_while_outside.reveal_for(start, Duration::from_secs(1));
    assert_eq!(
        forced_while_outside.deadline(),
        Some(start + Duration::from_secs(1))
    );
    forced_while_outside.advance(start + Duration::from_secs(1));
    assert!(!forced_while_outside.is_open());
}

#[test]
fn hover_close_cycles_without_confirmed_content_never_select_minimal() {
    let mut hover = HoverState::default();
    let mut now = Instant::now();

    for cycle in 0..10 {
        hover.set_inside(true, now);
        if cycle == 0 {
            now += HOVER_OPEN_DELAY;
            hover.advance(now);
        }
        assert!(hover.is_open());
        assert_eq!(
            resolve_view(&NotchContext {
                is_hovered: hover.is_open(),
                ..NotchContext::default()
            }),
            NotchView::Music
        );

        hover.set_inside(false, now);
        now += HOVER_LEAVE_GRACE;
        hover.advance(now);
        assert!(!hover.is_open());
        assert_eq!(
            resolve_view(&NotchContext {
                is_hovered: hover.is_open(),
                has_nearby_content: false,
                ..NotchContext::default()
            }),
            NotchView::Closed
        );
        now += Duration::from_millis(1);
    }
}

#[test]
fn content_detection_requires_two_matching_valid_samples() {
    let mut stability = ContentStability::default();
    assert_eq!(stability.observe(Some(ContentSample::Detailed)), None);
    assert_eq!(stability.observe(None), None);
    assert_eq!(stability.observe(Some(ContentSample::Detailed)), None);
    assert_eq!(stability.observe(Some(ContentSample::Detailed)), Some(true));

    assert_eq!(stability.observe(Some(ContentSample::Clear)), None);
    assert_eq!(stability.observe(None), None);
    assert_eq!(stability.observe(Some(ContentSample::Clear)), None);
    assert_eq!(stability.observe(Some(ContentSample::Clear)), Some(false));
}

#[test]
fn resetting_content_detection_clears_context_and_reports_disappearance() {
    let mut stability = ContentStability::default();
    assert_eq!(stability.observe(Some(ContentSample::Detailed)), None);
    assert_eq!(stability.observe(Some(ContentSample::Detailed)), Some(true));
    assert_eq!(stability.reset(), Some(false));
    assert_eq!(stability.observe(Some(ContentSample::Detailed)), None);
    assert_eq!(stability.observe(Some(ContentSample::Detailed)), Some(true));
    assert_eq!(stability.reset(), Some(false));
    assert_eq!(stability.reset(), None);
}

#[test]
fn nearby_detail_counts_horizontal_rgb_edges_and_ignores_alpha() {
    let alternating: Vec<[u8; 4]> = (0..13)
        .map(|index| [if index % 2 == 0 { 0 } else { 14 }, 0, 0, 255])
        .collect();
    assert_eq!(nearby_detail(&alternating, 13), ContentSample::Detailed);

    let eleven_edges: Vec<[u8; 4]> = (0..12)
        .map(|index| [if index % 2 == 0 { 0 } else { 14 }, 0, 0, 255])
        .collect();
    assert_eq!(nearby_detail(&eleven_edges, 12), ContentSample::Uncertain);

    let alpha_only: Vec<[u8; 4]> = (0..13)
        .map(|index| [0, 0, 0, if index % 2 == 0 { 0 } else { 255 }])
        .collect();
    assert_eq!(nearby_detail(&alpha_only, 13), ContentSample::Clear);
}

#[test]
fn nearby_detail_never_counts_edges_between_rows() {
    let pixels: Vec<[u8; 4]> = (0..13)
        .flat_map(|row| {
            let color = if row % 2 == 0 { 0 } else { 255 };
            [[color, color, color, 255]; 2]
        })
        .collect();
    assert_eq!(nearby_detail(&pixels, 2), ContentSample::Clear);
    assert_eq!(
        nearby_detail(&[[0, 0, 0, 255]], 1),
        ContentSample::Uncertain
    );
}

#[test]
fn content_sample_thresholds_leave_a_noise_band_between_clear_and_detailed() {
    assert_eq!(ContentSample::from_edges(0), ContentSample::Clear);
    assert_eq!(ContentSample::from_edges(5), ContentSample::Clear);
    assert_eq!(ContentSample::from_edges(6), ContentSample::Uncertain);
    assert_eq!(ContentSample::from_edges(11), ContentSample::Uncertain);
    assert_eq!(ContentSample::from_edges(12), ContentSample::Detailed);
    assert_eq!(ContentSample::from_edges(20), ContentSample::Detailed);

    assert_eq!(
        ContentSample::Uncertain.combine(ContentSample::Clear),
        ContentSample::Uncertain
    );
    assert_eq!(
        ContentSample::Detailed.combine(ContentSample::Uncertain),
        ContentSample::Detailed
    );
}

#[test]
fn uncertain_samples_preserve_confirmed_content_and_cannot_dismiss_minimal_view() {
    let context = (1, [0, 0, 1920, 1080], 1.0_f64);
    let mut detection = BackgroundDetection::default();
    assert_eq!(
        detection.observe_context(BackgroundObservation::Window(context)),
        None
    );
    assert_eq!(
        detection.observe_sample(&context, Some(ContentSample::Detailed)),
        None
    );
    assert_eq!(
        detection.observe_sample(&context, Some(ContentSample::Detailed)),
        Some(true)
    );
    let mut view_context = NotchContext {
        has_nearby_content: true,
        ..NotchContext::default()
    };

    for _ in 0..10 {
        for detected in [
            detection.observe_context(BackgroundObservation::Unavailable),
            detection.observe_sample(&context, None),
            detection.observe_sample(&context, Some(ContentSample::Uncertain)),
        ]
        .into_iter()
        .flatten()
        {
            view_context.has_nearby_content = detected;
        }
        assert_eq!(resolve_view(&view_context), NotchView::Minimal);
    }

    let clear_candidate = detection.observe_sample(&context, Some(ContentSample::Clear));
    assert_eq!(clear_candidate, None);
    if let Some(detected) = detection.observe_context(BackgroundObservation::Unavailable) {
        view_context.has_nearby_content = detected;
    }
    assert_eq!(resolve_view(&view_context), NotchView::Minimal);
    assert_eq!(
        detection.observe_sample(&context, Some(ContentSample::Clear)),
        None
    );
    if let Some(detected) = detection.observe_sample(&context, Some(ContentSample::Clear)) {
        view_context.has_nearby_content = detected;
    }
    assert!(!view_context.has_nearby_content);
    assert_eq!(resolve_view(&view_context), NotchView::Closed);
}

#[test]
fn two_decisive_clear_samples_allow_capsule_view_again() {
    let context = 7_u32;
    let mut detection = BackgroundDetection::default();
    detection.observe_context(BackgroundObservation::Window(context));
    assert_eq!(
        detection.observe_sample(&context, Some(ContentSample::Detailed)),
        None
    );
    assert_eq!(
        detection.observe_sample(&context, Some(ContentSample::Detailed)),
        Some(true)
    );
    assert_eq!(
        resolve_view(&NotchContext {
            has_nearby_content: true,
            ..NotchContext::default()
        }),
        NotchView::Minimal
    );

    assert_eq!(
        detection.observe_sample(&context, Some(ContentSample::Clear)),
        None
    );
    assert_eq!(
        detection.observe_sample(&context, Some(ContentSample::Clear)),
        Some(false)
    );
    assert_eq!(resolve_view(&NotchContext::default()), NotchView::Closed);
}

#[test]
fn changed_window_or_monitor_context_clears_old_confirmation_and_ignores_stale_samples() {
    let first = (1, [0, 0, 1920, 1080], 1.0_f64);
    let second_window = (2, [0, 0, 1920, 1080], 1.0_f64);
    let moved_window = (1, [0, 1, 1920, 1081], 1.0_f64);
    let second_scale = (1, [0, 0, 1920, 1080], 1.5_f64);

    for next in [second_window, moved_window, second_scale] {
        let mut detection = BackgroundDetection::default();
        detection.observe_context(BackgroundObservation::Window(first));
        detection.observe_sample(&first, Some(ContentSample::Detailed));
        assert_eq!(
            detection.observe_sample(&first, Some(ContentSample::Detailed)),
            Some(true)
        );

        assert_eq!(
            detection.observe_context(BackgroundObservation::Window(next)),
            Some(false)
        );
        assert_eq!(
            detection.observe_sample(&first, Some(ContentSample::Detailed)),
            None
        );
        assert_eq!(
            detection.observe_sample(&next, Some(ContentSample::Detailed)),
            None
        );
        assert_eq!(
            detection.observe_sample(&next, Some(ContentSample::Detailed)),
            Some(true)
        );
    }
}

#[test]
fn confirmed_exclusion_clears_old_content_and_repeated_unavailable_does_not_stick() {
    let context = 3_u32;
    let mut detection = BackgroundDetection::default();
    detection.observe_context(BackgroundObservation::Window(context));
    detection.observe_sample(&context, Some(ContentSample::Detailed));
    assert_eq!(
        detection.observe_sample(&context, Some(ContentSample::Detailed)),
        Some(true)
    );
    assert_eq!(
        detection.observe_context(BackgroundObservation::Excluded),
        Some(false)
    );
    assert_eq!(
        detection.observe_sample(&context, Some(ContentSample::Detailed)),
        None
    );

    for _ in 0..10 {
        detection.observe_context(BackgroundObservation::Unavailable);
        assert_eq!(
            detection.observe_context(BackgroundObservation::Excluded),
            None
        );
    }
    detection.observe_context(BackgroundObservation::Window(context));
    assert_eq!(
        detection.observe_sample(&context, Some(ContentSample::Detailed)),
        None
    );
    assert_eq!(
        detection.observe_sample(&context, Some(ContentSample::Detailed)),
        Some(true)
    );
}

#[test]
fn manual_and_detected_content_each_select_minimal_only_while_closed() {
    assert_eq!(resolve_view(&NotchContext::default()), NotchView::Closed);
    assert_eq!(
        resolve_view(&NotchContext {
            is_minimal_notch: true,
            ..NotchContext::default()
        }),
        NotchView::Minimal
    );
    assert_eq!(
        resolve_view(&NotchContext {
            has_nearby_content: true,
            ..NotchContext::default()
        }),
        NotchView::Minimal
    );
}

#[test]
fn actionable_pomodoro_alert_survives_hover_and_waits_for_interaction_and_file_drag() {
    let mut context = NotchContext {
        is_hovered: true,
        page: NotchPage::Pomodoro,
        peek: Some(Peek::Hud),
        is_actionable_alert: true,
        is_minimal_notch: true,
        has_nearby_content: true,
        ..NotchContext::default()
    };

    assert_eq!(resolve_view(&context), NotchView::Hud);

    context.is_interaction_locked = true;
    assert_eq!(resolve_view(&context), NotchView::Pomodoro);
    assert_eq!(context.peek, Some(Peek::Hud));
    assert!(context.is_actionable_alert);

    context.is_interaction_locked = false;
    assert_eq!(resolve_view(&context), NotchView::Hud);

    context.is_file_drag_over = true;
    assert_eq!(resolve_view(&context), NotchView::Shelf);
    context.is_file_drag_over = false;
    assert_eq!(resolve_view(&context), NotchView::Hud);

    // Dismissal clears the transient alert; a normal closed state takes over.
    context.is_hovered = false;
    context.peek = None;
    context.is_actionable_alert = false;
    context.is_minimal_notch = false;
    context.has_nearby_content = false;
    context.is_mascot_enabled = false;
    assert_eq!(resolve_view(&context), NotchView::Closed);
}

#[test]
fn open_lock_and_settings_changes_preserve_the_open_page_and_existing_priorities() {
    let mut context = NotchContext {
        is_hovered: true,
        is_minimal_notch: true,
        has_nearby_content: true,
        ..NotchContext::default()
    };
    assert_eq!(resolve_view(&context), NotchView::Music);

    context.is_hovered = false;
    context.is_interaction_locked = true;
    context.is_minimal_notch = false;
    context.has_nearby_content = false;
    assert_eq!(resolve_view(&context), NotchView::Music);

    context.is_interaction_locked = false;
    context.is_minimal_notch = true;
    assert_eq!(resolve_view(&context), NotchView::Minimal);

    context.is_file_drag_over = true;
    context.peek = Some(Peek::Charging);
    assert_eq!(resolve_view(&context), NotchView::Shelf);

    context.is_file_drag_over = false;
    assert_eq!(resolve_view(&context), NotchView::ChargingPeek);

    context.peek = None;
    context.is_lyrics_mode = true;
    context.has_lyrics = true;
    context.is_playing = true;
    assert_eq!(resolve_view(&context), NotchView::Lyrics);

    context.page = NotchPage::Headphones;
    context.is_interaction_locked = true;
    assert_eq!(resolve_view(&context), NotchView::Headphones);

    // With Mimo enabled while closed, file drag and voice capture stay in the compact capsule
    // so Mimo's live animations are visible; if already open, the active page is preserved.
    context.is_interaction_locked = false;
    context.is_mascot_enabled = true;
    context.is_minimal_notch = true;
    context.is_quiet = true;
    context.is_playing = false;
    context.is_file_drag_over = true;
    assert_eq!(resolve_view(&context), NotchView::Closed);
    context.is_hovered = true;
    assert_eq!(resolve_view(&context), NotchView::Shelf);
    context.is_hovered = false;
    context.is_file_drag_over = false;
    context.is_voice_busy = true;
    assert_eq!(resolve_view(&context), NotchView::Closed);
    context.is_hovered = true;
    context.page = NotchPage::Todo;
    assert_eq!(resolve_view(&context), NotchView::Todo);
}
