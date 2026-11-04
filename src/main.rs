#![cfg_attr(windows, windows_subsystem = "windows")]
//! Composition root for the native Windows notch and unchanged application services.
#[cfg(not(windows))]
compile_error!("Lumi requires Windows.");
mod adapters;
mod application;
mod domain;
mod i18n;
mod infrastructure;
use crate::adapters::native::host::NativeHost;
use crate::application::controller::{ControllerServices, NotchController};
use crate::application::events::{EventReceiver, EventSender, create_event_channel};
use crate::application::media_session::{MediaPorts, MediaSession};
use crate::application::presenter::NotchPresenter;
use crate::application::settings::{LanguagePreference, SettingsService};
use crate::application::shelf_service::ShelfService;
use crate::application::todo_service::TodoService;
use crate::application::voice_capture::VoiceCapture;
use crate::domain::ports::{PortError, ShelfPlatform, ShelfRepository, TaskRepository};
use crate::infrastructure::artwork::HttpArtworkLoader;
use crate::infrastructure::lrclib::LrclibLyricsProvider;
use crate::infrastructure::microphone::CpalAudioRecorder;
use crate::infrastructure::music_assistant::spawn_music_assistant;
use crate::infrastructure::settings_store::TomlSettingsRepository;
use crate::infrastructure::shelf_platform::NativeShelfPlatform;
use crate::infrastructure::shelf_store::JsonShelfRepository;
use crate::infrastructure::task_store::JsonTaskRepository;
use crate::infrastructure::wyoming::WyomingSpeechToText;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;
#[derive(Debug, thiserror::Error)]
enum StartupError {
    #[error(transparent)]
    Port(#[from] PortError),
    #[error(transparent)]
    Windows(#[from] windows::core::Error),
    #[error(transparent)]
    Runtime(#[from] std::io::Error),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
}
struct SystemWatchers {
    #[cfg(windows)]
    _caps_lock: Option<infrastructure::windows::desktop_status::CapsLockWatcher>,
    #[cfg(windows)]
    _headphones: Option<infrastructure::windows::headphones::HeadphonesWatcher>,
    #[cfg(windows)]
    _notifications: Option<infrastructure::windows::notifications::NotificationWatcher>,
}
fn main() -> ExitCode {
    let Ok(data_dir) = infrastructure::paths::data_directory() else {
        return ExitCode::FAILURE;
    };
    let system_locale = detect_system_locale();
    i18n::select_language(LanguagePreference::System, system_locale.as_deref());
    let _log_guard = infrastructure::logging::init_logging(&data_dir);
    tracing::info!(event = "log.app.starting");
    match run_application(&data_dir, system_locale) {
        Ok(()) => {
            tracing::info!(event = "log.app.stopping");
            ExitCode::SUCCESS
        }
        Err(error) => {
            tracing::error!(
                event = "log.app.startupFailed",
                error = &error as &(dyn std::error::Error + 'static)
            );
            ExitCode::FAILURE
        }
    }
}
fn run_application(data_dir: &Path, system_locale: Option<String>) -> Result<(), StartupError> {
    let settings = load_settings_service(data_dir, system_locale.as_deref())?;
    let runtime = build_async_runtime()?;
    let (event_tx, event_rx) = create_event_channel();
    let host = NativeHost::new(Arc::clone(&settings), event_tx.clone(), system_locale)?;
    let presenter: Arc<dyn NotchPresenter> = host.presenter.clone();
    let _watchers =
        spawn_background_services(&runtime, data_dir, &settings, presenter, event_tx, event_rx)?;
    tracing::info!(event = "log.app.ready");
    host.run()?;
    Ok(())
}
fn load_settings_service(
    data_dir: &Path,
    system_locale: Option<&str>,
) -> Result<Arc<SettingsService>, StartupError> {
    let repository = Box::new(TomlSettingsRepository::new(data_dir.join("settings.toml")));
    let service = Arc::new(SettingsService::new(
        repository,
        Box::new(infrastructure::startup::NativeStartupRegistration),
    )?);
    i18n::select_language(service.current().language, system_locale);
    Ok(service)
}

fn build_async_runtime() -> Result<tokio::runtime::Runtime, StartupError> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("lumi-io")
        .enable_all()
        .build()
        .map_err(StartupError::from)
}

fn spawn_background_services(
    runtime: &tokio::runtime::Runtime,
    data_dir: &Path,
    settings: &Arc<SettingsService>,
    presenter: Arc<dyn NotchPresenter>,
    event_tx: EventSender,
    event_rx: EventReceiver,
) -> Result<SystemWatchers, StartupError> {
    let _enter = runtime.enter();
    runtime.spawn(application::mouse_mover::run(
        settings.subscribe(),
        Arc::new(infrastructure::windows::mouse_activity::WindowsMouseActivity),
        event_tx.clone(),
    ));

    let media = build_media_session(settings, &event_tx, Arc::clone(&presenter))?;
    let shelf = load_shelf_service(
        data_dir,
        event_tx.clone(),
        settings.current().desktop.empty_shelf_on_start,
    )?;
    let todo = load_todo_service(data_dir)?;
    let voice = build_voice_capture(settings, event_tx.clone());
    let water = crate::application::water_service::WaterService::load(
        Arc::new(infrastructure::water_store::JsonWaterRepository::new(
            data_dir.join("water.json"),
        )),
        settings.current().water,
        chrono::Local::now(),
    )?;
    let services = ControllerServices {
        water,
        media,
        shelf,
        todo,
        voice,
    };
    let controller = NotchController::new(services, presenter, &settings.current());
    let watchers = start_system_watchers(&event_tx);
    runtime.spawn(controller.run(event_rx));
    Ok(watchers)
}

fn build_media_session(
    settings: &Arc<SettingsService>,
    event_tx: &EventSender,
    presenter: Arc<dyn NotchPresenter>,
) -> Result<MediaSession, StartupError> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("Lumi/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(15))
        .build()?;
    let controller = Arc::new(spawn_music_assistant(
        settings.subscribe(),
        event_tx.clone(),
    ));
    let artwork = Arc::new(HttpArtworkLoader::new(client.clone(), settings.subscribe()));
    let lyrics = Arc::new(LrclibLyricsProvider::new(client));
    let (windows, volume) = build_windows_media(event_tx)?;
    let ports = MediaPorts {
        controller,
        windows,
        volume,
        artwork,
        lyrics,
    };
    Ok(MediaSession::new(ports, event_tx.clone(), presenter))
}

type SystemMediaPorts = (
    Arc<dyn crate::domain::ports::MediaController>,
    Arc<dyn crate::domain::ports::VolumeController>,
);

#[cfg(windows)]
fn build_windows_media(events: &EventSender) -> Result<SystemMediaPorts, StartupError> {
    let controller = infrastructure::windows::media::spawn(events.clone())?;
    Ok((controller.clone(), controller))
}

#[cfg(not(windows))]
fn build_windows_media(_events: &EventSender) -> Result<SystemMediaPorts, StartupError> {
    Err(PortError::new("music.error.windowsMedia", "Windows is required").into())
}

fn load_todo_service(data_dir: &Path) -> Result<TodoService, StartupError> {
    let repository: Arc<dyn TaskRepository> =
        Arc::new(JsonTaskRepository::new(data_dir.join("tasks.json")));
    Ok(TodoService::load(repository)?)
}

fn load_shelf_service(
    data_dir: &Path,
    event_tx: EventSender,
    empty_on_start: bool,
) -> Result<ShelfService, StartupError> {
    let repository: Arc<dyn ShelfRepository> =
        Arc::new(JsonShelfRepository::new(data_dir.join("shelf.json")));
    let platform: Arc<dyn ShelfPlatform> = Arc::new(NativeShelfPlatform);
    if empty_on_start {
        repository.load()?;
        repository.save(&[])?;
    }
    Ok(ShelfService::load(repository, platform, event_tx)?)
}

fn build_voice_capture(settings: &Arc<SettingsService>, event_tx: EventSender) -> VoiceCapture {
    let recorder = Arc::new(CpalAudioRecorder::default());
    let speech = Arc::new(WyomingSpeechToText::new(settings.subscribe()));
    VoiceCapture::new(recorder, speech, event_tx)
}

#[cfg(windows)]
fn start_system_watchers(event_tx: &EventSender) -> SystemWatchers {
    let notifications =
        infrastructure::windows::notifications::NotificationWatcher::start(event_tx.clone())
            .map_err(
                |error| tracing::warn!(event = "log.notifications.subscribeFailed", error = %error),
            )
            .ok();
    let caps_lock =
        infrastructure::windows::desktop_status::CapsLockWatcher::install(event_tx.clone())
            .map_err(|error| tracing::warn!(event = "log.hud.keyboardFailed", error = %error))
            .ok();
    if let Err(error) = infrastructure::windows::desktop_status::spawn(event_tx.clone()) {
        tracing::warn!(event = "log.hud.initializeFailed", error = %error);
    }
    if let Err(error) = infrastructure::windows::power::watch_power(event_tx.clone()) {
        tracing::error!(
            event = "log.power.watcherFailed",
            error = &error as &(dyn std::error::Error + 'static)
        );
    }
    let headphones = match infrastructure::windows::headphones::watch_headphones(event_tx.clone()) {
        Ok(watcher) => Some(watcher),
        Err(error) => {
            tracing::error!(
                event = "log.headphones.watcherFailed",
                error = &error as &(dyn std::error::Error + 'static)
            );
            None
        }
    };
    SystemWatchers {
        _notifications: notifications,
        _headphones: headphones,
        _caps_lock: caps_lock,
    }
}

#[cfg(not(windows))]
fn start_system_watchers(_event_tx: &EventSender) -> SystemWatchers {
    SystemWatchers {}
}

#[cfg(windows)]
fn detect_system_locale() -> Option<String> {
    infrastructure::windows::locale::get_user_locale()
}

#[cfg(not(windows))]
fn detect_system_locale() -> Option<String> {
    None
}
