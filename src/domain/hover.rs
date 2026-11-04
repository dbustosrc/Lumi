use std::time::{Duration, Instant};

pub const HOVER_OPEN_DELAY: Duration = Duration::from_millis(250);
pub const HOVER_LEAVE_GRACE: Duration = Duration::from_millis(120);
pub const fn motion_settle_delay(fast: bool) -> Duration {
    Duration::from_millis(if fast { 340 } else { 580 })
}

/// Pointer presence and the delayed opening are separate facts.
#[derive(Default)]
pub struct HoverState {
    fast_motion: bool,
    is_inside: bool,
    is_open: bool,
    enter_until: Option<Instant>,
    leave_until: Option<Instant>,
    reopen_until: Option<Instant>,
}

impl HoverState {
    pub fn set_fast_motion(&mut self, fast: bool) {
        self.fast_motion = fast;
    }
    pub fn set_inside(&mut self, is_inside: bool, now: Instant) {
        self.set_inside_with_policy(is_inside, now, true);
    }

    pub fn set_inside_with_policy(&mut self, is_inside: bool, now: Instant, open_on_hover: bool) {
        self.is_inside = is_inside;
        if is_inside {
            self.leave_until = None;
            if self.is_open || self.reopen_until.is_some_and(|until| now < until) {
                self.is_open = true;
                self.enter_until = None;
            } else if !open_on_hover {
                self.enter_until = None;
            } else if self.enter_until.is_none() {
                self.enter_until = Some(now + HOVER_OPEN_DELAY);
            }
        } else {
            self.enter_until = None;
            if self.is_open && self.leave_until.is_none() {
                self.leave_until = Some(now + HOVER_LEAVE_GRACE);
            }
        }
    }

    pub fn reveal_for(&mut self, now: Instant, duration: Duration) {
        self.is_open = true;
        self.enter_until = None;
        self.leave_until = (!self.is_inside).then_some(now + duration);
    }

    pub fn advance(&mut self, now: Instant) {
        if self.enter_until.is_some_and(|until| until <= now) {
            self.enter_until = None;
            self.is_open = self.is_inside;
        }
        if self.leave_until.is_some_and(|until| until <= now) {
            self.leave_until = None;
            self.is_open = false;
            self.reopen_until = Some(now + motion_settle_delay(self.fast_motion));
        }
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.enter_until.into_iter().chain(self.leave_until).min()
    }
}
