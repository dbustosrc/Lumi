#[path = "../src/domain/countdown.rs"]
mod countdown;

use std::time::{Duration, Instant};

use countdown::Countdown;

#[test]
fn countdown_starts_paused_and_toggle_starts_it() {
    let start = Instant::now();
    let mut countdown = Countdown::new(Duration::from_secs(10));

    assert!(!countdown.running());
    assert_eq!(countdown.remaining(start), Duration::from_secs(10));

    countdown.toggle(start);

    assert!(countdown.running());
    assert_eq!(
        countdown.remaining(start + Duration::from_secs(3)),
        Duration::from_secs(7)
    );
}

#[test]
fn pausing_preserves_remaining_time_until_resumed() {
    let start = Instant::now();
    let pause_at = start + Duration::from_secs(4);
    let mut countdown = Countdown::new(Duration::from_secs(10));

    countdown.toggle(start);
    countdown.toggle(pause_at);

    assert!(!countdown.running());
    assert_eq!(countdown.remaining(pause_at), Duration::from_secs(6));
    assert_eq!(
        countdown.remaining(pause_at + Duration::from_secs(20)),
        Duration::from_secs(6)
    );

    let resume_at = pause_at + Duration::from_secs(20);
    countdown.toggle(resume_at);
    assert_eq!(countdown.remaining(resume_at), Duration::from_secs(6));
    assert!(countdown.running());
}

#[test]
fn finish_reports_due_once_and_clears_the_countdown() {
    let start = Instant::now();
    let due_at = start + Duration::from_secs(5);
    let mut countdown = Countdown::new(Duration::from_secs(5));
    countdown.toggle(start);

    assert!(!countdown.finish(due_at - Duration::from_millis(1)));
    assert!(countdown.finish(due_at));
    assert!(!countdown.finish(due_at + Duration::from_secs(1)));
    assert!(!countdown.running());
    assert_eq!(countdown.remaining(due_at), Duration::ZERO);
}

#[test]
fn zero_duration_does_not_start_or_finish() {
    let now = Instant::now();
    let mut countdown = Countdown::new(Duration::ZERO);

    countdown.toggle(now);

    assert!(!countdown.running());
    assert_eq!(countdown.remaining(now), Duration::ZERO);
    assert!(!countdown.finish(now));
}
