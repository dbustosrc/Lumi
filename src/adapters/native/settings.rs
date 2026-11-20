use super::model::{Rect, UiModel, cubic_bezier, tr};
use super::notices::COMPONENTS;
use super::scene::{Action, Field, Paint, Preference, Scene};
use super::views;
use crate::application::settings::{AppSettings, LanguagePreference};
use crate::domain::ports::PortError;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct Display {
    pub id: String,
    pub label: String,
    pub rect: Rect,
    pub work: Rect,
    pub scale: f32,
    pub primary: bool,
}

pub(super) fn display_map(displays: &[Display], area: Rect) -> Vec<(&Display, Rect)> {
    let Some(bounds) = displays.iter().map(|display| display.rect).reduce(|a, b| {
        let left = a.x.min(b.x);
        let top = a.y.min(b.y);
        let right = (a.x + a.w).max(b.x + b.w);
        let bottom = (a.y + a.h).max(b.y + b.h);
        Rect::new(left, top, right - left, bottom - top)
    }) else {
        return Vec::new();
    };
    let scale = (area.w / bounds.w).min(area.h / bounds.h);
    let left = area.x + (area.w - bounds.w * scale) / 2.;
    let top = area.y + (area.h - bounds.h * scale) / 2.;
    displays
        .iter()
        .map(|display| {
            let r = display.rect;
            (
                display,
                Rect::new(
                    left + (r.x - bounds.x) * scale,
                    top + (r.y - bounds.y) * scale,
                    r.w * scale,
                    r.h * scale,
                ),
            )
        })
        .collect()
}

pub struct SettingsScrollbar {
    pub track: Rect,
    pub thumb: Rect,
    pub max_scroll: f32,
}
impl SettingsScrollbar {
    pub fn new(track: Rect, max_scroll: f32, scroll: f32) -> Self {
        let height = (track.h * track.h / (track.h + max_scroll))
            .max(20.)
            .min(track.h);
        let fraction = if max_scroll > 0. {
            (scroll / max_scroll).clamp(0., 1.)
        } else {
            0.
        };
        Self {
            track,
            thumb: Rect::new(
                track.x + (track.w - 3.) / 2.,
                track.y + fraction * (track.h - height),
                3.,
                height,
            ),
            max_scroll,
        }
    }
    pub fn grab_offset(&self, y: f32) -> f32 {
        if (self.thumb.y..=self.thumb.y + self.thumb.h).contains(&y) {
            y - self.thumb.y
        } else {
            self.thumb.h / 2.
        }
    }
    pub fn scroll_at(&self, y: f32, grab_offset: f32) -> f32 {
        let travel = self.track.h - self.thumb.h;
        if travel <= 0. {
            return 0.;
        }
        ((y - self.track.y - grab_offset) / travel).clamp(0., 1.) * self.max_scroll
    }
}
#[derive(Clone, Copy)]
pub enum NoticesPage {
    Index,
    Component { index: usize, index_scroll: f32 },
}

use crate::domain::notch::{NotchPage, PageOrder};
use crate::domain::widgets::{CITIES, StatusRing, Widget};

const ABOUT: usize = 10;
const WIDGETS: usize = 13;
const QUICK_SHARE: usize = 14;
pub(super) const PAGES: usize = 15;
const NAV_TOP: f32 = 44.;
const NAV_STEP: f32 = 31.;
const NAV_ROW_HEIGHT: f32 = 30.;
const NAV_FOOTER: f32 = 52.;
const NAV_SECTIONS: [usize; 13] = [0, 1, PAGES, 2, 3, WIDGETS, 4, 6, 7, 8, QUICK_SHARE, 9, 11];
pub(super) const SETTINGS_MIN_HEIGHT: f32 =
    NAV_TOP + NAV_SECTIONS.len() as f32 * NAV_STEP + NAV_FOOTER;
const SWITCH_DURATION: Duration = Duration::from_millis(175);
const PAGE_ROW_HEIGHT: f32 = 32.;
const PAGE_SLIDE_DURATION: Duration = Duration::from_millis(160);

struct SwitchMotion {
    key: Action,
    from: f32,
    to: f32,
    started: Instant,
}

struct PageSlide {
    page: NotchPage,
    from: f32,
    to: f32,
    started: Instant,
    duration: Duration,
}

impl PageSlide {
    fn position(&self, now: Instant) -> f32 {
        let t = (now.saturating_duration_since(self.started).as_secs_f32()
            / self.duration.as_secs_f32())
        .min(1.);
        self.from + (self.to - self.from) * (1. - (1. - t).powi(3))
    }
}

pub struct SettingsUi {
    pub section: usize,
    pub widget_right: bool,
    pub notices: Option<NoticesPage>,
    pub changed: Instant,
    pub scroll: f32,
    pub mouse: (f32, f32),
    pub page_drag: Option<(NotchPage, Option<NotchPage>)>,
    lifted_page: Option<NotchPage>,
    page_slides: Vec<PageSlide>,
    switch_motion: Vec<SwitchMotion>,
    pub hover: Option<usize>,
    pub previous_hover: Option<usize>,
    pub hover_changed: Instant,
    pub fields: BTreeMap<Field, String>,
    pub status: String,
    pub error: bool,
    pub dark: bool,
    pub api_error: Option<&'static str>,
    pub api_tokens: Vec<crate::infrastructure::task_tokens::TokenInfo>,
    pub issued_token: Option<crate::infrastructure::task_tokens::IssuedToken>,
    pub api_notice: Option<(&'static str, bool)>,
}
impl SettingsUi {
    pub fn new(settings: &AppSettings, dark: bool) -> Self {
        let mut ui = Self {
            section: 0,
            widget_right: false,
            notices: None,
            changed: Instant::now(),
            scroll: 0.,
            mouse: (-1., -1.),
            page_drag: None,
            lifted_page: None,
            page_slides: Vec::new(),
            switch_motion: Vec::new(),
            hover: None,
            previous_hover: None,
            hover_changed: Instant::now() - std::time::Duration::from_secs(1),
            fields: BTreeMap::new(),
            status: String::new(),
            error: false,
            dark,
            api_error: Some("taskApi.error.listen"),
            api_tokens: Vec::new(),
            issued_token: None,
            api_notice: None,
        };
        ui.load_drafts(settings);
        ui
    }
    pub fn switch_position(&self, action: &Action, on: bool, now: Instant) -> f32 {
        let Some(motion) = self
            .switch_motion
            .iter()
            .find(|motion| &motion.key == action)
        else {
            return u8::from(on).into();
        };
        let progress = (now.saturating_duration_since(motion.started).as_secs_f32()
            / SWITCH_DURATION.as_secs_f32())
        .min(1.);
        motion.from + (motion.to - motion.from) * cubic_bezier(progress, 0.25, 0.1, 0.25, 1.)
    }
    pub fn animate_switch(&mut self, action: Action, from: bool, to: bool, now: Instant) {
        let from = self.switch_position(&action, from, now);
        self.switch_motion.retain(|motion| motion.key != action);
        self.switch_motion.push(SwitchMotion {
            key: action,
            from,
            to: f32::from(u8::from(to)),
            started: now,
        });
    }
    pub fn update_motions(&mut self, now: Instant) {
        self.switch_motion
            .retain(|motion| now.saturating_duration_since(motion.started) < SWITCH_DURATION);
        for slide in &mut self.page_slides {
            if now.saturating_duration_since(slide.started) >= slide.duration {
                slide.from = slide.to;
            }
        }
        if self.page_drag.is_none() && self.page_slides.iter().all(|slide| slide.from == slide.to) {
            self.page_slides.clear();
            self.lifted_page = None;
        }
    }
    pub fn controls_animating(&self) -> bool {
        !self.switch_motion.is_empty()
            || self.page_slides.iter().any(|slide| slide.from != slide.to)
    }
    pub fn track_page_drag(
        &mut self,
        pages: &PageOrder,
        source: NotchPage,
        target: Option<NotchPage>,
        offset: f32,
        now: Instant,
    ) {
        self.page_drag = Some((source, target));
        self.lifted_page = Some(source);
        let mut preview = pages.clone();
        if let Some(target) = target {
            preview.move_page(source, target);
        }
        for (index, &page) in pages.enabled().iter().enumerate() {
            let to = if page == source {
                index as f32 + offset / PAGE_ROW_HEIGHT
            } else {
                preview
                    .enabled()
                    .iter()
                    .position(|item| *item == page)
                    .expect("preview keeps all enabled pages") as f32
            };
            let duration = if page == source {
                Duration::from_millis(65)
            } else {
                PAGE_SLIDE_DURATION
            };
            self.slide_page(page, index as f32, to, now, duration);
        }
    }
    pub fn settle_pages(&mut self, pages: &PageOrder, now: Instant) {
        if self.lifted_page.is_none() {
            return;
        }
        self.page_slides.retain(|slide| pages.contains(slide.page));
        for (index, &page) in pages.enabled().iter().enumerate() {
            self.slide_page(page, index as f32, index as f32, now, PAGE_SLIDE_DURATION);
        }
    }
    fn slide_page(
        &mut self,
        page: NotchPage,
        index: f32,
        to: f32,
        now: Instant,
        duration: Duration,
    ) {
        if let Some(slide) = self.page_slides.iter_mut().find(|slide| slide.page == page) {
            if slide.to != to {
                *slide = PageSlide {
                    page,
                    from: slide.position(now),
                    to,
                    started: now,
                    duration,
                };
            }
        } else if index != to {
            self.page_slides.push(PageSlide {
                page,
                from: index,
                to,
                started: now,
                duration,
            });
        }
    }
    fn page_position(&self, page: NotchPage, index: usize, now: Instant) -> f32 {
        self.page_slides
            .iter()
            .find(|slide| slide.page == page)
            .map_or(index as f32, |slide| slide.position(now))
    }
    pub fn load_drafts(&mut self, s: &AppSettings) {
        self.fields = Self::setting_fields(s);
    }
    fn setting_fields(s: &AppSettings) -> BTreeMap<Field, String> {
        BTreeMap::from([
            (Field::RestSeconds, s.rest_after_secs.to_string()),
            (Field::MaUrl, s.music_assistant.server_url.clone()),
            (Field::MaToken, s.music_assistant.token.clone()),
            (Field::MaQueue, s.music_assistant.queue_id.clone()),
            (Field::VoiceHost, s.wyoming.host.clone()),
            (Field::QuickShareName, s.quick_share.device_name.clone()),
            (Field::VoicePort, s.wyoming.port.to_string()),
            (Field::VoiceLanguage, s.wyoming.language.clone()),
            (
                Field::CountdownDate,
                s.desktop.widgets.countdown_date.clone(),
            ),
            (
                Field::CountdownName,
                s.desktop.widgets.countdown_name.clone(),
            ),
        ])
    }
    pub fn has_unsaved_fields(&self, current: &AppSettings) -> bool {
        Self::setting_fields(current).iter().any(|(key, saved)| {
            self.fields
                .get(key)
                .is_some_and(|draft| draft.trim() != saved)
        })
    }
    pub fn confirmed_settings(&self, current: &AppSettings) -> Result<AppSettings, PortError> {
        let mut next = current.clone();
        let get = |key| {
            self.fields.get(&key).map(String::as_str).ok_or_else(|| {
                PortError::new("settings.status.invalidNumber", "missing settings draft")
            })
        };
        next.rest_after_secs = get(Field::RestSeconds)?
            .trim()
            .parse()
            .map_err(|e| PortError::new("settings.status.invalidNumber", e))?;
        next.music_assistant.server_url = get(Field::MaUrl)?.trim().into();
        next.music_assistant.token = get(Field::MaToken)?.trim().into();
        next.music_assistant.queue_id = get(Field::MaQueue)?.trim().into();
        next.wyoming.host = get(Field::VoiceHost)?.trim().into();
        next.quick_share.device_name = get(Field::QuickShareName)?.trim().into();
        next.wyoming.port = get(Field::VoicePort)?
            .trim()
            .parse()
            .map_err(|e| PortError::new("settings.status.invalidNumber", e))?;
        next.wyoming.language = get(Field::VoiceLanguage)?.trim().into();
        next.desktop.widgets.countdown_date = get(Field::CountdownDate)?.trim().into();
        next.desktop.widgets.countdown_name = get(Field::CountdownName)?.trim().into();
        next.validate()?;
        Ok(next)
    }
}
macro_rules! preference_fields {
    (bool {$($b:ident => $($bf:ident).+),* $(,)?} number {$($n:ident => $($nf:ident).+),* $(,)?})=>{
        impl Preference {
            pub fn is_boolean(self)->bool {matches!(self,$(Self::$b)|*)}
            pub fn value(self,s:&AppSettings)->i32 {match self {$(Self::$b=>i32::from(s.$($bf).+),)*$(Self::$n=>i32::from(s.$($nf).+),)*Self::StatusRing=>s.desktop.status_ring as i32,Self::WidgetLeft=>s.desktop.widgets.left as i32,Self::WidgetRight=>s.desktop.widgets.right as i32,Self::Language=>match s.language{LanguagePreference::System=>0,LanguagePreference::English=>1,LanguagePreference::Spanish=>2}}}
            pub fn assign(self,current:&mut AppSettings,value:i32)->Result<(),PortError> {let mut s=current.clone();match self {$(Self::$b=>{let target=&mut s.$($bf).+; *target=match value{0=>false,1=>true,_=>return Err(PortError::new("settings.status.invalidNumber","expected boolean preference"))};},)*$(Self::$n=>{let target=&mut s.$($nf).+; *target=u16::try_from(value).map_err(|e|PortError::new("settings.status.invalidNumber",e))?;},)*Self::StatusRing=>s.desktop.status_ring=select(&StatusRing::ALL,value)?,Self::WidgetLeft=>s.desktop.widgets.left=select(&Widget::ALL,value)?,Self::WidgetRight=>s.desktop.widgets.right=select(&Widget::ALL,value)?,Self::Language=>s.language=match value{0=>LanguagePreference::System,1=>LanguagePreference::English,2=>LanguagePreference::Spanish,_=>return Err(PortError::new("settings.status.invalidNumber","invalid locale choice"))}}s.validate()?;*current=s;Ok(())}
        }
    }
}
fn select<T: Copy>(choices: &[T], index: i32) -> Result<T, PortError> {
    choices
        .get(index as usize)
        .copied()
        .ok_or_else(|| PortError::new("settings.status.invalidNumber", "invalid choice"))
}
preference_fields! {
    bool {Floating=>desktop.appearance.floating,Glass=>desktop.appearance.glass,MotionBlur=>desktop.appearance.motion_blur,MouseMove=>is_mouse_move_enabled,Startup=>desktop.start_with_windows,Minimal=>is_minimal_notch,Mascot=>is_mascot_enabled,Sounds=>reminders.sounds,HideFullscreen=>desktop.hide_in_fullscreen,OpenClick=>desktop.open_on_click,PageIcons=>desktop.page_icons,Fast=>desktop.fast_animations,Lyrics=>is_lyrics_enabled,MaEnabled=>music_assistant.is_enabled,VoiceEnabled=>wyoming.is_enabled,TaskStart=>reminders.task_start,TaskEnd=>reminders.task_end,Monday=>desktop.calendar_starts_monday,Week=>desktop.calendar_week_view,WaterEnabled=>water.enabled,EmptyShelf=>desktop.empty_shelf_on_start,HudVolume=>desktop.hud_volume,HudBrightness=>desktop.hud_brightness,HudCaps=>desktop.hud_caps_lock,HudLanguage=>desktop.hud_input_language,PopupBattery=>desktop.popup_battery,PopupWifi=>desktop.popup_wifi,PopupHeadphones=>desktop.popup_headphones,QuickShareVisible=>quick_share.visible}
    number {GlassTint=>desktop.appearance.tint_percent,FirstCity=>desktop.widgets.first_city,SecondCity=>desktop.widgets.second_city,Scale=>desktop.scale_percent,Snooze=>reminders.snooze_minutes,TaskMinutes=>reminders.task_minutes,FocusMinutes=>reminders.focus_minutes,ShortMinutes=>reminders.short_break_minutes,LongMinutes=>reminders.long_break_minutes,Cycles=>reminders.focus_cycles,Goal=>water.goal_ml,Bottle=>water.bottle_ml,From=>water.active_from_minute,Until=>water.active_until_minute}
}
impl Preference {
    pub fn display_value(self, value: i32) -> String {
        if matches!(self, Self::From | Self::Until) {
            format!("{:02}:{:02}", value / 60, value % 60)
        } else {
            value.to_string()
        }
    }
    pub fn parse_value(self, text: &str) -> Result<i32, PortError> {
        let invalid =
            || PortError::new("settings.status.invalidNumber", "invalid preference value");
        if matches!(self, Self::From | Self::Until) {
            let (hour, minute) = text.trim().split_once(':').ok_or_else(invalid)?;
            let hour = hour.parse::<i32>().map_err(|_| invalid())?;
            let minute = minute.parse::<i32>().map_err(|_| invalid())?;
            if !(0..60).contains(&minute)
                || !(0..=24).contains(&hour)
                || (hour == 24 && minute != 0)
            {
                return Err(invalid());
            }
            Ok(hour * 60 + minute)
        } else {
            text.trim().parse().map_err(|_| invalid())
        }
    }
}

pub const SECTIONS: [&str; 16] = [
    "nav-general",
    "nav-display",
    "nav-appearance",
    "nav-music",
    "nav-todo",
    "",
    "nav-pomodoro",
    "nav-water",
    "nav-shelf",
    "nav-hud",
    "nav-about",
    "nav-mouse-move",
    "",
    "nav-widgets",
    "nav-quick-share",
    "nav-pages",
];

pub(super) fn section_available(index: usize, settings: &AppSettings) -> bool {
    let page = match index {
        3 => NotchPage::Music,
        4 => NotchPage::Todo,
        6 => NotchPage::Pomodoro,
        7 => NotchPage::Water,
        8 => NotchPage::Shelf,
        WIDGETS => NotchPage::Widgets,
        QUICK_SHARE => NotchPage::QuickShare,
        _ => return index == ABOUT || NAV_SECTIONS.contains(&index),
    };
    settings.desktop.pages.contains(page)
}
struct Form<'a> {
    scene: &'a mut Scene,
    settings: &'a AppSettings,
    ui: &'a SettingsUi,
    now: Instant,
    x: f32,
    y: f32,
    width: f32,
    ink: u32,
    dim: u32,
    card: u32,
    group: Option<(f32, usize)>,
}
impl Form<'_> {
    fn finish_group(&mut self) {
        if let Some((top, index)) = self.group.take() {
            let height = (self.y - top).max(0.);
            self.scene.paint.insert(
                index,
                Paint::Round(Rect::new(self.x, top, self.width, height), 14., self.card),
            );
            self.y += 5.;
        }
    }
    fn heading(&mut self, key: &str) {
        self.finish_group();
        self.scene.text(
            Rect::new(self.x, self.y + 3., self.width, 18.),
            tr(key),
            10.,
            0x99999c,
            false,
            false,
        );
        self.y += 23.;
        self.group = Some((self.y, self.scene.paint.len()));
    }

    fn page_row(&mut self, page: NotchPage, position: Option<usize>) {
        let on = position.is_some();
        let row = Rect::new(self.x + 8., self.y, self.width - 16., PAGE_ROW_HEIGHT);
        if self.ui.lifted_page != Some(page) {
            let y = position.map_or(self.y, |index| {
                self.y
                    + (self.ui.page_position(page, index, self.now) - index as f32)
                        * PAGE_ROW_HEIGHT
            });
            self.paint_page_row(page, y, on, false);
        }
        if let Some(index) = position
            && let Some((source, target)) = self.ui.page_drag
        {
            if page == source && self.ui.lifted_page.is_none() {
                self.scene.alpha_round(row, 6., 0x46a8ff, 0.18);
            } else if page != source && target == Some(page) {
                let pages = self.settings.desktop.pages.enabled();
                let after = pages[..index].contains(&source);
                let y = if after {
                    row.y + row.h - 2.
                } else {
                    row.y + 1.
                };
                self.scene.line(row.x, y, row.x + row.w, y, 0x46a8ff, 2.);
            }
        }
        let label = tr(super::views::page_appearance(page).1);
        if on {
            self.scene.hit(
                row,
                Action::ReorderPage(page),
                format!("{} {label}", tr("pages-reorder")),
            );
        }
        self.scene
            .hit(self.page_switch(self.y), Action::TogglePage(page), &label);
        self.y += PAGE_ROW_HEIGHT;
        self.separator();
    }

    fn page_switch(&self, y: f32) -> Rect {
        Rect::new(self.x + self.width - 52., y + 5., 38., 22.)
    }

    fn paint_page_row(&mut self, page: NotchPage, y: f32, on: bool, lifted: bool) {
        let (glyph, label, accent) = super::views::page_appearance(page);
        if lifted {
            let row = Rect::new(self.x + 7., y + 1., self.width - 14., PAGE_ROW_HEIGHT - 2.);
            self.scene.round(row, 6., 0x303a49);
            self.scene.border(row, 6., 0x559bdb, 1.);
        }
        let badge = Rect::new(self.x + 14., y + 6., 22., 20.);
        self.scene
            .alpha_round(badge, 5., accent, if on { 0.24 } else { 0.12 });
        self.scene
            .icon(badge.inset(4.), glyph, if on { accent } else { self.dim });
        self.scene.text(
            Rect::new(
                self.x + 45.,
                y,
                (self.width - 132.).max(32.),
                PAGE_ROW_HEIGHT,
            ),
            tr(label),
            12.,
            if on { self.ink } else { self.dim },
            false,
            false,
        );
        let area = self.page_switch(y);
        if on {
            for column in 0..2 {
                for dot in 0..3 {
                    self.scene.round(
                        Rect::new(
                            area.x - 19. + column as f32 * 5.,
                            y + 10. + dot as f32 * 5.,
                            2.5,
                            2.5,
                        ),
                        1.25,
                        self.dim,
                    );
                }
            }
        }
        draw_switch(
            self.scene,
            area,
            self.ui
                .switch_position(&Action::TogglePage(page), on, self.now),
        );
    }
    fn separator(&mut self) {
        self.scene.line(
            self.x + 14.,
            self.y - 1.,
            self.x + self.width - 14.,
            self.y - 1.,
            0x343437,
            1.,
        );
    }
    fn toggle(&mut self, key: Preference, label: &str, help: Option<&str>) {
        let height = if help.is_some() { 50. } else { 40. };
        self.scene.text(
            Rect::new(
                self.x + 14.,
                self.y + if help.is_some() { 8. } else { 0. },
                self.width - 82.,
                if help.is_some() { 18. } else { height },
            ),
            label,
            12.,
            0xf4f4f5,
            false,
            false,
        );
        if let Some(help) = help {
            self.scene.text(
                Rect::new(self.x + 14., self.y + 26., self.width - 82., 18.),
                help,
                10.,
                0x98989c,
                false,
                false,
            );
        }
        let r = Rect::new(
            self.x + self.width - 52.,
            self.y + (height - 22.) / 2.,
            38.,
            22.,
        );
        let on = key.value(self.settings) != 0;
        draw_switch(
            self.scene,
            r,
            self.ui
                .switch_position(&Action::Preference(key, 0), on, self.now),
        );
        self.scene.hit(
            Rect::new(self.x, self.y, self.width, height),
            Action::Preference(key, i32::from(!on)),
            label,
        );
        self.y += height;
    }
    fn choices(&mut self, key: Preference, label: &str, items: &[(String, i32)]) {
        let scale = key == Preference::Scale;
        let height = if scale { 72. } else { 40. };
        let width = if scale {
            self.width - 28.
        } else {
            match key {
                Preference::OpenClick => 190.,
                Preference::Language => 180.,
                _ => 150.,
            }
        };
        let x = if scale {
            self.x + 14.
        } else {
            self.x + self.width - width - 14.
        };
        let top = self.y + if scale { 46. } else { 9. };
        self.scene.text(
            Rect::new(
                self.x + 14.,
                self.y + if scale { 8. } else { 0. },
                if scale {
                    self.width - 28.
                } else {
                    self.width - width - 30.
                },
                if scale { 18. } else { height },
            ),
            label,
            12.,
            0xf4f4f5,
            false,
            false,
        );
        if scale {
            self.scene.text(
                Rect::new(self.x + 14., self.y + 25., self.width - 28., 18.),
                tr("size-help"),
                10.,
                0x99999e,
                false,
                false,
            );
        }
        self.scene
            .round(Rect::new(x, top, width, 22.), 7., 0x39393c);
        let item_width = (width - 4.) / items.len() as f32;
        for (i, (name, value)) in items.iter().enumerate() {
            let r = Rect::new(x + 2. + i as f32 * item_width, top + 2., item_width, 18.);
            let selected = key.value(self.settings) == *value;
            if selected {
                self.scene.round(r, 5., 0x66666a);
            }
            self.scene.text(
                r,
                name,
                10.,
                if selected { 0xffffff } else { 0xb9b9bc },
                false,
                true,
            );
            self.scene.hit(
                r,
                Action::Preference(key, *value),
                format!("{label} {name}"),
            );
        }
        self.y += height;
    }
    fn number(&mut self, key: Preference, label: &str, step: i32) {
        self.scene.text(
            Rect::new(self.x + 14., self.y, self.width - 160., 40.),
            label,
            12.,
            self.ink,
            false,
            false,
        );
        let value = key.value(self.settings);
        let control = Rect::new(self.x + self.width - 142., self.y + 5., 128., 32.);
        self.scene.round(control, 4., 0x333336);
        self.scene.border(control, 4., 0x444446, 1.);
        for (i, (n, glyph)) in [(value + step, "spin-up"), (value - step, "spin-down")]
            .into_iter()
            .enumerate()
        {
            let r = Rect::new(control.x + 66. + i as f32 * 32., control.y + 4., 28., 24.);
            let enabled = (1..=i32::from(u16::MAX)).contains(&n);
            self.scene.round(r, 3., 0x333336);
            self.scene.icon(
                Rect::new(r.x + 8., r.y + 6., 12., 12.),
                glyph,
                if enabled { 0xd1d1d3 } else { 0x77777a },
            );
            self.scene.hit(r, Action::Preference(key, n), label);
            if let Some(hit) = self.scene.hits.last_mut() {
                hit.enabled = enabled;
            }
        }
        self.scene.input(
            Field::Preference(key),
            Rect::new(control.x + 12., control.y + 6., 50., 20.),
            &self
                .ui
                .fields
                .get(&Field::Preference(key))
                .cloned()
                .unwrap_or_else(|| key.display_value(value)),
            false,
            false,
            label,
        );
        self.y += 42.;
    }
    fn step(&mut self, key: Preference, label: &str, step: i32) {
        self.scene.text(
            Rect::new(self.x + 14., self.y, self.width - 178., 40.),
            label,
            12.,
            0xf1f1f3,
            false,
            false,
        );
        let value = key.value(self.settings);
        let (minimum, maximum) = match key {
            Preference::Bottle => (50, 65500),
            Preference::GlassTint => (0, 100),
            Preference::FirstCity | Preference::SecondCity => (0, CITIES.len() as i32 - 1),
            Preference::From => (0, Preference::Until.value(self.settings) - 15),
            Preference::Until => (Preference::From.value(self.settings) + 15, 1440),
            _ => unreachable!("step control preference"),
        };
        for (i, direction) in [-1, 1].iter().enumerate() {
            let next = value + direction * step;
            let enabled = (minimum..=maximum).contains(&next)
                && (key != Preference::GlassTint || self.settings.desktop.appearance.glass);
            let r = Rect::new(
                self.x + self.width - 132. + i as f32 * 96.,
                self.y + 9.,
                22.,
                22.,
            );
            self.scene.round(r, 11., 0x3e3e42);
            self.scene.text(
                r,
                if *direction < 0 { "−" } else { "+" },
                17.,
                if enabled { 0xf1f1f3 } else { 0x77777a },
                false,
                true,
            );
            self.scene.hit(r, Action::Preference(key, next), label);
            if let Some(hit) = self.scene.hits.last_mut() {
                hit.enabled = enabled;
            }
        }
        self.scene.text(
            Rect::new(self.x + self.width - 106., self.y, 66., 40.),
            match key {
                Preference::From | Preference::Until => key.display_value(value),
                Preference::GlassTint => format!("{value}%"),
                Preference::FirstCity | Preference::SecondCity => {
                    CITIES[value as usize].name.into()
                }
                _ => format!("{value} ml"),
            },
            11.,
            0xf1f1f3,
            false,
            true,
        );
        self.y += 40.;
        self.separator();
    }
    fn tiles(
        &mut self,
        key: Preference,
        items: &[(&str, &str)],
        columns: usize,
        preview_height: f32,
        mut preview: impl FnMut(&mut Scene, Rect, usize),
    ) {
        let gap = 8.;
        let tile_width = (self.width - 24. - (columns - 1) as f32 * gap) / columns as f32;
        let tile_height = preview_height
            + if items.iter().any(|(_, help)| !help.is_empty()) {
                42.
            } else {
                32.
            };
        self.y += 8.;
        for (i, (label, help)) in items.iter().enumerate() {
            let tile = Rect::new(
                self.x + 12. + (i % columns) as f32 * (tile_width + gap),
                self.y + (i / columns) as f32 * (tile_height + gap),
                tile_width,
                tile_height,
            );
            let selected = key.value(self.settings) == i as i32;
            self.scene
                .round(tile, 10., if selected { 0x38383d } else { 0x313134 });
            if selected {
                self.scene.border(tile.inset(0.5), 10., 0x168cff, 1.5);
            }
            let frame = Rect::new(tile.x + 7., tile.y + 7., tile.w - 14., preview_height - 6.);
            self.scene.round(frame, 6., 0x242733);
            preview(self.scene, frame, i);
            if selected {
                let badge = Rect::new(
                    tile.x + tile.w - 21.,
                    tile.y + preview_height + 4.,
                    13.,
                    13.,
                );
                self.scene.round(badge, 6.5, 0x168cff);
                self.scene.icon(badge.inset(2.), "\u{e73e}", 0xffffff);
            }
            self.scene.text(
                Rect::new(
                    tile.x + 8.,
                    tile.y + preview_height + 2.,
                    tile.w - if selected { 29. } else { 16. },
                    17.,
                ),
                tr(label),
                11.,
                self.ink,
                true,
                false,
            );
            if !help.is_empty() {
                self.scene.text(
                    Rect::new(
                        tile.x + 8.,
                        tile.y + preview_height + 18.,
                        tile.w - 16.,
                        14.,
                    ),
                    tr(help),
                    9.,
                    self.dim,
                    false,
                    false,
                );
            }
            self.scene
                .hit(tile, Action::Preference(key, i as i32), tr(label));
        }
        self.y += items.len().div_ceil(columns) as f32 * (tile_height + gap);
    }

    fn field(&mut self, key: Field, label: &str, secret: bool) {
        self.scene.text(
            Rect::new(self.x + 14., self.y + 5., self.width - 28., 18.),
            label,
            10.,
            0xa3a3a8,
            false,
            false,
        );
        let r = Rect::new(self.x + 14., self.y + 26., self.width - 28., 31.);
        self.scene.round(r, 7., 0x333336);
        self.scene.input(
            key,
            Rect::new(r.x + 9., r.y + 5., r.w - 18., 21.),
            self.ui.fields.get(&key).map_or("", String::as_str),
            secret,
            false,
            label,
        );
        self.y += 67.;
    }
    fn text(&mut self, text: &str) {
        self.scene.text(
            Rect::new(self.x + 8., self.y, self.width - 16., 38.),
            text,
            11.,
            self.dim,
            false,
            false,
        );
        self.y += 42.;
    }

    fn notices(&mut self, page: NoticesPage, content: Rect) {
        match page {
            NoticesPage::Index => {
                let row_height = 42.;
                for (index, component) in COMPONENTS.iter().enumerate() {
                    let row = Rect::new(self.x, self.y, self.width, row_height - 4.);
                    self.y += row_height;
                    if row.y + row.h <= content.y || row.y >= content.y + content.h {
                        continue;
                    }
                    let title = format!("{} {}", component.name, component.version);
                    self.scene.round(row, 7., self.card);
                    self.scene.text(
                        Rect::new(row.x + 10., row.y + 2., row.w - 20., 18.),
                        &title,
                        12.,
                        self.ink,
                        true,
                        false,
                    );
                    self.scene.text(
                        Rect::new(row.x + 10., row.y + 19., row.w - 20., 16.),
                        component.license,
                        10.,
                        self.dim,
                        false,
                        false,
                    );
                    self.scene.hit(
                        row,
                        Action::OpenNotice(index),
                        format!("{title} · {}", component.license),
                    );
                }
            }
            NoticesPage::Component { index, .. } => {
                let component = &COMPONENTS[index];
                let title = format!("{} {}", component.name, component.version);
                self.scene.text(
                    Rect::new(self.x, self.y, self.width, 25.),
                    &title,
                    15.,
                    self.ink,
                    true,
                    false,
                );
                self.y += 25.;
                self.text(component.license);
                let reader = Rect::new(
                    self.x,
                    self.y,
                    self.width,
                    content.y + content.h - self.y - 8.,
                );
                self.scene.round(reader, 7., 0x333336);
                self.scene.input(
                    Field::ThirdPartyNotice(index),
                    reader.inset(7.),
                    component.text,
                    false,
                    true,
                    &title,
                );
            }
        }
    }

    fn task_api(&mut self) {
        self.heading("task-api");
        if let Some(error) = self.ui.api_error {
            self.text(&crate::i18n::translate(error));
            return;
        }
        self.text(&format!(
            "http://{}",
            crate::infrastructure::task_api::ADDRESS
        ));
        let (key, action) = if self.ui.issued_token.is_some() {
            ("task-api-copy", Action::CopyTaskToken)
        } else {
            ("task-api-generate", Action::GenerateTaskToken)
        };
        self.scene.button(
            Rect::new(self.x + 14., self.y, self.width - 28., 28.),
            tr(key),
            "",
            action,
            0x343437,
        );
        self.y += 34.;
        if let Some((key, error)) = self.ui.api_notice {
            self.scene.paint.push(Paint::Paragraph(
                Rect::new(self.x + 14., self.y, self.width - 28., 31.),
                crate::i18n::translate(key),
                10.,
                if error { 0xff9292 } else { self.dim },
            ));
            self.y += 36.;
        }
        for token in &self.ui.api_tokens {
            let id = token.id.simple().to_string();
            let date = token
                .created_at
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d");
            self.separator();
            self.scene.text(
                Rect::new(self.x + 14., self.y, self.width - 128., 36.),
                format!("Token {} · {date}", &id[..8]),
                11.,
                self.ink,
                false,
                false,
            );
            self.scene.button(
                Rect::new(self.x + self.width - 102., self.y + 6., 88., 24.),
                tr("task-api-revoke"),
                "",
                Action::RevokeTaskToken(token.id),
                0x493134,
            );
            self.y += 36.;
        }
        self.y += 6.;
    }
}
fn clock_preview(scene: &mut Scene, frame: Rect, mode: Option<StatusRing>, offset: f32) {
    let width = frame.w.min(73.);
    let r = Rect::new(
        frame.x + (frame.w - width) / 2.,
        frame.y + offset,
        width,
        20.,
    );
    scene.round(r, 9., 0x08090c);
    if let Some(mode) = mode {
        super::widgets::ring(
            scene,
            Rect::new(r.x + 7., r.y + 5., 10., 10.),
            mode,
            0.72,
            true,
        );
    }
    let left = if mode == Some(StatusRing::Off) {
        0.
    } else {
        13.
    };
    scene.text(
        Rect::new(r.x + left, r.y, r.w - left, r.h),
        "10:08",
        9.,
        0xffffff,
        true,
        true,
    );
}

fn draw_switch(scene: &mut Scene, r: Rect, position: f32) {
    scene.round(r, 11., 0x49494d);
    scene.alpha_round(r, 11., 0x24c856, position);
    scene.round(
        Rect::new(r.x + 2. + 16. * position, r.y + 2., 18., 18.),
        9.,
        0xffffff,
    );
}

pub fn settings_scene(
    m: &UiModel,
    ui: &SettingsUi,
    displays: &[Display],
    width: f32,
    height: f32,
    now: Instant,
) -> (Scene, f32) {
    let mut scene = Scene::default();
    let bg = 0x1d1d1f;
    let side = 0x151517;
    let ink = 0xededf0;
    let dim = 0x99999c;
    let card = 0x272729;
    scene.round(Rect::new(0., 0., width, height), 13., bg);
    scene.border(
        Rect::new(0.5, 0.5, width - 1., height - 1.),
        13.,
        0x39393c,
        1.,
    );
    scene.round(Rect::new(1., 1., 192., height - 2.), 0., side);
    scene.round(Rect::new(14., 14., 12., 12.), 6., 0xff5c5c);
    scene.hit(Rect::new(8., 8., 25., 25.), Action::CloseSettings, "Close");
    let colors = [
        0x8c8c91, 0x6760ec, 0x88888c, 0xff3d68, 0x23c451, 0xff5148, 0xff7a27, 0x078bff, 0xffb026,
        0x30b3cd, 0x858589, 0x6c9bed, 0x23c451, 0x168cff, 0x4385f5, 0x28a8cf,
    ];
    scene.line(
        10.,
        height - NAV_FOOTER,
        182.,
        height - NAV_FOOTER,
        0x2c2c30,
        1.,
    );
    let order = NAV_SECTIONS
        .into_iter()
        .filter(|index| section_available(*index, &m.settings))
        .chain(std::iter::once(ABOUT));
    for (ordinal, i) in order.enumerate() {
        let label = SECTIONS[i];
        let y = if i == ABOUT {
            height - 44.
        } else {
            NAV_TOP + ordinal as f32 * NAV_STEP
        };
        let item_height = NAV_ROW_HEIGHT;
        let row = Rect::new(10., y, 172., item_height);
        if i == ui.section {
            scene.round(row, 8., 0x2b2b2e);
        } else if row.contains(ui.mouse.0, ui.mouse.1) {
            scene.round(row, 8., 0x212124);
        }
        let icon_size = (item_height - 6.).min(22.);
        let icon_y = y + (item_height - icon_size) / 2.;
        scene.round(Rect::new(18., icon_y, icon_size, icon_size), 6., colors[i]);
        let icon = Rect::new(20., icon_y + 2., icon_size - 4., icon_size - 4.);
        match i {
            WIDGETS => scene.icon(icon, super::widgets::WIDGETS_ICON, 0xffffff),
            QUICK_SHARE => scene.icon(icon, "quick-share-radar", 0xffffff),
            PAGES => scene.icon(icon, super::views::TODO, 0xffffff),
            11 => scene.icon(icon, "mouse-move", 0xffffff),
            _ => scene.icon(icon, &format!("settings-{i}"), 0xffffff),
        }
        scene.text(
            Rect::new(50., y, 128., item_height),
            tr(label),
            12.,
            ink,
            false,
            false,
        );
        scene.hit(row, Action::Section(i), tr(label));
    }
    let (title, header_space) = if ui.notices.is_some() {
        scene.button(
            Rect::new(width - 119., 20., 96., 25.),
            tr("about-notices-back"),
            "",
            Action::BackNotices,
            0x343437,
        );
        (tr("about-third-party"), 108.)
    } else {
        (tr(SECTIONS[ui.section]), 0.)
    };
    scene.text(
        Rect::new(214., 19., width - 238. - header_space, 27.),
        title,
        18.,
        ink,
        true,
        false,
    );
    let manual_save = matches!(ui.section, 2..=4 | WIDGETS | QUICK_SHARE);
    let content = Rect::new(
        214.,
        57.,
        width - 236.,
        height - if manual_save { 99. } else { 67. },
    );
    scene.clip(content);
    let t = (now.duration_since(ui.changed).as_secs_f32() / 0.22).clamp(0., 1.);
    scene.paint.push(Paint::Opacity(t));
    let mut f = Form {
        scene: &mut scene,
        settings: &m.settings,
        ui,
        now,
        x: content.x,
        y: content.y - ui.scroll + (1. - t) * 5.,
        width: content.w,
        ink,
        dim,
        card,
        group: None,
    };
    use Preference as P;
    match ui.section {
        0 => {
            f.heading("section-startup");
            f.toggle(P::Startup, &tr("start-windows"), None);
            f.heading("section-notch");
            f.choices(
                P::Scale,
                &tr("size"),
                &[
                    (tr("auto"), 0),
                    ("100%".into(), 100),
                    ("115%".into(), 115),
                    ("130%".into(), 130),
                    ("150%".into(), 150),
                ],
            );
            f.toggle(
                P::Minimal,
                &crate::i18n::translate("settings.general.minimalNotchLabel"),
                Some(&tr("minimal-help")),
            );
            f.separator();
            f.toggle(
                P::Mascot,
                &crate::i18n::translate("settings.general.mascotLabel"),
                Some(&tr("mascot-help")),
            );
            f.heading("section-sounds");
            f.toggle(P::Sounds, &tr("reminder-sounds"), Some(&tr("sounds-help")));
            f.heading("section-fullscreen");
            f.toggle(
                P::HideFullscreen,
                &tr("hide-fullscreen"),
                Some(&tr("fullscreen-help")),
            );
        }
        1 => {
            f.heading("ring-next-clock");
            f.tiles(
                P::StatusRing,
                &[
                    ("auto", "ring-auto-help"),
                    ("nav-water", "ring-water-help"),
                    ("nav-todo", "ring-todo-help"),
                    ("off", "ring-off-help"),
                ],
                4,
                68.,
                |scene, frame, i| clock_preview(scene, frame, Some(StatusRing::ALL[i]), 7.),
            );
            f.heading("section-notch");
            let top = f.y;
            f.scene.text(
                Rect::new(f.x + 14., top + 12., f.width - 28., 18.),
                tr("nav-display"),
                12.,
                0xf4f4f5,
                false,
                false,
            );
            f.scene.text(
                Rect::new(f.x + 14., top + 30., f.width - 28., 18.),
                tr("display-help"),
                10.,
                0x99999e,
                false,
                false,
            );
            let area = Rect::new(f.x + 17., top + 60., f.width - 34., 132.);
            for (display, r) in display_map(displays, area) {
                let number = display
                    .id
                    .strip_prefix(r"\\.\DISPLAY")
                    .unwrap_or(&display.label);
                let caption = format!(
                    "{}×{}{}",
                    display.rect.w as i32,
                    display.rect.h as i32,
                    if display.primary {
                        format!(" · {}", tr("primary"))
                    } else {
                        String::new()
                    }
                );
                let selected = m.settings.desktop.display_id == display.id
                    || (m.settings.desktop.display_id.is_empty() && display.primary);
                let radius = (r.w.min(r.h) * 0.08).min(7.);
                let card = r.inset(0.5);
                f.scene
                    .round(card, radius, if selected { 0x24372d } else { 0x303033 });
                if selected {
                    f.scene.border(card, radius, 0x277e42, 1.);
                    if r.w > 25. && r.h > 32. {
                        let w = (r.w / 4.).min(30.);
                        f.scene.round(
                            Rect::new(r.x + (r.w - w) / 2., r.y + 5., w, 4.),
                            2.,
                            0x16241c,
                        );
                    }
                }
                let show_caption = r.w >= 92. && r.h >= 54.;
                if r.w >= 14. && r.h >= 18. {
                    let center = r.y + r.h / 2.;
                    let height = r.h.min(24.);
                    let y = center - height / 2. - if show_caption { 8. } else { 0. };
                    let font_size = (r.h * 0.17).clamp(10., 18.);
                    f.scene.text(
                        Rect::new(r.x, y, r.w, height),
                        number,
                        font_size,
                        0xdedee0,
                        true,
                        true,
                    );
                    if show_caption {
                        f.scene.text(
                            Rect::new(r.x, center + 5., r.w, 16.),
                            &caption,
                            10.,
                            0x99999e,
                            false,
                            true,
                        );
                    }
                }
                f.scene.hit(
                    r,
                    Action::Display(display.id.clone()),
                    format!("{number} · {caption}"),
                );
            }
            let auto = Rect::new(f.x + 14., top + 201., 90., 18.);
            f.scene.text(auto, tr("auto"), 10., 0x9ad9aa, false, false);
            f.scene
                .hit(auto, Action::Display(String::new()), tr("auto"));
            f.y += 226.;
        }
        2 => {
            f.heading("notch-style");
            f.tiles(
                P::Floating,
                &[
                    ("notch-classic", "notch-classic-help"),
                    ("notch-floating", "notch-floating-help"),
                ],
                2,
                62.,
                |scene, frame, i| {
                    if i == 0 {
                        scene.round(Rect::new(frame.x, frame.y, frame.w, 2.), 1., 0x464a56);
                    }
                    clock_preview(
                        scene,
                        frame,
                        Some(StatusRing::Auto),
                        if i == 0 { 0. } else { 8. },
                    );
                },
            );
            f.separator();
            f.toggle(P::Glass, &tr("notch-glass"), Some(&tr("notch-glass-help")));
            f.step(P::GlassTint, &tr("notch-tint"), 5);
            f.heading("section-notch");
            f.choices(
                P::OpenClick,
                &tr("open-notch"),
                &[(tr("hover"), 0), (tr("click"), 1)],
            );
            f.toggle(
                P::PageIcons,
                &tr("page-icons"),
                Some(&tr("page-icons-help")),
            );
            f.heading("section-motion");
            f.choices(
                P::Fast,
                &tr("animation-speed"),
                &[(tr("normal"), 0), (tr("fast"), 1)],
            );
            f.toggle(
                P::MotionBlur,
                &tr("motion-blur"),
                Some(&tr("motion-blur-help")),
            );
            f.heading("section-options");
            f.choices(
                P::Language,
                &crate::i18n::translate("settings.general.languageLabel"),
                &[
                    (crate::i18n::translate("settings.general.languageSystem"), 0),
                    ("English".into(), 1),
                    ("Español".into(), 2),
                ],
            );
            f.field(
                Field::RestSeconds,
                &crate::i18n::translate("settings.general.restAfterLabel"),
                false,
            );
        }
        3 => {
            f.heading("nav-music");
            f.toggle(P::Lyrics, &tr("show-lyrics"), Some(&tr("lyrics-help")));
            f.text(&tr("source-priority"));
            f.text(&tr("master-volume"));
            f.heading("section-connection");
            f.toggle(
                P::MaEnabled,
                &crate::i18n::translate("settings.musicAssistant.enabledLabel"),
                None,
            );
            for (key, label, secret) in [
                (
                    Field::MaUrl,
                    "settings.musicAssistant.serverUrlLabel",
                    false,
                ),
                (Field::MaToken, "settings.musicAssistant.tokenLabel", true),
                (
                    Field::MaQueue,
                    "settings.musicAssistant.queueIdLabel",
                    false,
                ),
            ] {
                f.field(key, &crate::i18n::translate(label), secret);
            }
        }
        4 => {
            f.heading("section-reminders");
            f.toggle(P::TaskStart, &tr("task-start"), None);
            f.toggle(P::TaskEnd, &tr("task-end"), None);
            f.choices(
                P::Snooze,
                &tr("snooze"),
                &[
                    ("5 min".into(), 5),
                    ("10 min".into(), 10),
                    ("15 min".into(), 15),
                ],
            );
            f.number(P::TaskMinutes, &tr("task-duration"), 1);
            f.heading("nav-calendar");
            f.choices(
                P::Monday,
                &tr("week-starts"),
                &[(tr("sunday"), 0), (tr("monday"), 1)],
            );
            f.choices(
                P::Week,
                &tr("default-view"),
                &[(tr("month"), 0), (tr("week"), 1)],
            );
            f.separator();
            let preview = Rect::new(f.x, f.y, f.width, 198.);
            views::draw_calendar(f.scene, m, preview, false);
            f.y += preview.h;
            f.task_api();
            f.heading("section-connection");
            f.toggle(
                P::VoiceEnabled,
                &crate::i18n::translate("settings.wyoming.enabledLabel"),
                None,
            );
            for (key, label) in [
                (Field::VoiceHost, "settings.wyoming.hostLabel"),
                (Field::VoicePort, "settings.wyoming.portLabel"),
                (Field::VoiceLanguage, "settings.wyoming.languageLabel"),
            ] {
                f.field(key, &crate::i18n::translate(label), false);
            }
        }
        6 => {
            f.heading("section-options");
            for (key, label) in [
                (P::FocusMinutes, "focus-duration"),
                (P::ShortMinutes, "short-break"),
                (P::LongMinutes, "long-break"),
                (P::Cycles, "focus-cycles"),
            ] {
                f.number(key, &tr(label), 1);
            }
            f.finish_group();
            f.scene.text(
                Rect::new(f.x, f.y + 12., f.width, 18.),
                tr(match m.utility.as_ref().map(|u| u.phase) {
                    Some(crate::application::reminders::FocusPhase::ShortBreak) => "short-break",
                    Some(crate::application::reminders::FocusPhase::LongBreak) => "long-break",
                    _ => "focus-time",
                }),
                11.,
                0x99999e,
                false,
                true,
            );
            f.scene.text(
                Rect::new(f.x, f.y + 31., f.width, 30.),
                super::model::format_duration(std::time::Duration::from_secs(
                    m.utility
                        .as_ref()
                        .map_or(u64::from(m.settings.reminders.focus_minutes) * 60, |u| {
                            u.focus_seconds
                        }),
                )),
                26.,
                0xf4f4f5,
                false,
                true,
            );
            let center = f.x + f.width / 2.;
            f.scene.button(
                Rect::new(center - 78., f.y + 70., 74., 24.),
                tr(if m.focus_running() { "pause" } else { "start" }),
                "",
                Action::Intent(crate::application::events::UserIntent::ToggleFocus),
                0x212121,
            );
            f.scene.button(
                Rect::new(center + 4., f.y + 70., 74., 24.),
                tr("reset"),
                "",
                Action::Intent(crate::application::events::UserIntent::ResetFocus),
                0x212121,
            );
            f.y += 95.;
        }
        7 => {
            f.heading("section-reminders");
            f.toggle(
                P::WaterEnabled,
                &tr("water-enable"),
                Some(&tr("water-spread-help")),
            );
            f.heading("water-plan");
            f.choices(
                P::Goal,
                &tr("water-daily-goal"),
                &[
                    ("1.5 L".into(), 1500),
                    ("2 L".into(), 2000),
                    ("2.5 L".into(), 2500),
                    ("3 L".into(), 3000),
                ],
            );
            f.step(P::Bottle, &tr("water-bottle"), 50);
            for (key, label) in [(P::From, "water-from"), (P::Until, "water-until")] {
                f.step(key, &tr(label), 15);
            }
            f.heading("today");
            f.scene.round(Rect::new(f.x, f.y, f.width, 40.), 12., card);
            f.scene.text(
                Rect::new(f.x + 14., f.y, f.width - 190., 40.),
                tr("water-progress-today"),
                12.,
                ink,
                false,
                false,
            );
            f.scene.text(
                Rect::new(f.x + f.width - 176., f.y, 90., 40.),
                format!(
                    "{:.1} / {:.1} L",
                    m.water
                        .as_ref()
                        .map_or(0., |w| w.consumed_ml as f64 / 1000.),
                    f64::from(m.settings.water.goal_ml) / 1000.
                ),
                11.,
                dim,
                false,
                true,
            );
            let reset = Rect::new(f.x + f.width - 88., f.y, 75., 40.);
            f.scene.text(reset, tr("reset"), 11., 0xff635e, false, true);
            f.scene.hit(
                reset,
                Action::Intent(crate::application::events::UserIntent::ResetWater),
                tr("reset"),
            );
            f.y += 40.;
        }
        8 => {
            f.heading("nav-shelf");
            f.toggle(
                P::EmptyShelf,
                &tr("empty-shelf"),
                Some(&tr("empty-shelf-help")),
            );
        }
        9 => {
            f.heading("section-indicators");
            for (key, label) in [
                (P::HudVolume, "volume"),
                (P::HudBrightness, "brightness"),
                (P::HudCaps, "caps-lock"),
                (P::HudLanguage, "input-language"),
            ] {
                f.toggle(key, &tr(label), None);
            }
            f.heading("section-popups");
            if m.settings.desktop.pages.contains(NotchPage::Charging) {
                f.toggle(P::PopupBattery, &tr("battery"), None);
            }
            f.toggle(P::PopupWifi, &tr("wifi"), None);
            if m.settings.desktop.pages.contains(NotchPage::Headphones) {
                f.toggle(P::PopupHeadphones, &tr("headset"), None);
            }
        }
        11 => {
            f.heading("nav-mouse-move");
            let help = crate::i18n::translate_with(
                "desktopUi.mouse-move-help",
                &[(
                    "seconds",
                    &crate::application::mouse_mover::MOUSE_MOVE_INTERVAL
                        .as_secs()
                        .to_string(),
                )],
            );
            f.toggle(P::MouseMove, &tr("mouse-move-enable"), Some(&help));
            if let Some(error) = m.mouse_move_error {
                f.scene.paint.push(Paint::Paragraph(
                    Rect::new(f.x + 8., f.y + 8., f.width - 16., 54.),
                    crate::i18n::translate(error),
                    11.,
                    0xff9292,
                ));
                f.y += 70.;
            }
        }
        WIDGETS => {
            f.heading("widgets-choose");
            for (i, label) in ["widgets-left", "widgets-right"].into_iter().enumerate() {
                let r = Rect::new(f.x + 14. + i as f32 * 71., f.y + 6., 65., 22.);
                let right = i == 1;
                f.scene.round(
                    r,
                    11.,
                    if ui.widget_right == right {
                        0x245888
                    } else {
                        0x333336
                    },
                );
                f.scene.text(r, tr(label), 11., ink, false, true);
                f.scene.hit(r, Action::WidgetSlot(right), tr(label));
            }
            f.y += 30.;
            let items = Widget::ALL.map(|widget| (super::widgets::label(widget), ""));
            f.tiles(
                if ui.widget_right {
                    P::WidgetRight
                } else {
                    P::WidgetLeft
                },
                &items,
                4,
                56.,
                |scene, r, i| {
                    let widget = Widget::ALL[i];
                    super::widgets::preview(scene, r, widget, m);
                    for (slot, selected, name) in [
                        (0, m.settings.desktop.widgets.left, "L"),
                        (1, m.settings.desktop.widgets.right, "R"),
                    ] {
                        if selected == widget {
                            let badge =
                                Rect::new(r.x + r.w - 12. - slot as f32 * 13., r.y, 12., 12.);
                            scene.round(badge, 6., 0x245888);
                            scene.text(badge, name, 8., 0xffffff, true, true);
                        }
                    }
                },
            );
            f.heading("section-options");
            f.step(P::FirstCity, &tr("widgets-city-first"), 1);
            f.step(P::SecondCity, &tr("widgets-city-second"), 1);
            f.field(Field::CountdownName, &tr("widgets-event"), false);
            f.field(Field::CountdownDate, &tr("widgets-date"), false);
        }
        PAGES => {
            f.text(&tr("pages-help"));
            let pages = m.settings.desktop.pages.enabled();
            let mut page_top = f.y;
            if !pages.is_empty() {
                f.heading("pages-visible");
                page_top = f.y;
                for (index, page) in pages.iter().copied().enumerate() {
                    f.page_row(page, Some(index));
                }
            }
            if pages.len() != NotchPage::ALL.len() {
                f.heading("pages-hidden");
                for page in NotchPage::ALL
                    .into_iter()
                    .filter(|page| !pages.contains(page))
                {
                    f.page_row(page, None);
                }
            }
            f.finish_group();
            if let Some(page) = ui.lifted_page
                && let Some(index) = pages.iter().position(|item| *item == page)
            {
                let y = page_top + ui.page_position(page, index, now) * PAGE_ROW_HEIGHT;
                f.paint_page_row(page, y, true, true);
            }
        }
        QUICK_SHARE => {
            f.heading("nav-quick-share");
            f.field(
                Field::QuickShareName,
                &crate::i18n::translate("quickShare.deviceName"),
                false,
            );
            f.toggle(
                P::QuickShareVisible,
                &crate::i18n::translate("quickShare.visible"),
                Some(&crate::i18n::translate("quickShare.visibilityHelp")),
            );
        }
        ABOUT if ui.notices.is_some() => f.notices(ui.notices.unwrap(), content),
        _ => {
            let icon = crate::adapters::branding::app_icon();
            f.scene.paint.push(Paint::Bitmap(
                Rect::new(f.x + 8., f.y + 8., 88., 88.),
                icon.width,
                icon.height,
                icon.rgba.clone(),
            ));
            f.scene.text(
                Rect::new(f.x + 112., f.y + 18., f.width - 120., 34.),
                "Lumi",
                26.,
                ink,
                true,
                false,
            );
            f.scene.text(
                Rect::new(f.x + 112., f.y + 54., f.width - 120., 24.),
                env!("CARGO_PKG_VERSION"),
                12.,
                dim,
                false,
                false,
            );
            f.y += 112.;
            f.text(&tr("about-description"));
            f.text(&tr("about-mascot"));
            f.text(&crate::i18n::translate_with(
                "desktopUi.about-author",
                &[("author", env!("CARGO_PKG_AUTHORS"))],
            ));
            f.text(env!("CARGO_PKG_REPOSITORY"));
            f.text(env!("CARGO_PKG_LICENSE"));
            f.scene.button(
                Rect::new(f.x + 8., f.y, f.width - 16., 28.),
                tr("about-third-party"),
                "",
                Action::OpenNotices,
                0x343437,
            );
            f.y += 34.;
        }
    }
    f.finish_group();
    let total = (f.y + ui.scroll - content.y).max(0.);
    scene.paint.push(Paint::Opacity(1.));
    scene.unclip();
    scene.hits.retain(|hit| {
        !matches!(
            hit.action,
            Action::Preference(..)
                | Action::WidgetSlot(_)
                | Action::Display(..)
                | Action::SelectDate(..)
                | Action::GenerateTaskToken
                | Action::CopyTaskToken
                | Action::RevokeTaskToken(_)
                | Action::OpenNotices
                | Action::OpenNotice(_)
        ) || hit.rect.y >= content.y && hit.rect.y + hit.rect.h <= content.y + content.h
    });
    scene.inputs.retain(|input| {
        input.rect.y >= content.y && input.rect.y + input.rect.h <= content.y + content.h
    });
    if total > content.h {
        let bar = SettingsScrollbar::new(
            Rect::new(width - 17., content.y, 13., content.h),
            total - content.h,
            ui.scroll,
        );
        scene.round(bar.thumb, 1.5, dim);
        scene.hit(bar.track, Action::ScrollSettings, tr("scroll-settings"));
    }
    let unsaved = ui.has_unsaved_fields(&m.settings);
    if manual_save || ui.error {
        scene.text(
            Rect::new(214., height - 32., width - 376., 20.),
            if !ui.error && unsaved {
                crate::i18n::translate("settings.status.unsaved")
            } else {
                ui.status.clone()
            },
            10.,
            if ui.error { 0xff9292 } else { 0x939399 },
            false,
            false,
        );
    }
    if manual_save && unsaved {
        let r = Rect::new(width - 149., height - 34., 127., 24.);
        scene.round(r, 7., 0x303034);
        scene.text(
            r,
            crate::i18n::translate("settings.actions.saveButton"),
            11.,
            0xededf0,
            false,
            true,
        );
        scene.hit(
            r,
            Action::SaveSettings,
            crate::i18n::translate("settings.actions.saveButton"),
        );
    }
    scene.hover_controls(
        ui.mouse,
        ui.previous_hover,
        (now.duration_since(ui.hover_changed).as_secs_f32() / 0.15).clamp(0., 1.),
    );
    (scene, (total - content.h).max(0.))
}
