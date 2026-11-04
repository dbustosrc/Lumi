use super::*;

use std::sync::Mutex;

use crate::application::events::create_event_channel;
use crate::application::presenter::{LyricsAvailability, NotchPresenter, ViewUpdate};
use crate::domain::lyrics::SyncedLyrics;
use crate::domain::media::{Artwork, MediaCommand, PlaybackSnapshot, PlaybackStatus, Track};
use crate::domain::ports::{
    ArtworkLoader, LyricsProvider, MediaController, PortError, PortFuture, VolumeController,
};

#[derive(Default)]
struct RecordingController(Mutex<Vec<MediaCommand>>);

impl MediaController for RecordingController {
    fn execute(&self, command: MediaCommand) {
        self.0.lock().unwrap().push(command);
    }
}

#[derive(Default)]
struct RecordingVolume(Mutex<Vec<f32>>);

impl VolumeController for RecordingVolume {
    fn set_volume(&self, level: f32) {
        self.0.lock().unwrap().push(level);
    }
}

#[derive(Default)]
struct EmptyArtworkLoader;

impl ArtworkLoader for EmptyArtworkLoader {
    fn load(&self, _url: String) -> PortFuture<Result<Artwork, PortError>> {
        Box::pin(async { Err(PortError::new("test.artwork", "not used")) })
    }
}

#[derive(Default)]
struct EmptyLyricsProvider;

impl LyricsProvider for EmptyLyricsProvider {
    fn find(&self, _track: Track) -> PortFuture<Result<Option<SyncedLyrics>, PortError>> {
        Box::pin(async { Ok(None) })
    }
}

#[derive(Default)]
struct RecordingPresenter(Mutex<Vec<ViewUpdate>>);

impl NotchPresenter for RecordingPresenter {
    fn apply(&self, update: ViewUpdate) {
        self.0.lock().unwrap().push(update);
    }
}

struct Fixture {
    session: MediaSession,
    assistant: Arc<RecordingController>,
    windows: Arc<RecordingController>,
    volume: Arc<RecordingVolume>,
    presenter: Arc<RecordingPresenter>,
    _events: crate::application::events::EventReceiver,
}

fn fixture() -> Fixture {
    let assistant = Arc::new(RecordingController::default());
    let windows = Arc::new(RecordingController::default());
    let volume = Arc::new(RecordingVolume::default());
    let presenter = Arc::new(RecordingPresenter::default());
    let (events, receiver) = create_event_channel();
    let session = MediaSession::new(
        MediaPorts {
            controller: assistant.clone(),
            windows: windows.clone(),
            volume: volume.clone(),
            artwork: Arc::new(EmptyArtworkLoader),
            lyrics: Arc::new(EmptyLyricsProvider),
        },
        events,
        presenter.clone(),
    );
    Fixture {
        session,
        assistant,
        windows,
        volume,
        presenter,
        _events: receiver,
    }
}

fn snapshot(id: &str, status: PlaybackStatus) -> PlaybackSnapshot {
    PlaybackSnapshot {
        track: Some(Track {
            id: id.into(),
            title: format!("Track {id}"),
            artist: "Test artist".into(),
            album: "Test album".into(),
            duration: Duration::from_secs(180),
            artwork_url: None,
        }),
        status,
        position: Duration::ZERO,
        sampled_at: Instant::now(),
    }
}

#[tokio::test]
async fn transport_switches_to_windows_when_assistant_pauses() {
    let mut fixture = fixture();
    fixture
        .session
        .on_playback(snapshot("assistant", PlaybackStatus::Playing));
    fixture
        .session
        .on_windows_playback(snapshot("windows", PlaybackStatus::Playing));

    fixture.session.execute(MediaCommand::Next);
    fixture
        .session
        .on_playback(snapshot("assistant", PlaybackStatus::Paused));
    fixture.session.execute(MediaCommand::Previous);

    assert_eq!(*fixture.assistant.0.lock().unwrap(), [MediaCommand::Next]);
    assert_eq!(*fixture.windows.0.lock().unwrap(), [MediaCommand::Previous]);
}

#[tokio::test]
async fn windows_artwork_is_cached_while_assistant_is_active_and_applied_on_handoff() {
    let mut fixture = fixture();
    fixture
        .session
        .on_playback(snapshot("assistant", PlaybackStatus::Playing));
    fixture
        .session
        .on_windows_playback(snapshot("windows", PlaybackStatus::Playing));
    fixture.session.on_windows_artwork(
        "windows".into(),
        Some(Artwork::from_rgba(vec![1, 2, 3, 4], 1, 1)),
    );

    assert!(
        !fixture
            .presenter
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|update| matches!(update, ViewUpdate::Artwork(Some(_))))
    );

    fixture
        .session
        .on_playback(snapshot("assistant", PlaybackStatus::Paused));

    let updates = fixture.presenter.0.lock().unwrap();
    let cached = updates
        .iter()
        .find_map(|update| match update {
            ViewUpdate::Artwork(Some(artwork)) => Some(artwork),
            _ => None,
        })
        .expect("cached Windows artwork should be applied after source handoff");
    assert_eq!(*cached.rgba, [1, 2, 3, 4]);
}

#[tokio::test]
async fn stale_artwork_and_disabled_or_stale_lyrics_are_ignored() {
    let mut fixture = fixture();
    fixture
        .session
        .on_windows_playback(snapshot("current", PlaybackStatus::Playing));
    fixture
        .session
        .on_windows_artwork("stale".into(), Some(Artwork::from_rgba(vec![9], 1, 1)));
    assert!(
        !fixture
            .presenter
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|update| matches!(update, ViewUpdate::Artwork(Some(_))))
    );

    let stale_lyrics = SyncedLyrics::parse_lrc("[00:00.00]old text");
    fixture
        .session
        .on_lyrics("stale", Some(stale_lyrics.clone()));
    assert!(!fixture.session.has_lyrics());

    fixture.session.on_lyrics("current", Some(stale_lyrics));
    assert!(fixture.session.has_lyrics());
    fixture.session.set_lyrics_enabled(false);
    assert!(!fixture.session.has_lyrics());
    fixture.session.on_lyrics(
        "current",
        Some(SyncedLyrics::parse_lrc("[00:00.00]late result")),
    );
    assert!(!fixture.session.has_lyrics());
    assert!(
        fixture
            .presenter
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|update| matches!(update, ViewUpdate::Lyrics(LyricsAvailability::Unavailable)))
    );
}

#[test]
fn volume_clamps_finite_values_and_ignores_non_finite_values() {
    let fixture = fixture();

    fixture.session.set_volume(-0.25);
    fixture.session.set_volume(0.4);
    fixture.session.set_volume(1.5);
    fixture.session.set_volume(f32::NAN);
    fixture.session.set_volume(f32::INFINITY);
    fixture.session.set_volume(f32::NEG_INFINITY);

    assert_eq!(*fixture.volume.0.lock().unwrap(), [0.0, 0.4, 1.0]);
}
