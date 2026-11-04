use crate::application::events::{AppEvent, EventSender};
use crate::domain::media::{Artwork, MediaCommand, PlaybackSnapshot, PlaybackStatus, Track};
use crate::domain::ports::{MediaController, PortError, VolumeController};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eMultimedia, eRender};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};

enum Command {
    Transport(MediaCommand),
    Volume(f32),
}
pub struct WindowsMediaController {
    commands: mpsc::UnboundedSender<Command>,
}
impl MediaController for WindowsMediaController {
    fn execute(&self, command: MediaCommand) {
        if self.commands.send(Command::Transport(command)).is_err() {
            tracing::error!(event = "log.windowsMedia.stopped");
        }
    }
}
impl VolumeController for WindowsMediaController {
    fn set_volume(&self, level: f32) {
        if self.commands.send(Command::Volume(level)).is_err() {
            tracing::error!(event = "log.windowsMedia.stopped");
        }
    }
}
pub fn spawn(events: EventSender) -> Result<Arc<WindowsMediaController>, PortError> {
    let (commands, receiver) = mpsc::unbounded_channel();
    std::thread::Builder::new()
        .name("windows-media".into())
        .spawn(move || {
            if let Err(error) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok() {
                tracing::error!(event = "log.windowsMedia.initializeFailed", error = %error);
                return;
            }
            match tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
            {
                Ok(runtime) => {
                    tokio::task::LocalSet::new().block_on(&runtime, run(events, receiver))
                }
                Err(error) => {
                    tracing::error!(event = "log.windowsMedia.initializeFailed", error = %error)
                }
            }
            unsafe { CoUninitialize() };
        })
        .map_err(|error| PortError::new("music.error.windowsMedia", error))?;
    Ok(Arc::new(WindowsMediaController { commands }))
}
async fn run(events: EventSender, mut commands: mpsc::UnboundedReceiver<Command>) {
    let mut manager = None;
    let mut manager_task = Some(tokio::spawn(async {
        tokio::time::timeout(Duration::from_secs(2), request_manager())
            .await
            .ok()
            .and_then(Result::ok)
    }));
    let mut retry_at = Instant::now();
    let mut session = None;
    let mut last_track = None;
    let mut last_volume = None;
    type MediaRead = (
        Session,
        Result<
            windows::core::Result<(
                PlaybackSnapshot,
                windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
            )>,
            tokio::time::error::Elapsed,
        >,
    );
    let mut reads: tokio::task::JoinSet<MediaRead> = tokio::task::JoinSet::new();
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    loop {
        tokio::select! {
            command = commands.recv() => {
                let Some(command) = command else { break };
                match command {
                    Command::Volume(level) => if let Err(error) = set_volume(level) { tracing::error!(event = "log.volume.failed", error = %error); },
                    Command::Transport(command) => if let Some(session) = &session && let Err(error) = transport(session, command).await { tracing::warn!(event = "log.windowsMedia.commandFailed", error = %error); },
                }
            }
            result = async { manager_task.as_mut().expect("guarded manager task").await }, if manager_task.is_some() => {
                manager_task = None;
                manager = result.ok().flatten();
                retry_at = Instant::now() + Duration::from_secs(5);
                if manager.is_none() { tracing::warn!(event = "log.windowsMedia.initializeFailed"); }
            }
            result = reads.join_next(), if !reads.is_empty() => {
                if let Some(Ok((target, Ok(Ok((playback, properties)))))) = result {
                    let id = playback.track.as_ref().map(|track| track.id.clone());
                    let changed = id != last_track;
                    last_track = id.clone();
                    session = Some(target);
                    let _ = events.send(AppEvent::WindowsPlayback(playback));
                    if changed && let Some(track_id) = id {
                        let events = events.clone();
                        tokio::task::spawn_local(async move {
                            let artwork = tokio::time::timeout(Duration::from_secs(2), read_artwork(properties)).await.ok().flatten();
                            let _ = events.send(AppEvent::WindowsArtwork { track_id, artwork });
                        });
                    }
                } else {
                    session = None; last_track = None;
                    let _ = events.send(AppEvent::WindowsPlayback(PlaybackSnapshot::idle()));
                }
            }
            _ = interval.tick() => {
                if let Ok(volume) = read_volume() && last_volume != Some(volume) {
                    last_volume = Some(volume);
                    let _ = events.send(AppEvent::Volume { level: volume.0, muted: volume.1 });
                }
                if manager.is_none() && manager_task.is_none() && Instant::now() >= retry_at {
                    manager_task = Some(tokio::spawn(async { tokio::time::timeout(Duration::from_secs(2), request_manager()).await.ok().and_then(Result::ok) }));
                }
                let Some(manager) = &manager else { continue };
                if !reads.is_empty() { continue; }
                if let Some(active) = select_session(manager) {
                    reads.spawn(async move {
                        let result = tokio::time::timeout(Duration::from_secs(2), snapshot(&active)).await;
                        (active, result)
                    });
                } else {
                    session = None;
                    if last_track.take().is_some() { let _ = events.send(AppEvent::WindowsPlayback(PlaybackSnapshot::idle())); }
                }
            }
        }
    }
    if let Some(task) = manager_task {
        task.abort();
    }
}

async fn read_artwork(
    properties: windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
) -> Option<Artwork> {
    let stream = properties
        .Thumbnail()
        .ok()?
        .OpenReadAsync()
        .ok()?
        .await
        .ok()?;
    let size = u32::try_from(stream.Size().ok()?).ok()?;
    // Media sessions are external providers; avoid an unbounded thumbnail allocation.
    if size > 32 * 1024 * 1024 {
        tracing::warn!(event = "log.windowsMedia.artworkTooLarge");
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    if reader.LoadAsync(size).ok()?.await.ok()? != size {
        return None;
    }
    let mut bytes = vec![0; size as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    tokio::task::spawn_blocking(move || {
        let image = image::load_from_memory(&bytes)
            .ok()?
            .thumbnail(256, 256)
            .to_rgba8();
        Some(Artwork::from_rgba(
            image.as_raw().clone(),
            image.width(),
            image.height(),
        ))
    })
    .await
    .ok()
    .flatten()
}
async fn request_manager() -> windows::core::Result<Manager> {
    Manager::RequestAsync()?.await
}
fn select_session(manager: &Manager) -> Option<Session> {
    let sessions = manager.GetSessions().ok()?;
    let current = manager.GetCurrentSession().ok();
    if let Some(session) = &current
        && is_playing(session)
    {
        return current;
    }
    for session in &sessions {
        if is_playing(&session) {
            return Some(session);
        }
    }
    current
}
fn is_playing(session: &Session) -> bool {
    session
        .GetPlaybackInfo()
        .and_then(|info| info.PlaybackStatus())
        .is_ok_and(|state| state == Status::Playing)
}
async fn snapshot(
    session: &Session,
) -> windows::core::Result<(
    PlaybackSnapshot,
    windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
)> {
    let properties = session.TryGetMediaPropertiesAsync()?.await?;
    let timeline = session.GetTimelineProperties()?;
    let start = timeline.StartTime()?.Duration;
    let duration = ticks(timeline.EndTime()?.Duration.saturating_sub(start));
    let position = ticks(timeline.Position()?.Duration.saturating_sub(start));
    let title = properties.Title()?.to_string();
    let artist = properties.Artist()?.to_string();
    let album = properties.AlbumTitle()?.to_string();
    let source = session.SourceAppUserModelId()?.to_string();
    let track = (!title.is_empty()).then(|| Track {
        id: format!("windows:{source}:{title}:{artist}:{album}"),
        title,
        artist,
        album,
        duration,
        artwork_url: None,
    });
    let status = match session.GetPlaybackInfo()?.PlaybackStatus()? {
        Status::Playing => PlaybackStatus::Playing,
        Status::Paused => PlaybackStatus::Paused,
        _ => PlaybackStatus::Idle,
    };
    Ok((
        PlaybackSnapshot {
            track,
            status,
            position,
            sampled_at: Instant::now(),
        },
        properties,
    ))
}
fn ticks(value: i64) -> Duration {
    Duration::from_nanos((value.max(0) as u64).saturating_mul(100))
}
async fn transport(session: &Session, command: MediaCommand) -> windows::core::Result<()> {
    let accepted = match command {
        MediaCommand::PlayPause => session.TryTogglePlayPauseAsync()?.await?,
        MediaCommand::Next => session.TrySkipNextAsync()?.await?,
        MediaCommand::Previous => session.TrySkipPreviousAsync()?.await?,
        MediaCommand::Seek(position) => {
            let start = session.GetTimelineProperties()?.StartTime()?.Duration;
            let ticks = i64::try_from(position.as_nanos() / 100).unwrap_or(i64::MAX);
            session
                .TryChangePlaybackPositionAsync(start.saturating_add(ticks))?
                .await?
        }
    };
    if !accepted {
        return Err(windows::core::Error::from_hresult(
            windows::Win32::Foundation::E_FAIL,
        ));
    }
    Ok(())
}
fn endpoint() -> windows::core::Result<IAudioEndpointVolume> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        enumerator
            .GetDefaultAudioEndpoint(eRender, eMultimedia)?
            .Activate(CLSCTX_ALL, None)
    }
}
fn read_volume() -> windows::core::Result<(f32, bool)> {
    let endpoint = endpoint()?;
    unsafe {
        Ok((
            endpoint.GetMasterVolumeLevelScalar()?,
            endpoint.GetMute()?.as_bool(),
        ))
    }
}
fn set_volume(level: f32) -> windows::core::Result<()> {
    let endpoint = endpoint()?;
    unsafe {
        endpoint
            .SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), &windows::core::GUID::zeroed())?;
        endpoint.SetMute(level <= 0.0, &windows::core::GUID::zeroed())
    }
}
