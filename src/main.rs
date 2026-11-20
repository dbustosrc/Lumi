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
fn main() -> ExitCode {
    let Ok(instance) = single_instance() else {
        return ExitCode::FAILURE;
    };
    let Some(_instance) = instance else {
        return ExitCode::SUCCESS;
    };
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

struct InstanceGuard(windows::Win32::Foundation::HANDLE);

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.0) };
    }
}

fn single_instance() -> windows::core::Result<Option<InstanceGuard>> {
    use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows::Win32::System::Threading::CreateMutexW;
    let handle = unsafe { CreateMutexW(None, false, windows::core::w!("Local\\dbustosrc.Lumi"))? };
    let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    Ok((!already_running).then_some(InstanceGuard(handle)))
}

fn run_application(data_dir: &Path, system_locale: Option<String>) -> Result<(), StartupError> {
    let settings = load_settings_service(data_dir, system_locale.as_deref())?;
    let runtime = build_async_runtime()?;
    let (event_tx, event_rx) = create_event_channel();
    let task_tokens = start_task_api(&runtime, data_dir, event_tx.clone(), settings.subscribe())
        .map_err(|error| {
            tracing::warn!(event = "log.taskApi.unavailable", reason = error.key);
            error.key
        });
    let host = NativeHost::new(
        Arc::clone(&settings),
        event_tx.clone(),
        system_locale,
        task_tokens,
    )?;
    let presenter: Arc<dyn NotchPresenter> = host.presenter.clone();
    spawn_background_services(&runtime, data_dir, &settings, presenter, event_tx, event_rx)?;
    tracing::info!(event = "log.app.ready");
    host.run()?;
    Ok(())
}

fn start_task_api(
    runtime: &tokio::runtime::Runtime,
    data_dir: &Path,
    events: EventSender,
    settings: tokio::sync::watch::Receiver<application::settings::AppSettings>,
) -> Result<Arc<infrastructure::task_tokens::TaskTokens>, PortError> {
    use infrastructure::{task_api, task_tokens::TaskTokens};
    let tokens = Arc::new(TaskTokens::load(data_dir.join("task-api-tokens.json"))?);
    let bind_error = |error| PortError::new("taskApi.error.listen", error);
    let _enter = runtime.enter();
    let listener = if settings
        .borrow()
        .desktop
        .pages
        .contains(domain::notch::NotchPage::Todo)
    {
        let listener = std::net::TcpListener::bind(task_api::ADDRESS).map_err(bind_error)?;
        listener.set_nonblocking(true).map_err(bind_error)?;
        Some(tokio::net::TcpListener::from_std(listener).map_err(bind_error)?)
    } else {
        None
    };
    runtime.spawn(task_api::serve_when_enabled(
        listener,
        tokens.clone(),
        events,
        settings,
    ));
    Ok(tokens)
}
fn load_settings_service(
    data_dir: &Path,
    system_locale: Option<&str>,
) -> Result<Arc<SettingsService>, StartupError> {
    infrastructure::settings_migration::migrate(&data_dir.join("settings.toml"))?;
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
) -> Result<(), StartupError> {
    let _enter = runtime.enter();
    runtime.spawn(application::mouse_mover::run(
        settings.subscribe(),
        Arc::new(infrastructure::windows::mouse_activity::WindowsMouseActivity),
        event_tx.clone(),
    ));

    let media = build_media_session(settings, &event_tx, Arc::clone(&presenter))?;
    let current = settings.current();
    let shelf = load_shelf_service(
        data_dir,
        event_tx.clone(),
        current.desktop.empty_shelf_on_start
            && current
                .desktop
                .pages
                .contains(crate::domain::notch::NotchPage::Shelf),
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
    let focus_store = infrastructure::focus_store::JsonFocusDayStore(data_dir.join("focus.json"));
    let focus_day = focus_store.load(chrono::Local::now().date_naive())?;
    let (quick_share, quick_share_rx) = tokio::sync::mpsc::unbounded_channel();
    runtime.spawn(infrastructure::quick_share::run(
        data_dir.join("quick-share"),
        settings.subscribe(),
        quick_share_rx,
        Arc::clone(&presenter),
    ));
    let services = ControllerServices {
        focus_store: Box::new(focus_store),
        focus_day,
        water,
        media,
        shelf,
        quick_share,
        todo,
        voice,
    };
    let controller = NotchController::new(services, presenter, &current);
    start_power_watcher(&event_tx);
    runtime.spawn(controller.run(event_rx));
    Ok(())
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
    let (windows, volume) = build_windows_media(event_tx, settings.subscribe())?;
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
fn build_windows_media(
    events: &EventSender,
    settings: tokio::sync::watch::Receiver<application::settings::AppSettings>,
) -> Result<SystemMediaPorts, StartupError> {
    let controller = infrastructure::windows::media::spawn(events.clone(), settings)?;
    Ok((controller.clone(), controller))
}

#[cfg(not(windows))]
fn build_windows_media(
    _events: &EventSender,
    _settings: tokio::sync::watch::Receiver<application::settings::AppSettings>,
) -> Result<SystemMediaPorts, StartupError> {
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
fn start_power_watcher(event_tx: &EventSender) {
    if let Err(error) = infrastructure::windows::power::watch_power(event_tx.clone()) {
        tracing::error!(
            event = "log.power.watcherFailed",
            error = &error as &(dyn std::error::Error + 'static)
        );
    }
}

#[cfg(not(windows))]
fn start_power_watcher(_event_tx: &EventSender) {}

#[cfg(windows)]
fn detect_system_locale() -> Option<String> {
    infrastructure::windows::locale::get_user_locale()
}

#[cfg(not(windows))]
fn detect_system_locale() -> Option<String> {
    None
}
