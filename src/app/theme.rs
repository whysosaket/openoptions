use eframe::egui::{
    self, Atom, Button, Color32, Context, FontData, FontDefinitions, FontFamily, FontId, Frame,
    Label, Pos2, Rect, RichText, Shape, Stroke, StrokeKind, TextStyle, Ui, Vec2, vec2,
};

pub(super) const WINDOW: Color32 = Color32::from_gray(12);
pub(super) const SURFACE: Color32 = Color32::from_gray(20);
pub(super) const ELEVATED: Color32 = Color32::from_gray(29);
pub(super) const HOVER: Color32 = Color32::from_gray(38);
pub(super) const BORDER: Color32 = Color32::from_gray(46);
pub(super) const TEXT: Color32 = Color32::from_gray(240);
pub(super) const MUTED: Color32 = Color32::from_gray(166);
pub(super) const FOCUS: Color32 = Color32::from_gray(148);
pub(super) const SOLID: Color32 = Color32::from_rgb(244, 241, 237);
pub(super) const SUCCESS: Color32 = Color32::from_rgb(126, 212, 153);
pub(super) const WARNING: Color32 = Color32::from_rgb(239, 193, 130);
pub(super) const CONTROL_HEIGHT: f32 = 32.;

fn semibold() -> FontFamily {
    FontFamily::Name("Inter Semibold".into())
}

pub(super) fn install(context: &Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        FontData::from_static(include_bytes!("../../assets/fonts/Inter-Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        "Inter Semibold".into(),
        FontData::from_static(include_bytes!("../../assets/fonts/Inter-SemiBold.ttf")).into(),
    );
    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional.insert(0, "Inter".into());
    let mut emphasis = proportional.clone();
    emphasis.insert(0, "Inter Semibold".into());
    fonts.families.insert(semibold(), emphasis);
    context.set_fonts(fonts);

    context.set_theme(egui::Theme::Dark);
    let mut style = (*context.global_style()).clone();
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(12.)),
        (TextStyle::Body, FontId::proportional(14.)),
        (TextStyle::Button, FontId::new(13., semibold())),
        (TextStyle::Heading, FontId::new(23., semibold())),
        (TextStyle::Monospace, FontId::monospace(12.)),
    ]
    .into();
    style.spacing.item_spacing = vec2(8., 8.);
    style.spacing.button_padding = vec2(11., 7.);
    style.spacing.interact_size = vec2(36., CONTROL_HEIGHT);
    style.spacing.scroll = egui::style::ScrollStyle::thin();
    style.spacing.scroll.foreground_color = false;

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = WINDOW;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = WINDOW;
    visuals.faint_bg_color = ELEVATED;
    visuals.selection.bg_fill = HOVER;
    visuals.selection.stroke = Stroke::new(1.0_f32, FOCUS);
    visuals.widgets.noninteractive.fg_stroke.color = TEXT;
    visuals.widgets.noninteractive.bg_stroke.color = BORDER;
    for (visual, fill, stroke) in [
        (&mut visuals.widgets.inactive, ELEVATED, BORDER),
        (&mut visuals.widgets.hovered, HOVER, BORDER),
        (&mut visuals.widgets.active, HOVER, FOCUS),
        (&mut visuals.widgets.open, ELEVATED, BORDER),
    ] {
        visual.bg_fill = fill;
        visual.weak_bg_fill = fill;
        visual.bg_stroke = Stroke::new(1.0_f32, stroke);
        visual.fg_stroke.color = TEXT;
        visual.corner_radius = 7.into();
        visual.expansion = 0.;
    }
    style.visuals = visuals;
    context.set_global_style(style);
}

pub(super) fn title(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text.into())
        .font(FontId::new(size, semibold()))
        .color(TEXT)
}

pub(super) fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).color(MUTED)
}

pub(super) fn line() -> Stroke {
    Stroke::new(1.0_f32, BORDER)
}

pub(super) fn surface(padding: i8) -> Frame {
    Frame::new()
        .fill(SURFACE)
        .stroke(line())
        .corner_radius(10)
        .inner_margin(padding)
}

pub(super) fn primary(text: impl Into<String>) -> Button<'static> {
    Button::new(
        RichText::new(text.into())
            .font(FontId::new(13., semibold()))
            .color(WINDOW),
    )
    .fill(SOLID)
    .stroke(Stroke::NONE)
    .corner_radius(7)
}

pub(super) fn clipped(ui: &mut Ui, text: RichText, full: &str) {
    let has_additional_text = text.text() != full;
    let response = ui.add(Label::new(text).truncate());
    if has_additional_text {
        response.on_hover_text(full);
    }
}

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Mouse,
    Keyboard,
    Device,
    Import,
    Profiles,
    File,
    Back,
    Refresh,
    Search,
}

pub(super) fn paint_icon(painter: &egui::Painter, rect: Rect, kind: Icon, color: Color32) {
    let p = |x, y| rect.min + vec2(x, y) * (rect.size() / 24.);
    let stroke = Stroke::new((rect.width() / 13.).clamp(1.2, 1.6), color);
    let path = |points: &[(f32, f32)]| {
        painter.add(Shape::line(
            points.iter().map(|&(x, y)| p(x, y)).collect(),
            stroke,
        ));
    };
    let box_outline = |a: (f32, f32), b: (f32, f32), radius: u8| {
        painter.rect_stroke(
            Rect::from_min_max(p(a.0, a.1), p(b.0, b.1)),
            radius,
            stroke,
            StrokeKind::Middle,
        );
    };
    match kind {
        Icon::Mouse => {
            box_outline((5., 2.), (19., 22.), 5);
            path(&[(5., 10.), (19., 10.)]);
            path(&[(12., 3.), (12., 8.)]);
        }
        Icon::Keyboard => {
            box_outline((2., 5.), (22., 19.), 2);
            for y in [9., 12.] {
                for x in [6., 10., 14., 18.] {
                    painter.circle_filled(p(x, y), stroke.width * 0.6, color);
                }
            }
            path(&[(7., 15.5), (17., 15.5)]);
        }
        Icon::Device => {
            box_outline((4., 7.), (20., 20.), 2);
            path(&[(8., 7.), (8., 3.), (16., 3.), (16., 7.)]);
            path(&[(9., 13.5), (15., 13.5)]);
        }
        Icon::Import => {
            path(&[(3., 14.), (3., 20.), (21., 20.), (21., 14.)]);
            path(&[(12., 3.), (12., 15.)]);
            path(&[(7.5, 10.5), (12., 15.), (16.5, 10.5)]);
        }
        Icon::Profiles => {
            box_outline((5., 3.), (19., 21.), 2);
            path(&[(8., 9.), (16., 9.)]);
            path(&[(8., 13.), (16., 13.)]);
            path(&[(8., 17.), (13., 17.)]);
        }
        Icon::File => {
            path(&[
                (6., 3.),
                (14., 3.),
                (19., 8.),
                (19., 21.),
                (6., 21.),
                (6., 3.),
            ]);
            path(&[(14., 3.), (14., 8.), (19., 8.)]);
        }
        Icon::Back => {
            path(&[(20., 12.), (5., 12.)]);
            path(&[(11., 6.), (5., 12.), (11., 18.)]);
        }
        Icon::Refresh => {
            path(&[(19., 8.), (16., 4.), (10., 3.), (5., 6.), (3., 12.)]);
            path(&[(19., 3.), (19., 8.), (14., 8.)]);
            path(&[(5., 16.), (8., 20.), (14., 21.), (19., 18.), (21., 12.)]);
            path(&[(5., 21.), (5., 16.), (10., 16.)]);
        }
        Icon::Search => {
            painter.circle_stroke(p(10., 10.), rect.width() * 0.26, stroke);
            path(&[(15., 15.), (21., 21.)]);
        }
    }
}

pub(super) fn icon(ui: &mut Ui, kind: Icon, size: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    paint_icon(ui.painter(), rect, kind, color);
}

pub(super) fn labeled_button(
    ui: &mut Ui,
    label: &str,
    kind: Icon,
    enabled: bool,
) -> egui::Response {
    let glyph = Atom::custom(egui::Id::new("control icon"), vec2(15., 15.));
    ui.add_enabled_ui(enabled, |ui| {
        let result = Button::new((glyph, label)).gap(7.).atom_ui(ui);
        let color = if enabled { MUTED } else { FOCUS };
        for (_, rect) in result.custom_rects() {
            paint_icon(ui.painter(), rect, kind, color);
        }
        result.response
    })
    .inner
}

pub(super) fn nav(ui: &mut Ui, label: &str, kind: Icon, active: bool) -> bool {
    let color = if active { TEXT } else { MUTED };
    let glyph = Atom::custom(egui::Id::new("nav icon"), vec2(16., 16.));
    let response = Button::new((glyph, RichText::new(label).color(color), Atom::grow()))
        .selected(active)
        .frame_when_inactive(active)
        .fill(if active { SURFACE } else { HOVER })
        .stroke(Stroke::NONE)
        .min_size(vec2(ui.available_width(), 35.))
        .gap(9.)
        .atom_ui(ui);
    for (_, rect) in response.custom_rects() {
        paint_icon(ui.painter(), rect, kind, color);
    }
    paint_focus(ui, &response.response);
    response.response.clicked()
}

pub(super) fn paint_focus(ui: &Ui, response: &egui::Response) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.shrink(1.),
            6,
            Stroke::new(1.0_f32, FOCUS),
            StrokeKind::Inside,
        );
    }
}

pub(super) fn mark(painter: &egui::Painter, center: Pos2) {
    let tile = Rect::from_center_size(center, Vec2::splat(24.));
    painter.rect_filled(tile, 6, ELEVATED);
    painter.circle_stroke(center, 7., Stroke::new(1.4_f32, TEXT));
    painter.circle_filled(center, 2.55, TEXT);
    painter.circle_filled(center + vec2(5., -5.), 1.8, ELEVATED);
    painter.circle_filled(center + vec2(5., -5.), 1.01, TEXT);
}
