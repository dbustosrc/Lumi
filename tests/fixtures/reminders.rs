use super::*;

use std::time::{Duration, Instant};
use uuid::Uuid;

#[test]
fn focus_uses_short_breaks_and_a_long_break_after_four_cycles() {
    let mut now = Instant::now();
    let mut reminders = Reminders::new(ReminderSettings::default());

    for cycle in 1..=4 {
        reminders.toggle_focus(now);
        let focus_due = now + Duration::from_secs(25 * 60);
        assert!(
            reminders
                .state(focus_due - Duration::from_secs(1))
                .focus_running
        );
        assert_eq!(
            reminders
                .state(focus_due - Duration::from_secs(1))
                .focus_seconds,
            1
        );
        let notices = reminders.tick(focus_due);
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].key, "desktopUi.break");
        assert_eq!(
            reminders.state(focus_due).phase,
            if cycle == 4 {
                FocusPhase::LongBreak
            } else {
                FocusPhase::ShortBreak
            }
        );
        assert!(reminders.tick(focus_due).is_empty());
        let break_seconds = if cycle == 4 { 15 * 60 } else { 5 * 60 };
        let paused_break = reminders.state(focus_due);
        assert_eq!(paused_break.focus_seconds, break_seconds);
        assert_eq!(paused_break.focus_total_seconds, break_seconds);
        assert!(!paused_break.focus_running);

        reminders.toggle_focus(focus_due);
        let break_due = focus_due + Duration::from_secs(break_seconds);
        assert_eq!(
            reminders
                .state(break_due - Duration::from_secs(1))
                .focus_seconds,
            1
        );
        let notices = reminders.tick(break_due);
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].key, "desktopUi.focus");
        assert!(reminders.tick(break_due).is_empty());
        let paused_focus = reminders.state(break_due);
        assert_eq!(paused_focus.phase, FocusPhase::Focus);
        assert_eq!(paused_focus.focus_seconds, 25 * 60);
        assert_eq!(paused_focus.focus_total_seconds, 25 * 60);
        assert!(!paused_focus.focus_running);
        now = break_due;
    }
}

#[test]
fn selecting_a_focus_phase_while_running_resets_it_paused_to_the_phase_duration() {
    let now = Instant::now();
    let mut reminders = Reminders::new(ReminderSettings::default());
    reminders.toggle_focus(now);
    assert!(reminders.state(now).focus_running);

    reminders.select_phase(FocusPhase::LongBreak);

    let state = reminders.state(now + Duration::from_secs(30));
    assert_eq!(state.phase, FocusPhase::LongBreak);
    assert_eq!(state.focus_total_seconds, 15 * 60);
    assert_eq!(state.focus_seconds, 15 * 60);
    assert!(!state.focus_running);
}

#[test]
fn task_start_and_task_end_notices_are_emitted_once() {
    let now = Instant::now();
    let settings = ReminderSettings {
        task_minutes: 1,
        ..ReminderSettings::default()
    };
    let mut reminders = Reminders::new(settings);
    let id = Uuid::new_v4();

    let started = reminders
        .start_task(id, "Review release".into(), now)
        .unwrap();
    assert_eq!(started.key, "desktopUi.task-started");
    assert_eq!(started.detail, "Review release");
    assert_eq!(reminders.state(now).task_id, Some(id));

    let due = now + Duration::from_secs(60);
    let ready = reminders.tick(due);
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].key, "desktopUi.task-ready");
    assert_eq!(ready[0].detail, "Review release");
    assert!(reminders.tick(due + Duration::from_secs(1)).is_empty());
}

#[test]
fn snooze_replays_the_last_notice_once_after_its_delay() {
    let now = Instant::now();
    let settings = ReminderSettings {
        task_minutes: 1,
        task_end: false,
        snooze_minutes: 10,
        ..ReminderSettings::default()
    };
    let mut reminders = Reminders::new(settings);
    let started = reminders
        .start_task(Uuid::new_v4(), "Write tests".into(), now)
        .unwrap();
    reminders.remember(&started);
    reminders.snooze(now + Duration::from_secs(60));

    let snooze_due = now + Duration::from_secs(11 * 60);
    assert!(
        reminders
            .tick(snooze_due - Duration::from_secs(1))
            .is_empty()
    );
    let notices = reminders.tick(snooze_due);
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].key, "desktopUi.task-started");
    assert!(
        reminders
            .tick(snooze_due + Duration::from_secs(1))
            .is_empty()
    );
}

#[test]
fn pomodoro_and_water_notices_do_not_replace_a_snoozed_todo_notice() {
    let now = Instant::now();
    let reminders_settings = ReminderSettings::default();
    let mut reminders = Reminders::new(reminders_settings);
    let task_notice = ReminderNotice {
        key: "desktopUi.task-ready",
        detail: "Write tests".into(),
        page: NotchPage::Todo,
    };
    reminders.remember(&task_notice);
    reminders.snooze(now);
    reminders.remember(&ReminderNotice {
        key: "desktopUi.break",
        detail: String::new(),
        page: NotchPage::Pomodoro,
    });
    reminders.remember(&ReminderNotice {
        key: "desktopUi.water-reminder",
        detail: String::new(),
        page: NotchPage::Water,
    });

    let notices = reminders.tick(now + Duration::from_secs(10 * 60));

    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].key, "desktopUi.task-ready");
    assert_eq!(notices[0].detail, "Write tests");
}

#[test]
fn starting_or_canceling_a_task_clears_the_previous_snooze() {
    let now = Instant::now();
    let settings = ReminderSettings::default();
    let mut reminders = Reminders::new(settings.clone());
    let previous = ReminderNotice {
        key: "desktopUi.task-ready",
        detail: "Old task".into(),
        page: NotchPage::Todo,
    };
    reminders.remember(&previous);
    reminders.snooze(now);
    let next_id = Uuid::new_v4();
    reminders.start_task(next_id, "New task".into(), now + Duration::from_secs(1));
    assert!(
        reminders
            .tick(now + Duration::from_secs(11 * 60))
            .is_empty()
    );

    let mut reminders = Reminders::new(settings);
    let current_id = Uuid::new_v4();
    let started = reminders
        .start_task(current_id, "Current task".into(), now)
        .unwrap();
    reminders.remember(&started);
    reminders.snooze(now);
    reminders.cancel_task(current_id);
    assert!(
        reminders
            .tick(now + Duration::from_secs(10 * 60))
            .is_empty()
    );
}
