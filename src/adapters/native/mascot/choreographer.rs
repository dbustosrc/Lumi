use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::super::model::{Rect, SURFACE_WIDTH};
use crate::application::presenter::VoiceState;
use crate::application::reminders::FocusPhase;
use crate::domain::device::PowerStatus;

pub const MASCOT_ALERT_DURATION: Duration = Duration::from_millis(2800);
pub const MASCOT_WAKE_DURATION: Duration = Duration::from_millis(1600);
pub const MASCOT_GREETING_DURATION: Duration = Duration::from_millis(2100);
pub const MASCOT_HEADPHONES_DURATION: Duration = Duration::from_millis(3600);
pub const VARIANTS_PER_CLIP: u8 = 3;

const REACTION_DURATION: Duration = Duration::from_millis(2800);
const OPEN_PANEL_REACTION_TTL: Duration = Duration::from_secs(15);
const PAUSED_CLIP_TTL: Duration = Duration::from_secs(15);
const STALK_TRANSITION: Duration = Duration::from_millis(260);
const PAW_SWIPE_DURATION: Duration = Duration::from_millis(420);
const PAW_SWIPE_COOLDOWN: Duration = Duration::from_millis(950);
const PAW_SWIPE_DISTANCE: f32 = 54.;
const MAX_QUEUED_REACTIONS: usize = 3;
const CLIP_KIND_COUNT: usize = 11;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MascotClip {
    WakeRubEye,
    GreetingWave,
    BellAlert,
    Headphones,
    WaterSip,
    PomodoroStart,
    PomodoroComplete,
    FileCatch,
    VoiceReaction,
    ChargerZap,
    TaskCheer,
}

impl MascotClip {
    pub const fn index(self) -> usize {
        match self {
            Self::WakeRubEye => 0,
            Self::GreetingWave => 1,
            Self::BellAlert => 2,
            Self::Headphones => 3,
            Self::WaterSip => 4,
            Self::PomodoroStart => 5,
            Self::PomodoroComplete => 6,
            Self::FileCatch => 7,
            Self::VoiceReaction => 8,
            Self::ChargerZap => 9,
            Self::TaskCheer => 10,
        }
    }

    pub fn duration(self) -> Duration {
        match self {
            Self::WakeRubEye => MASCOT_WAKE_DURATION,
            Self::GreetingWave => MASCOT_GREETING_DURATION,
            Self::BellAlert => MASCOT_ALERT_DURATION,
            Self::Headphones => MASCOT_HEADPHONES_DURATION,
            Self::WaterSip
            | Self::PomodoroStart
            | Self::PomodoroComplete
            | Self::FileCatch
            | Self::VoiceReaction
            | Self::ChargerZap
            | Self::TaskCheer => REACTION_DURATION,
        }
    }

    pub fn is_wake_sequence(self) -> bool {
        matches!(self, Self::WakeRubEye | Self::GreetingWave)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CursorSample {
    pub point: Option<(f32, f32)>,
    pub capsule: Rect,
    pub radius: f32,
    pub is_compact: bool,
    pub is_enabled: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct MascotLiveState {
    pub start: Instant,
    pub is_compact: bool,
    pub is_resting_view: bool,
    pub is_dozing: bool,
    pub is_playing: bool,
    pub has_headphones: bool,
    pub voice: VoiceState,
    pub shelf_drag: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct PawSwipe {
    pub progress: f32,
    pub target: (f32, f32),
    pub side: f32,
}

#[derive(Clone, Copy, Debug)]
struct ActiveClip {
    clip: MascotClip,
    variant: u8,
    chained_variant: u8,
    elapsed_before: Duration,
    resumed_at: Option<Instant>,
    paused_at: Option<Instant>,
}

impl ActiveClip {
    fn new_running(clip: MascotClip, variant: u8, chained_variant: u8, now: Instant) -> Self {
        Self {
            clip,
            variant,
            chained_variant,
            elapsed_before: Duration::ZERO,
            resumed_at: Some(now),
            paused_at: None,
        }
    }

    fn total_duration(self) -> Duration {
        if self.clip == MascotClip::WakeRubEye {
            MASCOT_WAKE_DURATION + MASCOT_GREETING_DURATION
        } else {
            self.clip.duration()
        }
    }

    fn elapsed(self, now: Instant) -> Duration {
        match self.resumed_at {
            Some(resumed) => self.elapsed_before + now.saturating_duration_since(resumed),
            None => self.elapsed_before,
        }
    }

    fn pause_expired(self, now: Instant) -> bool {
        self.paused_at
            .is_some_and(|paused| now.saturating_duration_since(paused) > PAUSED_CLIP_TTL)
    }

    fn sync_visibility(&mut self, is_visible: bool, now: Instant) -> (bool, bool) {
        if is_visible {
            if self.resumed_at.is_none() {
                if self.pause_expired(now) {
                    return (false, true);
                }
                self.resumed_at = Some(now);
                self.paused_at = None;
                return (true, false);
            }
            (false, false)
        } else {
            if let Some(resumed) = self.resumed_at.take() {
                self.elapsed_before += now.saturating_duration_since(resumed);
                self.paused_at = Some(now);
            } else if self.pause_expired(now) {
                return (false, true);
            }
            (false, false)
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PeekStageClip {
    clip: MascotClip,
    variant: u8,
    started_at: Instant,
}

#[derive(Clone, Copy, Debug)]
struct QueuedReaction {
    clip: MascotClip,
    variant: u8,
    expires_at: Option<Instant>,
}

pub struct MascotChoreographer {
    active: Option<ActiveClip>,
    peek_active: bool,
    last_peek: Option<PeekStageClip>,
    queue: VecDeque<QueuedReaction>,
    next_variant: [u8; CLIP_KIND_COUNT],
    is_cursor_near: bool,
    gaze_from: (f32, f32),
    gaze_to: (f32, f32),
    gaze_changed: Instant,
    stalk_from: f32,
    stalk_changed: Instant,
    paw_swipe_started: Option<Instant>,
    swipe_target: (f32, f32),
    swipe_side: f32,
    prev_water_ml: Option<u64>,
    prev_focus_running: Option<bool>,
    prev_shelf_count: Option<usize>,
    prev_tasks_counts: Option<(usize, usize)>,
    prev_plugged_in: Option<bool>,
    prev_voice_busy: bool,
}

impl MascotChoreographer {
    pub fn new(now: Instant) -> Self {
        let settled = now - Duration::from_secs(2);
        Self {
            active: None,
            peek_active: false,
            last_peek: None,
            queue: VecDeque::with_capacity(MAX_QUEUED_REACTIONS),
            next_variant: [0; CLIP_KIND_COUNT],
            is_cursor_near: false,
            gaze_from: (0., 0.),
            gaze_to: (0., 0.),
            gaze_changed: settled,
            stalk_from: 0.,
            stalk_changed: settled,
            paw_swipe_started: None,
            swipe_target: (SURFACE_WIDTH / 2. + 24., 42.),
            swipe_side: 1.,
            prev_water_ml: None,
            prev_focus_running: None,
            prev_shelf_count: None,
            prev_tasks_counts: None,
            prev_plugged_in: None,
            prev_voice_busy: false,
        }
    }

    fn take_variant(&mut self, clip: MascotClip) -> u8 {
        let slot = &mut self.next_variant[clip.index()];
        let chosen = *slot % VARIANTS_PER_CLIP;
        *slot = (*slot + 1) % VARIANTS_PER_CLIP;
        chosen
    }

    #[cfg(test)]
    pub fn start_clip(&mut self, clip: MascotClip, started_at: Instant) {
        self.start_clip_variant(clip, 0, started_at);
    }

    #[cfg(test)]
    pub fn start_clip_variant(&mut self, clip: MascotClip, variant: u8, started_at: Instant) {
        let v = variant % VARIANTS_PER_CLIP;
        self.active = Some(ActiveClip::new_running(clip, v, v, started_at));
    }

    #[cfg(test)]
    pub fn clear_active(&mut self) {
        self.active = None;
    }

    fn has_active_clip(&self, now: Instant) -> bool {
        self.active
            .is_some_and(|a| !a.pause_expired(now) && a.elapsed(now) < a.total_duration())
    }

    pub fn has_pending(&self, clip: MascotClip) -> bool {
        self.queue.iter().any(|entry| entry.clip == clip)
    }

    #[cfg(test)]
    pub fn active_started_at(&self, clip: MascotClip, now: Instant) -> Option<Instant> {
        let active = self.active?;
        if active.pause_expired(now) {
            return None;
        }
        let elapsed = active.elapsed(now);
        if active.clip == clip && elapsed < clip.duration() {
            return now.checked_sub(elapsed);
        }
        if clip == MascotClip::GreetingWave
            && active.clip == MascotClip::WakeRubEye
            && elapsed >= MASCOT_WAKE_DURATION
            && elapsed < MASCOT_WAKE_DURATION + MASCOT_GREETING_DURATION
        {
            return now.checked_sub(elapsed - MASCOT_WAKE_DURATION);
        }
        None
    }

    pub fn resolved_clip(
        &self,
        now: Instant,
        is_compact: bool,
        has_headphones: bool,
    ) -> Option<(MascotClip, u8, f32)> {
        if !is_compact {
            return None;
        }
        let active = self.active?;
        if active.pause_expired(now) {
            return None;
        }
        if active.clip == MascotClip::Headphones && !has_headphones {
            return None;
        }
        let elapsed = active.elapsed(now);
        if active.clip == MascotClip::WakeRubEye {
            if elapsed < MASCOT_WAKE_DURATION {
                return Some((
                    MascotClip::WakeRubEye,
                    active.variant,
                    elapsed.as_secs_f32() / MASCOT_WAKE_DURATION.as_secs_f32(),
                ));
            }
            let greet_elapsed = elapsed - MASCOT_WAKE_DURATION;
            if greet_elapsed < MASCOT_GREETING_DURATION {
                return Some((
                    MascotClip::GreetingWave,
                    active.chained_variant,
                    greet_elapsed.as_secs_f32() / MASCOT_GREETING_DURATION.as_secs_f32(),
                ));
            }
            return None;
        }
        let duration = active.clip.duration();
        (elapsed < duration).then(|| {
            (
                active.clip,
                active.variant,
                elapsed.as_secs_f32() / duration.as_secs_f32(),
            )
        })
    }

    pub fn sync_stage(
        &mut self,
        peek_clip: Option<MascotClip>,
        is_compact: bool,
        now: Instant,
        is_dozing: &mut bool,
    ) {
        if let Some(clip) = peek_clip {
            *is_dozing = false;
            let mut claimed_variant = None;
            if self.active.is_some_and(|active| active.clip == clip) {
                claimed_variant = self.active.take().map(|active| active.variant);
            } else if let Some(active) = &mut self.active {
                let (_, expired) = active.sync_visibility(false, now);
                if expired {
                    self.active = None;
                }
            }
            if let Some(pos) = self.queue.iter().position(|entry| entry.clip == clip)
                && let Some(removed) = self.queue.remove(pos)
            {
                claimed_variant.get_or_insert(removed.variant);
            }
            let reuse_running =
                self.peek_active && self.last_peek.is_some_and(|stage| stage.clip == clip);
            if !reuse_running {
                let variant = claimed_variant.unwrap_or_else(|| self.take_variant(clip));
                self.last_peek = Some(PeekStageClip {
                    clip,
                    variant,
                    started_at: now,
                });
            }
            self.peek_active = true;
        } else {
            self.peek_active = false;
            if let Some(active) = &mut self.active {
                let (_, expired) = active.sync_visibility(is_compact, now);
                if expired {
                    self.active = None;
                }
            }
        }
    }

    pub fn resolved_peek_clip(&self, expected: MascotClip, now: Instant) -> (u8, f32) {
        if let Some(stage) = self.last_peek.filter(|stage| stage.clip == expected) {
            let elapsed = now.saturating_duration_since(stage.started_at).as_secs_f32();
            let duration = expected.duration().as_secs_f32().max(0.001);
            let progress = if elapsed < duration {
                elapsed / duration
            } else {
                (elapsed / duration).fract()
            };
            (stage.variant, progress)
        } else {
            (0, 0.45)
        }
    }

    #[cfg(test)]
    pub fn clip_progress(
        &self,
        clip: MascotClip,
        now: Instant,
        is_compact: bool,
        has_headphones: bool,
    ) -> Option<f32> {
        self.resolved_clip(now, is_compact, has_headphones)
            .and_then(|(current, _, progress)| (current == clip).then_some(progress))
    }

    #[cfg(test)]
    pub fn active_variant(
        &self,
        now: Instant,
        is_compact: bool,
        has_headphones: bool,
    ) -> Option<u8> {
        self.resolved_clip(now, is_compact, has_headphones)
            .map(|(_, variant, _)| variant)
    }

    pub fn voice_live_variant(&self) -> u8 {
        self.next_variant[MascotClip::VoiceReaction.index()] % VARIANTS_PER_CLIP
    }

    pub fn shelf_hover_variant(&self) -> u8 {
        self.next_variant[MascotClip::FileCatch.index()] % VARIANTS_PER_CLIP
    }

    pub fn wake(&mut self, now: Instant, is_enabled: bool, is_dozing: &mut bool) {
        if !is_enabled || !*is_dozing {
            return;
        }
        *is_dozing = false;
        let has_priority_clip = self.active.is_some_and(|a| {
            !a.clip.is_wake_sequence()
                && !a.pause_expired(now)
                && a.elapsed(now) < a.total_duration()
        });
        if !self.is_cursor_near && !has_priority_clip && !self.peek_active {
            let wake_variant = self.take_variant(MascotClip::WakeRubEye);
            let greet_variant = self.take_variant(MascotClip::GreetingWave);
            self.active = Some(ActiveClip::new_running(
                MascotClip::WakeRubEye,
                wake_variant,
                greet_variant,
                now,
            ));
        }
    }

    pub fn enqueue(
        &mut self,
        clip: MascotClip,
        now: Instant,
        expires_after: Option<Duration>,
    ) {
        let already_active = self.active.is_some_and(|a| {
            a.clip == clip && !a.pause_expired(now) && a.elapsed(now) < a.total_duration()
        }) || (self.peek_active && self.last_peek.is_some_and(|s| s.clip == clip));
        if already_active || self.has_pending(clip) {
            return;
        }
        if self.queue.len() >= MAX_QUEUED_REACTIONS {
            self.queue.pop_front();
        }
        let variant = self.take_variant(clip);
        self.queue.push_back(QueuedReaction {
            clip,
            variant,
            expires_at: expires_after.map(|ttl| now + ttl),
        });
    }

    pub fn advance(
        &mut self,
        now: Instant,
        is_enabled: bool,
        is_compact: bool,
        has_headphones: bool,
        is_dozing: &mut bool,
    ) -> bool {
        if !is_enabled {
            self.active = None;
            self.peek_active = false;
            self.last_peek = None;
            self.queue.clear();
            self.paw_swipe_started = None;
            return false;
        }
        self.queue.retain(|entry| {
            entry.expires_at.is_none_or(|exp| now <= exp)
                && (entry.clip != MascotClip::Headphones || has_headphones)
        });
        let mut resumed_active = false;
        if let Some(mut active) = self.active {
            let (resumed, pause_expired) = active.sync_visibility(is_compact, now);
            resumed_active = resumed;
            let expired = pause_expired
                || (active.clip == MascotClip::Headphones && !has_headphones)
                || active.elapsed(now) >= active.total_duration();
            self.active = (!expired).then_some(active);
        }
        if !is_compact {
            return false;
        }
        let blocking_reaction = self.active.is_some_and(|a| {
            !a.clip.is_wake_sequence() && a.elapsed(now) < a.total_duration()
        });
        if blocking_reaction {
            return resumed_active;
        }
        let Some(next) = self.queue.pop_front() else {
            return resumed_active;
        };
        *is_dozing = false;
        self.active = Some(ActiveClip::new_running(
            next.clip,
            next.variant,
            next.variant,
            now,
        ));
        true
    }

    pub fn gaze(&self, now: Instant) -> (f32, f32) {
        let t = (now
            .saturating_duration_since(self.gaze_changed)
            .as_secs_f32()
            / 0.18)
            .clamp(0., 1.);
        let ease = 1. - (1. - t).powi(3);
        (
            self.gaze_from.0 + (self.gaze_to.0 - self.gaze_from.0) * ease,
            self.gaze_from.1 + (self.gaze_to.1 - self.gaze_from.1) * ease,
        )
    }

    pub fn stalk_amount(&self, now: Instant) -> f32 {
        let t = (now
            .saturating_duration_since(self.stalk_changed)
            .as_secs_f32()
            / STALK_TRANSITION.as_secs_f32())
        .min(1.);
        let eased = t * t * (3. - 2. * t);
        let target = if self.is_cursor_near { 1. } else { 0. };
        self.stalk_from + (target - self.stalk_from) * eased
    }

    pub fn is_cursor_near(&self) -> bool {
        self.is_cursor_near
    }

    pub fn paw_swipe(&self, now: Instant) -> Option<PawSwipe> {
        let started = self.paw_swipe_started?;
        let elapsed = now.saturating_duration_since(started);
        (elapsed < PAW_SWIPE_DURATION).then(|| PawSwipe {
            progress: elapsed.as_secs_f32() / PAW_SWIPE_DURATION.as_secs_f32(),
            target: self.swipe_target,
            side: self.swipe_side,
        })
    }

    pub fn observe_cursor(
        &mut self,
        sample: CursorSample,
        is_dozing: &mut bool,
        now: Instant,
    ) -> bool {
        let mut dist_to_capsule = f32::INFINITY;
        let nearby = sample.point.filter(|(x, y)| {
            if !sample.is_compact || sample.radius <= 0. {
                return false;
            }
            let dx = x - x.clamp(sample.capsule.x, sample.capsule.x + sample.capsule.w);
            let dy = y - y.clamp(0., sample.capsule.h);
            dist_to_capsule = dx.hypot(dy);
            dist_to_capsule <= sample.radius
        });
        let near = nearby.is_some();
        let target = nearby.map_or((0., 0.), |(x, y)| {
            (
                ((x - SURFACE_WIDTH / 2.) / sample.radius).clamp(-1., 1.) * 3.5,
                ((y - 15.) / sample.radius).clamp(-1., 1.) * 2.,
            )
        });
        let mut swipe_triggered = false;
        if let Some((x, y)) = nearby {
            let can_swipe = !self.has_active_clip(now)
                && self
                    .paw_swipe_started
                    .is_none_or(|s| now.saturating_duration_since(s) >= PAW_SWIPE_COOLDOWN);
            if dist_to_capsule > 2. && dist_to_capsule <= PAW_SWIPE_DISTANCE && y >= 18. && can_swipe
            {
                self.paw_swipe_started = Some(now);
                self.swipe_target = (x, y.min(48.));
                let offset = x - SURFACE_WIDTH / 2.;
                self.swipe_side = if offset < -8. {
                    -1.
                } else if offset > 8. {
                    1.
                } else {
                    -self.swipe_side
                };
                swipe_triggered = true;
            }
        }
        if near != self.is_cursor_near {
            self.stalk_from = self.stalk_amount(now);
            self.stalk_changed = now;
        }
        let changed = near != self.is_cursor_near
            || swipe_triggered
            || (target.0 - self.gaze_to.0).hypot(target.1 - self.gaze_to.1) > 0.05;
        if changed {
            self.gaze_from = self.gaze(now);
            self.gaze_to = target;
            self.gaze_changed = now;
            self.is_cursor_near = near;
        }
        if near && sample.is_enabled && *is_dozing {
            *is_dozing = false;
        }
        changed
    }

    pub fn on_alert(&mut self, is_enabled: bool, now: Instant) {
        if is_enabled {
            self.enqueue(MascotClip::BellAlert, now, None);
        }
    }

    pub fn on_headphones_connected(
        &mut self,
        is_enabled: bool,
        has_headphones: bool,
        now: Instant,
    ) {
        if is_enabled && has_headphones {
            self.enqueue(MascotClip::Headphones, now, None);
        }
    }

    pub fn on_headphones_updated(&mut self, has_headphones: bool) {
        if !has_headphones {
            self.queue
                .retain(|entry| entry.clip != MascotClip::Headphones);
            if self
                .active
                .is_some_and(|a| a.clip == MascotClip::Headphones)
            {
                self.active = None;
            }
        }
    }

    pub fn on_water_updated(&mut self, consumed_ml: u64, is_enabled: bool, now: Instant) {
        if let Some(prev) = self.prev_water_ml
            && is_enabled
            && consumed_ml > prev
        {
            self.enqueue(MascotClip::WaterSip, now, Some(OPEN_PANEL_REACTION_TTL));
        }
        self.prev_water_ml = Some(consumed_ml);
    }

    pub fn on_water_reminder(&mut self, is_enabled: bool, now: Instant) {
        if is_enabled {
            self.enqueue(MascotClip::WaterSip, now, Some(OPEN_PANEL_REACTION_TTL));
        }
    }

    pub fn on_utility_updated(&mut self, focus_running: bool, is_enabled: bool, now: Instant) {
        if let Some(was_running) = self.prev_focus_running
            && is_enabled
            && !was_running
            && focus_running
        {
            self.enqueue(MascotClip::PomodoroStart, now, Some(OPEN_PANEL_REACTION_TTL));
        }
        self.prev_focus_running = Some(focus_running);
    }

    pub fn on_focus_reminder(&mut self, phase: FocusPhase, is_enabled: bool, now: Instant) {
        if is_enabled {
            let clip = if phase == FocusPhase::Focus {
                MascotClip::PomodoroStart
            } else {
                MascotClip::PomodoroComplete
            };
            self.enqueue(clip, now, Some(Duration::from_secs(20)));
        }
    }

    pub fn on_shelf_updated(&mut self, count: usize, is_enabled: bool, now: Instant) {
        if let Some(prev) = self.prev_shelf_count
            && is_enabled
            && count > prev
        {
            self.enqueue(MascotClip::FileCatch, now, Some(OPEN_PANEL_REACTION_TTL));
        }
        self.prev_shelf_count = Some(count);
    }

    pub fn on_tasks_updated(
        &mut self,
        completed: usize,
        total: usize,
        is_enabled: bool,
        now: Instant,
    ) {
        if let Some((prev_done, prev_total)) = self.prev_tasks_counts
            && is_enabled
            && completed > prev_done
            && total >= prev_total
        {
            self.enqueue(MascotClip::TaskCheer, now, Some(OPEN_PANEL_REACTION_TTL));
        }
        self.prev_tasks_counts = Some((completed, total));
    }

    pub fn on_power_updated(&mut self, status: PowerStatus, is_enabled: bool, now: Instant) {
        if let Some(was_plugged) = self.prev_plugged_in
            && is_enabled
            && !was_plugged
            && status.is_plugged_in
            && status.has_battery
        {
            self.enqueue(MascotClip::ChargerZap, now, Some(OPEN_PANEL_REACTION_TTL));
        }
        self.prev_plugged_in = Some(status.is_plugged_in);
    }

    pub fn on_voice_updated(&mut self, state: VoiceState, is_enabled: bool, now: Instant) {
        let is_busy = matches!(state, VoiceState::Recording | VoiceState::Transcribing);
        if self.prev_voice_busy && state == VoiceState::Idle && is_enabled {
            self.enqueue(MascotClip::VoiceReaction, now, Some(OPEN_PANEL_REACTION_TTL));
        } else if self.prev_voice_busy && !is_busy {
            let _ = self.take_variant(MascotClip::VoiceReaction);
        }
        self.prev_voice_busy = is_busy;
    }

    pub fn has_closed_stage(
        &self,
        now: Instant,
        is_compact: bool,
        has_headphones: bool,
        focus_running: bool,
        voice: VoiceState,
        shelf_drag: bool,
    ) -> bool {
        self.resolved_clip(now, is_compact, has_headphones).is_some()
            || matches!(voice, VoiceState::Recording | VoiceState::Transcribing)
            || shelf_drag
            || (self.stalk_amount(now) > 0.01 && !focus_running)
    }

    pub fn is_animating(&self, now: Instant, state: MascotLiveState) -> bool {
        if self.peek_active {
            return true;
        }
        if !state.is_compact {
            return false;
        }
        if self
            .resolved_clip(now, state.is_compact, state.has_headphones)
            .is_some()
            || self.paw_swipe(now).is_some()
            || matches!(state.voice, VoiceState::Recording | VoiceState::Transcribing)
            || state.shelf_drag
            || now.saturating_duration_since(self.stalk_changed) < STALK_TRANSITION
            || now.saturating_duration_since(self.gaze_changed) < Duration::from_millis(180)
        {
            return true;
        }
        let elapsed = now.saturating_duration_since(state.start).as_secs_f32();
        if self.is_cursor_near {
            let wiggle = elapsed % 2.4;
            let blinking = elapsed > 3.4 && elapsed % 3.4 < 0.28;
            return (1.45..2.10).contains(&wiggle) || blinking;
        }
        if state.is_resting_view {
            if state.is_dozing {
                return elapsed % 7.0 < 3.2;
            }
            if state.is_playing {
                return true;
            }
            return (elapsed > 3.4 && elapsed % 3.4 < 0.28) || elapsed % 2.6 < 0.28;
        }
        false
    }
}
