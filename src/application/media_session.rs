use std::sync::Arc;
use std::time::{Duration, Instant};

use super::events::{AppEvent, EventSender};
use super::presenter::{LyricsAvailability, NotchPresenter, ViewUpdate};
use crate::domain::lyrics::SyncedLyrics;
use crate::domain::media::{
    Artwork, MediaCommand, MediaSource, PlaybackSnapshot, Track, preferred_source,
};
use crate::domain::ports::{ArtworkLoader, LyricsProvider, MediaController, VolumeController};

/// Ports the media session depends on.
pub struct MediaPorts {
    pub controller: Arc<dyn MediaController>,
    pub windows: Arc<dyn MediaController>,
    pub volume: Arc<dyn VolumeController>,
    pub artwork: Arc<dyn ArtworkLoader>,
    pub lyrics: Arc<dyn LyricsProvider>,
}

/// Music use cases: follows playback, loads artwork and lyrics for each new
/// track and keeps progress / active lyric line in sync.
pub struct MediaSession {
    ports: MediaPorts,
    events: EventSender,
    presenter: Arc<dyn NotchPresenter>,
    playback: PlaybackSnapshot,
    assistant_playback: PlaybackSnapshot,
    windows_playback: PlaybackSnapshot,
    source: MediaSource,
    windows_artwork: Option<(String, Artwork)>,
    lyrics: Option<SyncedLyrics>,
    active_lyric: Option<usize>,
    last_progress_second: Option<u64>,
    is_lyrics_enabled: bool,
}

impl MediaSession {
    pub fn new(ports: MediaPorts, events: EventSender, presenter: Arc<dyn NotchPresenter>) -> Self {
        Self {
            ports,
            events,
            presenter,
            playback: PlaybackSnapshot::idle(),
            assistant_playback: PlaybackSnapshot::idle(),
            windows_playback: PlaybackSnapshot::idle(),
            source: MediaSource::MusicAssistant,
            windows_artwork: None,
            lyrics: None,
            active_lyric: None,
            last_progress_second: None,
            is_lyrics_enabled: true,
        }
    }

    pub fn set_lyrics_enabled(&mut self, is_enabled: bool) {
        let was_enabled = self.is_lyrics_enabled;
        self.is_lyrics_enabled = is_enabled;
        if !is_enabled {
            self.lyrics = None;
            self.active_lyric = None;
            self.presenter
                .apply(ViewUpdate::Lyrics(LyricsAvailability::Unavailable));
        } else if !was_enabled && let Some(track) = self.playback.track.clone() {
            self.request_lyrics(track);
        }
    }

    pub fn is_playing(&self) -> bool {
        self.playback.is_playing()
    }

    pub fn has_lyrics(&self) -> bool {
        self.lyrics
            .as_ref()
            .is_some_and(|lyrics| !lyrics.is_empty())
    }

    pub fn lyrics_enabled(&self) -> bool {
        self.is_lyrics_enabled
    }

    pub fn execute(&self, command: MediaCommand) {
        match self.source {
            MediaSource::MusicAssistant => self.ports.controller.execute(command),
            MediaSource::Windows => self.ports.windows.execute(command),
        }
    }

    pub fn set_volume(&self, level: f32) {
        if level.is_finite() {
            self.ports.volume.set_volume(level.clamp(0.0, 1.0));
        }
    }

    pub fn seek_to_fraction(&self, fraction: f32) {
        let Some(track) = &self.playback.track else {
            return;
        };
        let target = track.duration.mul_f32(fraction.clamp(0.0, 1.0));
        self.execute(MediaCommand::Seek(target));
    }

    pub fn seek_to_lyric(&self, index: usize) {
        let line = self
            .lyrics
            .as_ref()
            .and_then(|lyrics| lyrics.lines().get(index));
        if let Some(line) = line {
            self.execute(MediaCommand::Seek(line.starts_at));
        }
    }

    pub fn on_playback(&mut self, snapshot: PlaybackSnapshot) {
        self.assistant_playback = snapshot;
        self.select_playback();
    }

    pub fn on_windows_playback(&mut self, snapshot: PlaybackSnapshot) {
        self.windows_playback = snapshot;
        self.select_playback();
    }

    pub fn on_windows_artwork(&mut self, track_id: String, artwork: Option<Artwork>) {
        if self
            .windows_playback
            .track
            .as_ref()
            .is_none_or(|track| track.id != track_id)
        {
            return;
        }
        self.windows_artwork = artwork.clone().map(|image| (track_id.clone(), image));
        if self.source == MediaSource::Windows {
            self.on_artwork(&track_id, artwork);
        }
    }

    fn select_playback(&mut self) {
        self.source = preferred_source(&self.assistant_playback, &self.windows_playback);
        let snapshot = match self.source {
            MediaSource::MusicAssistant => self.assistant_playback.clone(),
            MediaSource::Windows => self.windows_playback.clone(),
        };
        let is_new_track = self.playback.track != snapshot.track;
        let is_new_status = self.playback.is_playing() != snapshot.is_playing();
        let is_new_track_id = self.playback.track.as_ref().map(|track| &track.id)
            != snapshot.track.as_ref().map(|track| &track.id);
        self.playback = snapshot;
        if is_new_track_id {
            self.on_track_changed();
        }
        if is_new_track || is_new_status {
            self.presenter.apply(ViewUpdate::Track {
                track: self.playback.track.clone(),
                is_playing: self.playback.is_playing(),
            });
        }
        self.last_progress_second = None;
        self.refresh_progress(Instant::now());
    }

    pub fn on_artwork(&self, track_id: &str, artwork: Option<Artwork>) {
        if self.is_current(track_id) {
            self.presenter.apply(ViewUpdate::Artwork(artwork));
        }
    }

    pub fn on_lyrics(&mut self, track_id: &str, lyrics: Option<SyncedLyrics>) {
        if !self.is_lyrics_enabled || !self.is_current(track_id) {
            return;
        }
        let availability = match &lyrics {
            Some(found) if !found.is_empty() => LyricsAvailability::Ready(
                found.lines().iter().map(|line| line.text.clone()).collect(),
            ),
            _ => LyricsAvailability::NotFound,
        };
        self.lyrics = lyrics;
        self.active_lyric = None;
        self.presenter.apply(ViewUpdate::Lyrics(availability));
        self.refresh_active_lyric(Instant::now());
    }

    /// Publishes the progress bar at most once per second of playback.
    pub fn refresh_progress(&mut self, now: Instant) {
        let position = self.playback.position_at(now);
        let second = position.as_secs();
        if self.last_progress_second == Some(second) {
            return;
        }
        self.last_progress_second = Some(second);
        let duration = self
            .playback
            .track
            .as_ref()
            .map_or(Duration::ZERO, |t| t.duration);
        self.presenter
            .apply(ViewUpdate::Progress { position, duration });
    }

    pub fn refresh_active_lyric(&mut self, now: Instant) {
        let Some(lyrics) = &self.lyrics else { return };
        let active = lyrics.active_index(self.playback.position_at(now));
        if active != self.active_lyric {
            self.active_lyric = active;
            self.presenter.apply(ViewUpdate::ActiveLyric(active));
        }
    }

    fn is_current(&self, track_id: &str) -> bool {
        self.playback
            .track
            .as_ref()
            .is_some_and(|track| track.id == track_id)
    }

    fn on_track_changed(&mut self) {
        self.lyrics = None;
        self.active_lyric = None;
        self.presenter.apply(ViewUpdate::Artwork(None));
        self.presenter.apply(ViewUpdate::ActiveLyric(None));
        let Some(track) = self.playback.track.clone() else {
            self.presenter
                .apply(ViewUpdate::Lyrics(LyricsAvailability::Unavailable));
            return;
        };
        self.request_artwork(&track);
        if self.source == MediaSource::Windows
            && let Some((id, artwork)) = &self.windows_artwork
            && *id == track.id
        {
            self.presenter
                .apply(ViewUpdate::Artwork(Some(artwork.clone())));
        }
        self.request_lyrics(track);
    }

    fn request_artwork(&self, track: &Track) {
        let Some(url) = track.artwork_url.clone() else {
            return;
        };
        let load = self.ports.artwork.load(url);
        let (events, track_id) = (self.events.clone(), track.id.clone());
        tokio::spawn(async move {
            let artwork = load.await.inspect_err(|error| {
                tracing::warn!(event = "log.media.artworkLoadFailed", error = %error);
            });
            send(
                &events,
                AppEvent::ArtworkLoaded {
                    track_id,
                    artwork: artwork.ok(),
                },
            );
        });
    }

    fn request_lyrics(&self, track: Track) {
        if !self.is_lyrics_enabled {
            self.presenter
                .apply(ViewUpdate::Lyrics(LyricsAvailability::Unavailable));
            return;
        }
        self.presenter
            .apply(ViewUpdate::Lyrics(LyricsAvailability::Loading));
        let (events, track_id) = (self.events.clone(), track.id.clone());
        let find = self.ports.lyrics.find(track);
        tokio::spawn(async move {
            let lyrics = find.await.unwrap_or_else(|error| {
                tracing::warn!(event = "log.media.lyricsLoadFailed", error = %error);
                None
            });
            send(&events, AppEvent::LyricsLoaded { track_id, lyrics });
        });
    }
}

fn send(events: &EventSender, event: AppEvent) {
    if events.send(event).is_err() {
        tracing::warn!(event = "log.events.receiverClosed");
    }
}

#[cfg(test)]
#[path = "../../tests/fixtures/media_session.rs"]
mod tests;
