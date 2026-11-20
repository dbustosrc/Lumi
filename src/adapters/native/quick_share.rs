use std::f32::consts::{FRAC_PI_2, TAU};
use std::sync::{Arc, LazyLock};
use std::time::Instant;

use super::model::{
    Rect, ShelfRemoval, ShelfRemovalFrame, Shell, UiModel, list_transition,
    list_transition_duration,
};
use super::scene::{Action, Paint, Scene, TextSpan};
use super::views::{
    SHELF_CARD_STEP, ShelfLayout, ShelfUse, TRAY_BACKGROUND, drop_zone_outline, icon_button,
    shelf_cards, shelf_extent,
};
use crate::application::quick_share::{Command, Request, Snapshot, Tray};
use crate::domain::shelf::format_byte_size;
use crate::i18n::translate as tr;

const WHITE: u32 = 0xffffff;
const DIM: u32 = 0xa4a4aa;
const BUTTON: u32 = 0x303034;
const RECEIVE_ERROR: u32 = 0xff8680;
const DEVICES_PER_PAGE: usize = 4;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Pane {
    #[default]
    Compact,
    Send,
    Receive,
    Radar,
    Request,
}

#[derive(Default)]
pub struct Ui {
    pane: Pane,
    detail_history: Vec<Pane>,
    tray_motion: Option<(f32, Instant)>,
    send_scroll: f32,
    receive_scroll: f32,
    send_removal: ShelfRemoval,
    receive_removal: ShelfRemoval,
    device_offset: usize,
}

impl Pane {
    fn expansion(self) -> Option<f32> {
        match self {
            Self::Compact => Some(0.),
            Self::Send => Some(1.),
            Self::Receive => Some(-1.),
            Self::Radar | Self::Request => None,
        }
    }
}

impl Ui {
    pub fn pane(&self) -> Pane {
        self.pane
    }

    pub fn navigate(&mut self, pane: Pane, now: Instant, fast: bool) {
        if self.pane == pane {
            return;
        }
        if pane.expansion().is_none() {
            self.detail_history.push(self.pane);
        } else {
            self.detail_history.clear();
        }
        self.tray_motion = self
            .pane
            .expansion()
            .zip(pane.expansion())
            .map(|_| (self.expansion(now, fast), now));
        self.pane = pane;
    }

    pub fn back(&mut self) {
        if let Some(origin) = self.detail_history.pop() {
            self.pane = origin;
            self.tray_motion = None;
        }
    }

    pub(super) fn expansion(&self, now: Instant, fast: bool) -> f32 {
        let target = self.pane.expansion().unwrap_or_default();
        self.tray_motion.map_or(target, |(from, start)| {
            list_transition(from, target, start, now, fast)
        })
    }

    pub fn animating(&self, now: Instant, fast: bool) -> bool {
        // Include the final frame so hit targets settle with their controls.
        self.pane == Pane::Radar
            || self.send_removal.animating(now)
            || self.receive_removal.animating(now)
            || self.tray_motion.is_some_and(|(_, start)| {
                now.duration_since(start).as_secs_f32() < list_transition_duration(fast) + 0.02
            })
    }

    pub(super) fn remember_removed(
        &mut self,
        previous: Snapshot,
        current: &Snapshot,
        now: Instant,
    ) {
        self.send_removal
            .remember(previous.outgoing, &current.outgoing, now);
        self.receive_removal
            .remember(previous.received, &current.received, now);
    }

    pub(super) fn expire_removals(&mut self, now: Instant) {
        self.send_removal.expire(now);
        self.receive_removal.expire(now);
    }

    fn removal(&self, tray: Tray, now: Instant) -> Option<ShelfRemovalFrame<'_>> {
        match tray {
            Tray::Outgoing => self.send_removal.frame(now),
            Tray::Received => self.receive_removal.frame(now),
        }
    }
}

static ICON: LazyLock<(u32, u32, Arc<Vec<u8>>)> = LazyLock::new(|| {
    let icon = image::load_from_memory(include_bytes!("../../../assets/branding/quick-share.webp"))
        .expect("embedded Quick Share icon")
        .into_rgba8();
    (icon.width(), icon.height(), Arc::new(icon.into_raw()))
});

pub fn draw(scene: &mut Scene, model: &UiModel, body: Rect, now: Instant) {
    draw_icon(scene, Rect::new(body.x + 15., 10., 17., 17.));
    scene.text_run(
        Rect::new(body.x + 37., 10., 103., 18.),
        vec![
            TextSpan::new("Quick", 14., WHITE, 600),
            TextSpan::new("Share", 14., 0x4385f5, 600),
        ],
        3.,
    );
    visibility(
        scene,
        body.x + body.w - 28.,
        model.quick_share.visible,
        now.duration_since(model.start).as_secs_f32(),
    );

    let pane = model.quick_share_ui.pane();
    match pane {
        Pane::Compact | Pane::Send | Pane::Receive => {
            let amount = model
                .quick_share_ui
                .expansion(now, model.settings.desktop.fast_animations);
            let minimum_width = card(body, true, true).w;
            for (outgoing, rect) in [true, false].into_iter().zip(tray_rects(body, amount)) {
                if rect.w <= 0. {
                    continue;
                }
                let active_pane = if outgoing { Pane::Send } else { Pane::Receive };
                let hits = scene.hits.len();
                let width = rect.w.max(minimum_width);
                let x = if outgoing {
                    rect.x
                } else {
                    rect.x + rect.w - width
                };
                scene.clip(rect);
                tray(
                    scene,
                    model,
                    Rect::new(x, rect.y, width, rect.h),
                    outgoing,
                    pane != active_pane,
                    now,
                );
                scene.unclip();
                if pane != Pane::Compact && pane != active_pane {
                    scene.hits.truncate(hits);
                }
            }
        }
        Pane::Radar => {
            let rect = card(body, false, true);
            detail(scene, rect);
            radar(scene, model, rect, now);
        }
        Pane::Request => {
            let rect = card(body, false, false);
            detail(scene, rect);
            if let Some(request) = &model.quick_share.request {
                request_card(scene, rect, request);
            }
        }
    }
}

pub(super) fn draw_icon(scene: &mut Scene, rect: Rect) {
    let (width, height, bytes) = &*ICON;
    scene
        .paint
        .push(Paint::Bitmap(rect, *width, *height, bytes.clone()));
}

fn visibility(scene: &mut Scene, x: f32, visible: bool, elapsed: f32) {
    let color = if visible { 0x44d879 } else { 0x74747a };
    let y = 18.;
    if visible {
        pulse_outline(scene, (x, y), (elapsed / 1.8).fract(), color);
    }
    scene.round(Rect::new(x - 3., y - 3., 6., 6.), 3., color);
}

fn pulse_outline(scene: &mut Scene, point: (f32, f32), phase: f32, color: u32) {
    let radius = 4. + 6. * phase;
    scene.paint.push(Paint::Ellipse {
        center: point,
        radius: (radius, radius),
        color,
        alpha: 0.52 * (1. - phase).powi(2),
        stroke: Some(1.1),
    });
}

fn card(body: Rect, compact: bool, outgoing: bool) -> Rect {
    let span = body.w - 84.;
    let half = (span - 10.) / 2.;
    Rect::new(
        body.x + 42. + if compact && !outgoing { half + 10. } else { 0. },
        34.,
        if compact { half } else { span },
        92.,
    )
}

fn tray_rects(body: Rect, expansion: f32) -> [Rect; 2] {
    let full = card(body, false, true);
    let half = card(body, true, true).w;
    let grow = full.w - half;
    let send = half + grow * expansion.max(0.) - half * (-expansion).max(0.);
    let receive = half + grow * (-expansion).max(0.) - half * expansion.max(0.);
    [
        Rect::new(full.x, full.y, send, full.h),
        Rect::new(full.x + full.w - receive, full.y, receive, full.h),
    ]
}

fn top_left(rect: Rect) -> Rect {
    Rect::new(rect.x + 6., rect.y + 4., 16., 16.)
}

fn detail(scene: &mut Scene, rect: Rect) {
    scene.round(rect, 13., TRAY_BACKGROUND);
    let button = top_left(rect);
    scene.round(button, 8., BUTTON);
    scene.icon(button.inset(3.5), "\u{e76b}", WHITE);
    scene.hit(button, Action::ShareBack, tr("quickShare.back"));
}

fn tray(
    scene: &mut Scene,
    model: &UiModel,
    rect: Rect,
    outgoing: bool,
    compact: bool,
    now: Instant,
) {
    let state = &model.quick_share;
    let kind = if outgoing {
        Tray::Outgoing
    } else {
        Tray::Received
    };
    let (files, offset, label, icon, next, width) = if outgoing {
        (
            &state.outgoing,
            model.quick_share_ui.send_scroll,
            "quickShare.devices",
            "quick-share-radar",
            Pane::Radar,
            90.,
        )
    } else {
        (
            &state.received,
            model.quick_share_ui.receive_scroll,
            "quickShare.review",
            "quick-share-incoming",
            Pane::Request,
            65.,
        )
    };
    let show_action = (outgoing || state.request.is_some()) && (!compact || files.is_empty());
    let show_clear = !files.is_empty() && (!outgoing || !state.sending);
    let removal = model.quick_share_ui.removal(kind, now);
    let clear_width = if compact { 16. } else { 62. };
    let mut controls_right = rect.x + rect.w - 6.;
    scene.round(rect, 13., TRAY_BACKGROUND);
    drop_zone_outline(scene, rect, model.shelf_drag && outgoing);
    let receive_error = state.received_status.filter(|key| {
        matches!(
            *key,
            "quickShare.receiveStartError"
                | "quickShare.transferError"
                | "quickShare.unsupportedReceive"
                | "quickShare.storageError"
        )
    });
    if files.is_empty() && removal.is_none() {
        let (title, hint) = if outgoing {
            ("notch.shelf.dropTitle", "notch.shelf.pasteHint")
        } else {
            ("quickShare.empty", "quickShare.receiveHint")
        };
        scene.icon(
            Rect::new(rect.x + (rect.w - 24.) / 2., rect.y + 27., 24., 24.),
            "\u{e896}",
            DIM,
        );
        scene.text(
            Rect::new(rect.x + 5., rect.y + 53., rect.w - 10., 16.),
            tr(title),
            11.5,
            WHITE,
            true,
            true,
        );
        let status = if outgoing {
            state.status
        } else {
            state.received_status
        };
        let hint_rect = Rect::new(rect.x + 5., rect.y + 66., rect.w - 10., 16.);
        scene.text(
            hint_rect,
            tr(status.unwrap_or(hint)),
            9.3,
            if outgoing || receive_error.is_none() {
                DIM
            } else {
                RECEIVE_ERROR
            },
            false,
            true,
        );
        if outgoing && status.is_none() {
            scene.hit(
                hint_rect,
                Action::Share(Command::Browse),
                tr("notch.shelf.browse"),
            );
        }
    } else {
        let viewport = file_viewport(rect);
        let total = shelf_extent(files.len());
        let max_scroll = (total - viewport.w).max(0.);
        let scroll = shelf_cards(
            scene,
            model,
            viewport,
            files,
            ShelfLayout {
                scroll: offset,
                removal,
                use_for: ShelfUse::QuickShare(kind),
            },
        );
        if !outgoing && let Some(key) = receive_error {
            let action_width = if show_action { width + 4. } else { 0. }
                + if show_clear { clear_width + 4. } else { 0. };
            scene.text(
                Rect::new(rect.x + 27., rect.y + 5., rect.w - 33. - action_width, 14.),
                tr(key),
                9.3,
                RECEIVE_ERROR,
                false,
                false,
            );
        }
        if max_scroll > 0. && removal.is_none() {
            let track = Rect::new(viewport.x + 3., rect.y + rect.h - 4., viewport.w - 6., 2.);
            let thumb = track.w * viewport.w / total;
            scene.round(track, 1., 0x37373c);
            scene.round(
                Rect::new(
                    track.x + scroll / max_scroll * (track.w - thumb),
                    track.y,
                    thumb,
                    track.h,
                ),
                1.,
                0x828289,
            );
        }
    }
    let button = top_left(rect);
    scene.round(button, 6., BUTTON);
    scene.icon(
        button.inset(2.),
        if compact {
            "quick-share-expand"
        } else {
            "quick-share-collapse"
        },
        WHITE,
    );
    let pane = if !compact {
        Pane::Compact
    } else if outgoing {
        Pane::Send
    } else {
        Pane::Receive
    };
    scene.hit(
        button,
        Action::SharePane(pane),
        tr(if compact {
            "quickShare.expand"
        } else {
            "quickShare.collapse"
        }),
    );
    if show_clear {
        controls_right -= clear_width;
        let area = Rect::new(controls_right, rect.y + 4., clear_width, 16.);
        let action = Action::Share(Command::Clear(kind));
        let label = tr("notch.shelf.clearButton");
        if compact {
            icon_button(scene, area, "\u{e74d}", action, &label, BUTTON);
        } else {
            scene.button(area, label, "\u{e74d}", action, BUTTON);
        }
        controls_right -= 4.;
    }
    if show_action {
        scene.button(
            Rect::new(controls_right - width, rect.y + 4., width, 16.),
            tr(label),
            icon,
            Action::SharePane(next),
            BUTTON,
        );
    }
}

fn file_viewport(card: Rect) -> Rect {
    Rect::new(card.x + 5., card.y + 14., card.w - 10., 74.)
}

pub fn scroll(model: &mut UiModel, delta: f32) {
    let state = &model.quick_share;
    let ui = &mut model.quick_share_ui;
    if ui.pane == Pane::Radar {
        ui.device_offset = (ui.device_offset as isize - delta.signum() as isize).clamp(
            0,
            state.devices.len().saturating_sub(DEVICES_PER_PAGE) as isize,
        ) as usize;
        return;
    }
    if ui.pane == Pane::Request {
        return;
    }
    let body = Shell::for_view(model.view, model.nav).body();
    let compact = ui.pane == Pane::Compact;
    let outgoing = ui.pane == Pane::Send || compact && model.mouse.0 < body.x + body.w / 2.;
    let rect = card(body, compact, outgoing);
    if !rect.contains(model.mouse.0, model.mouse.1) {
        return;
    }
    let (offset, files) = if outgoing {
        (&mut ui.send_scroll, &state.outgoing)
    } else {
        (&mut ui.receive_scroll, &state.received)
    };
    let max = (shelf_extent(files.len()) - file_viewport(rect).w).max(0.);
    *offset = (*offset - delta * SHELF_CARD_STEP).clamp(0., max);
}

fn radar(scene: &mut Scene, model: &UiModel, rect: Rect, now: Instant) {
    let state = &model.quick_share;
    let elapsed = now.duration_since(model.start).as_secs_f32();
    scene.text(
        Rect::new(rect.x + 29., rect.y + 3., 170., 17.),
        tr("quickShare.nearby"),
        10.5,
        WHITE,
        true,
        false,
    );
    let center = (rect.x + rect.w / 2., rect.y + 48.);
    let radius = 28.;
    scene.paint.push(Paint::Ellipse {
        center,
        radius: (radius, radius),
        color: 0x142334,
        alpha: 0.8,
        stroke: None,
    });
    scene.line(
        center.0 - radius,
        center.1,
        center.0 + radius,
        center.1,
        0x293d52,
        0.7,
    );
    scene.line(
        center.0,
        center.1 - radius,
        center.0,
        center.1 + radius,
        0x293d52,
        0.7,
    );
    for radius in [9., 18., radius] {
        scene.paint.push(Paint::Ellipse {
            center,
            radius: (radius, radius),
            color: 0x6aa9ed,
            alpha: 0.38,
            stroke: Some(0.8),
        });
    }
    let sweep = (elapsed / 2.5).fract() * TAU - FRAC_PI_2;
    for (index, color) in [0x1e334a, 0x243f59, 0x2c4d6b, 0x355e84, 0x487dad, 0x69b4fa]
        .into_iter()
        .enumerate()
    {
        let angle = sweep - (5 - index) as f32 * 0.085;
        let (sin, cos) = angle.sin_cos();
        scene.line(
            center.0,
            center.1,
            center.0 + radius * cos,
            center.1 + radius * sin,
            color,
            if index == 5 { 1.3 } else { 2.4 },
        );
    }
    scene.round(
        Rect::new(center.0 - 3., center.1 - 3., 6., 6.),
        3.,
        0xb6ddff,
    );
    scene.text(
        Rect::new(center.0 - 25., center.1 + 5., 50., 11.),
        tr("quickShare.thisPc"),
        8.,
        0xb9d7f9,
        true,
        true,
    );
    let can_send = !state.outgoing.is_empty() && !state.sending;
    let offset = model
        .quick_share_ui
        .device_offset
        .min(state.devices.len().saturating_sub(DEVICES_PER_PAGE));
    for (index, device) in state
        .devices
        .iter()
        .skip(offset)
        .take(DEVICES_PER_PAGE)
        .enumerate()
    {
        let right = index % 2 != 0;
        let direction = if right { 1. } else { -1. };
        let y = center.1 - 14. + (index / 2) as f32 * 27.;
        let point = (center.0 + direction * 20., y);
        let edge = center.0 + direction * 40.;
        let width = rect.w / 2. - 51.;
        let label = Rect::new(if right { edge } else { edge - width }, y - 8.5, width, 17.);
        scene.line(point.0, point.1, edge, y, 0x355673, 0.8);
        pulse_outline(
            scene,
            point,
            (elapsed / 1.7 + index as f32 * 0.19).fract(),
            0x73b5ff,
        );
        scene.round(
            Rect::new(point.0 - 2.5, point.1 - 2.5, 5., 5.),
            2.5,
            0x73b5ff,
        );
        if can_send {
            scene.button(
                label,
                &device.name,
                "",
                Action::Share(Command::Send(device.id.clone())),
                0x235589,
            );
        } else {
            scene.round(label, 8.5, BUTTON);
            scene.text(label.inset(2.), &device.name, 10.5, DIM, true, true);
        }
    }
    let status = if let Some(pin) = &state.pin {
        format!("{}  {pin}", tr("quickShare.code"))
    } else if let Some(key) = state.status {
        tr(key)
    } else if state.devices.is_empty() {
        tr("quickShare.searching")
    } else if state.outgoing.is_empty() {
        tr("quickShare.addBeforeSend")
    } else if state.outgoing.len() > 1
        && state
            .outgoing
            .iter()
            .any(|entry| entry.item.text().is_some())
    {
        tr("quickShare.separateTransfers")
    } else {
        String::new()
    };
    scene.text(
        Rect::new(rect.x + 10., rect.y + 78., rect.w - 20., 12.),
        status,
        9.,
        DIM,
        false,
        true,
    );
}

fn request_card(scene: &mut Scene, rect: Rect, request: &Request) {
    let title = format!("{} {}", tr("quickShare.from"), request.sender);
    let detail = match request.files.as_slice() {
        [] => request
            .text_description
            .as_deref()
            .filter(|title| !title.trim().is_empty())
            .map_or_else(|| tr("quickShare.text"), str::to_owned),
        [file] => format!("{file} · {}", format_byte_size(request.bytes)),
        files => format!(
            "{} {} · {}",
            files.len(),
            tr("quickShare.files"),
            format_byte_size(request.bytes)
        ),
    };
    scene.text(
        Rect::new(rect.x + 13., rect.y + 30., 126., 17.),
        title,
        11.5,
        WHITE,
        true,
        false,
    );
    scene.text(
        Rect::new(rect.x + 13., rect.y + 50., 126., 15.),
        detail,
        9.5,
        DIM,
        false,
        false,
    );
    scene.text(
        Rect::new(rect.x + 137., rect.y + 35., 50., 13.),
        tr("quickShare.code"),
        9.,
        DIM,
        false,
        true,
    );
    scene.text(
        Rect::new(rect.x + 137., rect.y + 48., 50., 20.),
        &request.pin,
        15.,
        0x73b5ff,
        700u16,
        true,
    );
    for (accept, width, inset, key, color) in [
        (false, 63., 144., "quickShare.reject", 0x343438),
        (true, 62., 73., "quickShare.accept", 0x0e502d),
    ] {
        scene.button(
            Rect::new(rect.x + rect.w - inset, rect.y + 48., width, 20.),
            tr(key),
            "",
            Action::Share(Command::Decide {
                id: request.id.clone(),
                accept,
            }),
            color,
        );
    }
}
