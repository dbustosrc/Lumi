use crate::application::events::{AppEvent, EventSender};
use crate::application::settings::AppSettings;
use crate::domain::media::{Artwork, MediaCommand, PlaybackSnapshot, PlaybackStatus, Track};
use crate::domain::notch::NotchPage;
use crate::domain::ports::{MediaController, PortError, VolumeController};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Notify, mpsc, watch};
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

use super::volume::VolumeMonitor;

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
pub fn spawn(
    events: EventSender,
    settings: watch::Receiver<AppSettings>,
) -> Result<Arc<WindowsMediaController>, PortError> {
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
                    tokio::task::LocalSet::new().block_on(&runtime, run(events, receiver, settings))
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
async fn run(
    events: EventSender,
    mut commands: mpsc::UnboundedReceiver<Command>,
    mut settings: watch::Receiver<AppSettings>,
) {
    let device_changed = Arc::new(Notify::new());
    let volume_changed = Arc::new(Notify::new());
    let (mut music_enabled, mut volume_enabled) = media_policy(&settings.borrow_and_update());
    let mut volume = volume_enabled
        .then(|| monitor_volume(&device_changed))
        .flatten();
    device_changed.notify_one();
    let media_changed = Arc::new(Notify::new());
    let mut manager = None;
    let mut manager_task: Option<tokio::task::JoinHandle<Option<Manager>>> = None;
    let mut retry_at = Instant::now();
    let mut subscriptions = Vec::new();
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
    loop {
        tokio::select! {
            changed = settings.changed() => {
                if changed.is_err() { break; }
                let (music, monitor) = media_policy(&settings.borrow_and_update());
                if music != music_enabled {
                    music_enabled = music;
                    if music {
                        retry_at = Instant::now();
                    } else {
                        if let Some(task) = manager_task.take() { task.abort(); }
                        reads.abort_all();
                        subscriptions.clear();
                        manager = None;
                        session = None;
                        last_track = None;
                        let _ = events.send(AppEvent::WindowsPlayback(PlaybackSnapshot::idle()));
                    }
                }
                if monitor != volume_enabled {
                    volume_enabled = monitor;
                    volume = monitor.then(|| monitor_volume(&device_changed)).flatten();
                    if monitor { device_changed.notify_one(); }
                }
            }
            command = commands.recv() => {
                let Some(command) = command else { break };
                match command {
                    Command::Volume(level) => if let Some(volume) = &volume && let Err(error) = volume.set(level) { tracing::error!(event = "log.volume.failed", error = %error); },
                    Command::Transport(command) => if let Some(session) = &session && let Err(error) = transport(session, command).await { tracing::warn!(event = "log.windowsMedia.commandFailed", error = %error); },
                }
            }
            _ = device_changed.notified() => {
                if let Some(volume) = &mut volume {
                    if volume.bind_default(volume_changed.clone()).is_ok() {
                        volume_changed.notify_one();
                    }
                }
            }
            _ = volume_changed.notified() => {
                if let Some((level, muted)) = volume.as_ref().and_then(VolumeMonitor::read)
                    && last_volume != Some((level, muted))
                {
                    last_volume = Some((level, muted));
                    let _ = events.send(AppEvent::Volume { level, muted });
                }
            }
            _ = tokio::time::sleep_until(retry_at.into()), if music_enabled && manager.is_none() && manager_task.is_none() => {
                manager_task = Some(tokio::spawn(async {
                    tokio::time::timeout(Duration::from_secs(2), request_manager())
                        .await.ok().and_then(Result::ok)
                }));
            }
            result = async { manager_task.as_mut().expect("guarded manager task").await }, if manager_task.is_some() => {
                manager_task = None;
                manager = result.ok().flatten().and_then(|manager| {
                    ManagerEvents::new(manager, &media_changed).ok()
                });
                retry_at = Instant::now() + Duration::from_secs(5);
                if manager.is_some() {
                    media_changed.notify_one();
                } else {
                    tracing::warn!(event = "log.windowsMedia.initializeFailed");
                }
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
            _ = media_changed.notified(), if manager.is_some() && reads.is_empty() => {
                let watched = &manager.as_ref().expect("guarded media manager").manager;
                let sessions: Vec<_> = match watched.GetSessions() {
                    Ok(sessions) => sessions.into_iter().collect(),
                    Err(error) => {
                        tracing::warn!(event = "log.windowsMedia.initializeFailed", error = %error);
                        subscriptions.clear();
                        manager = None;
                        retry_at = Instant::now() + Duration::from_secs(5);
                        session = None;
                        last_track = None;
                        let _ = events.send(AppEvent::WindowsPlayback(PlaybackSnapshot::idle()));
                        continue;
                    }
                };
                subscriptions.retain(|entry: &SessionEvents| sessions.contains(&entry.session));
                for session in &sessions {
                    if !subscriptions.iter().any(|entry| &entry.session == session) {
                        subscriptions.push(SessionEvents::new(session.clone(), &media_changed));
                    }
                }
                if let Some(active) = select_session(watched, &sessions) {
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

fn media_policy(settings: &AppSettings) -> (bool, bool) {
    let music = settings.desktop.pages.contains(NotchPage::Music);
    (music, music || settings.desktop.hud_volume)
}

fn monitor_volume(changed: &Arc<Notify>) -> Option<VolumeMonitor> {
    VolumeMonitor::new(changed.clone())
        .inspect_err(|error| {
            tracing::warn!(event = "log.volume.failed", error = %error);
        })
        .ok()
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
        let image = super::thumbnail::from_bytes(&bytes, 256).ok()?;
        let (width, height) = image.dimensions();
        Some(Artwork::from_rgba(image.into_raw(), width, height))
    })
    .await
    .ok()
    .flatten()
}
async fn request_manager() -> windows::core::Result<Manager> {
    Manager::RequestAsync()?.await
}
fn select_session(manager: &Manager, sessions: &[Session]) -> Option<Session> {
    let current = manager.GetCurrentSession().ok();
    if let Some(session) = &current
        && is_playing(session)
    {
        return current;
    }
    for session in sessions {
        if is_playing(session) {
            return Some(session.clone());
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
fn change_handler<
    T: windows::core::RuntimeType + 'static,
    U: windows::core::RuntimeType + 'static,
>(
    changed: &Arc<Notify>,
) -> TypedEventHandler<T, U> {
    let changed = changed.clone();
    TypedEventHandler::new(move |_, _| {
        changed.notify_one();
        Ok(())
    })
}

struct ManagerEvents {
    manager: Manager,
    sessions_token: i64,
    current_token: Option<i64>,
}

impl ManagerEvents {
    fn new(manager: Manager, changed: &Arc<Notify>) -> windows::core::Result<Self> {
        let sessions_token = manager.SessionsChanged(&change_handler(changed))?;
        let mut watch = Self {
            manager,
            sessions_token,
            current_token: None,
        };
        watch.current_token = Some(
            watch
                .manager
                .CurrentSessionChanged(&change_handler(changed))?,
        );
        Ok(watch)
    }
}

impl Drop for ManagerEvents {
    fn drop(&mut self) {
        let _ = self.manager.RemoveSessionsChanged(self.sessions_token);
        if let Some(token) = self.current_token {
            let _ = self.manager.RemoveCurrentSessionChanged(token);
        }
    }
}

struct SessionEvents {
    session: Session,
    playback: Option<i64>,
    metadata: Option<i64>,
    timeline: Option<i64>,
}

impl SessionEvents {
    fn new(session: Session, changed: &Arc<Notify>) -> Self {
        let playback = session.PlaybackInfoChanged(&change_handler(changed)).ok();
        let metadata = session
            .MediaPropertiesChanged(&change_handler(changed))
            .ok();
        let timeline = session
            .TimelinePropertiesChanged(&change_handler(changed))
            .ok();
        if playback.is_none() || metadata.is_none() || timeline.is_none() {
            tracing::warn!(event = "log.windowsMedia.initializeFailed");
        }
        Self {
            session,
            playback,
            metadata,
            timeline,
        }
    }
}

impl Drop for SessionEvents {
    fn drop(&mut self) {
        if let Some(token) = self.playback {
            let _ = self.session.RemovePlaybackInfoChanged(token);
        }
        if let Some(token) = self.metadata {
            let _ = self.session.RemoveMediaPropertiesChanged(token);
        }
        if let Some(token) = self.timeline {
            let _ = self.session.RemoveTimelinePropertiesChanged(token);
        }
    }
}
