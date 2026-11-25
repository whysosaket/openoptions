use super::{OpenOptions, action_labels::action_name, controls::card, screen, theme::*};
use eframe::egui::{
    self, Align2, CentralPanel, Color32, Context, Frame, Id, Key, Modifiers, Pos2, Rect, Sense,
    Stroke, Ui, Vec2, vec2,
};
use openoptions::controls::{RING_SLOTS, Ring};

const RING_EDGE: f32 = 288.;

pub(super) struct RingWindow {
    actions: Ring,
    pointer: Pos2,
    display: Option<Rect>,
    position: Pos2,
    scale: f32,
    keyboard_pending: bool,
    seen_focus: bool,
}

impl RingWindow {
    pub fn new(context: &Context, actions: Ring, pointer: (i32, i32)) -> Self {
        let display = screen::containing_screen(pointer);
        let pointer = egui::pos2(pointer.0 as f32, pointer.1 as f32);
        let scale = window_scale(context);
        Self {
            actions,
            pointer,
            display,
            position: screen::overlay_origin(pointer, display, scale, RING_EDGE),
            scale,
            keyboard_pending: true,
            seen_focus: false,
        }
    }
}

fn window_scale(context: &Context) -> f32 {
    if cfg!(target_os = "macos") {
        context.zoom_factor()
    } else {
        context.pixels_per_point()
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
                .with_clamp_size_to_monitor_size(true)
                .with_active(true)
                .with_transparent(true)
                .with_taskbar(false)
                .with_window_level(egui::WindowLevel::AlwaysOnTop),
            |ui, class| {
                if class == egui::ViewportClass::EmbeddedWindow {
                    let initial = std::mem::take(&mut window.keyboard_pending);
                    let response = egui::Modal::new(Id::new("actions-ring-embedded"))
                        .show(ui.ctx(), |ui| {
                            ring(ui, &window.actions, None, RING_EDGE, initial)
                        });
                    let close = response.should_close() && response.inner.0.is_none();
                    let outside = response.backdrop_response.clicked() || response.inner.1;
                    return (response.inner.0, close || outside, close && !outside);
                }
                let scale = window_scale(ui.ctx());
                if scale != window.scale {
                    window.scale = scale;
                    window.position =
                        screen::overlay_origin(window.pointer, window.display, scale, RING_EDGE);
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::OuterPosition(window.position));
                }
                let (focused, close) = ui.input(|input| {
                    (
                        input.focused,
                        input.viewport().close_requested() || input.key_pressed(Key::Escape),
                    )
                });
                let lost_focus = window.seen_focus && !focused;
                window.seen_focus |= focused;
                let initial = focused && std::mem::take(&mut window.keyboard_pending);
                let layout = CentralPanel::default()
                    .frame(Frame::NONE)
                    .show_inside(ui, |ui| ring(ui, &window.actions, None, RING_EDGE, initial));
                let (choice, outside) = layout.inner;
                if choice.is_some() || close || lost_focus || outside {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Visible(false));
                }
                (
                    choice,
                    close || lost_focus || outside,
                    close && !lost_focus && !outside,
                )
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
    ring(ui, actions, None, RING_EDGE, false).0.is_some()
}

pub(super) fn ring_editor(ui: &mut Ui, actions: &Ring, selected: &mut usize, max_edge: f32) {
    card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let edge = ui.available_width().min(max_edge);
        ui.vertical_centered(|ui| {
            if let Some(index) = ring(ui, actions, Some(*selected), edge, false).0 {
                *selected = index;
            }
        });
    });
}

fn ring(
    ui: &mut Ui,
    actions: &Ring,
    selected: Option<usize>,
    edge: f32,
    initial_focus: bool,
) -> (Option<usize>, bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(edge), Sense::hover());
    let center = rect.center();
    let radius = edge * 0.344;
    let bubble = edge * 0.105;
    let painter = ui.painter_at(rect);
    painter.circle_filled(center, radius + bubble + 5., SURFACE.gamma_multiply(0.98));
    painter.circle_stroke(center, radius + bubble + 4., line());
    painter.circle_stroke(center, radius, line());
    let slot_id = |index| ui.id().with(("ring-slot", index));
    let focused = ui.memory(|memory| memory.focused());
    let current = (0..RING_SLOTS).find(|&index| focused == Some(slot_id(index)));
    let arrow = if current.is_some() || initial_focus {
        ui.input_mut(|input| {
            if input.consume_key(Modifiers::NONE, Key::ArrowRight)
                || input.consume_key(Modifiers::NONE, Key::ArrowDown)
            {
                1
            } else if input.consume_key(Modifiers::NONE, Key::ArrowLeft)
                || input.consume_key(Modifiers::NONE, Key::ArrowUp)
            {
                -1
            } else {
                0
            }
        })
    } else {
        0
    };
    let move_to = if arrow != 0 {
        ui.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
        (1..=RING_SLOTS)
            .map(|step| {
                let from = current.map_or(if arrow > 0 { -1 } else { RING_SLOTS as isize }, |i| {
                    i as isize
                });
                (from + arrow * step as isize).rem_euclid(RING_SLOTS as isize) as usize
            })
            .find(|&index| selected.is_some() || actions[index].is_some())
    } else if initial_focus {
        actions.iter().position(Option::is_some)
    } else {
        None
    };
    let mut choice = None;
    let mut center_label = selected.map(|_| "Actions Ring".to_owned());
    for (index, action) in actions.iter().enumerate() {
        let angle =
            index as f32 * std::f32::consts::TAU / RING_SLOTS as f32 - std::f32::consts::FRAC_PI_2;
        let point = center + vec2(angle.cos(), angle.sin()) * radius;
        let slot = Rect::from_center_size(point, Vec2::splat(bubble * 2.));
        let name = action
            .as_ref()
            .map_or_else(|| "Unassigned".into(), action_name);
        let interactive = selected.is_some() || action.is_some();
        let response = ui.interact(
            slot,
            slot_id(index),
            if interactive {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        let active = selected == Some(index);
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                interactive,
                active,
                format!("Slot {} · {name}", index + 1),
            )
        });
        let response = response.on_hover_text(&name);
        if move_to == Some(index) {
            response.request_focus();
        }
        if interactive && response.clicked() {
            response.request_focus();
            choice = Some(index);
        }
        let focus = response.has_focus();
        if active || response.hovered() || focus {
            center_label = Some(name);
        }
        let tone = if active || response.hovered() || focus {
            FOCUS
        } else {
            BORDER
        };
        let fill = if active {
            SOLID
        } else if action.is_some() {
            HOVER
        } else {
            ELEVATED
        };
        painter.circle_filled(point, bubble, fill);
        painter.circle_stroke(
            point,
            bubble,
            Stroke::new(if active { 1.5_f32 } else { 1.0_f32 }, tone),
        );
        let glyph = if selected.is_some() || action.is_some() {
            (index + 1).to_string()
        } else {
            "·".into()
        };
        painter.text(
            point,
            Align2::CENTER_CENTER,
            glyph,
            egui::FontId::proportional((edge * 0.065).clamp(10., 15.)),
            if active { Color32::WHITE } else { TEXT },
        );
        if focus {
            painter.circle_stroke(point, bubble + 3., Stroke::new(2.0_f32, FOCUS));
        }
    }
    if let Some(label) = center_label {
        let width = (radius - bubble) * 2. - 8.;
        let mut layout =
            egui::text::LayoutJob::simple(label, egui::FontId::proportional(12.), TEXT, width);
        layout.wrap.max_rows = 1;
        let text = painter.layout_job(layout);
        painter.galley(center - text.size() * 0.5, text, TEXT);
    }
    let outside = ui.input(|input| {
        input.pointer.any_click()
            && input.pointer.interact_pos().is_some_and(|point| {
                rect.contains(point) && point.distance(center) > radius + bubble + 5.
            })
    });
    (choice, outside)
}
