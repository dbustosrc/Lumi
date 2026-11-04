use super::settings::ReminderSettings;
use crate::domain::countdown::Countdown;
use crate::domain::notch::NotchPage;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusPhase {
    Focus,
    ShortBreak,
    LongBreak,
}
#[derive(Clone, Debug)]
pub struct ReminderNotice {
    pub key: &'static str,
    pub detail: String,
    pub page: NotchPage,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UtilityState {
    pub phase: FocusPhase,
    pub focus_seconds: u64,
    pub focus_running: bool,
    pub focus_total_seconds: u64,
    pub task_id: Option<Uuid>,
    pub task_title: String,
    pub task_seconds: u64,
    pub can_snooze: bool,
}

pub struct Reminders {
    settings: ReminderSettings,
    focus: Countdown,
    phase: FocusPhase,
    focus_cycles: u16,
    task: Option<(Uuid, String, Countdown)>,
    last_notice: Option<ReminderNotice>,
    snooze_at: Option<Instant>,
}

impl Reminders {
    pub fn task_start_enabled(&self) -> bool {
        self.settings.task_start
    }
    pub fn new(settings: ReminderSettings) -> Self {
        let focus = Countdown::new(minutes(settings.focus_minutes));
        Self {
            settings,
            focus,
            phase: FocusPhase::Focus,
            focus_cycles: 0,
            task: None,
            last_notice: None,
            snooze_at: None,
        }
    }
    pub fn configure(&mut self, settings: &ReminderSettings) {
        let old_duration = self.phase_minutes();
        self.settings = settings.clone();
        if !self.focus.running() && old_duration != self.phase_minutes() {
            self.focus = Countdown::new(minutes(self.phase_minutes()));
        }
    }
    fn phase_minutes(&self) -> u16 {
        match self.phase {
            FocusPhase::Focus => self.settings.focus_minutes,
            FocusPhase::ShortBreak => self.settings.short_break_minutes,
            FocusPhase::LongBreak => self.settings.long_break_minutes,
        }
    }
    pub fn toggle_focus(&mut self, now: Instant) {
        self.focus.toggle(now);
    }
    pub fn select_phase(&mut self, phase: FocusPhase) {
        self.phase = phase;
        self.focus = Countdown::new(minutes(self.phase_minutes()));
    }
    pub fn reset_focus(&mut self) {
        self.phase = FocusPhase::Focus;
        self.focus_cycles = 0;
        self.focus = Countdown::new(minutes(self.settings.focus_minutes));
    }
    pub fn start_task(&mut self, id: Uuid, title: String, now: Instant) -> Option<ReminderNotice> {
        self.last_notice = None;
        self.snooze_at = None;
        let mut timer = Countdown::new(minutes(self.settings.task_minutes));
        timer.toggle(now);
        self.task = Some((id, title.clone(), timer));
        self.settings.task_start.then_some(ReminderNotice {
            key: "desktopUi.task-started",
            detail: title,
            page: NotchPage::Todo,
        })
    }
    pub fn cancel_task(&mut self, id: Uuid) {
        if self.task.as_ref().is_some_and(|task| task.0 == id) {
            self.task = None;
            self.last_notice = None;
            self.snooze_at = None;
        }
    }
    pub fn snooze(&mut self, now: Instant) {
        if self.last_notice.is_some() {
            self.snooze_at = Some(now + minutes(self.settings.snooze_minutes));
        }
    }
    pub fn remember(&mut self, notice: &ReminderNotice) {
        if notice.page == NotchPage::Todo {
            self.last_notice = Some(notice.clone());
            self.snooze_at = None;
        }
    }
    pub fn tick(&mut self, now: Instant) -> Vec<ReminderNotice> {
        let mut notices = Vec::new();
        if self.focus.finish(now) {
            self.phase = match self.phase {
                FocusPhase::Focus => {
                    self.focus_cycles += 1;
                    if self.focus_cycles >= self.settings.focus_cycles {
                        self.focus_cycles = 0;
                        FocusPhase::LongBreak
                    } else {
                        FocusPhase::ShortBreak
                    }
                }
                _ => FocusPhase::Focus,
            };
            self.focus = Countdown::new(minutes(self.phase_minutes()));
            notices.push(ReminderNotice {
                key: if self.phase == FocusPhase::Focus {
                    "desktopUi.focus"
                } else {
                    "desktopUi.break"
                },
                detail: String::new(),
                page: NotchPage::Pomodoro,
            });
        }
        if let Some((_, title, timer)) = &mut self.task
            && timer.finish(now)
            && self.settings.task_end
        {
            notices.push(ReminderNotice {
                key: "desktopUi.task-ready",
                detail: title.clone(),
                page: NotchPage::Todo,
            });
        }
        if self.snooze_at.is_some_and(|deadline| now >= deadline) {
            self.snooze_at = None;
            if let Some(notice) = &self.last_notice {
                notices.push(notice.clone());
            }
        }
        notices
    }
    pub fn needs_tick(&self) -> bool {
        self.focus.running()
            || self.task.as_ref().is_some_and(|task| task.2.running())
            || self.snooze_at.is_some()
    }
    pub fn state(&self, now: Instant) -> UtilityState {
        UtilityState {
            phase: self.phase,
            focus_seconds: seconds(self.focus.remaining(now)),
            focus_running: self.focus.running(),
            focus_total_seconds: u64::from(self.phase_minutes()) * 60,
            task_id: self.task.as_ref().map(|task| task.0),
            task_title: self
                .task
                .as_ref()
                .map_or_else(String::new, |task| task.1.clone()),
            task_seconds: self
                .task
                .as_ref()
                .map_or(0, |task| seconds(task.2.remaining(now))),
            can_snooze: self.last_notice.is_some(),
        }
    }
}
fn minutes(value: u16) -> Duration {
    Duration::from_secs(u64::from(value) * 60)
}
fn seconds(value: Duration) -> u64 {
    value.as_secs() + u64::from(value.subsec_nanos() > 0)
}

#[cfg(test)]
#[path = "../../tests/fixtures/reminders.rs"]
mod tests;
