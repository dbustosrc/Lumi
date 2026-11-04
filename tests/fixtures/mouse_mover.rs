use super::{MOUSE_MOVE_INTERVAL, MouseMover};
use crate::domain::ports::{MouseActivity, PortError};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

struct FakeMouse {
    calls: AtomicUsize,
    fail: AtomicBool,
}

impl FakeMouse {
    fn new(fail: bool) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            fail: AtomicBool::new(fail),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl MouseActivity for FakeMouse {
    fn pulse(&self) -> Result<(), PortError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            Err(PortError::new("mouseMove.error.blocked", "fixture failure"))
        } else {
            Ok(())
        }
    }
}

#[test]
fn mouse_opt_in_schedules_first_and_repeated_pulses_at_fixed_thirty_second_intervals() {
    let now = Instant::now();
    let mouse = FakeMouse::new(false);
    let mut mover = MouseMover::default();

    mover.configure(false, now);
    assert!(mover.deadline.is_none());
    assert!(!mover.tick(now + Duration::from_secs(300), &mouse).unwrap());
    assert_eq!(mouse.calls(), 0);

    mover.configure(true, now);
    let first_deadline = now + MOUSE_MOVE_INTERVAL;
    assert_eq!(mover.deadline, Some(first_deadline));

    // An unrelated settings change still configures enabled=true and must not reset the timer.
    mover.configure(true, now + Duration::from_secs(12));
    assert_eq!(mover.deadline, Some(first_deadline));
    assert!(
        !mover
            .tick(first_deadline - Duration::from_millis(1), &mouse)
            .unwrap()
    );
    assert_eq!(mouse.calls(), 0);

    assert!(mover.tick(first_deadline, &mouse).unwrap());
    assert_eq!(mouse.calls(), 1);
    let second_deadline = first_deadline + MOUSE_MOVE_INTERVAL;
    assert_eq!(mover.deadline, Some(second_deadline));
    assert!(!mover.tick(first_deadline, &mouse).unwrap());
    assert_eq!(mouse.calls(), 1);

    assert!(mover.tick(second_deadline, &mouse).unwrap());
    assert_eq!(mouse.calls(), 2);
    assert_eq!(mover.deadline, Some(second_deadline + MOUSE_MOVE_INTERVAL));
}

#[test]
fn mouse_disabling_cancels_the_deadline_and_reenabling_waits_a_full_interval() {
    let now = Instant::now();
    let mouse = FakeMouse::new(false);
    let mut mover = MouseMover::default();
    mover.configure(true, now);
    let first_deadline = now + MOUSE_MOVE_INTERVAL;

    let disabled_at = now + Duration::from_secs(10);
    mover.configure(false, disabled_at);
    assert!(mover.deadline.is_none());
    assert!(
        !mover
            .tick(first_deadline + Duration::from_secs(100), &mouse)
            .unwrap()
    );
    assert_eq!(mouse.calls(), 0);

    let enabled_at = first_deadline + Duration::from_secs(100);
    mover.configure(true, enabled_at);
    assert_eq!(mover.deadline, Some(enabled_at + MOUSE_MOVE_INTERVAL));
    assert!(!mover.tick(enabled_at, &mouse).unwrap());
    assert_eq!(mouse.calls(), 0);
    assert!(
        mover
            .tick(enabled_at + MOUSE_MOVE_INTERVAL, &mouse)
            .unwrap()
    );
    assert_eq!(mouse.calls(), 1);
}

#[test]
fn mouse_delayed_or_failed_pulses_do_not_catch_up_and_keep_the_next_interval() {
    let now = Instant::now();
    let mouse = FakeMouse::new(true);
    let mut mover = MouseMover::default();
    mover.configure(true, now);
    let first_deadline = now + MOUSE_MOVE_INTERVAL;
    let delayed = first_deadline + Duration::from_secs(95);

    let error = mover.tick(delayed, &mouse).unwrap_err();
    assert_eq!(error.key, "mouseMove.error.blocked");
    assert_eq!(mouse.calls(), 1);
    let next_deadline = delayed + MOUSE_MOVE_INTERVAL;
    assert_eq!(mover.deadline, Some(next_deadline));
    assert!(!mover.tick(delayed, &mouse).unwrap());
    assert_eq!(mouse.calls(), 1);

    mouse.fail.store(false, Ordering::SeqCst);
    assert!(mover.tick(next_deadline, &mouse).unwrap());
    assert_eq!(mouse.calls(), 2);
    assert_eq!(mover.deadline, Some(next_deadline + MOUSE_MOVE_INTERVAL));
}
