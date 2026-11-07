use std::f32::consts::{PI, TAU};
use std::time::Instant;

use super::super::model::{Rect, UiModel};
use super::super::scene::{PathSegment, Scene};
use super::choreographer::PawSwipe;
use super::rig::{
    BLUSH_ROSE, EyeSpec, IVORY, OUTLINE_DARK, PAW_BEAN, PURE_WHITE, PaintLayer, draw_ambient_glow,
    draw_articulated_hand, draw_blush, draw_expressive_eye, draw_happy_arch_eye, draw_peace_hand,
    draw_rim_paw, draw_sleep_eye, draw_squint_eye, draw_stalk_cat_ears, draw_star_sparkle,
    smooth_step,
};

pub const RIM_Y: f32 = 29.8;

pub fn draw_dozing_bubble(scene: &mut Scene, body: Rect, elapsed: f32) {
    let cycle = elapsed % 7.0;
    let active_puff = cycle < 3.2;
    let breath = if active_puff {
        0.5 - 0.5 * (cycle / 3.2 * TAU).cos()
    } else {
        0.0
    };
    let cx = body.x + body.w * 0.5;
    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 0.52 + 0.12 * breath);
        for side in [-1_f32, 1.] {
            draw_sleep_eye(&mut inside, cx + side * 13.5, 15.8 + breath * 0.9, 10.5, 0.95);
        }
        draw_blush(&mut inside, cx, 21.5 + breath * 0.9, 18.5, 0.34);
    }

    let mut over = PaintLayer(&mut scene.overlay);
    let bubble_r = 3.2 + breath * 4.4;
    let bx = cx + 21.0 + bubble_r * 0.55;
    let by = 16.5 - bubble_r * 0.25;
    over.fill_ellipse(bx, by + 0.8, bubble_r + 0.6, bubble_r + 0.6, OUTLINE_DARK, 0.30);
    over.fill_ellipse(bx, by, bubble_r, bubble_r, 0x7dd3fc, 0.38);
    over.stroke_ellipse((bx, by), (bubble_r, bubble_r), 0xe0f2fe, 1.15, 0.88);
    let spec_r = (bubble_r * 0.24).max(0.8);
    over.fill_ellipse(
        bx - bubble_r * 0.34,
        by - bubble_r * 0.34,
        spec_r,
        spec_r,
        PURE_WHITE,
        0.92,
    );

    if active_puff {
        for i in 0..2 {
            let zp = ((cycle / 3.2) + i as f32 * 0.45).fract();
            let alpha = (zp * PI).sin() * 0.86;
            if alpha > 0.05 {
                over.set_opacity(alpha);
                let zx = cx - 32.0 - zp * 10.0 + (zp * 6.0).sin() * 2.5;
                let zy = (16.0 - zp * 13.0).max(1.5);
                over.text(
                    Rect::new(zx, zy, 14., 14.),
                    if i == 0 { "Z" } else { "z" },
                    9.0 + zp * 3.0,
                    0x93c5fd,
                    700,
                    true,
                );
                over.set_opacity(1.);
            }
        }
    }
}

pub fn draw_idle_alive(scene: &mut Scene, model: &UiModel, body: Rect, elapsed: f32) {
    let cx = body.x + body.w * 0.5;
    let phase = elapsed % 3.4;
    let blink = if elapsed > 3.4 && phase < 0.14 {
        (phase / 0.12).clamp(0., 1.)
    } else if elapsed > 3.4 && phase < 0.27 {
        1. - ((phase - 0.14) / 0.13).clamp(0., 1.)
    } else {
        0.
    };
    let step = (elapsed / 2.6).floor() as usize % 4;
    let offsets = [0_f32, -2.5, 0., 2.5];
    let t = ((elapsed % 2.6) / 0.28).clamp(0., 1.);
    let glance = offsets[(step + 3) % 4]
        + (offsets[step] - offsets[(step + 3) % 4]) * (1. - (1. - t).powi(3));
    let beat = if model.playing {
        (elapsed * 9.5).sin()
    } else {
        0.
    };
    let bob = if model.playing {
        beat.abs() * -1.5
    } else {
        0.
    };

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        for side in [-1_f32, 1.] {
            let spec = EyeSpec::standard(cx + side * 13.0 + glance, 15.0 + bob, 1.0 - blink);
            draw_expressive_eye(&mut inside, spec, 1.0);
        }
        if model.playing {
            draw_blush(&mut inside, cx + glance * 0.5, 21.5 + bob, 19.0, 0.42);
        }
    }

    if model.playing {
        let mut over = PaintLayer(&mut scene.overlay);
        draw_rim_paw(&mut over, cx - 24., RIM_Y, 0.9, beat.max(0.) * 0.55);
        let note_p = (elapsed * 0.7).fract();
        let alpha = (note_p * PI).sin() * 0.8;
        if alpha > 0.05 {
            over.set_opacity(alpha);
            over.text(
                Rect::new(cx + 28. + note_p * 8., (13. - note_p * 10.).max(1.5), 16., 16.),
                "♪",
                12.,
                0xc4b5fd,
                700,
                true,
            );
            over.set_opacity(1.);
        }
    }
}

pub fn draw_cat_stalking(
    scene: &mut Scene,
    model: &UiModel,
    body: Rect,
    now: Instant,
    elapsed: f32,
    stalk: f32,
) {
    let cx = body.x + body.w * 0.5;
    let gaze = model.gaze(now);
    let dx = (gaze.0 / 3.5).clamp(-1., 1.);
    let dy = (gaze.1 / 2.0).clamp(-0.4, 1.);

    let wiggle_window = elapsed % 2.4;
    let is_wiggling = model.cursor_near && (1.45..2.05).contains(&wiggle_window);
    let shimmy = if is_wiggling {
        (elapsed * 48.).sin() * 2.8 * stalk
    } else {
        0.
    };
    let phase = elapsed % 3.4;
    let blink = if elapsed > 3.4 && phase < 0.16 {
        (phase / 0.08).min(2. - phase / 0.08).clamp(0., 1.) * 0.85
    } else {
        0.
    };

    let eye_y = 15.0 + stalk * 3.6 + dy * 1.8;
    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        let rx = 4.3 + stalk * 2.3;
        let ry = 6.2 + stalk * 0.9 + if is_wiggling { 0.7 } else { 0. };
        for side in [-1_f32, 1.] {
            let mut spec = EyeSpec::dilated(
                cx + shimmy + side * 13.5 + dx * 4.4,
                eye_y,
                rx,
                ry,
                dx * 2.2,
                dy * 1.8,
            );
            spec.openness = 1.0 - blink;
            draw_expressive_eye(&mut inside, spec, 1.0);
        }
        draw_blush(
            &mut inside,
            cx + shimmy + dx * 3.0,
            eye_y + 5.2,
            20.5,
            0.44 * stalk,
        );
    }

    let mut over = PaintLayer(&mut scene.overlay);
    if stalk > 0.15 {
        draw_stalk_cat_ears(&mut over, cx + shimmy, (eye_y - 8.4).max(10.4), elapsed);
    }
    let paw_scale = (0.55 + 0.45 * stalk).clamp(0.55, 1.0);
    let press = if is_wiggling { 0.5 } else { 0. };

    if let Some(PawSwipe {
        progress: sp,
        target: (tx, ty),
        side,
    }) = model.choreographer.paw_swipe(now)
    {
        let swipe_dir = if side < 0. { -1_f32 } else { 1. };
        draw_rim_paw(
            &mut over,
            cx - swipe_dir * 23. + shimmy * 0.6,
            RIM_Y,
            paw_scale,
            press,
        );
        let reach = (sp * PI).sin();
        let clamped_tx = tx.clamp(cx - 46., cx + 46.);
        let clamped_ty = ty.clamp(32., 46.);
        let start_x = cx + swipe_dir * 22.;
        let paw_x = start_x + (clamped_tx - start_x) * 0.72 * reach;
        let paw_y = RIM_Y + (clamped_ty - RIM_Y) * 0.82 * reach;
        over.line(start_x, RIM_Y - 1.5, paw_x, paw_y, OUTLINE_DARK, 7.6);
        over.line(start_x, RIM_Y - 1.5, paw_x, paw_y, IVORY, 6.2);
        let aim = (clamped_ty - RIM_Y).atan2(clamped_tx - start_x) + 1.25;
        draw_articulated_hand(&mut over, paw_x, paw_y, 0.94, aim, sp * 22., swipe_dir);
    } else {
        draw_rim_paw(&mut over, cx - 23. + shimmy * 0.6, RIM_Y, paw_scale, press);
        draw_rim_paw(&mut over, cx + 23. + shimmy * 0.6, RIM_Y, paw_scale, press);
    }
}

pub fn draw_wake_rub_eye(scene: &mut Scene, body: Rect, variant: u8, p: f32) {
    match variant % 3 {
        1 => draw_wake_yawn(scene, body, p),
        2 => draw_wake_cheek_pat(scene, body, p),
        _ => draw_wake_rub_classic(scene, body, p),
    }
}

fn draw_bubble_pop_burst(over: &mut PaintLayer<'_>, cx: f32, pp: f32) {
    over.set_opacity((1. - pp).clamp(0., 1.));
    for i in 0..7 {
        let ang = i as f32 * TAU / 7.;
        let r0 = 4.0 + pp * 4.5;
        let r1 = 6.5 + pp * 7.0;
        over.line(
            cx + 24. + ang.cos() * r0,
            15.5 + ang.sin() * r0,
            cx + 24. + ang.cos() * r1,
            15.5 + ang.sin() * r1,
            0x7dd3fc,
            1.6,
        );
    }
    over.set_opacity(1.);
}

fn draw_wake_rub_classic(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let stretch = if p >= 0.68 {
        ((p - 0.68) / 0.32 * PI).sin()
    } else {
        0.
    };
    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 0.75 + 0.25 * p);
        if p < 0.20 {
            for side in [-1_f32, 1.] {
                let mut spec = EyeSpec::standard(cx + side * 13.5, 16.0, 0.45 + p * 1.2);
                spec.ry = 4.6;
                draw_expressive_eye(&mut inside, spec, 1.0);
            }
        } else if p < 0.68 {
            draw_happy_arch_eye(&mut inside, cx - 13.5, 16.5, 9.5, 3.0, 0., IVORY);
            let spec = EyeSpec::dilated(cx + 13.5, 15.8, 4.6, 5.8, -0.8, 0.);
            draw_expressive_eye(&mut inside, spec, 1.0);
        } else {
            for side in [-1_f32, 1.] {
                draw_happy_arch_eye(
                    &mut inside,
                    cx + side * 13.5,
                    15.2 - stretch * 1.5,
                    10.8,
                    5.2,
                    0.,
                    IVORY,
                );
            }
            draw_blush(&mut inside, cx, 21.2, 19.5, 0.58);
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    if p < 0.20 {
        draw_bubble_pop_burst(&mut over, cx, p / 0.20);
    } else if p < 0.68 {
        let rub = (p * 38.).sin() * 2.2;
        draw_articulated_hand(
            &mut over,
            cx - 19. + rub,
            19.5,
            0.86,
            0.52 + rub * 0.08,
            p * 24.,
            -1.,
        );
    } else {
        for side in [-1_f32, 1.] {
            draw_articulated_hand(
                &mut over,
                cx + side * (31. + stretch * 4.),
                19.5 - stretch * 3.5,
                0.88,
                side * 0.38,
                p * 22.,
                side,
            );
        }
    }
}

fn draw_wake_yawn(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let yawning = (0.18..0.76).contains(&p);
    let yawn = if yawning {
        ((p - 0.18) / 0.58 * PI).sin()
    } else {
        0.
    };
    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 0.8 + 0.2 * p);
        if p < 0.18 {
            for side in [-1_f32, 1.] {
                draw_sleep_eye(&mut inside, cx + side * 13.5, 16.0, 10.2, 0.95);
            }
        } else if yawning {
            for side in [-1_f32, 1.] {
                draw_happy_arch_eye(
                    &mut inside,
                    cx + side * 14.0,
                    13.6 - yawn * 1.1,
                    10.2,
                    4.2,
                    side * 0.08,
                    IVORY,
                );
            }
            inside.fill_ellipse(cx + 20.5, 15.8, 1.5, 1.9, 0x7dd3fc, 0.9 * yawn);
            let mrx = 2.2 + yawn * 2.8;
            let mry = 1.6 + yawn * 3.2;
            inside.fill_ellipse(cx, 21.8, mrx + 0.8, mry + 0.8, OUTLINE_DARK, 0.55);
            inside.fill_ellipse(cx, 21.8, mrx, mry, 0x9f1239, 0.95);
            inside.fill_ellipse(cx, 21.8 + mry * 0.35, mrx * 0.72, mry * 0.52, PAW_BEAN, 0.95);
        } else {
            for side in [-1_f32, 1.] {
                let spec = EyeSpec::standard(cx + side * 13.5, 15.4, 1.0);
                draw_expressive_eye(&mut inside, spec, 1.0);
            }
            draw_blush(&mut inside, cx, 21.4, 19.0, 0.52);
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    if p < 0.18 {
        draw_bubble_pop_burst(&mut over, cx, p / 0.18);
    } else if yawning {
        for side in [-1_f32, 1.] {
            draw_articulated_hand(
                &mut over,
                cx + side * (29. + yawn * 6.),
                20.0 - yawn * 4.2,
                0.88,
                side * (0.30 + yawn * 0.25),
                p * 18.,
                side,
            );
        }
    } else {
        draw_rim_paw(&mut over, cx - 22., RIM_Y, 0.88, 0.);
        draw_rim_paw(&mut over, cx + 22., RIM_Y, 0.88, 0.);
    }
}

fn draw_wake_cheek_pat(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let patting = (0.18..0.68).contains(&p);
    let pat = if patting {
        ((p - 0.18) * 44.).sin().abs()
    } else {
        0.
    };
    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 0.85 + 0.15 * p);
        if p < 0.18 {
            for side in [-1_f32, 1.] {
                let spec = EyeSpec::standard(cx + side * 13.5, 16.0, 0.52);
                draw_expressive_eye(&mut inside, spec, 1.0);
            }
        } else if patting {
            let squeeze = 1.0 - pat * 0.35;
            for side in [-1_f32, 1.] {
                let spec = EyeSpec::standard(cx + side * (13.0 - pat * 1.2), 15.6, squeeze);
                draw_expressive_eye(&mut inside, spec, 1.0);
            }
            draw_blush(&mut inside, cx, 21.0, 17.5, 0.35 + pat * 0.45);
        } else {
            for side in [-1_f32, 1.] {
                let spec = EyeSpec::dilated(cx + side * 13.5, 15.2, 5.0, 6.6, 0., -0.3);
                draw_expressive_eye(&mut inside, spec, 1.0);
            }
            draw_blush(&mut inside, cx, 21.4, 19.5, 0.62);
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    if p < 0.18 {
        draw_bubble_pop_burst(&mut over, cx, p / 0.18);
    } else if patting {
        for side in [-1_f32, 1.] {
            draw_articulated_hand(
                &mut over,
                cx + side * (25.5 - pat * 4.8),
                20.5,
                0.84,
                side * -0.52,
                p * 26.,
                side,
            );
            if pat > 0.65 {
                over.line(
                    cx + side * 31.,
                    14.,
                    cx + side * 35.,
                    12.,
                    BLUSH_ROSE,
                    1.6,
                );
            }
        }
    } else {
        let sp = ((p - 0.68) / 0.32 * PI).sin();
        draw_rim_paw(&mut over, cx - 23., RIM_Y, 0.90, 0.);
        draw_rim_paw(&mut over, cx + 23., RIM_Y, 0.90, 0.);
        draw_star_sparkle(&mut over, (cx - 31., 11.5), 4.2 * sp, 0xfde047, 0.95);
        draw_star_sparkle(&mut over, (cx + 31., 11.5), 4.2 * sp, 0xfde047, 0.95);
    }
}

pub fn draw_greeting_wave(scene: &mut Scene, body: Rect, variant: u8, p: f32) {
    match variant % 3 {
        1 => draw_greeting_double_wave(scene, body, p),
        2 => draw_greeting_peace(scene, body, p),
        _ => draw_greeting_classic(scene, body, p),
    }
}

fn draw_greeting_classic(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let env = smooth_step(p / 0.12) * smooth_step((1. - p) / 0.14);
    let wave_phase = p * 26.;
    let tilt = (p * 13.).sin() * 0.08;
    let bounce = (p * 13.).sin().abs() * -1.8;
    let is_winking = (0.40..0.58).contains(&p);

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        draw_happy_arch_eye(
            &mut inside,
            cx - 14.5,
            15.4 + bounce,
            10.8,
            5.2,
            tilt - 0.05,
            IVORY,
        );
        draw_happy_arch_eye(
            &mut inside,
            cx + 10.5,
            15.6 + bounce,
            10.8,
            if is_winking { 1.8 } else { 5.2 },
            tilt + 0.05,
            IVORY,
        );
        draw_blush(&mut inside, cx - 2., 21.4 + bounce, 19.0, 0.58 * env);
    }

    let mut over = PaintLayer(&mut scene.overlay);
    draw_rim_paw(
        &mut over,
        cx - 26.,
        RIM_Y,
        0.92 * env.max(0.45),
        (p * 13.).sin().abs() * 0.25,
    );
    let wrist_swing = wave_phase.sin() * 0.38;
    let hand_x = cx + 33.;
    let hand_y = 20.0 + bounce * 0.4 + (1. - env) * 5.0;
    draw_articulated_hand(
        &mut over,
        hand_x,
        hand_y,
        1.04 * env.max(0.3),
        wrist_swing,
        wave_phase,
        1.,
    );
    if env > 0.35 {
        over.quad_stroke(
            (hand_x + 7. + wrist_swing * 3., hand_y - 10.),
            (hand_x + 13. + wrist_swing * 4., hand_y - 5.),
            (hand_x + 11. + wrist_swing * 3., hand_y + 1.),
            IVORY,
            1.4,
            0.48 * env,
        );
    }
}

fn draw_greeting_double_wave(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let env = smooth_step(p / 0.12) * smooth_step((1. - p) / 0.14);
    let wave = p * 28.;
    let bounce = (p * 18.).sin().abs() * -2.4;

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        for side in [-1_f32, 1.] {
            draw_happy_arch_eye(
                &mut inside,
                cx + side * 13.5,
                15.2 + bounce,
                10.8,
                5.3,
                side * 0.06,
                IVORY,
            );
        }
        draw_blush(&mut inside, cx, 21.2 + bounce, 19.5, 0.64 * env);
    }

    let mut over = PaintLayer(&mut scene.overlay);
    for (idx, side) in [-1_f32, 1.].into_iter().enumerate() {
        let phase = wave + idx as f32 * 0.85;
        let swing = side * 0.25 + phase.sin() * 0.34;
        draw_articulated_hand(
            &mut over,
            cx + side * 34.,
            19.5 + bounce * 0.45 + (1. - env) * 4.5,
            0.96 * env.max(0.32),
            swing,
            phase,
            side,
        );
    }
    let twinkle = (p * PI).sin() * env;
    draw_star_sparkle(&mut over, (cx - 23., 8.5), 3.6 * twinkle, 0xfde047, 0.9);
    draw_star_sparkle(&mut over, (cx + 23., 8.5), 3.6 * twinkle, 0x38bdf8, 0.9);
}

fn draw_greeting_peace(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let env = smooth_step(p / 0.12) * smooth_step((1. - p) / 0.14);
    let bounce = (p * 14.).sin().abs() * -1.6;
    let tilt = 0.09 * env;

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        let left = EyeSpec::dilated(cx - 14.0, 15.5 + bounce, 4.9, 6.4, 0.8, -0.4);
        draw_expressive_eye(&mut inside, left, 1.0);
        draw_happy_arch_eye(&mut inside, cx + 11.0, 15.8 + bounce, 10.6, 4.8, tilt, IVORY);
        draw_blush(&mut inside, cx - 1.5, 21.4 + bounce, 19.0, 0.62 * env);
    }

    let mut over = PaintLayer(&mut scene.overlay);
    draw_rim_paw(&mut over, cx - 25., RIM_Y, 0.90 * env.max(0.45), 0.15);
    let hand_y = 20.5 + bounce * 0.35 + (1. - env) * 5.0;
    let wrist = 0.16 + (p * 18.).sin() * 0.12;
    draw_peace_hand(
        &mut over,
        (cx + 32., hand_y),
        1.02 * env.max(0.35),
        wrist,
        p * 24.,
        1.,
    );
    draw_star_sparkle(
        &mut over,
        (cx + 45., 11.0),
        4.2 * env,
        0xfde047,
        0.92 * env,
    );
    draw_star_sparkle(
        &mut over,
        (cx + 21., 8.2),
        2.8 * env,
        BLUSH_ROSE,
        0.85 * env,
    );
}

pub fn draw_bell_alert(scene: &mut Scene, body: Rect, variant: u8, p: f32) {
    match variant % 3 {
        1 => draw_bell_cover_ears(scene, body, p),
        2 => draw_bell_town_crier(scene, body, p),
        _ => draw_bell_swat_classic(scene, body, p),
    }
}

fn draw_golden_bell(over: &mut PaintLayer<'_>, pos: (f32, f32), angle: f32, damp: f32, p: f32) {
    over.push_transform(pos.0, pos.1, angle, 1., 1.);
    if damp > 0.2 {
        for side in [-1_f32, 1.] {
            for r in [11.5_f32, 15.2] {
                over.quad_stroke(
                    (side * (r - 1.5), 3.0),
                    (side * (r + 2.2), 8.5),
                    (side * (r - 1.5), 14.0),
                    0xfde047,
                    1.6,
                    0.62 * damp,
                );
            }
        }
    }
    let clapper_lag = (p * 56. - 0.65).sin() * 3.5 * damp;
    over.fill_ellipse(clapper_lag, 14.8, 2.8, 2.8, 0xd97706, 1.0);
    over.path(
        (0., 0.),
        vec![
            PathSegment::Cubic(-7.2, 0., -8.0, 8.8, -10.5, 13.2),
            PathSegment::Quad(0., 15.6, 10.5, 13.2),
            PathSegment::Cubic(8.0, 8.8, 7.2, 0., 0., 0.),
        ],
        true,
        Some((0xfacc15, 1.0)),
        Some((0x92400e, 1.1, 0.7)),
    );
    over.quad_stroke((-4.2, 3.0), (-5.2, 7.5), (-6.4, 11.2), 0xfef9c3, 1.5, 0.85);
    over.stroke_ellipse((0., -1.8), (2.1, 2.1), 0xfef08a, 1.8, 1.0);
    over.pop_transform();
}

fn draw_bell_swat_classic(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let startled = p < 0.20;
    let ringing = p < 0.58;
    let swatting = (0.50..0.76).contains(&p);
    let relieved = p >= 0.72;
    let jump = if startled {
        (p / 0.20 * PI).sin() * 4.2
    } else {
        0.
    };
    let eye_y = 15.5 - jump * 0.75;

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        if relieved {
            draw_happy_arch_eye(&mut inside, cx - 15.5, eye_y, 10.5, 5.0, 0., IVORY);
            draw_happy_arch_eye(&mut inside, cx + 8.5, eye_y, 10.5, 5.0, 0., IVORY);
            draw_blush(&mut inside, cx - 3.5, eye_y + 5.8, 19.0, 0.58);
        } else {
            let look_x = if ringing { 4.8 } else { 1.5 };
            let ry = if startled { 7.4 } else { 6.3 };
            let left_eye = EyeSpec::dilated(cx - 15.5 + look_x, eye_y, 5.1, ry, 1.8, -0.4);
            draw_expressive_eye(&mut inside, left_eye, 1.0);

            let is_squinting = ringing && (p * 56.).cos() > 0.12;
            if is_squinting {
                draw_squint_eye(&mut inside, cx + 9.5 + look_x, eye_y, false);
            } else {
                let right_eye =
                    EyeSpec::dilated(cx + 9.5 + look_x, eye_y, 4.9, ry * 0.94, 1.8, -0.4);
                draw_expressive_eye(&mut inside, right_eye, 1.0);
            }
            if startled {
                inside.text(Rect::new(cx - 10., 2., 14., 14.), "!", 12.5, 0xfde047, 800, true);
            }
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    draw_rim_paw(&mut over, cx - 26., RIM_Y, 0.88, 0.);

    let bell_x = cx + 41.;
    let bell_y = 7.2;
    let fade = smooth_step(p / 0.05) * smooth_step((1. - p) / 0.10);
    let damp = if ringing {
        1.0
    } else {
        (1. - (p - 0.58) * 5.5).clamp(0., 1.)
    };
    let bell_angle = (p * 56.).sin() * 0.48 * damp;

    if swatting {
        let sp = (p - 0.50) / 0.26;
        let reach = (sp * PI).sin();
        draw_articulated_hand(
            &mut over,
            cx + 16. + reach * 16.,
            22. - reach * 4.5,
            0.86,
            0.38 + reach * 0.28,
            p * 30.,
            1.,
        );
        if (0.40..0.66).contains(&sp) {
            for a in 0..6 {
                let ang = a as f32 * PI / 3.;
                let ix = bell_x - 8.;
                let iy = bell_y + 9.;
                over.line(
                    ix + ang.cos() * 3.0,
                    iy + ang.sin() * 3.0,
                    ix + ang.cos() * 7.2,
                    iy + ang.sin() * 7.2,
                    0xfef08a,
                    1.6,
                );
            }
        }
    }

    over.set_opacity(fade);
    draw_golden_bell(&mut over, (bell_x, bell_y), bell_angle, damp, p);
    over.set_opacity(1.);
}

fn draw_bell_cover_ears(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let ringing = p < 0.70;
    let shiver = if ringing { (p * 88.).sin() * 1.4 } else { 0. };
    let fade = smooth_step(p / 0.06) * smooth_step((1. - p) / 0.10);

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        if ringing {
            draw_squint_eye(&mut inside, cx - 14.5 + shiver, 16.2, false);
            draw_squint_eye(&mut inside, cx + 8.5 + shiver, 16.2, true);
            draw_blush(&mut inside, cx - 3. + shiver, 21.8, 17.5, 0.45);
        } else {
            draw_happy_arch_eye(&mut inside, cx - 14.5, 15.6, 10.5, 4.8, 0., IVORY);
            draw_happy_arch_eye(&mut inside, cx + 9.5, 15.6, 10.5, 4.8, 0., IVORY);
            draw_blush(&mut inside, cx - 2.5, 21.4, 19.0, 0.58);
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    if ringing {
        draw_articulated_hand(
            &mut over,
            cx - 25. + shiver,
            17.5,
            0.84,
            0.58,
            p * 24.,
            -1.,
        );
        draw_articulated_hand(
            &mut over,
            cx + 19. + shiver,
            17.5,
            0.84,
            -0.58,
            p * 24.,
            1.,
        );
    } else {
        draw_rim_paw(&mut over, cx - 23., RIM_Y, 0.88, 0.);
        draw_rim_paw(&mut over, cx + 19., RIM_Y, 0.88, 0.);
        over.fill_ellipse(cx - 29., 12.0, 2.2, 3.0, 0x7dd3fc, 0.85 * fade);
    }

    let damp = if ringing {
        1.0
    } else {
        (1. - (p - 0.70) * 6.0).clamp(0., 1.)
    };
    over.set_opacity(fade);
    draw_golden_bell(&mut over, (cx + 43., 7.4), (p * 62.).sin() * 0.52 * damp, damp, p);
    over.set_opacity(1.);
}

fn draw_bell_town_crier(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let fade = smooth_step(p / 0.06) * smooth_step((1. - p) / 0.12);
    let damp = (1. - (p - 0.78).max(0.) * 4.5).clamp(0., 1.);
    let shake = (p * 54.).sin() * damp;
    let bob = (p * 27.).sin().abs() * -1.8;

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        for side in [-1_f32, 1.] {
            let spec = EyeSpec::dilated(cx + side * 13.0, 15.4 + bob, 5.0, 6.6, 1.2, -0.4);
            draw_expressive_eye(&mut inside, spec, 1.0);
        }
        draw_blush(&mut inside, cx, 21.4 + bob, 19.0, 0.56 * fade);
        if damp > 0.35 {
            inside.text(Rect::new(cx - 7., 2., 14., 14.), "!", 12.0, 0xfde047, 800, true);
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.set_opacity(fade);
    draw_articulated_hand(
        &mut over,
        cx - 33.,
        19.5 + bob * 0.4,
        0.90,
        -0.28 + (p * 22.).sin() * 0.24,
        p * 26.,
        -1.,
    );

    let hand_x = cx + 27.;
    let hand_y = 22.5 + bob * 0.3;
    over.push_transform(cx + 35., 12.5, shake * 0.48, 0.92, 0.92);
    over.fill_round(Rect::new(-2.0, -5.5, 4.0, 6.5), 1.8, 0x92400e, 1.0);
    pop_and_ring_handbell(&mut over, shake, damp, p);
    draw_articulated_hand(&mut over, hand_x, hand_y, 0.82, 0.22 + shake * 0.22, p * 20., 1.);
    over.set_opacity(1.);
}

fn pop_and_ring_handbell(over: &mut PaintLayer<'_>, shake: f32, damp: f32, p: f32) {
    if damp > 0.2 {
        for side in [-1_f32, 1.] {
            over.quad_stroke(
                (side * 10.5, 3.0),
                (side * 13.5, 8.0),
                (side * 10.5, 13.0),
                0xfde047,
                1.6,
                0.65 * damp,
            );
        }
    }
    over.fill_ellipse((p * 54. - 0.6).sin() * 3.2 * damp, 13.8, 2.5, 2.5, 0xd97706, 1.0);
    over.path(
        (0., 0.),
        vec![
            PathSegment::Cubic(-6.5, 0., -7.5, 8.0, -9.8, 12.2),
            PathSegment::Quad(0., 14.4, 9.8, 12.2),
            PathSegment::Cubic(7.5, 8.0, 6.5, 0., 0., 0.),
        ],
        true,
        Some((0xfacc15, 1.0)),
        Some((0x92400e, 1.1, 0.7)),
    );
    let _ = shake;
    over.pop_transform();
}

pub fn draw_headphones_groove(scene: &mut Scene, body: Rect, variant: u8, p: f32) {
    match variant % 3 {
        1 => draw_headphones_dj_scratch(scene, body, p),
        2 => draw_headphones_rock_drum(scene, body, p),
        _ => draw_headphones_classic(scene, body, p),
    }
}

fn draw_floating_music_notes(over: &mut PaintLayer<'_>, head_x: f32, p: f32, fade: f32) {
    let glyphs = ["♪", "♫"];
    let colors = [0x38bdf8_u32, 0xc084fc];
    for i in 0..2 {
        let np = (p * 2.8 + i as f32 * 0.5).fract();
        let side = if i == 0 { -1_f32 } else { 1. };
        let nx = head_x + side * (33. + np * 14.);
        let ny = (20. - np * 15.).max(1.5);
        let alpha = (np * PI).sin() * fade;
        if alpha > 0.05 {
            over.set_opacity(alpha);
            over.text(Rect::new(nx - 8., ny, 16., 16.), glyphs[i], 13., colors[i], 700, true);
            over.set_opacity(fade);
        }
    }
}

fn draw_headphones_classic(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let putting_on = p < 0.25;
    let snapping = (0.25..0.36).contains(&p);
    let grooving = p >= 0.30;
    let fade = smooth_step(p / 0.06) * smooth_step((1. - p) / 0.12);

    let beat = (p * 38.).sin();
    let sway_x = if grooving { (p * 19.).sin() * 4.8 } else { 0. };
    let bob_y = if grooving {
        beat.abs() * 2.5
    } else if snapping {
        ((p - 0.25) / 0.11 * PI).sin() * 3.0
    } else {
        0.
    };
    let head_x = cx + sway_x;
    let head_y = 15.5 + bob_y * 0.5;

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        if putting_on {
            for side in [-1_f32, 1.] {
                let spec = EyeSpec::dilated(head_x + side * 13.0, head_y - 1.2, 4.9, 6.4, 0., -2.0);
                draw_expressive_eye(&mut inside, spec, 1.0);
            }
        } else {
            for side in [-1_f32, 1.] {
                draw_happy_arch_eye(
                    &mut inside,
                    head_x + side * 13.0,
                    head_y + 0.5,
                    10.5,
                    5.0,
                    sway_x * 0.02,
                    IVORY,
                );
            }
            draw_blush(&mut inside, head_x, head_y + 5.8, 19.0, 0.58 * fade);
        }
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.set_opacity(fade);
    let drop_y = if putting_on {
        9.0 + smooth_step(p / 0.25) * 7.0
    } else {
        head_y
    };
    let stretch_x = if putting_on {
        1.0 + (p / 0.25 * PI).sin() * 0.22
    } else if snapping {
        0.92
    } else {
        1.0
    };
    let span = 24.5 * stretch_x;
    over.quad_stroke(
        (head_x - span, drop_y - 0.5),
        (head_x, (drop_y - 14.5).max(2.0)),
        (head_x + span, drop_y - 0.5),
        0xc4b5fd,
        3.2,
        1.0,
    );

    let bass = if grooving {
        1.0 + beat.max(0.) * 0.16
    } else {
        1.0
    };
    for side in [-1_f32, 1.] {
        let cup_x = head_x + side * span;
        over.push_transform(cup_x, drop_y + 0.5, 0., bass, bass);
        over.fill_round(Rect::new(-4.8, -7.4, 9.6, 14.8), 4.8, OUTLINE_DARK, 0.45);
        over.fill_round(Rect::new(-4.3, -6.9, 8.6, 13.8), 4.3, 0x818cf8, 1.0);
        let cushion_x = if side < 0. { 0.8 } else { -3.6 };
        over.fill_round(Rect::new(cushion_x, -5.3, 2.8, 10.6), 1.4, 0xe0e7ff, 1.0);
        over.pop_transform();

        if p < 0.34 {
            draw_articulated_hand(
                &mut over,
                cup_x + side * 3.2,
                drop_y + 5.2,
                0.78,
                side * -0.48,
                p * 20.,
                side,
            );
        }
    }

    if grooving {
        draw_rim_paw(&mut over, head_x - 20., RIM_Y, 0.86, beat.max(0.) * 0.6);
        draw_floating_music_notes(&mut over, head_x, p, fade);
    }
    over.set_opacity(1.);
}

fn draw_headphones_dj_scratch(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let fade = smooth_step(p / 0.06) * smooth_step((1. - p) / 0.12);
    let beat = (p * 42.).sin();
    let head_x = cx - 3.0 + (p * 21.).sin() * 2.5;
    let head_y = 15.6 + beat.abs() * 1.4;

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        draw_happy_arch_eye(&mut inside, head_x - 13.0, head_y, 10.4, 5.0, -0.06, IVORY);
        let right = EyeSpec::dilated(head_x + 12.0, head_y, 4.9, 6.2, 1.4, 1.2);
        draw_expressive_eye(&mut inside, right, 1.0);
        draw_blush(&mut inside, head_x, head_y + 5.6, 18.5, 0.54 * fade);
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.set_opacity(fade);
    over.quad_stroke(
        (head_x - 24., head_y + 0.5),
        (head_x - 2., 3.2),
        (head_x + 19., head_y - 3.5),
        0x38bdf8,
        3.0,
        1.0,
    );
    over.push_transform(head_x - 24., head_y + 1.0, 0.12, 1., 1.);
    over.fill_round(Rect::new(-4.8, -7.2, 9.6, 14.4), 4.8, OUTLINE_DARK, 0.45);
    over.fill_round(Rect::new(-4.2, -6.6, 8.4, 13.2), 4.2, 0x0284c7, 1.0);
    over.pop_transform();
    draw_articulated_hand(&mut over, head_x - 27., head_y + 5.5, 0.80, 0.48, p * 18., -1.);

    let disc_x = cx + 25.;
    let disc_y = 29.8;
    over.fill_ellipse(disc_x, disc_y + 0.8, 14.5, 5.4, OUTLINE_DARK, 0.5);
    over.fill_ellipse(disc_x, disc_y, 13.8, 4.8, 0x1e293b, 1.0);
    over.stroke_ellipse((disc_x, disc_y), (9.6, 3.2), 0x475569, 1.0, 0.85);
    over.fill_ellipse(disc_x, disc_y, 4.4, 1.7, 0x38bdf8, 1.0);

    let scratch = (p * 48.).sin() * 4.2;
    draw_articulated_hand(
        &mut over,
        disc_x + 4. + scratch,
        24.8,
        0.84,
        -0.42 + scratch * 0.05,
        p * 34.,
        1.,
    );
    draw_floating_music_notes(&mut over, cx, p, fade);
    over.set_opacity(1.);
}

fn draw_headphones_rock_drum(scene: &mut Scene, body: Rect, p: f32) {
    let cx = body.x + body.w * 0.5;
    let fade = smooth_step(p / 0.06) * smooth_step((1. - p) / 0.12);
    let beat = (p * 44.).sin();
    let sway_x = (p * 22.).sin() * 5.2;
    let head_x = cx + sway_x;
    let head_y = 15.8 + beat.abs() * 2.0;

    {
        let mut inside = PaintLayer(&mut scene.paint);
        draw_ambient_glow(&mut inside, body, 1.0);
        for side in [-1_f32, 1.] {
            draw_happy_arch_eye(
                &mut inside,
                head_x + side * 13.0,
                head_y,
                11.0,
                5.4,
                sway_x * 0.03 + side * 0.08,
                IVORY,
            );
        }
        draw_blush(&mut inside, head_x, head_y + 5.6, 19.5, 0.65 * fade);
    }

    let mut over = PaintLayer(&mut scene.overlay);
    over.set_opacity(fade);
    let span = 24.8;
    over.quad_stroke(
        (head_x - span, head_y - 0.5),
        (head_x, (head_y - 14.2).max(2.2)),
        (head_x + span, head_y - 0.5),
        0xfda4af,
        3.2,
        1.0,
    );
    let pulse = 1.0 + beat.abs() * 0.14;
    for side in [-1_f32, 1.] {
        over.push_transform(head_x + side * span, head_y + 0.5, 0., pulse, pulse);
        over.fill_round(Rect::new(-4.8, -7.2, 9.6, 14.4), 4.8, OUTLINE_DARK, 0.45);
        over.fill_round(Rect::new(-4.2, -6.6, 8.4, 13.2), 4.2, 0xf43f5e, 1.0);
        over.pop_transform();
    }

    draw_rim_paw(&mut over, cx - 22., RIM_Y, 0.92, beat.max(0.) * 0.85);
    draw_rim_paw(&mut over, cx + 22., RIM_Y, 0.92, (-beat).max(0.) * 0.85);
    draw_floating_music_notes(&mut over, head_x, p, fade);
    draw_star_sparkle(&mut over, (head_x, 6.5), 3.4 * beat.abs(), 0xfde047, 0.9 * fade);
    over.set_opacity(1.);
}
