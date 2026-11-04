use std::path::PathBuf;

use tokio::sync::mpsc;
use uuid::Uuid;

use super::settings::AppSettings;
use crate::domain::device::{HeadphoneStatus, PowerStatus};
use crate::domain::lyrics::SyncedLyrics;
use crate::domain::media::{Artwork, MediaCommand, MediaConnection, PlaybackSnapshot};
use crate::domain::notch::NotchPage;
use crate::domain::ports::PortError;
use crate::domain::shelf::ShelfThumbnail;

/// Actions requested by the user through the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum UserIntent {
    CalendarMove(i32),
    ToggleFocus,
    SelectFocusPhase(super::reminders::FocusPhase),
    ResetFocus,
    StartFocusFromReminder,
    DismissAlert,
    DrinkWater,
    ResetWater,
    StartTask(Uuid),
    SnoozeReminder,
    Activate,
    AddShelfText(String),
    SetVolume(f32),
    Hover(bool),
    /// Keeps the notch open and keyboard-focusable while a text field is edited.
    LockInteraction(bool),
    ShowPage(NotchPage),
    NextPage,
    PreviousPage,
    ToggleLyrics,
    Media(MediaCommand),
    SeekToFraction(f32),
    SeekToLyric(usize),
    AddTask {
        title: String,
        date: String,
        time: String,
        daily: bool,
        color_index: i32,
    },
    RenameTask {
        id: Uuid,
        title: String,
        color_index: i32,
    },
    ToggleTask(Uuid),
    DeleteTask(Uuid),
    ToggleVoiceCapture,
    BrowseShelfFiles,
    OpenShelfItem(Uuid),
    RemoveShelfItem(Uuid),
    ClearShelf,
}

/// Everything the controller reacts to: infrastructure notifications,
/// asynchronous results and user intents.
#[derive(Debug, Clone)]
pub enum AppEvent {
    MouseMoveStatus(Option<&'static str>),
    MascotProximity(bool),
    MascotActivity,
    NotificationArrived,
    Brightness(u8),
    CapsLock(bool),
    InputLanguage(String),
    Wifi(crate::domain::device::WifiState),
    ShelfTextDropped(String),
    Fullscreen(bool),
    WindowsPlayback(PlaybackSnapshot),
    WindowsArtwork {
        track_id: String,
        artwork: Option<Artwork>,
    },
    Volume {
        level: f32,
        muted: bool,
    },
    BackgroundContent(bool),
    Playback(PlaybackSnapshot),
    MediaConnection(MediaConnection),
    ArtworkLoaded {
        track_id: String,
        artwork: Option<Artwork>,
    },
    LyricsLoaded {
        track_id: String,
        lyrics: Option<SyncedLyrics>,
    },
    Power(PowerStatus),
    HeadphonesState {
        status: Option<HeadphoneStatus>,
        is_newly_connected: bool,
    },
    VoiceTranscribed(Result<String, PortError>),
    FileDragHover(bool),
    FilesDropped(Vec<PathBuf>),
    FilesPicked(Vec<PathBuf>),
    ShelfThumbnailLoaded {
        id: Uuid,
        thumbnail: Option<ShelfThumbnail>,
    },
    OutboundFileDragState {
        is_active: bool,
        is_cursor_inside: bool,
    },
    SettingsChanged(AppSettings),
    Intent(UserIntent),
}

pub type EventSender = mpsc::UnboundedSender<AppEvent>;
pub type EventReceiver = mpsc::UnboundedReceiver<AppEvent>;

pub fn create_event_channel() -> (EventSender, EventReceiver) {
    mpsc::unbounded_channel()
}
