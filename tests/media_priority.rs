#[path = "../src/domain/media.rs"]
#[allow(dead_code)]
mod media;

use std::time::{Duration, Instant};

use media::{MediaSource, PlaybackSnapshot, PlaybackStatus, Track, preferred_source};

fn snapshot(status: PlaybackStatus, has_track: bool) -> PlaybackSnapshot {
    PlaybackSnapshot {
        track: has_track.then(|| Track {
            id: "track-1".into(),
            title: "Example".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            duration: Duration::from_secs(180),
            artwork_url: None,
        }),
        status,
        position: Duration::from_secs(30),
        sampled_at: Instant::now(),
    }
}

#[test]
fn music_assistant_playing_has_priority_over_windows_playing() {
    assert_eq!(
        preferred_source(
            &snapshot(PlaybackStatus::Playing, true),
            &snapshot(PlaybackStatus::Playing, true),
        ),
        MediaSource::MusicAssistant,
    );
}

#[test]
fn windows_playing_is_selected_when_music_assistant_is_not_playing() {
    for assistant_status in [PlaybackStatus::Paused, PlaybackStatus::Idle] {
        assert_eq!(
            preferred_source(
                &snapshot(assistant_status, true),
                &snapshot(PlaybackStatus::Playing, true),
            ),
            MediaSource::Windows,
        );
    }
}

#[test]
fn loaded_music_assistant_track_is_selected_when_neither_source_is_playing() {
    assert_eq!(
        preferred_source(
            &snapshot(PlaybackStatus::Paused, true),
            &snapshot(PlaybackStatus::Paused, true),
        ),
        MediaSource::MusicAssistant,
    );
    assert_eq!(
        preferred_source(
            &snapshot(PlaybackStatus::Idle, true),
            &snapshot(PlaybackStatus::Idle, false),
        ),
        MediaSource::MusicAssistant,
    );
}

#[test]
fn windows_is_the_fallback_when_music_assistant_has_no_track() {
    assert_eq!(
        preferred_source(
            &snapshot(PlaybackStatus::Idle, false),
            &snapshot(PlaybackStatus::Paused, true),
        ),
        MediaSource::Windows,
    );
    assert_eq!(
        preferred_source(&PlaybackSnapshot::idle(), &PlaybackSnapshot::idle()),
        MediaSource::Windows,
    );
}

#[test]
fn playback_hands_off_between_sources_as_playing_state_changes() {
    let assistant_playing = snapshot(PlaybackStatus::Playing, true);
    let windows_playing = snapshot(PlaybackStatus::Playing, true);
    let assistant_paused = snapshot(PlaybackStatus::Paused, true);

    assert_eq!(
        preferred_source(&assistant_playing, &windows_playing),
        MediaSource::MusicAssistant,
    );
    assert_eq!(
        preferred_source(&assistant_paused, &windows_playing),
        MediaSource::Windows,
    );
    assert_eq!(
        preferred_source(&assistant_playing, &windows_playing),
        MediaSource::MusicAssistant,
    );
}

#[test]
fn playing_status_without_a_track_does_not_count_as_playing() {
    assert_eq!(
        preferred_source(
            &snapshot(PlaybackStatus::Playing, false),
            &snapshot(PlaybackStatus::Paused, false),
        ),
        MediaSource::Windows,
    );
}
