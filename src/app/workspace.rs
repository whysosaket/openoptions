use super::theme::*;
use eframe::egui::{
    self, Align, Atom, Button, FontId, Frame, Layout, ScrollArea, Stroke, TextEdit, Ui, vec2,
};

const HEADER_HEIGHT: f32 = 52.;
const COLUMN_HEIGHT: f32 = 34.;
const ROW_HEIGHT: f32 = 44.;

pub(super) fn header(
    ui: &mut Ui,
    name: &str,
    back_enabled: Option<bool>,
    actions: impl FnOnce(&mut Ui),
) -> bool {
    let mut back = false;
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), HEADER_HEIGHT),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.add_space(16.);
            if let Some(enabled) = back_enabled {
                back = labeled_button(ui, "Back", Icon::Back, enabled).clicked();
            }
            ui.label(title(name, 14.));
            trailing(ui, |ui| {
                ui.add_space(16.);
                actions(ui);
            });
        },
    );
    divider(ui);
    back
}

pub(super) fn trailing(ui: &mut Ui, contents: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), CONTROL_HEIGHT),
        Layout::right_to_left(Align::Center),
        contents,
    );
}

pub(super) fn divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.), egui::Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, line());
}

pub(super) fn search(ui: &mut Ui, value: &mut String, hint: &str) {
    Frame::new()
        .fill(ELEVATED)
        .stroke(line())
        .corner_radius(7)
        .inner_margin(egui::Margin::symmetric(9, 0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                icon(ui, Icon::Search, 15., MUTED);
                let width = (ui.available_width() * 0.55).clamp(100., 226.);
                ui.add(
                    TextEdit::singleline(value)
                        .hint_text(hint)
                        .desired_width(width)
                        .frame(Frame::new()),
                );
            });
        });
}

#[derive(Clone, Copy)]
pub(super) struct Columns([f32; 3]);

impl Columns {
    fn for_width(width: f32) -> Self {
        let trailing = if width < 520. { 94. } else { 112. };
        let first = (width - trailing) * 0.65;
        Self([first, width - first - trailing, trailing])
    }

    pub(super) fn row(
        self,
        ui: &mut Ui,
        name: impl FnOnce(&mut Ui),
        metadata: impl FnOnce(&mut Ui),
        end: impl FnOnce(&mut Ui),
    ) {
        self.cells(ui, ROW_HEIGHT, name, metadata, end);
    }

    fn cells(
        self,
        ui: &mut Ui,
        height: f32,
        first: impl FnOnce(&mut Ui),
        second: impl FnOnce(&mut Ui),
        third: impl FnOnce(&mut Ui),
    ) {
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), height),
            Layout::left_to_right(Align::Center),
            |ui| {
                cell(ui, self.0[0], height, first);
                cell(ui, self.0[1], height, second);
                cell(ui, self.0[2], height, third);
            },
        );
    }

    pub(super) fn placeholder(self, ui: &mut Ui) {
        let bar = |ui: &mut Ui, width| {
            let (rect, _) = ui.allocate_exact_size(vec2(width, 8.), egui::Sense::hover());
            ui.painter().rect_filled(rect, 3, ELEVATED);
        };
        self.row(ui, |ui| bar(ui, 98.), |ui| bar(ui, 60.), |ui| bar(ui, 48.));
    }
}

fn cell(ui: &mut Ui, width: f32, height: f32, contents: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        vec2(width, height),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_min_size(vec2(width, height));
            ui.add_space(11.);
            contents(ui);
        },
    );
}

pub(super) fn index(
    ui: &mut Ui,
    id: &str,
    headers: [&str; 3],
    clear_below: f32,
    rows: impl FnOnce(&mut Ui, Columns),
) {
    let body_height = (ui.available_height() - COLUMN_HEIGHT - clear_below).max(ROW_HEIGHT);
    surface(0).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.spacing_mut().item_spacing = vec2(0., 0.);
        ui.visuals_mut().clip_rect_margin = 0.;
        let columns = Columns::for_width(ui.available_width());
        columns.cells(
            ui,
            COLUMN_HEIGHT,
            |ui| {
                ui.label(muted(headers[0]).size(12.));
            },
            |ui| {
                ui.label(muted(headers[1]).size(12.));
            },
            |ui| {
                ui.label(muted(headers[2]).size(12.));
            },
        );
        divider(ui);
        ScrollArea::vertical()
            .id_salt(id)
            .max_height(body_height)
            .auto_shrink([false, true])
            .show(ui, |ui| rows(ui, columns));
    });
}

pub(super) fn row_title(ui: &mut Ui, name: &str, symbol: Icon) -> bool {
    let glyph = Atom::custom(egui::Id::new("row icon"), vec2(16., 16.));
    let button = Button::new((glyph, title(name, 14.)))
        .gap(8.)
        .truncate()
        .frame_when_inactive(false)
        .stroke(Stroke::NONE)
        .atom_ui(ui);
    for (_, rect) in button.custom_rects() {
        paint_icon(ui.painter(), rect, symbol, MUTED);
    }
    paint_focus(ui, &button.response);
    button.response.on_hover_text(name).clicked()
}

pub(super) fn badge(ui: &mut Ui, caption: &str, active: bool) {
    let color = if active { SUCCESS } else { MUTED };
    let text = ui
        .painter()
        .layout_no_wrap(caption.to_string(), FontId::proportional(11.), color);
    let (rect, response) =
        ui.allocate_exact_size(text.size() + vec2(14., 6.), egui::Sense::hover());
    ui.painter().rect_filled(rect, 7, ELEVATED);
    ui.painter()
        .galley(rect.center() - text.size() * 0.5, text, color);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, caption));
}

pub(super) fn empty(ui: &mut Ui, title_text: &str, message: &str, action: impl FnOnce(&mut Ui)) {
    surface(20).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(title(title_text, 16.));
        ui.label(muted(message));
        action(ui);
    });
}

pub(super) fn profile_count(count: usize) -> String {
    format!(
        "{count} {}",
        if count == 1 { "profile" } else { "profiles" }
    )
}
