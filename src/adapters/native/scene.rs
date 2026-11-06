use super::model::Rect;
use crate::application::events::UserIntent;
use std::sync::Arc;
use uuid::Uuid;

pub const VOLUME_THUMB_SIZE: f32 = 20.;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Field {
    Preference(Preference),
    TaskTitle,
    TaskTime,
    ShelfText,
    RestSeconds,
    MaUrl,
    MaToken,
    MaQueue,
    VoiceHost,
    VoicePort,
    VoiceLanguage,
    ThirdPartyNotice(usize),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Preference {
    MouseMove,
    Startup,
    Minimal,
    Mascot,
    Sounds,
    HideFullscreen,
    Scale,
    Language,
    OpenClick,
    PageIcons,
    Fast,
    Lyrics,
    MaEnabled,
    VoiceEnabled,
    TaskStart,
    TaskEnd,
    Snooze,
    TaskMinutes,
    FocusMinutes,
    ShortMinutes,
    LongMinutes,
    Cycles,
    Monday,
    Week,
    WaterEnabled,
    Goal,
    Bottle,
    From,
    Until,
    EmptyShelf,
    HudVolume,
    HudBrightness,
    HudCaps,
    HudLanguage,
    PopupBattery,
    PopupWifi,
    PopupHeadphones,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    None,
    Intent(UserIntent),
    Settings,
    CloseSettings,
    Section(usize),
    OpenNotices,
    OpenNotice(usize),
    BackNotices,
    Preference(Preference, i32),
    Display(String),
    SaveSettings,
    GenerateTaskToken,
    CopyTaskToken,
    RevokeTaskToken(Uuid),
    ScrollSettings,
    Calendar,
    SelectDate(String),
    ExpandTasks,
    AddTask,
    EditTasks,
    EditTask(Uuid),
    SaveTask,
    CancelTask,
    Palette,
    Color(usize),
    Daily,
    WaterSettings,
    WaterGraph,
    ToggleWater,
    ShelfMenu,
    ShelfText,
    SaveShelfText,
    CancelShelfText,
    PasteShelf,
    DragShelf(Uuid),
    Seek,
    Volume,
    ShowVolume,
    Lyric(usize),
}
#[derive(Clone, Debug)]
pub struct Hit {
    pub rect: Rect,
    pub action: Action,
    pub label: String,
    pub enabled: bool,
}
pub struct FontWeight(pub u16);
impl From<bool> for FontWeight {
    fn from(value: bool) -> Self {
        Self(if value { 600 } else { 400 })
    }
}
impl From<u16> for FontWeight {
    fn from(value: u16) -> Self {
        Self(value)
    }
}
#[derive(Clone)]
pub struct TextSpan {
    pub text: String,
    pub size: f32,
    pub color: u32,
    pub weight: u16,
    pub bottom: bool,
}
impl TextSpan {
    pub fn new(text: impl Into<String>, size: f32, color: u32, weight: u16) -> Self {
        Self {
            text: text.into(),
            size,
            color,
            weight,
            bottom: false,
        }
    }
}
#[derive(Clone)]
pub enum Paint {
    Round(Rect, f32, u32),
    AlphaRound(Rect, f32, u32, f32),
    AlphaBorder(Rect, f32, u32, f32, f32),
    GradientRound(Rect, f32, u32, u32),
    Border(Rect, f32, u32, f32),
    Line(f32, f32, f32, f32, u32, f32),
    Text(Rect, String, f32, u32, u16, bool),
    Icon(Rect, String, u32),
    TextRun(Rect, Vec<TextSpan>, f32),
    Strike(Rect, String, f32, u32),
    Paragraph(Rect, String, f32, u32),
    RoundedBitmap(Rect, u32, u32, Arc<Vec<u8>>, f32),
    ButtonContent(Rect, String, String),
    Ring(Rect, f32, u32, f32, u32),
    ProgressRing(Rect, f32, u32, bool),
    Bitmap(Rect, u32, u32, Arc<Vec<u8>>),
    Clip(Rect),
    Unclip,
    Opacity(f32),
}
#[derive(Clone)]
pub struct Input {
    pub key: Field,
    pub rect: Rect,
    pub text: String,
    pub secret: bool,
    pub multiline: bool,
    pub label: String,
}
#[derive(Clone, Default)]
pub struct Scene {
    pub paint: Vec<Paint>,
    pub hits: Vec<Hit>,
    pub inputs: Vec<Input>,
    clips: Vec<Rect>,
}
impl Scene {
    pub fn round(&mut self, r: Rect, radius: f32, color: u32) {
        self.paint.push(Paint::Round(r, radius, color));
    }
    pub fn alpha_round(&mut self, r: Rect, radius: f32, color: u32, alpha: f32) {
        self.paint.push(Paint::AlphaRound(r, radius, color, alpha));
    }
    pub fn alpha_border(&mut self, r: Rect, radius: f32, color: u32, width: f32, alpha: f32) {
        self.paint
            .push(Paint::AlphaBorder(r, radius, color, width, alpha));
    }
    pub fn border(&mut self, r: Rect, radius: f32, color: u32, width: f32) {
        self.paint.push(Paint::Border(r, radius, color, width));
    }
    pub fn text(
        &mut self,
        r: Rect,
        text: impl Into<String>,
        size: f32,
        color: u32,
        weight: impl Into<FontWeight>,
        center: bool,
    ) {
        self.paint.push(Paint::Text(
            r,
            text.into(),
            size,
            color,
            weight.into().0,
            center,
        ));
    }
    pub fn text_run(&mut self, r: Rect, spans: Vec<TextSpan>, spacing: f32) {
        self.paint.push(Paint::TextRun(r, spans, spacing));
    }
    pub fn icon(&mut self, r: Rect, glyph: &str, color: u32) {
        self.paint.push(Paint::Icon(r, glyph.into(), color));
    }
    pub fn line(&mut self, x: f32, y: f32, x2: f32, y2: f32, color: u32, width: f32) {
        self.paint.push(Paint::Line(x, y, x2, y2, color, width));
    }
    pub fn ring(&mut self, r: Rect, progress: f32, color: u32, width: f32) {
        self.paint.push(Paint::Ring(
            r,
            progress.clamp(0., 1.),
            color,
            width,
            0x0e3e22,
        ));
    }
    pub fn ring_track(&mut self, r: Rect, value: f32, color: u32, width: f32, track: u32) {
        self.paint
            .push(Paint::Ring(r, value.clamp(0., 1.), color, width, track));
    }
    pub fn progress_ring(&mut self, r: Rect, value: f32, tint: u32, open: bool) {
        self.paint
            .push(Paint::ProgressRing(r, value.clamp(0., 1.), tint, open));
    }
    pub fn hit(&mut self, r: Rect, action: Action, label: impl Into<String>) {
        let r = self.clips.iter().fold(r, |r, clip| r.intersect(*clip));
        if r.w <= 0. || r.h <= 0. {
            return;
        }
        self.hits.push(Hit {
            rect: r,
            action,
            label: label.into(),
            enabled: true,
        });
    }
    pub fn clip(&mut self, r: Rect) {
        self.paint.push(Paint::Clip(r));
        self.clips.push(r);
    }
    pub fn unclip(&mut self) {
        self.paint.push(Paint::Unclip);
        self.clips.pop();
    }
    pub fn button(
        &mut self,
        r: Rect,
        label: impl Into<String>,
        glyph: &str,
        action: Action,
        tint: u32,
    ) {
        let label = label.into();
        self.round(r, r.h / 2., tint);
        self.paint
            .push(Paint::ButtonContent(r, label.clone(), glyph.to_owned()));
        self.hit(r, action, label);
    }
    pub fn input(
        &mut self,
        key: Field,
        r: Rect,
        text: &str,
        secret: bool,
        multiline: bool,
        label: &str,
    ) {
        self.inputs.push(Input {
            key,
            rect: r,
            text: text.into(),
            secret,
            multiline,
            label: label.into(),
        });
    }
    pub fn hover_controls(&mut self, mouse: (f32, f32), previous: Option<usize>, progress: f32) {
        let current = self.action_at(mouse.0, mouse.1).map(|(i, _)| i);
        for (index, amount) in [(previous, 1. - progress), (current, progress)] {
            let Some(index) = index else {
                continue;
            };
            let Some(hit) = self.hits.get(index) else {
                continue;
            };
            if !hit.enabled || amount <= 0. {
                continue;
            }
            if let Some(Paint::Round(_, _, color)) = self
                .paint
                .iter_mut()
                .rev()
                .find(|paint| matches!(paint,Paint::Round(rect,_,_) if *rect==hit.rect))
            {
                let target: u32 = match *color {
                    0x212121 => 0x3d3d3d,
                    0x424242 => 0x6b6b6b,
                    0x1d1d20 => 0x38383c,
                    0x25282a => 0x45494b,
                    0x3e3e42 => 0x505056,
                    0xff4b55 => 0xff6871,
                    0x303034 => 0x434347,
                    0x290c0e => 0x57191d,
                    _ => continue,
                };
                let blend = |shift: u32| {
                    let a = (*color >> shift) & 255u32;
                    let b = (target >> shift) & 255u32;
                    ((a as f32 + (b as f32 - a as f32) * amount).round() as u32) << shift
                };
                *color = blend(16) | blend(8) | blend(0);
            }
        }
    }
    pub fn action_at(&self, x: f32, y: f32) -> Option<(usize, &Hit)> {
        self.hits
            .iter()
            .enumerate()
            .rev()
            .find(|(_, h)| h.enabled && h.rect.contains(x, y))
    }
}
