use super::{OpenOptions, action_labels::action_name, controls::card, theme::*};
use eframe::egui::{
    self, Align2, CentralPanel, Color32, Context, Frame, Id, Pos2, Rect, Sense, Stroke, Ui, Vec2,
    vec2,
};
use openoptions::controls::{RING_SLOTS, Ring};

const RING_EDGE: f32 = 288.;

pub(super) struct RingWindow {
    actions: Ring,
    position: Pos2,
    seen_focus: bool,
}

impl RingWindow {
    pub fn new(context: &Context, actions: Ring, pointer: (i32, i32)) -> Self {
        let scale = if cfg!(target_os = "macos") {
            1.
        } else {
            context.native_pixels_per_point().unwrap_or(1.)
        };
        let position = egui::pos2(pointer.0 as f32 / scale, pointer.1 as f32 / scale)
            - Vec2::splat(RING_EDGE / 2.);
        Self {
            actions,
            position,
            seen_focus: false,
        }
    }
}

impl OpenOptions {
    pub fn show_actions_ring(&mut self, root: &mut Ui) {
        let Some(window) = &mut self.actions_ring else {
            return;
        };
        let ctx = root.ctx().clone();
        let outcome = ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("actions-ring-popup"),
            egui::ViewportBuilder::default()
                .with_title("Actions Ring")
                .with_inner_size([RING_EDGE, RING_EDGE])
                .with_position(window.position)
                .with_decorations(false)
                .with_resizable(false)
                .with_transparent(true)
                .with_taskbar(false)
                .with_window_level(egui::WindowLevel::AlwaysOnTop),
            |ui, class| {
                if class == egui::ViewportClass::EmbeddedWindow {
                    let response = egui::Modal::new(Id::new("actions-ring-embedded"))
                        .show(ui.ctx(), |ui| ring(ui, &window.actions, None, RING_EDGE));
                    let close = response.should_close() && response.inner.is_none();
                    return (response.inner, close, close);
                }
                let (focused, close) = ui.input(|input| {
                    (
                        input.focused,
                        input.viewport().close_requested() || input.key_pressed(egui::Key::Escape),
                    )
                });
                let lost_focus = window.seen_focus && !focused;
                window.seen_focus |= focused;
                let mut choice = None;
                CentralPanel::default()
                    .frame(Frame::NONE)
                    .show_inside(ui, |ui| {
                        choice = ring(ui, &window.actions, None, RING_EDGE);
                    });
                (choice, close || lost_focus, close && !lost_focus)
            },
        );
        if outcome.0.is_some() || outcome.1 {
            if let Some(live) = &self.live_controls {
                if outcome.1 {
                    live.session.dismiss_ring(outcome.2);
                } else if let Some(index) = outcome.0 {
                    live.session.choose_ring_slot(index);
                }
            }
            self.actions_ring = None;
            root.ctx().request_repaint();
        }
    }
}

pub(super) fn ring_preview(ui: &mut Ui, actions: &Ring) -> bool {
    ring(ui, actions, None, RING_EDGE).is_some()
}

pub(super) fn ring_editor(ui: &mut Ui, actions: &Ring, selected: &mut usize) {
    card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let edge = ui.available_width().clamp(180., RING_EDGE);
        ui.vertical_centered(|ui| {
            if let Some(index) = ring(ui, actions, Some(*selected), edge) {
                *selected = index;
            }
        });
    });
}

fn ring(ui: &mut Ui, actions: &Ring, selected: Option<usize>, edge: f32) -> Option<usize> {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(edge), Sense::hover());
    let center = rect.center();
    let radius = edge * 0.344;
    let bubble = edge * 0.105;
    let painter = ui.painter_at(rect);
    painter.circle_filled(center, radius + bubble + 5., SURFACE.gamma_multiply(0.98));
    painter.circle_stroke(center, radius + bubble + 4., line());
    painter.circle_stroke(center, radius, line());
    if selected.is_some() {
        painter.text(
            center,
            Align2::CENTER_CENTER,
            "Actions Ring",
            egui::FontId::proportional(12.),
            MUTED,
        );
    }
    let mut choice = None;
    for (index, action) in actions.iter().enumerate() {
        let angle =
            index as f32 * std::f32::consts::TAU / RING_SLOTS as f32 - std::f32::consts::FRAC_PI_2;
        let point = center + vec2(angle.cos(), angle.sin()) * radius;
        let slot = Rect::from_center_size(point, Vec2::splat(bubble * 2.));
        let name = action
            .as_ref()
            .map_or_else(|| "Unassigned".into(), action_name);
        let editable = selected.is_some() || action.is_some();
        let response = ui.interact(
            slot,
            ui.id().with(("ring-slot", index)),
            if editable {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        let active = selected == Some(index);
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                editable,
                active,
                format!("Slot {} · {name}", index + 1),
            )
        });
        let response = response.on_hover_text(&name);
        if editable && response.clicked() {
            choice = Some(index);
        }
        let tone = if active || response.hovered() {
            FOCUS
        } else {
            BORDER
        };
        painter.circle_filled(point, bubble, if active { SOLID } else { ELEVATED });
        painter.circle_stroke(
            point,
            bubble,
            Stroke::new(if active { 1.5_f32 } else { 1.0_f32 }, tone),
        );
        let glyph = if action.is_some() && selected.is_none() {
            name.chars().take(2).collect::<String>().to_uppercase()
        } else if selected.is_some() {
            (index + 1).to_string()
        } else {
            "·".into()
        };
        painter.text(
            point,
            Align2::CENTER_CENTER,
            glyph,
            egui::FontId::proportional(15.),
            if active { Color32::WHITE } else { TEXT },
        );
        if response.has_focus() {
            painter.circle_stroke(point, bubble + 3., Stroke::new(2.0_f32, FOCUS));
        }
    }
    choice
}
