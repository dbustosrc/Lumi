use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use uuid::Uuid;

use crate::application::presenter::{LyricsAvailability, ShelfEntry, ViewUpdate, VoiceState};
use crate::application::reminders::{FocusPhase, UtilityState};
use crate::application::settings::AppSettings;
use crate::domain::calendar::CalendarGrid;
use crate::domain::device::{HeadphoneStatus, PowerStatus};
use crate::domain::media::{Artwork, MediaConnection, Track};
use crate::domain::notch::NotchView;
use crate::domain::todo::{TodoProgress, TodoTask};
use crate::domain::water::WaterState;

#[cfg(test)]
pub use super::mascot::{MascotClip, VARIANTS_PER_CLIP};
pub use super::mascot::{CursorSample, MascotChoreographer, MascotLiveState};

pub const MASCOT_PROXIMITY_DIPS: f32 = 96. * 5. / 2.54;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
    pub fn inset(self, d: f32) -> Self {
        Self::new(
            self.x + d,
            self.y + d,
            (self.w - 2. * d).max(0.),
            (self.h - 2. * d).max(0.),
        )
    }
    pub fn intersect(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Self::new(
            x,
            y,
            (self.x + self.w).min(other.x + other.w).max(x) - x,
            (self.y + self.h).min(other.y + other.h).max(y) - y,
        )
    }
}

pub const SURFACE_WIDTH: f32 = 584.;
pub const SURFACE_HEIGHT: f32 = 182.;
pub const TASK_COLORS: [u32; 5] = [0x38d36f, 0x27a9ff, 0xffb649, 0xff6677, 0xbc8aff];

#[derive(Clone, Copy, Debug)]
pub struct SurfacePlacement {
    pub bounds: Rect,
    pub scale: f32,
}
pub fn surface_placement(
    monitor: Rect,
    dpi_scale: f32,
    user_percent: u16,
) -> Option<SurfacePlacement> {
    if monitor.w <= 0. || monitor.h <= 0. || !dpi_scale.is_finite() || dpi_scale <= 0. {
        return None;
    }
    let user = if user_percent == 0 {
        1.
    } else {
        f32::from(user_percent) / 100.
    };
    let scale = (dpi_scale * user)
        .min(monitor.w / SURFACE_WIDTH)
        .min(monitor.h / SURFACE_HEIGHT);
    let width = (SURFACE_WIDTH * scale).ceil();
    let height = (SURFACE_HEIGHT * scale).ceil();
    Some(SurfacePlacement {
        bounds: Rect::new(
            monitor.x + ((monitor.w - width) / 2.).round(),
            monitor.y,
            width,
            height,
        ),
        scale,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shell {
    pub width: f32,
    pub height: f32,
    pub radius: f32,
    pub ear: f32,
}
impl Shell {
    pub fn for_view(view: NotchView, nav: bool) -> Self {
        let (width, height, radius, ear) = match view {
            NotchView::Minimal => (46., 4., 2., 2.),
            NotchView::Closed | NotchView::Resting => (150., 30., 14., 10.),
            NotchView::Hud | NotchView::ChargingPeek | NotchView::HeadphonesPeek => {
                (310., 60., 19., 13.)
            }
            NotchView::Lyrics => (520., 140. + if nav { 30. } else { 0. }, 32., 22.),
            _ => (420., 140. + if nav { 30. } else { 0. }, 32., 22.),
        };
        Self {
            width,
            height,
            radius,
            ear,
        }
    }
    pub fn body(self) -> Rect {
        Rect::new(
            (SURFACE_WIDTH - self.width) / 2.,
            0.,
            self.width,
            self.height,
        )
    }
}

pub struct Motion {
    from: Shell,
    to: Shell,
    start: Instant,
    duration: Duration,
}
impl Motion {
    pub fn new(view: NotchView) -> Self {
        let s = Shell::for_view(view, false);
        Self {
            from: s,
            to: s,
            start: Instant::now(),
            duration: Duration::ZERO,
        }
    }
    pub fn set(&mut self, to: Shell, fast: bool, now: Instant) {
        if self.to == to {
            return;
        }
        self.from = self.sample(now);
        self.to = to;
        self.start = now;
        self.duration = Duration::from_millis(if fast { 297 } else { 540 });
    }
    pub fn running(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) < self.duration
    }
    pub fn sample(&self, now: Instant) -> Shell {
        if !self.running(now) {
            return self.to;
        }
        let x = (now.saturating_duration_since(self.start).as_secs_f32()
            / self.duration.as_secs_f32())
        .clamp(0., 1.);
        let spring = cubic_bezier(x, 0.34, 1.32, 0.5, 1.);
        let radius_progress = (now.saturating_duration_since(self.start).as_secs_f32()
            / (self.duration.as_secs_f32() * 420. / 540.))
            .clamp(0., 1.);
        let ease = cubic_bezier(radius_progress, 0.25, 0.1, 0.25, 1.);
        let width = (self.from.width + (self.to.width - self.from.width) * spring).max(40.);
        let height = (self.from.height + (self.to.height - self.from.height) * spring).max(4.);
        let ear = (self.from.ear + (self.to.ear - self.from.ear) * ease)
            .min(height * 0.45)
            .max(2.);
        Shell {
            width,
            height,
            ear,
            radius: (self.from.radius + (self.to.radius - self.from.radius) * ease)
                .min(width * 0.45)
                .min(height - ear)
                .max(2.),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TaskDraft {
    pub id: Option<Uuid>,
    pub title: String,
    pub date: String,
    pub time: String,
    pub daily: bool,
    pub color: usize,
    pub saving: bool,
}

pub struct UiModel {
    pub mouse_move_error: Option<&'static str>,
    pub view: NotchView,
    pub previous_view: NotchView,
    pub view_changed: Instant,
    pub page_direction: f32,
    pub motion: Motion,
    pub clock: DateTime<Local>,
    pub power: PowerStatus,
    pub headphones: Option<HeadphoneStatus>,
    pub track: Option<Track>,
    pub playing: bool,
    pub connection: MediaConnection,
    pub artwork: Option<Artwork>,
    pub position: Duration,
    pub duration: Duration,
    pub lyrics: LyricsAvailability,
    pub active_lyric: Option<usize>,
    pub lyrics_mode: bool,
    pub volume: f32,
    pub muted: bool,
    pub volume_shown: bool,
    pub volume_changed: Instant,
    pub volume_from: f32,
    pub tasks: Vec<TodoTask>,
    pub progress: TodoProgress,
    pub voice: VoiceState,
    pub voice_error: Option<&'static str>,
    pub calendar: Option<CalendarGrid>,
    pub calendar_open: bool,
    pub selected_date: String,
    pub task_scroll: f32,
    pub list_expanded: bool,
    pub edit_mode: bool,
    pub draft: Option<TaskDraft>,
    pub palette: bool,
    pub editor_error: Option<&'static str>,
    pub shelf: Vec<ShelfEntry>,
    pub previous_shelf: Vec<ShelfEntry>,
    pub shelf_menu: bool,
    pub shelf_scroll: f32,
    pub shelf_drag: bool,
    pub shelf_text_open: bool,
    pub shelf_text: String,
    pub shelf_change: Instant,
    pub utility: Option<UtilityState>,
    pub water: Option<WaterState>,
    pub water_settings: bool,
    pub water_graph: bool,
    pub water_change: Instant,
    pub previous_water: u64,
    pub previous_hours: [u64; 24],
    pub hud_title: String,
    pub hud_detail: String,
    pub hud_level: Option<f32>,
    pub hud_alert: bool,
    pub hud_reminder: bool,
    pub water_error: Option<&'static str>,
    pub focus_notice: Option<(FocusPhase, u64)>,
    pub mascot: bool,
    pub dozing: bool,
    pub cursor_near: bool,
    pub choreographer: MascotChoreographer,
    pub alert_started: Instant,
    pub nav: bool,
    pub nav_changed: Instant,
    pub nav_from: f32,
    pub list_changed: Instant,
    pub list_from: f32,
    pub lyric_changed: Instant,
    pub lyric_from: f32,
    pub settings: AppSettings,
    pub fullscreen: bool,
    pub input_locked: bool,
    pub hover: Option<usize>,
    pub previous_hover: Option<usize>,
    pub hover_changed: Instant,
    pub mouse: (f32, f32),
    pub start: Instant,
}
impl UiModel {
    pub fn new(settings: AppSettings) -> Self {
        let now = Instant::now();
        let settled = now - Duration::from_secs(1);
        Self {
            mouse_move_error: None,
            view: NotchView::Closed,
            previous_view: NotchView::Closed,
            view_changed: now,
            page_direction: 1.,
            motion: Motion::new(NotchView::Closed),
            clock: Local::now(),
            power: PowerStatus::default(),
            headphones: None,
            track: None,
            playing: false,
            connection: MediaConnection::NotConfigured,
            artwork: None,
            position: Duration::ZERO,
            duration: Duration::ZERO,
            lyrics: LyricsAvailability::Unavailable,
            active_lyric: None,
            lyrics_mode: false,
            volume: 0.,
            muted: false,
            volume_shown: false,
            volume_changed: settled,
            volume_from: 0.,
            tasks: vec![],
            progress: TodoProgress::default(),
            voice: VoiceState::Idle,
            voice_error: None,
            calendar: None,
            calendar_open: false,
            selected_date: String::new(),
            task_scroll: 0.,
            list_expanded: false,
            edit_mode: false,
            draft: None,
            palette: false,
            editor_error: None,
            shelf: vec![],
            previous_shelf: vec![],
            shelf_menu: false,
            shelf_scroll: 0.,
            shelf_drag: false,
            shelf_text_open: false,
            shelf_text: String::new(),
            shelf_change: settled,
            utility: None,
            water: None,
            water_settings: false,
            water_graph: false,
            water_change: settled,
            previous_water: 0,
            previous_hours: [0; 24],
            hud_title: String::new(),
            hud_detail: String::new(),
            hud_level: None,
            hud_alert: false,
            hud_reminder: false,
            water_error: None,
            focus_notice: None,
            mascot: settings.is_mascot_enabled,
            dozing: false,
            cursor_near: false,
            choreographer: MascotChoreographer::new(now),
            alert_started: now,
            nav: false,
            nav_changed: settled,
            nav_from: 0.,
            list_changed: settled,
            list_from: 0.,
            lyric_changed: settled,
            lyric_from: 0.,
            settings,
            fullscreen: false,
            input_locked: false,
            hover: None,
            previous_hover: None,
            hover_changed: settled,
            mouse: (-1., -1.),
            start: now,
        }
    }
    pub fn apply(&mut self, update: ViewUpdate, now: Instant) {
        match update {
            ViewUpdate::MouseMoveStatus(error) => self.mouse_move_error = error,
            ViewUpdate::MascotHeadphones => {
                self.choreographer.on_headphones_connected(
                    self.mascot,
                    self.headphones.is_some(),
                    now,
                );
            }
            ViewUpdate::MascotAlert => {
                self.choreographer.on_alert(self.mascot, now);
                self.advance_mascot_reactions(now);
            }
            ViewUpdate::View(view) => {
                if self.view != view {
                    self.previous_view = self.view;
                    if let (Some(old), Some(new)) = (page_index(self.view), page_index(view)) {
                        self.page_direction = if new >= old { 1. } else { -1. };
                    }
                    self.view = view;
                    self.view_changed = now;
                    self.hover = None;
                    self.previous_hover = None;
                    self.hover_changed = now;
                    self.nav &= is_expanded(self.previous_view) && is_expanded(view);
                    if view != NotchView::Hud {
                        self.hud_alert = false;
                        self.hud_reminder = false;
                    }
                    self.motion.set(
                        Shell::for_view(view, self.nav),
                        self.settings.desktop.fast_animations,
                        now,
                    );
                }
            }
            ViewUpdate::Clock(value) => self.clock = value,
            ViewUpdate::Track { track, is_playing } => {
                self.track = track;
                self.playing = is_playing;
            }
            ViewUpdate::Progress { position, duration } => {
                self.position = position;
                self.duration = duration;
            }
            ViewUpdate::Artwork(value) => self.artwork = value,
            ViewUpdate::Lyrics(value) => self.lyrics = value,
            ViewUpdate::ActiveLyric(value) => {
                if self.active_lyric != value {
                    self.lyric_from = self.lyric_position(now);
                    self.active_lyric = value;
                    self.lyric_changed = now;
                }
            }
            ViewUpdate::LyricsMode(value) => self.lyrics_mode = value,
            ViewUpdate::Tasks { tasks, progress } => {
                self.choreographer.on_tasks_updated(
                    progress.done,
                    progress.total,
                    self.mascot,
                    now,
                );
                self.tasks = tasks;
                self.progress = progress;
            }
            ViewUpdate::Calendar { grid, tasks } => {
                self.calendar = Some(grid);
                self.tasks = tasks;
            }
            ViewUpdate::TodoEditorResult { error_key } => {
                self.editor_error = error_key;
                if error_key.is_none() {
                    self.draft = None;
                } else if let Some(d) = &mut self.draft {
                    d.saving = false;
                }
            }
            ViewUpdate::RecoverTodoDraft { text, error_key } => {
                self.draft = Some(TaskDraft {
                    id: None,
                    title: text,
                    date: self.clock.date_naive().to_string(),
                    time: String::new(),
                    daily: false,
                    color: 0,
                    saving: false,
                });
                self.editor_error = Some(error_key);
            }
            ViewUpdate::Voice(value) => {
                self.choreographer.on_voice_updated(value, self.mascot, now);
                self.voice = value;
                self.voice_error = if let VoiceState::Failed(key) = value {
                    Some(key)
                } else {
                    None
                };
            }
            ViewUpdate::ShelfItems(items) => {
                self.choreographer
                    .on_shelf_updated(items.len(), self.mascot, now);
                self.previous_shelf = std::mem::replace(&mut self.shelf, items);
                self.shelf_change = now;
            }
            ViewUpdate::ShelfThumbnail { id, thumbnail } => {
                if let Some(item) = self.shelf.iter_mut().find(|i| i.item.id == id) {
                    item.thumbnail = Some(thumbnail);
                }
            }
            ViewUpdate::ShelfDragHover(value) => self.shelf_drag = value,
            ViewUpdate::Power(value) => {
                self.choreographer
                    .on_power_updated(value, self.mascot, now);
                self.power = value;
            }
            ViewUpdate::Headphones(value) => {
                self.choreographer.on_headphones_updated(value.is_some());
                self.headphones = value;
            }
            ViewUpdate::MediaConnection(value) => self.connection = value,
            ViewUpdate::Utility(value) => {
                self.choreographer
                    .on_utility_updated(value.focus_running, self.mascot, now);
                self.utility = Some(value);
            }
            ViewUpdate::Water(value) => {
                self.choreographer
                    .on_water_updated(value.consumed_ml, self.mascot, now);
                let old = self.water.as_ref().map_or(0, |w| w.consumed_ml);
                if old != value.consumed_ml {
                    self.previous_water = old;
                    self.previous_hours = self.water.as_ref().map_or([0; 24], |w| w.hourly_ml);
                    self.water_change = now;
                }
                self.water = Some(value);
            }
            ViewUpdate::Volume { level, muted } => {
                self.volume = level;
                self.muted = muted;
            }
            ViewUpdate::Mascot {
                is_dozing,
                is_enabled,
            } => {
                self.mascot = is_enabled;
                if !is_enabled {
                    let _ = self
                        .choreographer
                        .advance(now, false, false, false, &mut self.dozing);
                }
                if self.dozing && !is_dozing {
                    self.wake_mascot(now);
                }
                self.dozing = is_dozing;
            }
            ViewUpdate::FocusReminder { phase, seconds } => {
                self.choreographer
                    .on_focus_reminder(phase, self.mascot, now);
                self.focus_notice = Some((phase, seconds));
                self.hud_alert = true;
                self.alert_started = now;
            }
            ViewUpdate::Hud {
                title_key,
                detail,
                level,
                alert,
                reminder,
            } => {
                if title_key == "desktopUi.water-reminder" {
                    self.choreographer.on_water_reminder(self.mascot, now);
                }
                self.hud_title = crate::i18n::translate(title_key);
                self.hud_detail = detail;
                self.hud_level = level;
                self.hud_alert = alert;
                self.hud_reminder = reminder;
                self.focus_notice = None;
                self.alert_started = now;
            }
            ViewUpdate::TextInputFocus(value) => self.input_locked = value,
            ViewUpdate::CapsLock(enabled) => {
                self.hud_title = tr("caps-lock");
                self.hud_detail = tr(if enabled { "on" } else { "off" });
                self.hud_level = None;
                self.hud_alert = false;
                self.hud_reminder = false;
                self.focus_notice = None;
            }
            ViewUpdate::ReminderSound => {}
        }
    }
    pub fn set_hover_index(&mut self, index: Option<usize>, now: Instant) {
        if self.hover != index {
            self.previous_hover = self.hover;
            self.hover = index;
            self.hover_changed = now;
        }
    }
    pub fn set_nav(&mut self, value: bool, now: Instant) {
        if self.nav != value {
            self.nav_from = self.nav_amount(now);
            self.nav = value;
            self.nav_changed = now;
            self.motion.set(
                Shell::for_view(self.view, value),
                self.settings.desktop.fast_animations,
                now,
            );
        }
    }
    pub fn nav_amount(&self, now: Instant) -> f32 {
        tween(
            self.nav_from,
            f32::from(self.nav as u8),
            self.nav_changed,
            now,
            if self.settings.desktop.fast_animations {
                0.15
            } else {
                0.26
            },
            (0.42, 0., 0.58, 1.),
        )
    }
    pub fn volume_amount(&self, now: Instant) -> f32 {
        tween(
            self.volume_from,
            f32::from(self.volume_shown as u8),
            self.volume_changed,
            now,
            if self.settings.desktop.fast_animations {
                0.11
            } else {
                0.2
            },
            (0.42, 0., 0.58, 1.),
        )
    }
    pub fn set_volume_shown(&mut self, value: bool, now: Instant) {
        if self.volume_shown != value {
            self.volume_from = self.volume_amount(now);
            self.volume_shown = value;
            self.volume_changed = now;
        }
    }
    pub fn list_amount(&self, now: Instant) -> f32 {
        tween(
            self.list_from,
            f32::from(self.list_expanded as u8),
            self.list_changed,
            now,
            if self.settings.desktop.fast_animations {
                0.11
            } else {
                0.2
            },
            (0.25, 0.1, 0.25, 1.),
        )
    }
    pub fn set_list_expanded(&mut self, value: bool, now: Instant) {
        if self.list_expanded != value {
            self.list_from = self.list_amount(now);
            self.list_expanded = value;
            self.list_changed = now;
        }
    }
    pub fn lyric_position(&self, now: Instant) -> f32 {
        tween(
            self.lyric_from,
            self.active_lyric.unwrap_or(0) as f32,
            self.lyric_changed,
            now,
            if self.settings.desktop.fast_animations {
                0.352
            } else {
                0.64
            },
            (0.34, 1.32, 0.5, 1.),
        )
    }
    pub fn open_task(&mut self, id: Option<Uuid>) {
        self.draft = if let Some(id) = id {
            self.tasks.iter().find(|t| t.id == id).map(|t| TaskDraft {
                id: Some(id),
                title: t.title.clone(),
                date: t.schedule.date.to_string(),
                time: t
                    .schedule
                    .time
                    .map_or_else(String::new, |t| t.format("%H:%M").to_string()),
                daily: t.schedule.daily,
                color: t.color.index() as usize,
                saving: false,
            })
        } else {
            Some(TaskDraft {
                id: None,
                title: String::new(),
                date: if self.selected_date.is_empty() {
                    self.clock.date_naive().to_string()
                } else {
                    self.selected_date.clone()
                },
                time: String::new(),
                daily: false,
                color: 0,
                saving: false,
            })
        };
        self.calendar_open = false;
        self.palette = false;
        self.editor_error = None;
    }
    pub fn select_date(&mut self, date: String) {
        if let Ok(day) = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d") {
            if let Some(index) = self.tasks.iter().position(|t| t.schedule.occurs_on(day)) {
                self.task_scroll = index as f32 * 34.;
            }
            self.selected_date = date;
        }
    }
    pub fn focus_running(&self) -> bool {
        self.utility.as_ref().is_some_and(|u| u.focus_running)
    }
    pub fn remaining(&self) -> f32 {
        self.utility.as_ref().map_or(1., |u| {
            u.focus_seconds as f32 / u.focus_total_seconds.max(1) as f32
        })
    }
    pub fn expanded(&self) -> bool {
        is_expanded(self.view)
    }
    pub fn compact_mascot(&self, now: Instant) -> bool {
        self.mascot
            && !self.fullscreen
            && matches!(self.view, NotchView::Closed | NotchView::Resting)
            && !self.motion.running(now)
    }
    pub fn wake_mascot(&mut self, now: Instant) {
        self.choreographer.wake(now, self.mascot, &mut self.dozing);
    }
    #[cfg(test)]
    pub fn wake_progress(&self, now: Instant) -> Option<f32> {
        self.choreographer.clip_progress(
            MascotClip::WakeRubEye,
            now,
            self.compact_mascot(now),
            self.headphones.is_some(),
        )
    }
    #[cfg(test)]
    pub fn greeting_progress(&self, now: Instant) -> Option<f32> {
        self.choreographer.clip_progress(
            MascotClip::GreetingWave,
            now,
            self.compact_mascot(now),
            self.headphones.is_some(),
        )
    }
    #[cfg(test)]
    pub fn headphones_progress(&self, now: Instant) -> Option<f32> {
        self.choreographer.clip_progress(
            MascotClip::Headphones,
            now,
            self.compact_mascot(now),
            self.headphones.is_some(),
        )
    }
    pub fn gaze(&self, now: Instant) -> (f32, f32) {
        self.choreographer.gaze(now)
    }
    pub fn stalk_amount(&self, now: Instant) -> f32 {
        self.choreographer.stalk_amount(now)
    }
    pub fn observe_mascot_cursor(
        &mut self,
        cursor: Option<(f32, f32)>,
        radius: f32,
        now: Instant,
    ) -> bool {
        let capsule = Shell::for_view(NotchView::Closed, false).body();
        let is_compact = self.compact_mascot(now);
        let changed = self.choreographer.observe_cursor(
            CursorSample {
                point: cursor,
                capsule,
                radius,
                is_compact,
                is_enabled: self.mascot,
            },
            &mut self.dozing,
            now,
        );
        self.cursor_near = self.choreographer.is_cursor_near();
        changed
    }
    #[cfg(test)]
    pub fn bell_progress(&self, now: Instant) -> Option<f32> {
        self.choreographer.clip_progress(
            MascotClip::BellAlert,
            now,
            self.compact_mascot(now),
            self.headphones.is_some(),
        )
    }
    pub fn advance_mascot_reactions(&mut self, now: Instant) -> bool {
        let is_compact = self.compact_mascot(now);
        self.choreographer.advance(
            now,
            self.mascot,
            is_compact,
            self.headphones.is_some(),
            &mut self.dozing,
        )
    }
    pub fn animating(&self, now: Instant) -> bool {
        self.motion.running(now)
            || self.choreographer.is_animating(
                now,
                MascotLiveState {
                    start: self.start,
                    is_compact: self.compact_mascot(now),
                    is_resting_view: self.view == NotchView::Resting,
                    is_dozing: self.dozing,
                    is_playing: self.playing,
                    has_headphones: self.headphones.is_some(),
                    voice: self.voice,
                    shelf_drag: self.shelf_drag,
                },
            )
            || now.duration_since(self.view_changed) < Duration::from_millis(300)
            || now.duration_since(self.nav_changed) < Duration::from_millis(280)
            || now.duration_since(self.volume_changed) < Duration::from_millis(220)
            || now.duration_since(self.water_change) < Duration::from_millis(900)
            || now.duration_since(self.shelf_change) < Duration::from_millis(310)
            || self.playing
                && matches!(
                    self.view,
                    NotchView::Closed | NotchView::Music | NotchView::Lyrics
                )
            || now.duration_since(self.list_changed) < Duration::from_millis(220)
            || self.view == NotchView::Lyrics
                && now.duration_since(self.lyric_changed) < Duration::from_millis(650)
            || now.duration_since(self.hover_changed) < Duration::from_millis(155)
    }
}
fn tween(
    from: f32,
    to: f32,
    start: Instant,
    now: Instant,
    duration: f32,
    curve: (f32, f32, f32, f32),
) -> f32 {
    let t = (now.duration_since(start).as_secs_f32() / duration).clamp(0., 1.);
    from + (to - from) * cubic_bezier(t, curve.0, curve.1, curve.2, curve.3)
}
pub fn page_index(view: NotchView) -> Option<usize> {
    Some(match view {
        NotchView::Music | NotchView::Lyrics => 0,
        NotchView::Pomodoro => 1,
        NotchView::Water => 2,
        NotchView::Shelf => 3,
        NotchView::Todo => 4,
        NotchView::Headphones => 5,
        NotchView::Charging => 6,
        _ => return None,
    })
}
pub fn is_expanded(view: NotchView) -> bool {
    page_index(view).is_some()
}

pub fn cubic_bezier(x: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let x = x.clamp(0., 1.);
    let mut t = x;
    for _ in 0..8 {
        let a = 1. - t;
        let bx = 3. * a * a * t * x1 + 3. * a * t * t * x2 + t * t * t;
        let dx = 3. * a * a * x1 + 6. * a * t * (x2 - x1) + 3. * t * t * (1. - x2);
        if dx > 0.0001 {
            t = (t - (bx - x) / dx).clamp(0., 1.);
        }
    }
    let a = 1. - t;
    3. * a * a * t * y1 + 3. * a * t * t * y2 + t * t * t
}

pub fn format_duration(value: Duration) -> String {
    let s = value.as_secs();
    format!("{:02}:{:02}", s / 60, s % 60)
}
pub fn tr(key: &str) -> String {
    crate::i18n::translate(&format!("desktopUi.{key}"))
}

#[cfg(test)]
#[path = "../../../tests/fixtures/native_model.rs"]
mod tests;
