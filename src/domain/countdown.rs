use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Countdown {
    remaining: Duration,
    deadline: Option<Instant>,
}
impl Countdown {
    pub fn new(duration: Duration) -> Self {
        Self {
            remaining: duration,
            deadline: None,
        }
    }
    pub fn toggle(&mut self, now: Instant) {
        if let Some(deadline) = self.deadline.take() {
            self.remaining = deadline.saturating_duration_since(now);
        } else if !self.remaining.is_zero() {
            self.deadline = Some(now + self.remaining);
        }
    }
    pub fn remaining(&self, now: Instant) -> Duration {
        self.deadline.map_or(self.remaining, |deadline| {
            deadline.saturating_duration_since(now)
        })
    }
    pub fn running(&self) -> bool {
        self.deadline.is_some()
    }
    pub fn finish(&mut self, now: Instant) -> bool {
        if self.deadline.is_some_and(|deadline| deadline <= now) {
            self.deadline = None;
            self.remaining = Duration::ZERO;
            return true;
        }
        false
    }
}
