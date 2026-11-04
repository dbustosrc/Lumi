use super::events::{AppEvent, EventSender};
use super::settings::AppSettings;
use crate::domain::ports::{MouseActivity, PortError};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;

pub const MOUSE_MOVE_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Default)]
struct MouseMover {
    deadline: Option<Instant>,
}
impl MouseMover {
    fn configure(&mut self, enabled: bool, now: Instant) {
        if enabled != self.deadline.is_some() {
            self.deadline = enabled.then_some(now + MOUSE_MOVE_INTERVAL);
        }
    }
    fn tick(&mut self, now: Instant, mouse: &dyn MouseActivity) -> Result<bool, PortError> {
        if !self.deadline.is_some_and(|deadline| now >= deadline) {
            return Ok(false);
        }
        // Resume from now rather than replaying missed ticks after sleep.
        self.deadline = Some(now + MOUSE_MOVE_INTERVAL);
        mouse.pulse()?;
        Ok(true)
    }
}

pub async fn run(
    mut settings: watch::Receiver<AppSettings>,
    mouse: Arc<dyn MouseActivity>,
    events: EventSender,
) {
    let mut mover = MouseMover::default();
    let enabled = settings.borrow_and_update().is_mouse_move_enabled;
    mover.configure(enabled, Instant::now());
    let mut last_error = None;
    loop {
        let Some(deadline) = mover.deadline else {
            if settings.changed().await.is_err() {
                break;
            }
            let enabled = settings.borrow_and_update().is_mouse_move_enabled;
            mover.configure(enabled, Instant::now());
            continue;
        };
        tokio::select! {
            biased;
            changed = settings.changed() => {
                if changed.is_err() { break; }
                let enabled = settings.borrow_and_update().is_mouse_move_enabled;
                mover.configure(enabled, Instant::now());
                if !enabled && last_error.take().is_some() {
                    let _ = events.send(AppEvent::MouseMoveStatus(None));
                }
            }
            _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => {
                let error = mover.tick(Instant::now(), mouse.as_ref()).err();
                let key = error.as_ref().map(|error| error.key);
                if key != last_error {
                    if let Some(error) = &error {
                        tracing::warn!(event = "log.mouseMove.failed", error = %error);
                    }
                    let _ = events.send(AppEvent::MouseMoveStatus(key));
                    last_error = key;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/fixtures/mouse_mover.rs"]
mod tests;
