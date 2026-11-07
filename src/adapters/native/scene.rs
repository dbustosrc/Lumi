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
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathSegment {
    Line(f32, f32),
    Quad(f32, f32, f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
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
    PushTransform {
        x: f32,
        y: f32,
        angle: f32,
        scale_x: f32,
        scale_y: f32,
    },
    PopTransform,
    Ellipse {
        center: (f32, f32),
        radius: (f32, f32),
        color: u32,
        alpha: f32,
        stroke: Option<f32>,
    },
    VectorPath {
        start: (f32, f32),
        segments: Vec<PathSegment>,
        closed: bool,
        fill: Option<(u32, f32)>,
        stroke: Option<(u32, f32, f32)>,
    },
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
    pub overlay: Vec<Paint>,
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
    pub fn overlay_bounds(&self) -> Option<Rect> {
        commands_bounds(&self.overlay)
    }
}

#[derive(Clone, Copy)]
struct Affine2d {
    m11: f32,
    m12: f32,
    m21: f32,
    m22: f32,
    dx: f32,
    dy: f32,
}

impl Affine2d {
    const IDENTITY: Self = Self {
        m11: 1.,
        m12: 0.,
        m21: 0.,
        m22: 1.,
        dx: 0.,
        dy: 0.,
    };

    fn from_trs(x: f32, y: f32, angle: f32, scale_x: f32, scale_y: f32) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self {
            m11: cos * scale_x,
            m12: sin * scale_y,
            m21: -sin * scale_x,
            m22: cos * scale_y,
            dx: x,
            dy: y,
        }
    }

    fn then(self, parent: Self) -> Self {
        Self {
            m11: self.m11 * parent.m11 + self.m12 * parent.m21,
            m12: self.m11 * parent.m12 + self.m12 * parent.m22,
            m21: self.m21 * parent.m11 + self.m22 * parent.m21,
            m22: self.m21 * parent.m12 + self.m22 * parent.m22,
            dx: self.dx * parent.m11 + self.dy * parent.m21 + parent.dx,
            dy: self.dx * parent.m12 + self.dy * parent.m22 + parent.dy,
        }
    }

    fn apply(self, x: f32, y: f32) -> (f32, f32) {
        (
            x * self.m11 + y * self.m21 + self.dx,
            x * self.m12 + y * self.m22 + self.dy,
        )
    }
}

pub fn commands_bounds(commands: &[Paint]) -> Option<Rect> {
    let mut current = Affine2d::IDENTITY;
    let mut stack = Vec::new();
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut include_rect = |r: Rect, xform: Affine2d| {
        for (px, py) in [
            (r.x, r.y),
            (r.x + r.w, r.y),
            (r.x, r.y + r.h),
            (r.x + r.w, r.y + r.h),
        ] {
            let (tx, ty) = xform.apply(px, py);
            min_x = min_x.min(tx);
            min_y = min_y.min(ty);
            max_x = max_x.max(tx);
            max_y = max_y.max(ty);
        }
    };
    for command in commands {
        match command {
            Paint::PushTransform {
                x,
                y,
                angle,
                scale_x,
                scale_y,
            } => {
                stack.push(current);
                current = Affine2d::from_trs(*x, *y, *angle, *scale_x, *scale_y).then(current);
            }
            Paint::PopTransform => {
                if let Some(prev) = stack.pop() {
                    current = prev;
                }
            }
            Paint::Round(r, _, _)
            | Paint::AlphaRound(r, _, _, _)
            | Paint::GradientRound(r, _, _, _)
            | Paint::Text(r, _, _, _, _, _)
            | Paint::Icon(r, _, _)
            | Paint::TextRun(r, _, _)
            | Paint::Strike(r, _, _, _)
            | Paint::Paragraph(r, _, _, _)
            | Paint::RoundedBitmap(r, _, _, _, _)
            | Paint::ButtonContent(r, _, _)
            | Paint::Ring(r, _, _, _, _)
            | Paint::ProgressRing(r, _, _, _)
            | Paint::Bitmap(r, _, _, _) => include_rect(*r, current),
            Paint::Border(r, _, _, w) | Paint::AlphaBorder(r, _, _, w, _) => {
                include_rect(r.inset(-w * 0.5), current);
            }
            Paint::Line(x, y, x2, y2, _, w) => {
                let pad = w * 0.5;
                include_rect(
                    Rect::new(
                        x.min(*x2) - pad,
                        y.min(*y2) - pad,
                        (x - x2).abs() + *w,
                        (y - y2).abs() + *w,
                    ),
                    current,
                );
            }
            Paint::Ellipse {
                center,
                radius,
                stroke,
                ..
            } => {
                let pad = stroke.unwrap_or(0.) * 0.5;
                include_rect(
                    Rect::new(
                        center.0 - radius.0 - pad,
                        center.1 - radius.1 - pad,
                        (radius.0 + pad) * 2.,
                        (radius.1 + pad) * 2.,
                    ),
                    current,
                );
            }
            Paint::VectorPath {
                start,
                segments,
                stroke,
                ..
            } => {
                let pad = stroke.map_or(0., |(_, w, _)| w * 0.5);
                let mut lx0 = start.0;
                let mut ly0 = start.1;
                let mut lx1 = start.0;
                let mut ly1 = start.1;
                let mut add_pt = |px: f32, py: f32| {
                    lx0 = lx0.min(px);
                    ly0 = ly0.min(py);
                    lx1 = lx1.max(px);
                    ly1 = ly1.max(py);
                };
                for seg in segments {
                    match *seg {
                        PathSegment::Line(x, y) => add_pt(x, y),
                        PathSegment::Quad(cx, cy, x, y) => {
                            add_pt(cx, cy);
                            add_pt(x, y);
                        }
                        PathSegment::Cubic(c1x, c1y, c2x, c2y, x, y) => {
                            add_pt(c1x, c1y);
                            add_pt(c2x, c2y);
                            add_pt(x, y);
                        }
                    }
                }
                include_rect(
                    Rect::new(lx0 - pad, ly0 - pad, lx1 - lx0 + pad * 2., ly1 - ly0 + pad * 2.),
                    current,
                );
            }
            Paint::Clip(_) | Paint::Unclip | Paint::Opacity(_) => {}
        }
    }
    (min_x <= max_x && min_y <= max_y).then(|| Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
}
