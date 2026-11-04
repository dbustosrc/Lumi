//! Contracts the domain/application need from the outside world. The
//! infrastructure layer provides the implementations, injected at startup.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use super::lyrics::SyncedLyrics;
use super::media::{Artwork, MediaCommand, Track};
use super::shelf::{ShelfItem, ShelfPathMetadata, ShelfThumbnail};
use super::todo::TodoTask;

/// Boxed future so ports stay object-safe and can be injected as `Arc<dyn _>`.
pub type PortFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// A single discrete mouse activity pulse without cursor displacement.
pub trait MouseActivity: Send + Sync {
    fn pulse(&self) -> Result<(), PortError>;
}

/// Failure reported by an external dependency. The message is an i18n key.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[error("{key}: {detail}")]
pub struct PortError {
    pub key: &'static str,
    pub detail: String,
}

impl PortError {
    pub fn new(key: &'static str, detail: impl ToString) -> Self {
        Self {
            key,
            detail: detail.to_string(),
        }
    }
}

/// Sends transport commands to the media source (Music Assistant).
pub trait MediaController: Send + Sync {
    fn execute(&self, command: MediaCommand);
}

pub trait VolumeController: Send + Sync {
    fn set_volume(&self, level: f32);
}

/// Downloads and decodes album artwork.
pub trait ArtworkLoader: Send + Sync {
    fn load(&self, url: String) -> PortFuture<Result<Artwork, PortError>>;
}

/// Finds time-synced lyrics for a track. `Ok(None)` means "no lyrics".
pub trait LyricsProvider: Send + Sync {
    fn find(&self, track: Track) -> PortFuture<Result<Option<SyncedLyrics>, PortError>>;
}

/// Persists the to-do list.
pub trait TaskRepository: Send + Sync {
    fn load(&self) -> Result<Vec<TodoTask>, PortError>;
    fn save(&self, tasks: &[TodoTask]) -> Result<(), PortError>;
}

/// Persists file shelf references across restarts without copying files.
pub trait ShelfRepository: Send + Sync {
    fn load(&self) -> Result<Vec<ShelfItem>, PortError>;
    fn save(&self, items: &[ShelfItem]) -> Result<(), PortError>;
}

pub trait WaterRepository: Send + Sync {
    fn load(&self) -> Result<Vec<super::water::WaterDrink>, PortError>;
    fn save(&self, drinks: &[super::water::WaterDrink]) -> Result<(), PortError>;
}

/// OS filesystem and Shell operations required by the file shelf use case.
pub trait ShelfPlatform: Send + Sync {
    fn copy_text(&self, text: &str) -> Result<(), PortError>;
    /// `None` means confirmed absence; other inspection failures preserve the reference.
    fn inspect_path(&self, path: &Path) -> Result<Option<ShelfPathMetadata>, PortError>;
    fn open_path(&self, path: &Path) -> Result<(), PortError>;
    fn pick_files(&self) -> PortFuture<Result<Vec<PathBuf>, PortError>>;
    fn load_thumbnail(&self, path: PathBuf) -> PortFuture<Option<ShelfThumbnail>>;
}

/// Mono 16-bit PCM audio captured from the microphone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcmAudio {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
}

/// Captures audio from the default microphone between `start` and `stop`.
pub trait AudioRecorder: Send + Sync {
    fn start(&self) -> Result<(), PortError>;
    fn stop(&self) -> Result<PcmAudio, PortError>;
}

/// Converts speech into text (Wyoming Whisper server).
pub trait SpeechToText: Send + Sync {
    fn transcribe(&self, audio: PcmAudio) -> PortFuture<Result<String, PortError>>;
}
