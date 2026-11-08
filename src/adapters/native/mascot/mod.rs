pub mod choreographer;
mod clips_core;
mod clips_events;
mod rig;

use std::time::Instant;

pub use choreographer::{CursorSample, MascotChoreographer, MascotClip, MascotLiveState};
#[cfg(test)]
pub use choreographer::{
    MASCOT_ALERT_DURATION, MASCOT_GREETING_DURATION, MASCOT_HEADPHONES_DURATION,
    MASCOT_WAKE_DURATION, VARIANTS_PER_CLIP,
};

use super::model::{Rect, UiModel};
use super::scene::{Paint, Scene};
use crate::application::presenter::VoiceState;
use crate::domain::notch::NotchView;

pub fn should_render_in_closed(model: &UiModel, now: Instant) -> bool {
    model.compact_mascot(now)
        && model.choreographer.has_closed_stage(
            now,
            true,
            model.headphones.is_some(),
            model.focus_running(),
            model.voice,
            model.shelf_drag,
        )
}

fn draw_clip(
    scene: &mut Scene,
    body: Rect,
    clip: MascotClip,
    variant: u8,
    progress: f32,
    elapsed: f32,
) {
    match clip {
        MascotClip::WakeRubEye => clips_core::draw_wake_rub_eye(scene, body, variant, progress),
        MascotClip::GreetingWave => clips_core::draw_greeting_wave(scene, body, variant, progress),
        MascotClip::BellAlert => clips_core::draw_bell_alert(scene, body, variant, progress),
        MascotClip::Headphones => {
            clips_core::draw_headphones_groove(scene, body, variant, progress)
        }
        MascotClip::WaterSip => clips_events::draw_water_sip(scene, body, variant, progress),
        MascotClip::PomodoroStart => {
            clips_events::draw_pomodoro_study(scene, body, variant, progress)
        }
        MascotClip::PomodoroComplete => {
            clips_events::draw_pomodoro_complete(scene, body, variant, progress)
        }
        MascotClip::FileCatch => {
            clips_events::draw_file_catch(scene, body, variant, progress, false, elapsed)
        }
        MascotClip::VoiceReaction => clips_events::draw_voice_listening(
            scene,
            body,
            variant,
            VoiceState::Idle,
            progress,
            elapsed,
        ),
        MascotClip::ChargerZap => clips_events::draw_charger_zap(scene, body, variant, progress),
        MascotClip::TaskCheer => clips_events::draw_task_cheer(scene, body, variant, progress),
    }
}

pub fn render_peek_mascot(
    scene: &mut Scene,
    model: &UiModel,
    stage_center_x: f32,
    clip: MascotClip,
    now: Instant,
) {
    const STAGE_SCALE: f32 = 0.68;
    const STAGE_TOP: f32 = 17.5;
    let elapsed = now.saturating_duration_since(model.start).as_secs_f32();
    let (variant, progress) = model.choreographer.resolved_peek_clip(clip, now);

    let stage_pill = Rect::new(stage_center_x - 30., STAGE_TOP, 60., 20.4);
    scene.round(stage_pill, 10.2, 0x13161e);
    scene.alpha_border(stage_pill, 10.2, 0xffffff, 1.0, 0.09);

    let mut stage = Scene::default();
    draw_clip(
        &mut stage,
        Rect::new(-45., 0., 90., 30.),
        clip,
        variant,
        progress,
        elapsed,
    );

    scene.paint.push(Paint::PushTransform {
        x: stage_center_x,
        y: STAGE_TOP,
        angle: 0.,
        scale_x: STAGE_SCALE,
        scale_y: STAGE_SCALE,
    });
    scene.paint.append(&mut stage.paint);
    scene.paint.append(&mut stage.overlay);
    scene.paint.push(Paint::Opacity(1.));
    scene.paint.push(Paint::PopTransform);
}

pub fn render_mascot(scene: &mut Scene, model: &UiModel, body: Rect, now: Instant) {
    if !model.compact_mascot(now) {
        return;
    }
    let elapsed = now.saturating_duration_since(model.start).as_secs_f32();

    if matches!(model.voice, VoiceState::Recording | VoiceState::Transcribing) {
        let variant = model.choreographer.voice_live_variant();
        clips_events::draw_voice_listening(scene, body, variant, model.voice, 0., elapsed);
        return;
    }
    if model.shelf_drag {
        let variant = model.choreographer.shelf_hover_variant();
        clips_events::draw_file_catch(scene, body, variant, 0., true, elapsed);
        return;
    }

    if let Some((clip, variant, progress)) =
        model
            .choreographer
            .resolved_clip(now, true, model.headphones.is_some())
    {
        draw_clip(scene, body, clip, variant, progress, elapsed);
        return;
    }

    if model.dozing {
        clips_core::draw_dozing_bubble(scene, body, elapsed);
        return;
    }

    let stalk = model.stalk_amount(now);
    if stalk > 0.01 && (model.view == NotchView::Resting || !model.focus_running()) {
        clips_core::draw_cat_stalking(scene, model, body, now, elapsed, stalk);
        return;
    }

    clips_core::draw_idle_alive(scene, model, body, elapsed);
}
