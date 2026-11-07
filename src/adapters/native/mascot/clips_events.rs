use std::f32::consts::{PI, TAU};

use super::super::model::Rect;
use super::super::scene::{PathSegment, Scene};
use super::clips_core::RIM_Y;
use super::rig::{
    BLUSH_ROSE, EyeSpec, IVORY, OUTLINE_DARK, PURE_WHITE, PaintLayer, draw_ambient_glow,
    draw_articulated_hand, draw_blush, draw_expressive_eye, draw_happy_arch_eye, draw_rim_paw,
    draw_star_sparkle, draw_thumbs_up_hand, smooth_step,
};
use crate::application::presenter::VoiceState;

pub fn draw_water_sip(scene: &mut Scene, body: Rect, variant: u8, p: f32) {
    match variant % 3 {
        1 => draw_water_bottle_chug(scene, body, p),
        2 => draw_water_droplet_splash(scene, body, p),
        _ => draw_water_straw_classic(scene, body, p),
    }
}

fn draw_water_straw_classic(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let sipping = p < 0.68;
    let glug = (p * 44.).sin();
    let fade = smooth_step(p / 0.08) * smooth_step((1. - p) / 0.12);

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        if sipping {
            draw_happy_arch_eye(&mut inside, cx - 15.5, 15.2, 9.6, 4.0, 0., IVORY);
            draw_happy_arch_eye(&mut inside, cx + 6.5, 15.2, 9.6, 4.0, 0., IVORY);
            let puff = 3.2 + (glug + 1.) * 0.95;
            inside.fill_ellipse(cx - 21.5, 20.5, puff, puff * 0.82, 0xff91af, 0.62);
            inside.fill_ellipse(cx + 13.5, 20.5, puff, puff * 0.82, 0xff91af, 0.62);
        } else {
            for (ox, side) in [(-15.0_f32, -1_f32), (9.0, 1.)] {
                let spec = EyeSpec::dilated(cx + ox, 15.5, 5.1, 6.6, side * 0.4, -0.4);
                draw_expressive_eye(&mut inside, spec, 1.0);
            }
            draw_blush(&mut inside, cx - 3., 21.6, 19.0, 0.62);
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.set_opacity(fade);
    if !sipping {
        draw_star_sparkle(&mut over, (cx - 33., 14.5), 4.4, 0x38bdf8, 0.92 * fade);
    }
    let cup_x = cx + 28.;
    let cup_y = 21.0 + if sipping { glug * 0.9 } else { 2.0 };
    over.path(
        (cup_x, cup_y),
        vec![
            PathSegment::Line(cup_x - 3.5, cup_y - 8.0),
            PathSegment::Line(cup_x - 17.5, cup_y - 5.5),
        ],
        false,
        None,
        Some((0xf43f5e, 2.5, 1.0)),
    );
    let cup_rect = Rect::new(cup_x - 7.5, cup_y - 3.0, 15.0, 18.0);
    over.fill_round(cup_rect.inset(-0.7), 4.0, OUTLINE_DARK, 0.42);
    over.fill_round(cup_rect, 3.5, 0xe0f2fe, 0.92);
    let level = if sipping {
        1.0 - (p / 0.68) * 0.65
    } else {
        0.35
    };
    let water_h = 13.0 * level;
    over.fill_round(
        Rect::new(cup_x - 5.8, cup_y - 1.2 + (13.0 - water_h), 11.6, water_h),
        2.2,
        0x0ea5e9,
        0.95,
    );
    draw_articulated_hand(&mut over, cup_x + 7.5, cup_y + 5.5, 0.80, -0.28, p * 12., 1.);
    over.set_opacity(1.);
}

fn draw_water_bottle_chug(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let chugging = p < 0.68;
    let glug = (p * 42.).sin();
    let fade = smooth_step(p / 0.08) * smooth_step((1. - p) / 0.12);

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        if chugging {
            for side in [-1_f32, 1.] {
                draw_happy_arch_eye(
                    &mut inside,
                    cx - 3. + side * 12.5,
                    14.2 + glug * 0.5,
                    10.0,
                    4.6,
                    -0.06,
                    IVORY,
                );
            }
            draw_blush(&mut inside, cx - 3., 20.6, 18.5, 0.52);
        } else {
            for side in [-1_f32, 1.] {
                let spec = EyeSpec::dilated(cx + side * 13.5, 15.4, 5.0, 6.6, 0., -0.4);
                draw_expressive_eye(&mut inside, spec, 1.0);
            }
            draw_blush(&mut inside, cx, 21.4, 19.5, 0.65);
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.set_opacity(fade);
    if chugging {
        draw_rim_paw(&mut over, cx - 24., RIM_Y, 0.88, 0.15);
        let tilt = -0.78 - glug * 0.06;
        over.push_transform(cx + 24., 19.5, tilt, 1., 1.);
        let bottle = Rect::new(-6.0, -9.0, 12.0, 19.0);
        over.fill_round(bottle.inset(-0.7), 4.2, OUTLINE_DARK, 0.45);
        over.fill_round(bottle, 3.8, 0xbae6fd, 0.92);
        over.fill_round(Rect::new(-4.8, -3.0, 9.6, 11.5), 2.8, 0x0284c7, 0.95);
        over.fill_round(Rect::new(-3.2, -12.2, 6.4, 3.6), 1.6, 0x0ea5e9, 1.0);
        let bub_y = 4.0 - ((p * 5.0).fract()) * 8.0;
        over.fill_ellipse(1.2, bub_y, 1.4, 1.4, PURE_WHITE, 0.85);
        over.pop_transform();
        draw_articulated_hand(&mut over, cx + 31., 22.0, 0.82, -0.35, p * 16., 1.);
    } else {
        draw_articulated_hand(&mut over, cx - 22., 18.5, 0.82, 0.35, p * 20., -1.);
        draw_rim_paw(&mut over, cx + 22., RIM_Y, 0.90, 0.);
        draw_star_sparkle(&mut over, (cx + 32., 11.5), 4.6, 0x38bdf8, 0.95 * fade);
    }
    over.set_opacity(1.);
}

fn draw_water_droplet_splash(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let fade = smooth_step(p / 0.08) * smooth_step((1. - p) / 0.12);
    let splashed = p >= 0.42;

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        if !splashed {
            for side in [-1_f32, 1.] {
                let spec = EyeSpec::dilated(cx + side * 13.0, 15.6, 5.1, 6.6, 1.6, -1.0);
                draw_expressive_eye(&mut inside, spec, 1.0);
            }
        } else {
            let shake = ((p - 0.42) * 38.).sin() * 2.0 * (1. - p);
            for side in [-1_f32, 1.] {
                draw_happy_arch_eye(
                    &mut inside,
                    cx + shake + side * 13.5,
                    15.4,
                    10.6,
                    5.2,
                    0.,
                    IVORY,
                );
            }
            draw_blush(&mut inside, cx + shake, 21.4, 19.5, 0.64);
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.set_opacity(fade);
    draw_rim_paw(&mut over, cx - 22., RIM_Y, 0.90, 0.);
    draw_rim_paw(&mut over, cx + 22., RIM_Y, 0.90, 0.);

    let drop_x = cx + 31.;
    if !splashed {
        let dp = p / 0.42;
        let dy = 6.5 + smooth_step(dp) * 11.5;
        over.path(
            (drop_x, dy - 4.5),
            vec![
                PathSegment::Quad(drop_x + 4.8, dy + 1.5, drop_x, dy + 5.2),
                PathSegment::Quad(drop_x - 4.8, dy + 1.5, drop_x, dy - 4.5),
            ],
            true,
            Some((0x38bdf8, 0.96)),
            Some((0xe0f2fe, 1.1, 0.85)),
        );
    } else {
        let sp = (p - 0.42) / 0.58;
        for i in 0..5 {
            let ang = -PI * 0.85 + (i as f32 / 4.) * PI * 0.70;
            let dist = 5.0 + sp * 16.0;
            let sx = drop_x + ang.cos() * dist;
            let sy = (18.0 + ang.sin() * dist * 0.75 + sp * sp * 9.0).clamp(3.0, 36.0);
            let r = (2.6 * (1. - sp * 0.55)).max(1.0);
            over.fill_ellipse(sx, sy, r, r * 1.2, 0x38bdf8, (1. - sp) * fade);
        }
        draw_star_sparkle(&mut over, (cx - 32., 12.0), 4.0 * (1. - sp * 0.4), 0x7dd3fc, fade);
    }
    over.set_opacity(1.);
}

pub fn draw_pomodoro_study(scene: &mut Scene, body: Rect, variant: u8, p: f32) {
    match variant % 3 {
        1 => draw_pomodoro_headband(scene, body, p),
        2 => draw_pomodoro_notepad(scene, body, p),
        _ => draw_pomodoro_glasses_classic(scene, body, p),
    }
}

fn draw_pomodoro_glasses_classic(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let fade = smooth_step(p / 0.08) * smooth_step((1. - p) / 0.12);
    let adjusting = (0.25..0.58).contains(&p);
    let lift = if adjusting {
        ((p - 0.25) / 0.33 * PI).sin() * -2.0
    } else {
        0.
    };
    let scan_x = (p * 14.).sin() * 1.8;

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        for side in [-1_f32, 1.] {
            let spec = EyeSpec::dilated(
                cx + side * 13.5 + scan_x * 0.6,
                15.8,
                4.6,
                5.6,
                scan_x * 0.6,
                0.8,
            );
            draw_expressive_eye(&mut inside, spec, 1.0);
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.set_opacity(fade);
    let gy = 15.8 + lift;
    for side in [-1_f32, 1.] {
        over.fill_ellipse(cx + side * 13.5, gy, 8.6, 8.6, 0xbae6fd, 0.16);
        over.stroke_ellipse((cx + side * 13.5, gy), (8.6, 8.6), 0xfbbf24, 2.0, 1.0);
    }
    over.quad_stroke(
        (cx - 4.8, gy - 0.8),
        (cx, gy - 3.4),
        (cx + 4.8, gy - 0.8),
        0xfbbf24,
        2.0,
        1.0,
    );
    if adjusting {
        draw_articulated_hand(&mut over, cx + 2.0, gy + 7.5, 0.78, -0.18, 0., 1.);
    } else {
        draw_rim_paw(&mut over, cx - 22., RIM_Y, 0.88, 0.);
        draw_rim_paw(&mut over, cx + 22., RIM_Y, 0.88, 0.);
    }
    over.set_opacity(1.);
}

fn draw_pomodoro_headband(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let fade = smooth_step(p / 0.08) * smooth_step((1. - p) / 0.12);
    let tying = p < 0.40;
    let flutter = (p * 32.).sin() * 2.0;

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        for side in [-1_f32, 1.] {
            let spec = EyeSpec::dilated(cx + side * 13.5, 17.0, 4.8, 5.6, 0., -0.4);
            draw_expressive_eye(&mut inside, spec, 1.0);
            inside.line(
                cx + side * 18.5,
                11.8,
                cx + side * 8.5,
                13.2,
                IVORY,
                2.0,
            );
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.set_opacity(fade);
    let band = Rect::new(cx - 24., 6.6, 48., 4.6);
    over.fill_round(band.inset(-0.6), 2.6, OUTLINE_DARK, 0.45);
    over.fill_round(band, 2.3, PURE_WHITE, 0.98);
    over.fill_ellipse(cx, 8.9, 2.5, 2.5, 0xef4444, 1.0);

    over.quad_stroke(
        (cx + 24., 8.5),
        (cx + 31., 10.5 + flutter * 0.5),
        (cx + 36., 14.0 + flutter),
        PURE_WHITE,
        2.6,
        0.95,
    );
    over.quad_stroke(
        (cx + 24., 9.5),
        (cx + 29., 13.5 - flutter * 0.4),
        (cx + 33., 18.0 - flutter * 0.6),
        0xe2e8f0,
        2.2,
        0.90,
    );

    if tying {
        for side in [-1_f32, 1.] {
            draw_articulated_hand(&mut over, cx + side * 28., 16.5, 0.80, side * -0.42, p * 20., side);
        }
    } else {
        draw_rim_paw(&mut over, cx - 22., RIM_Y, 0.92, 0.25);
        draw_rim_paw(&mut over, cx + 22., RIM_Y, 0.92, 0.25);
        draw_star_sparkle(&mut over, (cx - 29., 14.5), 3.8, 0xfde047, 0.9 * fade);
    }
    over.set_opacity(1.);
}

fn draw_pomodoro_notepad(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let fade = smooth_step(p / 0.08) * smooth_step((1. - p) / 0.12);
    let scribble = (p * 44.).sin() * 3.0;

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        for side in [-1_f32, 1.] {
            let spec = EyeSpec::dilated(
                cx - 2. + side * 13.0 + scribble * 0.35,
                16.2,
                4.8,
                6.0,
                1.2 + scribble * 0.3,
                1.6,
            );
            draw_expressive_eye(&mut inside, spec, 1.0);
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.set_opacity(fade);
    draw_rim_paw(&mut over, cx - 23., RIM_Y, 0.88, 0.1);

    let pad = Rect::new(cx + 6., 23.0, 22., 14.5);
    over.fill_round(pad.inset(-0.7), 3.0, OUTLINE_DARK, 0.45);
    over.fill_round(pad, 2.5, 0xfefce8, 0.98);
    over.line(cx + 9.5, 27.5, cx + 24.5, 27.5, 0x93c5fd, 1.1);
    over.line(cx + 9.5, 31.5, cx + 22.5, 31.5, 0x93c5fd, 1.1);

    let pen_x = cx + 18. + scribble;
    over.push_transform(pen_x, 23.5, 0.58, 1., 1.);
    over.fill_round(Rect::new(-1.6, -9.5, 3.2, 10.5), 1.2, 0xfacc15, 1.0);
    over.fill_round(Rect::new(-1.6, -11.5, 3.2, 2.4), 1.0, BLUSH_ROSE, 1.0);
    over.fill_ellipse(0., 1.8, 1.1, 1.4, 0x1e293b, 1.0);
    over.pop_transform();

    draw_articulated_hand(&mut over, pen_x + 7.5, 21.5, 0.78, -0.35, p * 28., 1.);
    over.set_opacity(1.);
}

pub fn draw_pomodoro_complete(scene: &mut Scene, body: Rect, variant: u8, p: f32) {
    match variant % 3 {
        1 => draw_pomodoro_cozy_mug(scene, body, p),
        2 => draw_pomodoro_trophy(scene, body, p),
        _ => draw_pomodoro_confetti_classic(scene, body, p),
    }
}

fn draw_pomodoro_confetti_classic(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let fade = smooth_step(p / 0.08) * smooth_step((1. - p) / 0.12);
    let bounce = (p * 20.).sin().abs() * -3.0;
    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        for side in [-1_f32, 1.] {
            draw_happy_arch_eye(&mut inside, cx + side * 13.5, 15.2 + bounce, 10.8, 5.2, 0., IVORY);
        }
        draw_blush(&mut inside, cx, 21.2 + bounce, 19.5, 0.65);
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.set_opacity(fade);
    for side in [-1_f32, 1.] {
        draw_articulated_hand(
            &mut over,
            cx + side * 34.,
            17.5 + bounce * 0.5,
            0.92,
            side * 0.45,
            p * 28.,
            side,
        );
    }
    let colors = [0xf43f5e_u32, 0x38bdf8, 0xfacc15, 0x4ade80, 0xc084fc];
    for i in 0..10 {
        let ang = PI + (i as f32 / 9.) * PI;
        let dist = 12. + p * 28.;
        let px = cx + ang.cos() * dist;
        let py = (18. + ang.sin() * dist * 0.6 + p * 16.).clamp(2., 46.);
        over.fill_round(Rect::new(px - 1.8, py - 1.8, 3.6, 3.6), 1.0, colors[i % 5], 1.0);
    }
    over.set_opacity(1.);
}

fn draw_pomodoro_cozy_mug(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let fade = smooth_step(p / 0.08) * smooth_step((1. - p) / 0.12);
    let sway = (p * 12.).sin() * 1.2;

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        for side in [-1_f32, 1.] {
            draw_happy_arch_eye(&mut inside, cx + side * 13.5, 14.5, 10.6, 4.8, 0., IVORY);
        }
        draw_blush(&mut inside, cx, 20.2, 19.5, 0.66 * fade);
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.set_opacity(fade);
    let mug_y = 20.5;
    over.stroke_ellipse((cx + 8.2, mug_y + 6.8), (3.4, 4.0), 0xf97316, 2.2, 1.0);
    let mug = Rect::new(cx - 7.5, mug_y, 15.0, 14.0);
    over.fill_round(mug.inset(-0.7), 3.6, OUTLINE_DARK, 0.45);
    over.fill_round(mug, 3.2, 0xfb923c, 1.0);
    over.fill_round(Rect::new(cx - 5.8, mug_y + 1.5, 11.6, 3.0), 1.5, 0x7c2d12, 0.85);

    for i in 0..2 {
        let sx = cx - 2.5 + i as f32 * 5.0;
        let drift = (p * 18. + i as f32 * 1.5).sin() * 2.0 + sway * 0.4;
        over.quad_stroke(
            (sx, mug_y - 2.0),
            (sx + drift, mug_y - 7.5),
            (sx - drift * 0.5, mug_y - 12.5),
            PURE_WHITE,
            1.6,
            0.68 * fade,
        );
    }

    draw_articulated_hand(&mut over, cx - 10.5, mug_y + 6.5, 0.78, 0.65, 0., -1.);
    draw_articulated_hand(&mut over, cx + 10.5, mug_y + 6.5, 0.78, -0.65, 0., 1.);
    over.set_opacity(1.);
}

fn draw_pomodoro_trophy(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let fade = smooth_step(p / 0.08) * smooth_step((1. - p) / 0.12);
    let lift = (p * PI).sin() * 1.8;

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        for side in [-1_f32, 1.] {
            draw_happy_arch_eye(
                &mut inside,
                cx + side * 16.5,
                13.0 - lift * 0.25,
                10.4,
                4.8,
                0.,
                IVORY,
            );
        }
        draw_blush(&mut inside, cx, 19.2, 21.5, 0.65 * fade);
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.set_opacity(fade);
    let ty = 21.2 - lift;
    for side in [-1_f32, 1.] {
        over.stroke_ellipse((cx + side * 7.8, ty + 4.5), (3.2, 3.8), 0xfacc15, 2.0, 1.0);
    }
    let cup = Rect::new(cx - 7.0, ty, 14.0, 10.5);
    over.fill_round(cup.inset(-0.7), 4.0, OUTLINE_DARK, 0.45);
    over.fill_round(cup, 3.5, 0xfacc15, 1.0);
    over.fill_round(Rect::new(cx - 2.0, ty + 10.0, 4.0, 3.5), 1.0, 0xd97706, 1.0);
    over.fill_round(Rect::new(cx - 5.5, ty + 13.0, 11.0, 2.8), 1.4, 0xb45309, 1.0);

    for side in [-1_f32, 1.] {
        draw_articulated_hand(&mut over, cx + side * 12.2, ty + 7.2, 0.72, side * 0.52, 0., side);
    }
    draw_star_sparkle(&mut over, (cx - 27., 9.5), 4.0, 0xfef08a, 0.95 * fade);
    draw_star_sparkle(&mut over, (cx + 27., 9.5), 4.0, 0xfef08a, 0.95 * fade);
    over.set_opacity(1.);
}

pub fn draw_file_catch(
    scene: &mut Scene,
    body: Rect,
    variant: u8,
    p: f32,
    is_hover_waiting: bool,
    elapsed: f32,
) {
    match variant % 3 {
        1 => draw_file_finger_spin(scene, body, p, is_hover_waiting, elapsed),
        2 => draw_file_archive_box(scene, body, p, is_hover_waiting, elapsed),
        _ => draw_file_catch_classic(scene, body, p, is_hover_waiting, elapsed),
    }
}

fn draw_folder_card(over: &mut PaintLayer<'_>, cx: f32, fy: f32, w: f32, h: f32) {
    let folder = Rect::new(cx - w * 0.5, fy, w, h);
    over.fill_round(folder.inset(-0.7), 3.6, OUTLINE_DARK, 0.42);
    over.fill_round(Rect::new(folder.x, fy - 2.2, w * 0.44, 4.8), 2.0, 0xd97706, 1.0);
    over.fill_round(folder, 3.2, 0xfbbf24, 1.0);
}

fn draw_file_catch_classic(
    scene: &mut Scene,
    body: Rect,
    p: f32,
    is_hover_waiting: bool,
    elapsed: f32,
) {
    let cx = body.x + body.w * 0.5;
    if is_hover_waiting || p < 0.40 {
        let fp = if is_hover_waiting {
            0.35 + (elapsed * 5.).sin() * 0.08
        } else {
            p / 0.40
        };
        {
            let mut inside = PaintLayer(&mut scene.paint);
            draw_ambient_glow(&mut inside, body, 1.0);
            for side in [-1_f32, 1.] {
                let spec = EyeSpec::dilated(cx + side * 13.5, 16.0, 5.5, 6.8, 0., 2.0);
                draw_expressive_eye(&mut inside, spec, 1.0);
            }
        }
        let mut over = PaintLayer(&mut scene.overlay);
        let wave = if is_hover_waiting { elapsed * 8. } else { p * 22. };
        for side in [-1_f32, 1.] {
            draw_articulated_hand(&mut over, cx + side * 34., 25.5, 0.94, side * 0.55, wave, side);
        }
        draw_folder_card(&mut over, cx, 43.0 - fp * 19.0, 23.0, 16.5);
    } else if p < 0.78 {
        let hp = (p - 0.40) / 0.38;
        let bounce = (hp * TAU).sin() * 2.6;
        {
            let mut inside = PaintLayer(&mut scene.paint);
            draw_ambient_glow(&mut inside, body, 1.0);
            for side in [-1_f32, 1.] {
                draw_happy_arch_eye(
                    &mut inside,
                    cx + side * 13.5,
                    13.2 - bounce * 0.3,
                    10.0,
                    4.6,
                    0.,
                    IVORY,
                );
            }
            draw_blush(&mut inside, cx, 18.5, 20.0, 0.65);
        }
        let mut over = PaintLayer(&mut scene.overlay);
        let fy = 18.5 - bounce * 0.4;
        draw_folder_card(&mut over, cx, fy, 24.0, 17.0);
        draw_articulated_hand(&mut over, cx - 9.5, fy + 8.5, 0.82, 0.78, 0., -1.);
        draw_articulated_hand(&mut over, cx + 9.5, fy + 8.5, 0.82, -0.78, 0., 1.);
    } else {
        {
            let mut inside = PaintLayer(&mut scene.paint);
            draw_ambient_glow(&mut inside, body, 1.0);
            draw_happy_arch_eye(&mut inside, cx - 13.5, 15.5, 10.5, 5.0, 0., IVORY);
            draw_happy_arch_eye(&mut inside, cx + 13.5, 15.8, 10.5, 1.8, 0., IVORY);
            draw_blush(&mut inside, cx, 21.5, 19.0, 0.58);
        }
        let mut over = PaintLayer(&mut scene.overlay);
        draw_rim_paw(&mut over, cx - 22., RIM_Y, 0.88, 0.);
        draw_rim_paw(&mut over, cx + 22., RIM_Y, 0.88, 0.);
    }
}

fn draw_file_finger_spin(
    scene: &mut Scene,
    body: Rect,
    p: f32,
    is_hover_waiting: bool,
    elapsed: f32,
) {
    let cx = body.x + body.w * 0.5;
    if is_hover_waiting || p < 0.72 {
        let spin_t = if is_hover_waiting { elapsed * 6. } else { p * 30. };
        let spin_scale_x = spin_t.cos().abs().max(0.28);
        {
            let mut inside = PaintLayer(&mut scene.paint);
            draw_ambient_glow(&mut inside, body, 1.0);
            for side in [-1_f32, 1.] {
                let spec = EyeSpec::dilated(cx + side * 13.0, 15.8, 5.2, 6.6, 1.5, 0.6);
                draw_expressive_eye(&mut inside, spec, 1.0);
            }
        }
        let mut over = PaintLayer(&mut scene.overlay);
        draw_rim_paw(&mut over, cx - 23., RIM_Y, 0.88, 0.);
        let sx = cx + 22.;
        let sy = if is_hover_waiting { 22.0 } else { 17.5 };
        over.stroke_ellipse((sx, sy + 1.5), (13.0, 3.2), 0xfde047, 1.3, 0.65);
        over.push_transform(sx, sy, 0.18, spin_scale_x, 1.);
        draw_folder_card(&mut over, 0., -7.5, 20.0, 14.0);
        over.pop_transform();
        draw_articulated_hand(&mut over, sx, sy + 9.5, 0.86, 0.08, spin_t, 1.);
    } else {
        {
            let mut inside = PaintLayer(&mut scene.paint);
            draw_ambient_glow(&mut inside, body, 1.0);
            for side in [-1_f32, 1.] {
                draw_happy_arch_eye(&mut inside, cx + side * 13.5, 14.8, 10.8, 5.2, 0., IVORY);
            }
            draw_blush(&mut inside, cx, 20.8, 19.5, 0.65);
        }
        let mut over = PaintLayer(&mut scene.overlay);
        draw_folder_card(&mut over, cx, 19.5, 22.0, 15.5);
        draw_articulated_hand(&mut over, cx - 12., 26.5, 0.82, 0.65, 0., -1.);
        draw_articulated_hand(&mut over, cx + 12., 26.5, 0.82, -0.65, 0., 1.);
        draw_star_sparkle(&mut over, (cx - 28., 10.5), 4.2, 0xfde047, 0.95);
        draw_star_sparkle(&mut over, (cx + 28., 10.5), 4.2, 0xfde047, 0.95);
    }
}

fn draw_file_archive_box(
    scene: &mut Scene,
    body: Rect,
    p: f32,
    is_hover_waiting: bool,
    elapsed: f32,
) {
    let cx = body.x + body.w * 0.5;
    let packing = is_hover_waiting || p < 0.62;
    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        if packing {
            for side in [-1_f32, 1.] {
                let spec = EyeSpec::dilated(cx - 3. + side * 12.5, 16.0, 5.0, 6.4, 1.4, 1.4);
                draw_expressive_eye(&mut inside, spec, 1.0);
            }
        } else {
            draw_happy_arch_eye(&mut inside, cx - 14.0, 15.4, 10.6, 5.0, 0., IVORY);
            draw_happy_arch_eye(&mut inside, cx + 10.0, 15.6, 10.6, 2.0, 0., IVORY);
            draw_blush(&mut inside, cx - 2., 21.4, 19.0, 0.62);
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    let bx = cx + 22.;
    let by = 23.0;
    if packing {
        let drop_p = if is_hover_waiting {
            0.25 + (elapsed * 4.).sin() * 0.1
        } else {
            (p / 0.62).clamp(0., 1.)
        };
        draw_rim_paw(&mut over, cx - 24., RIM_Y, 0.88, 0.);
        draw_folder_card(&mut over, bx, 10.5 + drop_p * 11.0, 17.5, 12.0);
        over.line(bx - 12., by, bx - 16., by - 4.5, 0x38bdf8, 2.2);
        over.line(bx + 12., by, bx + 16., by - 4.5, 0x38bdf8, 2.2);
    } else {
        draw_thumbs_up_hand(&mut over, (cx - 29., 20.5), 0.96, -0.10, p * 22., -1.);
        draw_star_sparkle(&mut over, (cx - 41., 11.0), 4.0, 0x4ade80, 0.95);
    }
    let archive = Rect::new(bx - 12.0, by, 24.0, 14.5);
    over.fill_round(archive.inset(-0.7), 3.2, OUTLINE_DARK, 0.45);
    over.fill_round(archive, 2.8, 0x0284c7, 1.0);
    over.fill_round(Rect::new(bx - 4.5, by + 4.5, 9.0, 3.0), 1.5, 0xe0f2fe, 0.9);
}

pub fn draw_voice_listening(
    scene: &mut Scene,
    body: Rect,
    variant: u8,
    voice: VoiceState,
    p: f32,
    elapsed: f32,
) {
    match variant % 3 {
        1 => draw_voice_reporter(scene, body, voice, p, elapsed),
        2 => draw_voice_speech_bubble(scene, body, voice, p, elapsed),
        _ => draw_voice_studio_classic(scene, body, voice, p, elapsed),
    }
}

fn draw_voice_studio_classic(
    scene: &mut Scene,
    body: Rect,
    voice: VoiceState,
    p: f32,
    elapsed: f32,
) {
    let cx = body.x + body.w * 0.5;
    let pulse = 0.5 + 0.5 * (elapsed * 10.).sin();
    let show_bulb = voice == VoiceState::Transcribing || (voice == VoiceState::Idle && p > 0.55);

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        for side in [-1_f32, 1.] {
            let spec = EyeSpec::dilated(cx + side * 12.5, 15.8, 5.0, 6.5, -1.2, 0.3);
            draw_expressive_eye(&mut inside, spec, 1.0);
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    draw_articulated_hand(
        &mut over,
        cx - 31.,
        17.5,
        0.92,
        -0.34 + (elapsed * 5.).sin() * 0.08,
        0.,
        -1.,
    );
    for r in [11.0_f32, 15.0] {
        over.quad_stroke(
            (cx - 34. - r * 0.7, 11.5),
            (cx - 35. - r, 17.5),
            (cx - 34. - r * 0.7, 23.5),
            0x38bdf8,
            1.6,
            0.38 + pulse * 0.45,
        );
    }

    let mic_x = cx + 30.;
    let mic_y = 22.5;
    over.stroke_ellipse(
        (mic_x, mic_y - 4.5),
        (7.0 + pulse * 3.0, 7.0 + pulse * 3.0),
        0xf43f5e,
        1.6,
        0.35 + pulse * 0.5,
    );
    let capsule = Rect::new(mic_x - 3.6, mic_y - 9.5, 7.2, 9.8);
    over.fill_round(capsule.inset(-0.6), 4.0, OUTLINE_DARK, 0.45);
    over.fill_round(capsule, 3.6, 0xe2e8f0, 1.0);
    over.fill_round(Rect::new(mic_x - 3.6, mic_y - 4.8, 7.2, 1.8), 0.9, 0xf43f5e, 1.0);
    draw_articulated_hand(&mut over, mic_x + 4.2, mic_y + 3.2, 0.76, -0.2, 0., 1.);

    if show_bulb {
        over.fill_ellipse(cx, 6.8, 4.0, 4.0, 0xfef08a, 0.95);
        over.stroke_ellipse((cx, 6.8), (5.5, 5.5), 0xfacc15, 1.2, 0.65);
    }
}

fn draw_voice_reporter(
    scene: &mut Scene,
    body: Rect,
    voice: VoiceState,
    p: f32,
    elapsed: f32,
) {
    let cx = body.x + body.w * 0.5;
    let done = voice == VoiceState::Idle && p > 0.45;
    let scribble = if done { 0. } else { (elapsed * 22.).sin() * 2.2 };

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        if done {
            for side in [-1_f32, 1.] {
                draw_happy_arch_eye(&mut inside, cx + side * 13.0, 15.4, 10.6, 5.0, 0., IVORY);
            }
            draw_blush(&mut inside, cx, 21.4, 19.0, 0.62);
        } else {
            for side in [-1_f32, 1.] {
                let spec = EyeSpec::dilated(cx + side * 12.5, 15.8, 4.9, 6.3, 1.0, 0.8);
                draw_expressive_eye(&mut inside, spec, 1.0);
            }
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.push_transform(cx - 28., 21.0, 0.28, 1., 1.);
    over.fill_round(Rect::new(-2.0, -2.0, 4.0, 10.0), 1.6, 0x334155, 1.0);
    over.fill_ellipse(0., -5.5, 4.8, 5.4, OUTLINE_DARK, 0.45);
    over.fill_ellipse(0., -5.5, 4.2, 4.8, 0x38bdf8, 1.0);
    over.pop_transform();
    draw_articulated_hand(&mut over, cx - 31., 24.5, 0.78, 0.25, 0., -1.);

    let pad = Rect::new(cx + 15., 20.5, 18., 15.5);
    over.fill_round(pad.inset(-0.6), 2.8, OUTLINE_DARK, 0.45);
    over.fill_round(pad, 2.4, PURE_WHITE, 0.98);
    if done {
        over.path(
            (cx + 20.0, 28.5),
            vec![
                PathSegment::Line(cx + 23.2, 31.8),
                PathSegment::Line(cx + 29.0, 24.8),
            ],
            false,
            None,
            Some((0x22c55e, 2.4, 1.0)),
        );
    } else {
        over.line(cx + 18., 26., cx + 29., 26., 0x94a3b8, 1.2);
        over.line(cx + 18., 30., cx + 27., 30., 0x94a3b8, 1.2);
    }
    draw_articulated_hand(&mut over, cx + 33. + scribble, 24.5, 0.78, -0.35, elapsed * 14., 1.);
}

fn draw_voice_speech_bubble(
    scene: &mut Scene,
    body: Rect,
    voice: VoiceState,
    p: f32,
    elapsed: f32,
) {
    let cx = body.x + body.w * 0.5;
    let captured = voice == VoiceState::Idle && p > 0.48;
    let bob = (elapsed * 6.).sin() * 1.4;

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        if captured {
            for side in [-1_f32, 1.] {
                draw_happy_arch_eye(&mut inside, cx + side * 13.5, 15.2, 10.8, 5.2, 0., IVORY);
            }
            draw_blush(&mut inside, cx, 21.2, 19.5, 0.65);
        } else {
            for side in [-1_f32, 1.] {
                let spec = EyeSpec::dilated(cx + side * 13.0, 15.8, 5.0, 6.5, 1.8, -0.6);
                draw_expressive_eye(&mut inside, spec, 1.0);
            }
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    if !captured {
        draw_rim_paw(&mut over, cx - 22., RIM_Y, 0.88, 0.);
        draw_rim_paw(&mut over, cx + 18., RIM_Y, 0.88, 0.);
        let by = 8.5 + bob;
        let bubble = Rect::new(cx + 20., by, 24., 13.5);
        over.fill_round(bubble.inset(-0.7), 6.0, OUTLINE_DARK, 0.42);
        over.fill_round(bubble, 5.5, PURE_WHITE, 0.96);
        for dot in 0..3 {
            let pulse = ((elapsed * 6. - dot as f32 * 0.6).sin() * 0.5 + 0.5).clamp(0.25, 1.);
            over.fill_ellipse(
                cx + 26.5 + dot as f32 * 5.5,
                by + 6.8,
                1.6,
                1.6,
                0x0284c7,
                pulse,
            );
        }
    } else {
        let card = Rect::new(cx - 10., 19.0, 20., 14.0);
        over.fill_round(card.inset(-0.7), 3.4, OUTLINE_DARK, 0.45);
        over.fill_round(card, 3.0, 0xfde047, 1.0);
        draw_articulated_hand(&mut over, cx - 11.5, 25.5, 0.80, 0.65, 0., -1.);
        draw_articulated_hand(&mut over, cx + 11.5, 25.5, 0.80, -0.65, 0., 1.);
        draw_star_sparkle(&mut over, (cx + 24., 10.5), 4.4, 0xfef08a, 0.95);
    }
}

pub fn draw_charger_zap(scene: &mut Scene, body: Rect, variant: u8, p: f32) {
    match variant % 3 {
        1 => draw_charger_battery_hug(scene, body, p),
        2 => draw_charger_power_aura(scene, body, p),
        _ => draw_charger_static_classic(scene, body, p),
    }
}

fn draw_charger_static_classic(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let zap = p < 0.38;
    let jitter = if zap { (p * 170.).sin() * 2.8 } else { 0. };

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        if zap {
            for side in [-1_f32, 1.] {
                let mut spec = EyeSpec::standard(cx + jitter + side * 13.5, 15.5, 1.0);
                spec.rx = 5.6;
                spec.ry = 7.2;
                spec.color = 0x86efac;
                draw_expressive_eye(&mut inside, spec, 1.0);
            }
        } else {
            let purr = (p * 24.).sin() * 1.1;
            for side in [-1_f32, 1.] {
                draw_happy_arch_eye(
                    &mut inside,
                    cx + side * 13.5,
                    15.5 + purr,
                    10.8,
                    5.2,
                    0.,
                    0xbbf7d0,
                );
            }
            draw_blush(&mut inside, cx, 21.5 + purr, 19.0, 0.62);
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    if zap {
        for i in 0..7 {
            let ang = (i as f32 / 6.) * PI;
            over.line(
                cx + jitter + ang.cos() * 28.,
                15. + ang.sin() * 13.,
                cx + jitter + ang.cos() * 38.,
                15. + ang.sin() * 20.,
                0x4ade80,
                1.9,
            );
        }
    } else {
        draw_rim_paw(&mut over, cx - 22., RIM_Y, 0.88, 0.);
        draw_rim_paw(&mut over, cx + 22., RIM_Y, 0.88, 0.);
        let bx = cx + 31.;
        let by = 11.;
        over.path(
            (bx + 2., by - 5.),
            vec![
                PathSegment::Line(bx - 2.5, by + 1.),
                PathSegment::Line(bx + 1.0, by + 1.),
                PathSegment::Line(bx - 1.5, by + 7.),
            ],
            false,
            None,
            Some((0x4ade80, 2.1, 0.95)),
        );
    }
}

fn draw_charger_battery_hug(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let fade = smooth_step(p / 0.08) * smooth_step((1. - p) / 0.12);
    let bars = ((p * 4.2) as usize).clamp(1, 3);

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        if bars < 3 {
            for side in [-1_f32, 1.] {
                let mut spec = EyeSpec::dilated(cx - 4. + side * 12.0, 15.8, 4.9, 6.4, 1.4, 0.4);
                spec.color = 0xdcfce7;
                draw_expressive_eye(&mut inside, spec, 1.0);
            }
        } else {
            draw_happy_arch_eye(&mut inside, cx - 16.0, 15.4, 10.4, 5.0, 0., 0xbbf7d0);
            draw_happy_arch_eye(&mut inside, cx + 8.0, 15.4, 10.4, 5.0, 0., 0xbbf7d0);
            draw_blush(&mut inside, cx - 4., 21.4, 18.0, 0.62 * fade);
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.set_opacity(fade);
    draw_rim_paw(&mut over, cx - 25., RIM_Y, 0.88, 0.);

    let bx = cx + 24.;
    let by = 13.5;
    over.fill_round(Rect::new(bx - 3.0, by - 2.6, 6.0, 2.8), 1.2, 0x94a3b8, 1.0);
    let cell = Rect::new(bx - 7.0, by, 14.0, 20.0);
    over.fill_round(cell.inset(-0.7), 3.4, OUTLINE_DARK, 0.48);
    over.fill_round(cell, 3.0, 0x0f172a, 0.95);
    over.stroke_ellipse((bx, by + 10.0), (9.5, 12.0), 0x4ade80, 1.1, 0.35);

    for b in 0..bars {
        let row_y = by + 14.0 - b as f32 * 5.6;
        over.fill_round(Rect::new(bx - 4.8, row_y, 9.6, 4.2), 1.4, 0x4ade80, 1.0);
    }
    draw_articulated_hand(&mut over, bx + 8.0, by + 10.5, 0.80, -0.52, p * 16., 1.);
    if bars == 3 {
        draw_star_sparkle(&mut over, (bx, 6.5), 4.0, 0x86efac, 0.95 * fade);
    }
    over.set_opacity(1.);
}

fn draw_charger_power_aura(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let fade = smooth_step(p / 0.08) * smooth_step((1. - p) / 0.12);
    let pulse = (p * 26.).sin().abs();

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        for side in [-1_f32, 1.] {
            let mut spec = EyeSpec::dilated(cx + side * 13.5, 15.0 - pulse * 1.2, 5.2, 6.8, 0., -0.5);
            spec.color = 0xa7f3d0;
            draw_expressive_eye(&mut inside, spec, 1.0);
        }
        draw_blush(&mut inside, cx, 21.2, 19.5, 0.55 * fade);
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.set_opacity(fade);
    for side in [-1_f32, 1.] {
        draw_articulated_hand(
            &mut over,
            cx + side * 32.,
            18.5 - pulse * 1.5,
            0.90,
            side * -0.45,
            p * 24.,
            side,
        );
        over.quad_stroke(
            (cx + side * 38., 25.0),
            (cx + side * (44. + pulse * 3.), 15.5),
            (cx + side * 36., 6.5),
            0x4ade80,
            2.0,
            0.78 * fade,
        );
    }
    draw_star_sparkle(&mut over, (cx - 22., 7.5), 3.8, 0xfef08a, 0.92 * fade);
    draw_star_sparkle(&mut over, (cx + 22., 7.5), 3.8, 0x4ade80, 0.92 * fade);
    over.set_opacity(1.);
}

pub fn draw_task_cheer(scene: &mut Scene, body: Rect, variant: u8, p: f32) {
    match variant % 3 {
        1 => draw_task_painted_check(scene, body, p),
        2 => draw_task_double_thumbs_up(scene, body, p),
        _ => draw_task_cheer_classic(scene, body, p),
    }
}

fn draw_task_cheer_classic(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let fade = smooth_step(p / 0.08) * smooth_step((1. - p) / 0.14);
    let jump = (p * 18.).sin().abs() * 4.5;
    let eye_y = 16.0 - jump * 0.65;

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        for side in [-1_f32, 1.] {
            draw_happy_arch_eye(&mut inside, cx + side * 13.5, eye_y, 10.8, 5.2, 0., IVORY);
        }
        draw_blush(&mut inside, cx, eye_y + 5.8, 19.5, 0.68 * fade);
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.set_opacity(fade);
    for side in [-1_f32, 1.] {
        let wave = (p * 30.).sin() * 0.24;
        draw_articulated_hand(
            &mut over,
            cx + side * 34.,
            16.5 - jump * 0.3,
            0.92,
            side * 0.42 - side * wave,
            p * 30.,
            side,
        );
    }
    let badge_y = (9.0 - jump * 0.35).max(6.2);
    over.fill_ellipse(cx, badge_y, 5.4, 5.4, OUTLINE_DARK, 0.45);
    over.fill_ellipse(cx, badge_y, 4.8, 4.8, 0x4ade80, 1.0);
    over.path(
        (cx - 2.3, badge_y),
        vec![
            PathSegment::Line(cx - 0.4, badge_y + 1.9),
            PathSegment::Line(cx + 2.6, badge_y - 1.9),
        ],
        false,
        None,
        Some((0x052e16, 1.8, 1.0)),
    );
    over.set_opacity(1.);
}

fn draw_task_painted_check(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let fade = smooth_step(p / 0.08) * smooth_step((1. - p) / 0.14);
    let draw_t = (p / 0.58).clamp(0., 1.);

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        if p < 0.58 {
            for side in [-1_f32, 1.] {
                let spec = EyeSpec::dilated(cx + side * 13.0, 15.6, 4.9, 6.4, 1.6, -0.4);
                draw_expressive_eye(&mut inside, spec, 1.0);
            }
        } else {
            for side in [-1_f32, 1.] {
                draw_happy_arch_eye(&mut inside, cx + side * 13.5, 15.2, 10.8, 5.2, 0., IVORY);
            }
            draw_blush(&mut inside, cx, 21.2, 19.5, 0.65 * fade);
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.set_opacity(fade);
    draw_rim_paw(&mut over, cx - 24., RIM_Y, 0.88, 0.);

    let p0 = (cx + 19.0, 18.5);
    let p1 = (cx + 24.5, 24.0);
    let p2 = (cx + 37.5, 9.5);
    let tip = if draw_t < 0.36 {
        let u = draw_t / 0.36;
        let cur = (p0.0 + (p1.0 - p0.0) * u, p0.1 + (p1.1 - p0.1) * u);
        over.line(p0.0, p0.1, cur.0, cur.1, 0x4ade80, 3.4);
        cur
    } else {
        let u = (draw_t - 0.36) / 0.64;
        let cur = (p1.0 + (p2.0 - p1.0) * u, p1.1 + (p2.1 - p1.1) * u);
        over.path(
            p0,
            vec![PathSegment::Line(p1.0, p1.1), PathSegment::Line(cur.0, cur.1)],
            false,
            None,
            Some((0x4ade80, 3.6, 1.0)),
        );
        cur
    };

    if p < 0.65 {
        draw_articulated_hand(&mut over, tip.0 + 3.0, tip.1 + 7.5, 0.82, -0.25, p * 20., 1.);
    } else {
        draw_rim_paw(&mut over, cx + 18., RIM_Y, 0.88, 0.);
        draw_star_sparkle(&mut over, (p2.0 + 3.0, p2.1 - 1.5), 4.2, 0xfef08a, 0.95 * fade);
    }
    over.set_opacity(1.);
}

fn draw_task_double_thumbs_up(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let fade = smooth_step(p / 0.08) * smooth_step((1. - p) / 0.14);
    let bounce = (p * 18.).sin().abs() * -2.6;

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        draw_happy_arch_eye(&mut inside, cx - 13.5, 15.2 + bounce, 10.8, 5.2, -0.05, IVORY);
        let right = EyeSpec::dilated(cx + 13.5, 15.4 + bounce, 4.9, 6.4, -0.6, -0.4);
        draw_expressive_eye(&mut inside, right, 1.0);
        draw_blush(&mut inside, cx, 21.4 + bounce, 19.5, 0.66 * fade);
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.set_opacity(fade);
    for side in [-1_f32, 1.] {
        draw_thumbs_up_hand(
            &mut over,
            (cx + side * 32., 20.0 + bounce * 0.45),
            0.96,
            side * 0.12,
            p * 24.,
            side,
        );
    }
    draw_star_sparkle(&mut over, (cx - 23., 8.0), 3.8, 0xfde047, 0.92 * fade);
    draw_star_sparkle(&mut over, (cx + 23., 8.0), 3.8, 0x4ade80, 0.92 * fade);
    over.set_opacity(1.);
}
