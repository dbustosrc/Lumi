use std::sync::Arc;
use std::time::{Duration, Instant};

/// Playback state reported by the media source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    #[default]
    Idle,
}

/// Metadata of the track currently loaded in the followed queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Duration,
    pub artwork_url: Option<String>,
}

/// Snapshot of the playback state at a given instant. The position is
/// interpolated locally so the UI does not depend on server push frequency.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackSnapshot {
    pub track: Option<Track>,
    pub status: PlaybackStatus,
    pub position: Duration,
    pub sampled_at: Instant,
}

impl PlaybackSnapshot {
    pub fn idle() -> Self {
        Self {
            track: None,
            status: PlaybackStatus::Idle,
            position: Duration::ZERO,
            sampled_at: Instant::now(),
        }
    }

    pub fn is_playing(&self) -> bool {
        self.status == PlaybackStatus::Playing && self.track.is_some()
    }

    /// Returns the estimated playback position at `now`, clamped to the
    /// track duration when it is known.
    pub fn position_at(&self, now: Instant) -> Duration {
        let elapsed = if self.is_playing() {
            now.saturating_duration_since(self.sampled_at)
        } else {
            Duration::ZERO
        };
        let position = self.position + elapsed;
        match &self.track {
            Some(track) if !track.duration.is_zero() => position.min(track.duration),
            _ => position,
        }
    }
}

/// Decoded album artwork in RGBA8 format.
#[derive(Debug, Clone)]
pub struct Artwork {
    pub rgba: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}

impl Artwork {
    pub fn from_rgba(rgba: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            rgba: Arc::new(rgba),
            width,
            height,
        }
    }
}

/// Transport commands the user can issue to the media source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MediaCommand {
    PlayPause,
    Next,
    Previous,
    Seek(Duration),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MediaSource {
    #[default]
    MusicAssistant,
    Windows,
}

pub fn preferred_source(assistant: &PlaybackSnapshot, windows: &PlaybackSnapshot) -> MediaSource {
    if assistant.is_playing() {
        MediaSource::MusicAssistant
    } else if windows.is_playing() {
        MediaSource::Windows
    } else if assistant.track.is_some() {
        MediaSource::MusicAssistant
    } else {
        MediaSource::Windows
    }
}

/// Connection state of the media source, surfaced to the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MediaConnection {
    #[default]
    NotConfigured,
    Connecting,
    Connected,
    AuthenticationFailed,
    Unavailable,
}
