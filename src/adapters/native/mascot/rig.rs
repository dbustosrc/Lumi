use std::sync::{Arc, OnceLock};

use super::super::model::Rect;
use super::super::scene::{Paint, PathSegment};

pub const IVORY: u32 = 0xfffdf7;
pub const PURE_WHITE: u32 = 0xffffff;
pub const PUPIL_DARK: u32 = 0x0b1120;
pub const EYE_HALO: u32 = 0x78beff;
pub const BLUSH_ROSE: u32 = 0xff82a5;
pub const PAW_BEAN: u32 = 0xfda4af;
pub const OUTLINE_DARK: u32 = 0x0f172a;

pub struct PaintLayer<'a>(pub &'a mut Vec<Paint>);

impl PaintLayer<'_> {
    pub fn push_transform(&mut self, x: f32, y: f32, angle: f32, scale_x: f32, scale_y: f32) {
        self.0.push(Paint::PushTransform {
            x,
            y,
            angle,
            scale_x,
            scale_y,
        });
    }

    pub fn pop_transform(&mut self) {
        self.0.push(Paint::PopTransform);
    }

    pub fn fill_round(&mut self, r: Rect, radius: f32, color: u32, alpha: f32) {
        self.0.push(Paint::AlphaRound(r, radius, color, alpha));
    }

    pub fn fill_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, color: u32, alpha: f32) {
        self.0.push(Paint::Ellipse {
            center: (cx, cy),
            radius: (rx, ry),
            color,
            alpha,
            stroke: None,
        });
    }

    pub fn stroke_ellipse(
        &mut self,
        center: (f32, f32),
        radius: (f32, f32),
        color: u32,
        width: f32,
        alpha: f32,
    ) {
        self.0.push(Paint::Ellipse {
            center,
            radius,
            color,
            alpha,
            stroke: Some(width),
        });
    }

    pub fn quad_stroke(
        &mut self,
        from: (f32, f32),
        control: (f32, f32),
        to: (f32, f32),
        color: u32,
        width: f32,
        alpha: f32,
    ) {
        self.0.push(Paint::VectorPath {
            start: from,
            segments: vec![PathSegment::Quad(control.0, control.1, to.0, to.1)],
            closed: false,
            fill: None,
            stroke: Some((color, width, alpha)),
        });
    }

    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, color: u32, width: f32) {
        self.0.push(Paint::Line(x1, y1, x2, y2, color, width));
    }

    pub fn path(
        &mut self,
        start: (f32, f32),
        segments: Vec<PathSegment>,
        closed: bool,
        fill: Option<(u32, f32)>,
        stroke: Option<(u32, f32, f32)>,
    ) {
        self.0.push(Paint::VectorPath {
            start,
            segments,
            closed,
            fill,
            stroke,
        });
    }

    pub fn text(
        &mut self,
        r: Rect,
        value: impl Into<String>,
        size: f32,
        color: u32,
        weight: u16,
        center: bool,
    ) {
        self.0
            .push(Paint::Text(r, value.into(), size, color, weight, center));
    }

    pub fn set_opacity(&mut self, alpha: f32) {
        self.0.push(Paint::Opacity(alpha.clamp(0., 1.)));
    }
}

#[derive(Clone, Copy)]
pub struct EyeSpec {
    pub x: f32,
    pub y: f32,
    pub rx: f32,
    pub ry: f32,
    pub openness: f32,
    pub color: u32,
    pub pupil: Option<(f32, f32)>,
    pub has_shine: bool,
}

impl EyeSpec {
    pub fn standard(x: f32, y: f32, openness: f32) -> Self {
        Self {
            x,
            y,
            rx: 4.3,
            ry: 6.2,
            openness,
            color: IVORY,
            pupil: None,
            has_shine: true,
        }
    }

    pub fn dilated(x: f32, y: f32, rx: f32, ry: f32, px: f32, py: f32) -> Self {
        Self {
            x,
            y,
            rx,
            ry,
            openness: 1.0,
            color: IVORY,
            pupil: Some((px, py)),
            has_shine: true,
        }
    }
}

pub fn smooth_step(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}

pub fn draw_ambient_glow(layer: &mut PaintLayer<'_>, body: Rect, alpha: f32) {
    if body.w <= 100. || alpha <= 0.01 {
        return;
    }
    layer.set_opacity(alpha);
    layer.0.push(Paint::Bitmap(
        Rect::new(body.x, 6., body.w, 24.),
        16,
        16,
        mascot_glow(),
    ));
    layer.set_opacity(1.);
}

pub fn draw_sleep_eye(layer: &mut PaintLayer<'_>, x: f32, y: f32, width: f32, alpha: f32) {
    if alpha <= 0.01 {
        return;
    }
    let hw = width * 0.5;
    layer.quad_stroke(
        (x - hw, y - 1.5),
        (x, y + 3.2),
        (x + hw, y - 1.5),
        IVORY,
        2.1,
        alpha,
    );
}

pub fn draw_happy_arch_eye(
    layer: &mut PaintLayer<'_>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    tilt: f32,
    color: u32,
) {
    layer.push_transform(x, y, tilt, 1., 1.);
    layer.fill_ellipse(0., 0., width * 0.65, height * 0.9 + 2., EYE_HALO, 0.18);
    let hw = width * 0.5;
    layer.quad_stroke(
        (-hw, height * 0.45),
        (0., -height * 0.85),
        (hw, height * 0.45),
        color,
        2.5,
        1.0,
    );
    layer.pop_transform();
}

pub fn draw_squint_eye(layer: &mut PaintLayer<'_>, x: f32, y: f32, points_left: bool) {
    let dir = if points_left { -1. } else { 1. };
    layer.path(
        (x - dir * 4.5, y - 3.2),
        vec![
            PathSegment::Line(x + dir * 3.8, y),
            PathSegment::Line(x - dir * 4.5, y + 3.2),
        ],
        false,
        None,
        Some((IVORY, 2.4, 1.0)),
    );
}

pub fn draw_expressive_eye(layer: &mut PaintLayer<'_>, spec: EyeSpec, alpha: f32) {
    if alpha <= 0.01 {
        return;
    }
    let reveal = smooth_step((spec.openness - 0.25) / 0.75);
    if reveal < 0.96 {
        draw_sleep_eye(layer, spec.x, spec.y, spec.rx * 2.3, (1. - reveal) * alpha);
    }
    if reveal <= 0.03 {
        return;
    }
    let rx = spec.rx * (0.65 + 0.35 * reveal);
    let ry = (spec.ry * reveal).max(1.2);
    if spec.openness > 0.85 {
        layer.set_opacity(smooth_step((spec.openness - 0.85) / 0.15) * alpha * 0.72);
        let halo_h = (ry * 3.2).min(28.);
        let halo_y = (spec.y - halo_h * 0.5).clamp(0.5, (29.5 - halo_h).max(0.5));
        layer.0.push(Paint::Icon(
            Rect::new(spec.x - rx * 2.0, halo_y, rx * 4.0, halo_h),
            "eye-halo".into(),
            EYE_HALO,
        ));
        layer.set_opacity(1.);
    }
    layer.fill_ellipse(spec.x, spec.y, rx, ry, spec.color, reveal * alpha);
    if let Some((px, py)) = spec.pupil {
        let prx = rx * 0.58;
        let pry = ry * 0.64;
        layer.fill_ellipse(
            spec.x + px,
            spec.y + py,
            prx,
            pry,
            PUPIL_DARK,
            reveal * alpha,
        );
        let shine_r = (rx * 0.24).max(1.1);
        layer.fill_ellipse(
            spec.x + px - rx * 0.20,
            spec.y + py - ry * 0.24,
            shine_r,
            shine_r,
            PURE_WHITE,
            reveal * alpha,
        );
        let mini_r = (rx * 0.12).max(0.65);
        layer.fill_ellipse(
            spec.x + px + rx * 0.20,
            spec.y + py + ry * 0.22,
            mini_r,
            mini_r,
            PURE_WHITE,
            reveal * alpha * 0.9,
        );
    } else if spec.has_shine && ry > 2.4 {
        let shine_r = (rx.min(ry) * 0.23).max(0.85);
        layer.fill_ellipse(
            spec.x - rx * 0.26,
            spec.y - ry * 0.28,
            shine_r,
            shine_r,
            PURE_WHITE,
            reveal * alpha * 0.95,
        );
    }
}

pub fn draw_blush(layer: &mut PaintLayer<'_>, cx: f32, y: f32, spread: f32, alpha: f32) {
    if alpha <= 0.01 {
        return;
    }
    for side in [-1., 1.] {
        layer.fill_ellipse(cx + side * spread, y, 4.3, 2.1, BLUSH_ROSE, alpha);
    }
}

/// Articulated cartoon hand with 4 phase-lagged fingers, thumb, palm, and pink bean pad.
pub fn draw_articulated_hand(
    layer: &mut PaintLayer<'_>,
    x: f32,
    y: f32,
    scale: f32,
    wrist_angle: f32,
    wave_phase: f32,
    flip_x: f32,
) {
    layer.push_transform(x, y, wrist_angle, flip_x * scale, scale);
    layer.fill_ellipse(0., 1.2, 6.4, 5.4, OUTLINE_DARK, 0.42);

    let finger_angles = [-0.42_f32, -0.14, 0.14, 0.40];
    let finger_lengths = [7.4_f32, 8.8, 8.4, 6.8];
    let spread = 1. + (wave_phase * 0.5).sin() * 0.08;
    for i in 0..4 {
        let lag = i as f32 * 0.55;
        let wiggle = (wave_phase - lag).sin() * 0.22;
        let len = finger_lengths[i];
        layer.push_transform(0., 0., finger_angles[i] * spread + wiggle, 1., 1.);
        let r = Rect::new(-2.1, -len - 3.0, 4.2, len + 3.2);
        layer.fill_round(r.inset(-0.55), 2.5, OUTLINE_DARK, 0.36);
        layer.fill_round(r, 2.1, IVORY, 1.0);
        layer.pop_transform();
    }

    let thumb_angle = -0.86 + (wave_phase + 0.4).sin() * 0.15;
    layer.push_transform(0., 0.4, thumb_angle, 1., 1.);
    let thumb = Rect::new(-1.9, -6.2, 3.8, 7.0);
    layer.fill_round(thumb.inset(-0.5), 2.3, OUTLINE_DARK, 0.36);
    layer.fill_round(thumb, 1.9, IVORY, 1.0);
    layer.pop_transform();

    layer.fill_ellipse(0., 0., 5.8, 5.0, IVORY, 1.0);
    layer.fill_ellipse(0., 0.7, 2.9, 2.1, PAW_BEAN, 0.92);
    layer.pop_transform();
}

/// Cartoon peace/victory hand (`V` fingers wiggling with phase lag + curled fingers + paw bean).
pub fn draw_peace_hand(
    layer: &mut PaintLayer<'_>,
    pos: (f32, f32),
    scale: f32,
    wrist_angle: f32,
    phase: f32,
    flip_x: f32,
) {
    layer.push_transform(pos.0, pos.1, wrist_angle, flip_x * scale, scale);
    layer.fill_ellipse(0., 1.2, 6.2, 5.2, OUTLINE_DARK, 0.42);

    for (idx, (base_angle, len)) in [(-0.28_f32, 9.6_f32), (0.26, 9.2)].into_iter().enumerate() {
        let wiggle = (phase - idx as f32 * 0.6).sin() * 0.14;
        layer.push_transform(0., -0.5, base_angle + wiggle, 1., 1.);
        let finger = Rect::new(-2.1, -len - 2.6, 4.2, len + 2.8);
        layer.fill_round(finger.inset(-0.55), 2.5, OUTLINE_DARK, 0.38);
        layer.fill_round(finger, 2.1, IVORY, 1.0);
        layer.pop_transform();
    }

    layer.fill_ellipse(0., 0.4, 5.6, 4.8, IVORY, 1.0);
    for (cx, cy) in [(2.4_f32, 0.6_f32), (3.6, 2.0)] {
        layer.fill_ellipse(cx, cy, 2.3, 1.8, OUTLINE_DARK, 0.28);
        layer.fill_ellipse(cx, cy, 1.9, 1.5, IVORY, 1.0);
    }
    layer.fill_ellipse(-2.0, 0.8, 2.5, 1.8, OUTLINE_DARK, 0.28);
    layer.fill_ellipse(-1.8, 0.8, 2.1, 1.5, IVORY, 1.0);
    layer.fill_ellipse(0.2, 2.0, 2.3, 1.6, PAW_BEAN, 0.88);
    layer.pop_transform();
}

/// Cartoon thumbs-up hand with extended wiggling thumb and stacked curled fingers.
pub fn draw_thumbs_up_hand(
    layer: &mut PaintLayer<'_>,
    pos: (f32, f32),
    scale: f32,
    wrist_angle: f32,
    phase: f32,
    flip_x: f32,
) {
    layer.push_transform(pos.0, pos.1, wrist_angle, flip_x * scale, scale);
    layer.fill_ellipse(0.4, 1.0, 6.2, 5.4, OUTLINE_DARK, 0.42);

    let thumb_wiggle = phase.sin() * 0.14;
    layer.push_transform(-2.4, -1.8, -0.16 + thumb_wiggle, 1., 1.);
    let thumb = Rect::new(-2.2, -7.8, 4.4, 8.2);
    layer.fill_round(thumb.inset(-0.55), 2.5, OUTLINE_DARK, 0.38);
    layer.fill_round(thumb, 2.2, IVORY, 1.0);
    layer.pop_transform();

    layer.fill_round(Rect::new(-4.2, -3.4, 8.8, 8.2), 3.6, IVORY, 1.0);
    for row in 0..4 {
        let fy = -2.8 + row as f32 * 1.95;
        let curl = Rect::new(0.2, fy, 5.2, 2.2);
        layer.fill_round(curl.inset(-0.35), 1.3, OUTLINE_DARK, 0.26);
        layer.fill_round(curl, 1.1, IVORY, 1.0);
    }
    layer.pop_transform();
}

/// Four-pointed curved cartoon star sparkle.
pub fn draw_star_sparkle(
    layer: &mut PaintLayer<'_>,
    center: (f32, f32),
    radius: f32,
    color: u32,
    alpha: f32,
) {
    if alpha <= 0.02 || radius <= 0.4 {
        return;
    }
    let (cx, cy) = center;
    let r = radius;
    let k = radius * 0.24;
    layer.path(
        (cx, cy - r),
        vec![
            PathSegment::Quad(cx + k, cy - k, cx + r, cy),
            PathSegment::Quad(cx + k, cy + k, cx, cy + r),
            PathSegment::Quad(cx - k, cy + k, cx - r, cy),
            PathSegment::Quad(cx - k, cy - k, cx, cy - r),
        ],
        true,
        Some((color, alpha)),
        None,
    );
}

/// Chubby 3-toed cartoon paw gripping the bottom notch rim (`y ≈ 30`).
pub fn draw_rim_paw(layer: &mut PaintLayer<'_>, x: f32, y: f32, scale: f32, press: f32) {
    let sy = scale * (1. - press * 0.08);
    layer.push_transform(x, y + press * 1.1, 0., scale, sy);
    layer.fill_ellipse(0., 0.9, 7.2, 3.5, OUTLINE_DARK, 0.45);
    for toe in [-1., 0., 1.] {
        layer.fill_ellipse(toe * 3.4, 0., 2.4, 3.1, IVORY, 1.0);
    }
    for divider in [-1.7_f32, 1.7] {
        layer.line(divider, -1.6, divider, 2.0, 0xcbd5e1, 0.85);
    }
    layer.pop_transform();
}

/// Metamorphic pointy cat ears used exclusively during Cat-Stalking mode.
pub fn draw_stalk_cat_ears(layer: &mut PaintLayer<'_>, cx: f32, top_y: f32, elapsed: f32) {
    for side in [-1_f32, 1.] {
        let twitch = (elapsed * 8.5 + side).sin() * 0.11;
        layer.push_transform(cx + side * 20.0, top_y, side * 0.24 + twitch, 1., 1.);
        layer.path(
            (-5.8, 0.),
            vec![PathSegment::Quad(0., -9.6, 5.8, 0.)],
            true,
            Some((0x1e293b, 0.98)),
            Some((EYE_HALO, 1.25, 0.72)),
        );
        layer.path(
            (-3.2, -0.7),
            vec![PathSegment::Quad(0., -6.2, 3.2, -0.7)],
            true,
            Some((PAW_BEAN, 0.82)),
            None,
        );
        layer.pop_transform();
    }
}

fn mascot_glow() -> Arc<Vec<u8>> {
    static GLOW: OnceLock<Arc<Vec<u8>>> = OnceLock::new();
    GLOW.get_or_init(|| {
        let mut pixels = Vec::with_capacity(16 * 16 * 4);
        for y in 0..16 {
            for x in 0..16 {
                let t = x as f32 / 15.;
                let (a, b, u) = if t < 0.55 {
                    ([25., 184., 255.], [45., 91., 255.], t / 0.55)
                } else {
                    ([45., 91., 255.], [123., 75., 255.], (t - 0.55) / 0.45)
                };
                for i in 0..3 {
                    pixels.push((a[i] + (b[i] - a[i]) * u) as u8);
                }
                let vertical = y as f32 / 15.;
                pixels.push((107. * vertical.powf(2.5)) as u8);
            }
        }
        Arc::new(pixels)
    })
    .clone()
}
