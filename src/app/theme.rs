use eframe::egui::{
    self, Atom, Button, Color32, Context, FontId, Frame, Pos2, Rect, RichText, Shape, Stroke,
    StrokeKind, TextStyle, Ui, Vec2, vec2,
};

pub(super) const CANVAS: Color32 = Color32::from_rgb(14, 20, 18);
pub(super) const RAIL: Color32 = Color32::from_rgb(19, 27, 23);
pub(super) const CARD: Color32 = Color32::from_rgb(24, 34, 29);
pub(super) const ELEVATED: Color32 = Color32::from_rgb(31, 45, 37);
pub(super) const BORDER: Color32 = Color32::from_rgb(48, 66, 55);
pub(super) const TEXT: Color32 = Color32::from_rgb(240, 247, 238);
pub(super) const MUTED: Color32 = Color32::from_rgb(166, 187, 173);
pub(super) const ACCENT: Color32 = Color32::from_rgb(192, 242, 187);
pub(super) const INK: Color32 = Color32::from_rgb(18, 39, 24);
pub(super) const WARNING: Color32 = Color32::from_rgb(245, 203, 149);

pub(super) fn install(context: &Context) {
    context.set_theme(egui::Theme::Dark);
    let mut style = (*context.global_style()).clone();
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(13.)),
        (TextStyle::Body, FontId::proportional(15.)),
        (TextStyle::Button, FontId::proportional(15.)),
        (TextStyle::Heading, FontId::proportional(29.)),
        (TextStyle::Monospace, FontId::monospace(13.)),
    ]
    .into();
    style.spacing.item_spacing = vec2(8., 8.);
    style.spacing.button_padding = vec2(13., 8.);
    style.spacing.interact_size = vec2(40., 36.);
    let mut colors = egui::Visuals::dark();
    colors.override_text_color = None;
    colors.panel_fill = CANVAS;
    colors.window_fill = CARD;
    colors.extreme_bg_color = CANVAS;
    colors.faint_bg_color = CARD;
    colors.selection.bg_fill = ELEVATED;
    colors.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    colors.widgets.noninteractive.fg_stroke.color = TEXT;
    colors.widgets.noninteractive.bg_stroke.color = BORDER;
    for (widget, fill, line, text) in [
        (&mut colors.widgets.inactive, CARD, BORDER, TEXT),
        (&mut colors.widgets.hovered, ELEVATED, ACCENT, TEXT),
        (&mut colors.widgets.active, ELEVATED, ACCENT, TEXT),
        (&mut colors.widgets.open, CARD, BORDER, TEXT),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = Stroke::new(1.0_f32, line);
        widget.fg_stroke.color = text;
        widget.corner_radius = 8.into();
        widget.expansion = 0.;
    }
    style.visuals = colors;
    context.set_global_style(style);
}

pub(super) fn title(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text.into()).size(size).color(TEXT).strong()
}

pub(super) fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).color(MUTED)
}

pub(super) fn panel(stroke: Color32, padding: i8) -> Frame {
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, stroke))
        .corner_radius(16)
        .inner_margin(padding)
}

pub(super) fn primary(text: impl Into<String>) -> Button<'static> {
    Button::new(RichText::new(text.into()).color(INK).strong())
        .fill(ACCENT)
        .stroke(Stroke::new(1.0_f32, ACCENT))
        .corner_radius(8)
}

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Mouse,
    Keyboard,
    Device,
    Import,
    Profiles,
    File,
    Bluetooth,
}

pub(super) fn icon(ui: &mut Ui, kind: Icon, size: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    paint_icon(ui.painter(), rect, kind, color);
}

pub(super) fn paint_icon(painter: &egui::Painter, rect: Rect, kind: Icon, color: Color32) {
    let p = |x, y| rect.min + vec2(x, y) * (rect.size() / 24.);
    let stroke = Stroke::new((rect.width() / 14.).clamp(1.25, 2.7), color);
    let line = |points: &[(f32, f32)]| {
        painter.add(Shape::line(
            points.iter().map(|&(x, y)| p(x, y)).collect(),
            stroke,
        ));
    };
    let outline = |a: (f32, f32), b: (f32, f32), radius: u8| {
        painter.rect_stroke(
            Rect::from_min_max(p(a.0, a.1), p(b.0, b.1)),
            radius,
            stroke,
            StrokeKind::Middle,
        );
    };
    match kind {
        Icon::Mouse => {
            outline((5., 2.5), (19., 21.5), 7);
            line(&[(5., 10.), (19., 10.)]);
            line(&[(12., 3.), (12., 8.)]);
        }
        Icon::Keyboard => {
            outline((2., 5.), (22., 19.), 3);
            for y in [9., 12.] {
                for x in [6., 10., 14., 18.] {
                    painter.circle_filled(p(x, y), stroke.width * 0.6, color);
                }
            }
            line(&[(7., 15.5), (17., 15.5)]);
        }
        Icon::Device => {
            outline((4., 7.), (20., 20.), 3);
            line(&[(8., 7.), (8., 3.), (16., 3.), (16., 7.)]);
            line(&[(9., 13.5), (15., 13.5)]);
        }
        Icon::Import => {
            line(&[(3., 14.), (3., 20.), (21., 20.), (21., 14.)]);
            line(&[(12., 3.), (12., 15.)]);
            line(&[(7.5, 10.5), (12., 15.), (16.5, 10.5)]);
        }
        Icon::Profiles => {
            outline((5., 3.), (19., 21.), 3);
            line(&[(8., 9.), (16., 9.)]);
            line(&[(8., 13.), (16., 13.)]);
            line(&[(8., 17.), (13., 17.)]);
        }
        Icon::File => {
            line(&[
                (6., 3.),
                (14., 3.),
                (19., 8.),
                (19., 21.),
                (6., 21.),
                (6., 3.),
            ]);
            line(&[(14., 3.), (14., 8.), (19., 8.)]);
        }
        Icon::Bluetooth => {
            line(&[
                (6., 6.),
                (18., 17.),
                (12., 22.),
                (12., 2.),
                (18., 7.),
                (6., 18.),
            ]);
        }
    }
}

pub(super) fn icon_tile(ui: &mut Ui, kind: Icon, side: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(side), egui::Sense::hover());
    ui.painter().rect_filled(rect, 10, ELEVATED);
    paint_icon(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(side * 0.5)),
        kind,
        ACCENT,
    );
}

pub(super) fn nav(ui: &mut Ui, label: &str, kind: Icon, selected: bool) -> bool {
    let color = if selected { ACCENT } else { MUTED };
    let glyph = Atom::custom(egui::Id::new("navigation glyph"), vec2(17., 17.));
    let button = Button::new((glyph, RichText::new(label).color(color), Atom::grow()))
        .selected(selected)
        .frame_when_inactive(selected)
        .stroke(Stroke::NONE)
        .min_size(vec2(ui.available_width(), 42.))
        .atom_ui(ui);
    for (_, rect) in button.custom_rects() {
        paint_icon(ui.painter(), rect, kind, color);
    }
    button.response.clicked()
}

pub(super) fn mark(painter: &egui::Painter, center: Pos2) {
    let tile = Rect::from_center_size(center, Vec2::splat(32.));
    painter.rect_filled(tile, 10, ACCENT);
    painter.circle_stroke(center, 8.5, Stroke::new(1.6_f32, INK));
    painter.circle_filled(center, 3.5, INK);
    painter.circle_filled(center + vec2(6., -6.), 2.55, ACCENT);
    painter.circle_filled(center + vec2(6., -6.), 1.35, INK);
}
