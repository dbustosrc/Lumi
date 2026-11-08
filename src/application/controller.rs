use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{Local, Timelike};

use super::events::{AppEvent, EventReceiver, UserIntent};
use super::media_session::MediaSession;
use super::presenter::{NotchPresenter, ViewUpdate, VoiceState};
use super::reminders::{ReminderNotice, Reminders, UtilityState};
use super::settings::AppSettings;
use super::settings::DesktopSettings;
use super::shelf_service::ShelfService;
use super::todo_service::{TodoService, TodoServiceError};
use super::voice_capture::VoiceCapture;
use crate::domain::device::PowerStatus;
use crate::domain::hover::{HOVER_LEAVE_GRACE, HoverState};
use crate::domain::notch::{NotchContext, NotchPage, NotchView, Peek, resolve_view};
use crate::domain::ports::PortError;

const PEEK_VISIBLE_FOR: Duration = Duration::from_millis(3500);
const HUD_VISIBLE_FOR: Duration = Duration::from_millis(1500);
const POST_DROP_VISIBLE_FOR: Duration = Duration::from_millis(1600);
const DOZE_AFTER_REST: Duration = Duration::from_secs(120);
const LYRICS_TICK: Duration = Duration::from_millis(200);
const PROGRESS_TICK: Duration = Duration::from_millis(500);

/// Use cases the controller coordinates.
pub struct ControllerServices {
    pub media: MediaSession,
    pub shelf: ShelfService,
    pub todo: TodoService,
    pub voice: VoiceCapture,
    pub water: super::water_service::WaterService,
}

/// Central use case of the notch: reacts to events, keeps the view state and
/// wakes up only when a timer is due, so it costs nothing while idle.
pub struct NotchController {
    services: ControllerServices,
    presenter: Arc<dyn NotchPresenter>,
    context: NotchContext,
    view: NotchView,
    peek_until: Option<Instant>,
    hover: HoverState,
    last_activity: Instant,
    rest_after: Duration,
    is_dozing: bool,
    cursor_near_mascot: bool,
    mascot_awake_until: Option<Instant>,
    is_text_editing: bool,
    is_outbound_dragging: bool,
    power: Option<PowerStatus>,
    clock_minute: Option<u32>,
    is_fullscreen: bool,
    open_on_click: bool,
    reminders: Reminders,
    utility_state: Option<UtilityState>,
    water_state: Option<crate::domain::water::WaterState>,
    voice_state: Option<VoiceState>,
    desktop: DesktopSettings,
    sounds: bool,
    calendar_offset: i32,
    volume_seen: bool,
    brightness_seen: bool,
    wifi: Option<crate::domain::device::WifiState>,
}

impl NotchController {
    pub fn new(
        services: ControllerServices,
        presenter: Arc<dyn NotchPresenter>,
        settings: &AppSettings,
    ) -> Self {
        let mut controller = Self {
            services,
            presenter,
            context: NotchContext::default(),
            view: NotchView::Closed,
            peek_until: None,
            hover: HoverState::default(),
            last_activity: Instant::now(),
            rest_after: settings.rest_after(),
            is_dozing: false,
            cursor_near_mascot: false,
            mascot_awake_until: None,
            is_text_editing: false,
            is_outbound_dragging: false,
            power: None,
            clock_minute: None,
            is_fullscreen: false,
            open_on_click: false,
            reminders: Reminders::new(settings.reminders.clone()),
            utility_state: None,
            water_state: None,
            voice_state: None,
            desktop: settings.desktop.clone(),
            sounds: settings.reminders.sounds,
            calendar_offset: 0,
            volume_seen: false,
            brightness_seen: false,
            wifi: None,
        };
        controller.apply_settings(settings);
        controller.view = resolve_view(&controller.context);
        controller
    }

    /// Runs until every event sender is dropped.
    pub async fn run(mut self, mut events: EventReceiver) {
        self.publish_initial_state();
        loop {
            let wake_at = tokio::time::Instant::from_std(self.next_wake(Instant::now()));
            tokio::select! {
                event = events.recv() => match event {
                    Some(event) => self.handle_event(event),
                    None => break,
                },
                () = tokio::time::sleep_until(wake_at) => {}
            }
            self.on_tick(Instant::now());
        }
        tracing::info!(event = "log.controller.stopped");
    }

    fn publish_initial_state(&mut self) {
        self.publish_calendar();
        self.publish_shelf();
        self.publish_tasks();
        self.presenter.apply(ViewUpdate::Headphones(None));
        self.presenter.apply(ViewUpdate::View(self.view));
        self.on_tick(Instant::now());
    }

    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::TaskApi { command, reply } => {
                if !reply.is_closed() {
                    let mutation = command.is_mutation();
                    let result = self.services.todo.execute(command);
                    if mutation {
                        self.after_todo_mutation(&result);
                    }
                    let _ = reply.send(result);
                }
            }
            AppEvent::MouseMoveStatus(error) => {
                self.presenter.apply(ViewUpdate::MouseMoveStatus(error))
            }
            AppEvent::Brightness(value) => {
                if self.brightness_seen && self.desktop.hud_brightness {
                    self.show_hud(
                        "desktopUi.brightness",
                        format!("{value}%"),
                        Some(f32::from(value) / 100.0),
                    );
                }
                self.brightness_seen = true;
            }
            AppEvent::CapsLock(enabled) => {
                if self.desktop.hud_caps_lock {
                    self.context.is_actionable_alert = false;
                    self.presenter.apply(ViewUpdate::CapsLock(enabled));
                    self.show_peek_for(Peek::Hud, HUD_VISIBLE_FOR);
                }
            }
            AppEvent::InputLanguage(language) => {
                if self.desktop.hud_input_language {
                    self.show_hud("desktopUi.input-language", language, None);
                }
            }
            AppEvent::Wifi(state) => {
                if self
                    .wifi
                    .as_ref()
                    .is_some_and(|previous| *previous != state)
                    && self.desktop.popup_wifi
                {
                    match &state {
                        crate::domain::device::WifiState::Connected(name) => {
                            self.show_hud("desktopUi.wifi-connected", name.clone(), None)
                        }
                        crate::domain::device::WifiState::Disconnected => {
                            self.show_hud("desktopUi.wifi-disconnected", String::new(), None)
                        }
                    }
                }
                self.wifi = Some(state);
            }
            AppEvent::NotificationArrived => {
                self.keep_mascot_awake(Instant::now());
                self.presenter.apply(ViewUpdate::MascotAlert);
            }
            AppEvent::MascotProximity(near) => {
                self.cursor_near_mascot = near;
                self.keep_mascot_awake(Instant::now());
            }
            AppEvent::MascotActivity => self.keep_mascot_awake(Instant::now()),
            AppEvent::ShelfTextDropped(text) => {
                self.on_file_drag_hover(false);
                self.on_drop_received();
                let result = self.services.shelf.add_text(text);
                self.after_shelf_mutation(result);
            }
            AppEvent::Fullscreen(active) => {
                self.is_fullscreen = active;
                if active {
                    self.hover = HoverState::default();
                    self.hover.set_fast_motion(self.desktop.fast_animations);
                    self.context.is_hovered = false;
                }
            }
            AppEvent::WindowsPlayback(snapshot) => {
                self.services.media.on_windows_playback(snapshot);
                self.mark_activity_if_playing();
            }
            AppEvent::WindowsArtwork { track_id, artwork } => {
                self.services.media.on_windows_artwork(track_id, artwork)
            }
            AppEvent::Volume { level, muted } => {
                self.presenter.apply(ViewUpdate::Volume { level, muted });
                if self.volume_seen && self.desktop.hud_volume {
                    self.show_hud(
                        "desktopUi.volume",
                        if muted {
                            "0%".into()
                        } else {
                            format!("{}%", (level * 100.0).round())
                        },
                        Some(if muted { 0.0 } else { level }),
                    );
                }
                self.volume_seen = true;
            }
            AppEvent::BackgroundContent(detected) => self.context.has_nearby_content = detected,
            AppEvent::Playback(snapshot) => {
                self.services.media.on_playback(snapshot);
                self.mark_activity_if_playing();
            }
            AppEvent::MediaConnection(state) => {
                self.presenter.apply(ViewUpdate::MediaConnection(state));
            }
            AppEvent::ArtworkLoaded { track_id, artwork } => {
                self.services.media.on_artwork(&track_id, artwork);
            }
            AppEvent::LyricsLoaded { track_id, lyrics } => {
                self.services.media.on_lyrics(&track_id, lyrics);
            }
            AppEvent::Power(status) => self.on_power(status),
            AppEvent::HeadphonesState {
                status,
                is_newly_connected,
            } => {
                let connected = status.is_some();
                self.presenter.apply(ViewUpdate::Headphones(status));
                if is_newly_connected && connected {
                    self.keep_mascot_awake(Instant::now());
                    self.presenter.apply(ViewUpdate::MascotHeadphones);
                }
                if is_newly_connected && self.desktop.popup_headphones {
                    self.show_peek(Peek::Headphones);
                }
            }
            AppEvent::VoiceTranscribed(result) => self.on_transcribed(result),
            AppEvent::SettingsChanged(settings) => self.apply_settings(&settings),
            AppEvent::Intent(intent) => self.handle_intent(intent),
            shelf_event => self.handle_shelf_event(shelf_event),
        }
    }

    fn handle_shelf_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::FileDragHover(is_hover) => self.on_file_drag_hover(is_hover),
            AppEvent::FilesDropped(paths) => self.on_files_dropped(&paths),
            AppEvent::FilesPicked(paths) => self.on_files_picked(&paths),
            AppEvent::ShelfThumbnailLoaded { id, thumbnail } => {
                if let Some(thumbnail) = thumbnail
                    && self
                        .services
                        .shelf
                        .on_thumbnail_loaded(id, Some(thumbnail.clone()))
                {
                    self.presenter
                        .apply(ViewUpdate::ShelfThumbnail { id, thumbnail });
                }
            }
            AppEvent::OutboundFileDragState {
                is_active,
                is_cursor_inside,
            } => {
                self.on_outbound_file_drag(is_active, is_cursor_inside);
            }
            _ => {}
        }
    }

    fn handle_intent(&mut self, intent: UserIntent) {
        match intent {
            UserIntent::CalendarMove(offset) => {
                self.calendar_offset = self.calendar_offset.saturating_add(offset);
                self.publish_calendar();
            }
            UserIntent::SelectFocusPhase(phase) => self.reminders.select_phase(phase),
            UserIntent::ToggleFocus => self.reminders.toggle_focus(Instant::now()),
            UserIntent::ResetFocus => self.reminders.reset_focus(),
            UserIntent::StartFocusFromReminder => {
                if !self.reminders.state(Instant::now()).focus_running {
                    self.reminders.toggle_focus(Instant::now());
                }
                self.context.page = NotchPage::Pomodoro;
                self.context.peek = None;
                self.context.is_actionable_alert = false;
                self.peek_until = None;
            }
            UserIntent::DismissAlert => {
                self.context.peek = None;
                self.context.is_actionable_alert = false;
                self.peek_until = None;
            }
            UserIntent::DrinkWater => {
                if let Err(error) = self.services.water.drink(Local::now()) {
                    self.show_hud(error.key, String::new(), None);
                }
            }
            UserIntent::ResetWater => {
                if let Err(error) = self.services.water.reset_today(Local::now()) {
                    self.show_hud(error.key, String::new(), None);
                }
            }
            UserIntent::SnoozeReminder => self.reminders.snooze(Instant::now()),
            UserIntent::StartTask(id) => {
                if let Some(task) = self
                    .services
                    .todo
                    .tasks()
                    .iter()
                    .find(|task| task.id == id && !task.is_done)
                {
                    let notice = self
                        .reminders
                        .start_task(id, task.title.clone(), Instant::now());
                    if let Some(notice) = notice {
                        self.show_reminder(notice);
                    }
                }
            }
            UserIntent::Activate => self.hover.reveal_for(Instant::now(), HOVER_LEAVE_GRACE),
            UserIntent::AddShelfText(text) => {
                let result = self.services.shelf.add_text(text);
                self.after_shelf_mutation(result);
            }
            UserIntent::SetVolume(level) => self.services.media.set_volume(level),
            UserIntent::Hover(is_hovered) => self.on_hover(is_hovered),
            UserIntent::LockInteraction(is_locked) => self.on_lock_interaction(is_locked),
            UserIntent::ShowPage(page) => self.show_page(page),
            UserIntent::NextPage => self.show_page(self.context.page.next()),
            UserIntent::PreviousPage => self.show_page(self.context.page.previous()),
            UserIntent::ToggleLyrics => self.toggle_lyrics(),
            UserIntent::Media(command) => self.services.media.execute(command),
            UserIntent::SeekToFraction(fraction) => self.services.media.seek_to_fraction(fraction),
            UserIntent::SeekToLyric(index) => self.services.media.seek_to_lyric(index),
            UserIntent::ToggleVoiceCapture => self.services.voice.toggle(),
            UserIntent::BrowseShelfFiles
            | UserIntent::OpenShelfItem(_)
            | UserIntent::RemoveShelfItem(_)
            | UserIntent::ClearShelf => self.handle_shelf_intent(intent),
            todo_intent => self.handle_todo_intent(todo_intent),
        }
    }

    fn handle_shelf_intent(&mut self, intent: UserIntent) {
        match intent {
            UserIntent::BrowseShelfFiles => self.services.shelf.request_file_picker(),
            UserIntent::OpenShelfItem(id) => {
                let (was_pruned, result) = self.services.shelf.open_item(id);
                if was_pruned {
                    self.publish_shelf();
                }
                if let Err(error) = result {
                    tracing::error!(event = "log.shelf.openFailed", error = %error);
                }
            }
            UserIntent::RemoveShelfItem(id) => {
                let result = self.services.shelf.remove(id);
                self.after_shelf_mutation(result);
            }
            UserIntent::ClearShelf => {
                let result = self.services.shelf.clear();
                self.after_shelf_mutation(result);
            }
            _ => {}
        }
    }

    fn on_file_drag_hover(&mut self, is_hover: bool) {
        self.mark_activity();
        if is_hover {
            self.context.page = NotchPage::Shelf;
            self.peek_until = None;
            self.context.peek = None;
        }
        if self.context.is_file_drag_over != is_hover {
            self.context.is_file_drag_over = is_hover;
            self.presenter.apply(ViewUpdate::ShelfDragHover(is_hover));
        }
    }

    fn on_files_dropped(&mut self, paths: &[PathBuf]) {
        self.on_file_drag_hover(false);
        self.on_drop_received();
        let result = self.services.shelf.add_paths(paths);
        self.after_shelf_mutation(result);
    }

    fn on_files_picked(&mut self, paths: &[PathBuf]) {
        self.reveal_shelf_briefly();
        let result = self.services.shelf.add_paths(paths);
        self.after_shelf_mutation(result);
    }

    fn on_drop_received(&mut self) {
        self.mark_activity();
        self.context.page = NotchPage::Shelf;
        let is_open = self.context.is_hovered || self.context.is_interaction_locked;
        if !self.context.is_mascot_enabled || is_open {
            self.hover.reveal_for(Instant::now(), POST_DROP_VISIBLE_FOR);
        } else {
            self.keep_mascot_awake(Instant::now());
        }
    }

    fn reveal_shelf_briefly(&mut self) {
        self.mark_activity();
        self.context.page = NotchPage::Shelf;
        self.hover.reveal_for(Instant::now(), POST_DROP_VISIBLE_FOR);
    }

    fn on_outbound_file_drag(&mut self, is_active: bool, is_cursor_inside: bool) {
        self.mark_activity();
        self.is_outbound_dragging = is_active;
        if !is_active {
            if is_cursor_inside {
                self.hover.reveal_for(Instant::now(), HOVER_LEAVE_GRACE);
            }
            self.hover.set_inside(is_cursor_inside, Instant::now());
        }
    }

    fn after_shelf_mutation(&mut self, result: Result<bool, PortError>) {
        match result {
            Ok(true) => self.publish_shelf(),
            Ok(false) => {}
            Err(error) => {
                tracing::error!(event = "log.shelf.storageFailed", error = %error);
                self.publish_shelf();
            }
        }
    }

    fn publish_shelf(&self) {
        self.presenter
            .apply(ViewUpdate::ShelfItems(self.services.shelf.entries()));
    }

    fn on_lock_interaction(&mut self, is_locked: bool) {
        if is_locked && self.is_fullscreen {
            return;
        }
        if self.is_text_editing != is_locked {
            self.is_text_editing = is_locked;
            self.presenter.apply(ViewUpdate::TextInputFocus(is_locked));
        }
    }

    fn handle_todo_intent(&mut self, intent: UserIntent) {
        let is_editor_save = matches!(
            &intent,
            UserIntent::AddTask { .. } | UserIntent::RenameTask { .. }
        );
        let todo = &mut self.services.todo;
        let result = match intent {
            UserIntent::AddTask {
                title,
                date,
                time,
                daily,
                color_index,
            } => todo.add_scheduled(&title, &date, &time, daily, color_index),
            UserIntent::RenameTask {
                id,
                title,
                color_index,
            } => todo.rename(id, &title, color_index),
            UserIntent::ToggleTask(id) => todo.toggle(id),
            UserIntent::DeleteTask(id) => todo.delete(id),
            _ => return,
        };
        let error_key = result.as_ref().err().map(TodoServiceError::message_key);
        self.after_todo_mutation(&result);
        if is_editor_save || error_key.is_some() {
            self.presenter
                .apply(ViewUpdate::TodoEditorResult { error_key });
        }
    }

    fn after_todo_mutation<T>(&mut self, result: &Result<T, TodoServiceError>) {
        match result {
            Ok(_) => self.publish_tasks(),
            Err(TodoServiceError::Rule(error)) => {
                tracing::warn!(event = "log.todo.mutationRejected", reason = %error);
            }
            Err(TodoServiceError::Storage(error)) => {
                tracing::error!(event = "log.todo.storageFailed", error = %error);
            }
        }
    }

    fn publish_calendar(&self) {
        if let Some(grid) = crate::domain::calendar::calendar_grid(
            Local::now().date_naive(),
            self.calendar_offset,
            self.desktop.calendar_starts_monday,
            self.desktop.calendar_week_view,
        ) {
            self.presenter.apply(ViewUpdate::Calendar {
                grid,
                tasks: self.services.todo.tasks().to_vec(),
            });
        }
    }

    fn show_hud(&mut self, key: &'static str, detail: String, level: Option<f32>) {
        self.context.is_actionable_alert = false;
        self.presenter.apply(ViewUpdate::Hud {
            title_key: key,
            detail,
            level,
            reminder: false,
            alert: false,
        });
        self.show_peek_for(Peek::Hud, HUD_VISIBLE_FOR);
    }

    fn show_reminder(&mut self, notice: ReminderNotice) {
        self.reminders.remember(&notice);
        if !self.context.is_hovered && !self.context.is_interaction_locked {
            self.context.page = notice.page;
        }
        self.context.is_actionable_alert = notice.page == NotchPage::Pomodoro;
        if self.context.is_actionable_alert {
            let state = self.reminders.state(Instant::now());
            self.presenter.apply(ViewUpdate::FocusReminder {
                phase: state.phase,
                seconds: state.focus_seconds,
            });
        } else {
            self.presenter.apply(ViewUpdate::Hud {
                title_key: notice.key,
                detail: notice.detail,
                level: None,
                reminder: notice.page == NotchPage::Todo,
                alert: true,
            });
        }
        self.show_peek(Peek::Hud);
        if self.context.is_actionable_alert {
            self.peek_until = None;
        }
        if self.sounds && !self.is_fullscreen {
            self.presenter.apply(ViewUpdate::ReminderSound);
        }
    }

    fn on_transcribed(&mut self, result: Result<String, PortError>) {
        self.mark_activity();
        self.keep_mascot_awake(Instant::now());
        if let Some(text) = self.services.voice.on_transcribed(result) {
            let added = self.services.todo.add(&text);
            let error_key = added.as_ref().err().map(TodoServiceError::message_key);
            self.after_todo_mutation(&added);
            if let Some(error_key) = error_key {
                self.context.page = NotchPage::Todo;
                self.presenter
                    .apply(ViewUpdate::RecoverTodoDraft { text, error_key });
            }
        }
    }

    fn publish_tasks(&self) {
        self.presenter.apply(ViewUpdate::Tasks {
            tasks: self.services.todo.tasks().to_vec(),
            progress: self.services.todo.progress(),
        });
        self.publish_calendar();
    }

    fn on_hover(&mut self, is_hovered: bool) {
        self.mark_activity();
        let now = Instant::now();
        self.hover
            .set_inside_with_policy(is_hovered, now, !self.open_on_click);
        if is_hovered && self.context.is_interaction_locked {
            self.hover.reveal_for(now, HOVER_LEAVE_GRACE);
        }
    }

    fn show_page(&mut self, page: NotchPage) {
        self.context.page = page;
    }

    fn toggle_lyrics(&mut self) {
        if !self.services.media.lyrics_enabled() {
            return;
        }
        self.context.is_lyrics_mode = !self.context.is_lyrics_mode;
        self.presenter
            .apply(ViewUpdate::LyricsMode(self.context.is_lyrics_mode));
    }

    fn on_power(&mut self, status: PowerStatus) {
        let was_plugged = self.power.map(|previous| previous.is_plugged_in);
        self.power = Some(status);
        self.presenter.apply(ViewUpdate::Power(status));
        if was_plugged == Some(false)
            && status.is_plugged_in
            && status.has_battery
            && self.desktop.popup_battery
        {
            self.show_peek(Peek::Charging);
        }
    }

    fn show_peek(&mut self, peek: Peek) {
        self.show_peek_for(peek, PEEK_VISIBLE_FOR);
    }

    fn show_peek_for(&mut self, peek: Peek, duration: Duration) {
        self.context.peek = Some(peek);
        self.peek_until = Some(Instant::now() + duration);
        self.mark_activity();
    }

    fn apply_settings(&mut self, settings: &AppSettings) {
        if !settings.is_lyrics_enabled {
            self.context.is_lyrics_mode = false;
            self.presenter.apply(ViewUpdate::LyricsMode(false));
        }
        if self.desktop.calendar_week_view != settings.desktop.calendar_week_view {
            self.calendar_offset = 0;
        }
        self.desktop = settings.desktop.clone();
        self.sounds = settings.reminders.sounds;
        self.reminders.configure(&settings.reminders);
        self.services.water.configure(&settings.water, Local::now());
        self.utility_state = None;
        self.water_state = None;
        self.voice_state = None;
        self.publish_calendar();
        self.open_on_click = settings.desktop.open_on_click;
        self.hover.set_fast_motion(settings.desktop.fast_animations);
        self.rest_after = settings.rest_after();
        self.context.is_minimal_notch = settings.is_minimal_notch;
        self.context.is_mascot_enabled = settings.is_mascot_enabled;
        self.presenter.apply(ViewUpdate::Mascot {
            is_dozing: self.is_dozing,
            is_enabled: settings.is_mascot_enabled,
        });
        self.services
            .media
            .set_lyrics_enabled(settings.is_lyrics_enabled);
        self.services.voice.configure(settings.wyoming.is_enabled);
    }

    fn mark_activity(&mut self) {
        self.last_activity = Instant::now();
    }
    fn keep_mascot_awake(&mut self, now: Instant) {
        self.mascot_awake_until = Some(now + DOZE_AFTER_REST);
    }

    fn mark_activity_if_playing(&mut self) {
        if self.services.media.is_playing() {
            self.mark_activity();
        }
    }

    /// Re-evaluates timers and the resulting view; cheap enough to run after
    /// every event.
    fn on_tick(&mut self, now: Instant) {
        let wall = Local::now();
        if self.services.water.tick(wall) {
            self.show_reminder(ReminderNotice {
                key: "desktopUi.water-reminder",
                detail: String::new(),
                page: NotchPage::Water,
            });
        }
        let water = self.services.water.state(wall);
        if self.water_state.as_ref() != Some(&water) {
            self.water_state = Some(water.clone());
            self.presenter.apply(ViewUpdate::Water(water));
        }
        if let Some(id) = self.reminders.state(now).task_id
            && self
                .services
                .todo
                .tasks()
                .iter()
                .all(|task| task.id != id || task.is_done)
        {
            self.reminders.cancel_task(id);
        }
        for notice in self.reminders.tick(now) {
            self.show_reminder(notice);
        }
        let state = self.reminders.state(now);
        if self.utility_state.as_ref() != Some(&state) {
            self.utility_state = Some(state.clone());
            self.presenter.apply(ViewUpdate::Utility(state));
        }
        self.refresh_hover(now);
        self.expire_peek(now);
        self.services.voice.on_deadline(now);
        self.refresh_clock();
        self.refresh_media(now);
        self.refresh_rest(now);
        let voice = self.services.voice.state();
        if self.voice_state != Some(voice) {
            self.voice_state = Some(voice);
            self.presenter.apply(ViewUpdate::Voice(voice));
        }
        self.refresh_view();
    }

    fn refresh_hover(&mut self, now: Instant) {
        self.hover.advance(now);
        let is_hovered = self.hover.is_open();
        if is_hovered && !self.context.is_hovered && !self.context.is_actionable_alert {
            self.peek_until = None;
            self.context.peek = None;
            self.context.is_actionable_alert = false;
        }
        if self.context.is_hovered && !is_hovered {
            self.last_activity = now;
        }
        self.context.is_hovered = is_hovered;
    }

    fn expire_peek(&mut self, now: Instant) {
        if self.peek_until.is_some_and(|until| until <= now) {
            self.peek_until = None;
            self.context.peek = None;
            self.context.is_actionable_alert = false;
        }
    }

    fn refresh_clock(&mut self) {
        let now = Local::now();
        let minute = now.hour() * 60 + now.minute();
        if self.clock_minute != Some(minute) {
            match self.services.todo.advance_daily(now.date_naive()) {
                Ok(true) => self.publish_tasks(),
                Ok(false) => {}
                Err(error) => self.show_hud(error.message_key(), String::new(), None),
            }
            if self.reminders.task_start_enabled() {
                let due: Vec<_> = self
                    .services
                    .todo
                    .tasks()
                    .iter()
                    .filter(|task| {
                        !task.is_done
                            && task.schedule.occurs_on(now.date_naive())
                            && task.schedule.time.is_some_and(|time| {
                                time.hour() == now.hour() && time.minute() == now.minute()
                            })
                    })
                    .map(|task| task.title.clone())
                    .collect();
                for title in due {
                    self.show_reminder(ReminderNotice {
                        key: "desktopUi.task-due",
                        detail: title,
                        page: NotchPage::Todo,
                    });
                }
            }
            self.publish_calendar();
            self.clock_minute = Some(minute);
            self.presenter.apply(ViewUpdate::Clock(now));
        }
    }

    fn refresh_media(&mut self, now: Instant) {
        let media = &mut self.services.media;
        if self.view == NotchView::Music {
            media.refresh_progress(now);
        }
        if self.view == NotchView::Lyrics {
            media.refresh_progress(now);
            media.refresh_active_lyric(now);
        }
        self.context.is_playing = media.is_playing();
        self.context.has_lyrics = media.has_lyrics();
    }

    fn refresh_rest(&mut self, now: Instant) {
        if self.context.is_hovered
            || self.context.is_playing
            || self
                .utility_state
                .as_ref()
                .is_some_and(|state| state.focus_running)
        {
            self.last_activity = now;
        }
        let idle_for = now.saturating_duration_since(self.last_activity);
        self.context.is_quiet = idle_for >= self.rest_after;
        let is_dozing = self.context.is_quiet
            && idle_for >= self.rest_after + DOZE_AFTER_REST
            && !self.cursor_near_mascot
            && self.mascot_awake_until.is_none_or(|until| now >= until);
        if is_dozing != self.is_dozing {
            self.is_dozing = is_dozing;
            self.presenter.apply(ViewUpdate::Mascot {
                is_dozing,
                is_enabled: self.context.is_mascot_enabled,
            });
        }
    }

    fn refresh_view(&mut self) {
        let voice_busy = self.services.voice.is_busy();
        self.context.is_voice_busy = voice_busy;
        self.context.is_interaction_locked = self.is_text_editing
            || self.is_outbound_dragging
            || (voice_busy && !self.context.is_mascot_enabled);
        let view = resolve_view(&self.context);
        if view != self.view {
            self.view = view;
            self.presenter.apply(ViewUpdate::View(view));
        }
    }

    fn next_wake(&self, now: Instant) -> Instant {
        let clock = now + seconds_to_next_minute();
        let rest = (!self.context.is_quiet).then(|| self.last_activity + self.rest_after);
        let doze = (!self.is_dozing && !self.cursor_near_mascot).then(|| {
            let idle_deadline = self.last_activity + self.rest_after + DOZE_AFTER_REST;
            self.mascot_awake_until
                .map_or(idle_deadline, |until| idle_deadline.max(until))
        });
        let media = self.media_tick().map(|tick| now + tick);
        [
            Some(clock),
            rest,
            doze,
            media,
            self.peek_until,
            self.hover.deadline(),
            self.services.voice.deadline(),
            (self.reminders.needs_tick() || self.services.water.state(Local::now()).enabled)
                .then_some(now + Duration::from_secs(1)),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(clock)
        .max(now + Duration::from_millis(16))
    }

    fn media_tick(&self) -> Option<Duration> {
        if !self.context.is_playing {
            return None;
        }
        match self.view {
            NotchView::Lyrics => Some(LYRICS_TICK),
            NotchView::Music => Some(PROGRESS_TICK),
            _ => None,
        }
    }
}

fn seconds_to_next_minute() -> Duration {
    let now = Local::now();
    let elapsed = Duration::new(u64::from(now.second()), now.nanosecond() % 1_000_000_000);
    Duration::from_secs(60).saturating_sub(elapsed) + Duration::from_millis(5)
}
