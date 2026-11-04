use std::time::Duration;

use chrono::{DateTime, Local};

use crate::domain::device::{HeadphoneStatus, PowerStatus};
use crate::domain::media::{Artwork, MediaConnection, Track};
use crate::domain::notch::NotchView;
use crate::domain::shelf::{ShelfItem, ShelfThumbnail};
use crate::domain::todo::{TodoProgress, TodoTask};

/// Enriched shelf item ready for UI presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShelfEntry {
    pub item: ShelfItem,
    pub badge: String,
    pub size_label: String,
    pub thumbnail: Option<ShelfThumbnail>,
}

/// Lyrics availability for the current track.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LyricsAvailability {
    #[default]
    Unavailable,
    Loading,
    NotFound,
    Ready(Vec<String>),
}

/// Voice capture lifecycle shown in the to-do page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VoiceState {
    #[default]
    Idle,
    Recording,
    Transcribing,
    /// Carries the i18n key describing the failure.
    Failed(&'static str),
}

/// Granular updates so the UI only re-renders what actually changed.
#[derive(Debug, Clone)]
pub enum ViewUpdate {
    MouseMoveStatus(Option<&'static str>),
    MascotHeadphones,
    MascotAlert,
    CapsLock(bool),
    Calendar {
        grid: crate::domain::calendar::CalendarGrid,
        tasks: Vec<TodoTask>,
    },
    Utility(super::reminders::UtilityState),
    Water(crate::domain::water::WaterState),
    FocusReminder {
        phase: super::reminders::FocusPhase,
        seconds: u64,
    },
    Hud {
        title_key: &'static str,
        detail: String,
        level: Option<f32>,
        reminder: bool,
        alert: bool,
    },
    ReminderSound,
    Volume {
        level: f32,
        muted: bool,
    },
    View(NotchView),
    TextInputFocus(bool),
    Clock(DateTime<Local>),
    Track {
        track: Option<Track>,
        is_playing: bool,
    },
    Progress {
        position: Duration,
        duration: Duration,
    },
    Artwork(Option<Artwork>),
    Lyrics(LyricsAvailability),
    ActiveLyric(Option<usize>),
    LyricsMode(bool),
    ShelfItems(Vec<ShelfEntry>),
    ShelfThumbnail {
        id: uuid::Uuid,
        thumbnail: ShelfThumbnail,
    },
    ShelfDragHover(bool),
    Tasks {
        tasks: Vec<TodoTask>,
        progress: TodoProgress,
    },
    TodoEditorResult {
        error_key: Option<&'static str>,
    },
    RecoverTodoDraft {
        text: String,
        error_key: &'static str,
    },
    Power(PowerStatus),
    Headphones(Option<HeadphoneStatus>),
    Voice(VoiceState),
    MediaConnection(MediaConnection),
    Mascot {
        is_dozing: bool,
        is_enabled: bool,
    },
}

/// Output port implemented by the presentation layer.
pub trait NotchPresenter: Send + Sync {
    fn apply(&self, update: ViewUpdate);
}
