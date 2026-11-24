use super::{OpenOptions, Page, theme::*, ui::matches_query, workspace};
use eframe::egui::{self, Align, Layout, Rect, ScrollArea, Sense, Ui, vec2};
use openoptions::{controls::Battery, devices::Device};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum View {
    Gallery,
    List,
}

#[derive(Clone, Copy)]
enum Open {
    Workspace,
    DeviceInfo,
}

impl OpenOptions {
    pub(super) fn devices_ui(&mut self, ui: &mut Ui) {
        let mut open = None;
        let mut retry = false;
        match &self.devices {
            _ if self.scanning && self.devices.as_ref().ok().is_none_or(Vec::is_empty) => {
                surface(18).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Looking for Logitech devices…");
                    });
                });
            }
            Err(error) => {
                workspace::empty(
                    ui,
                    "Couldn’t refresh devices",
                    "Check the connection and scan again.",
                    |ui| {
                        ui.collapsing("Scan error", |ui| {
                            ui.label(muted(error));
                        });
                        retry = labeled_button(ui, "Try again", Icon::Refresh, !self.scanning)
                            .clicked();
                    },
                );
            }
            Ok(devices) if devices.is_empty() => {
                ui.label(muted("0 devices"));
                workspace::empty(
                    ui,
                    "No devices found",
                    "Use Add Device to connect over Bluetooth or with a supported receiver.",
                    |_| {},
                );
            }
            Ok(devices) => {
                let query = self.device_query.trim().to_lowercase();
                let visible: Vec<_> = devices
                    .iter()
                    .filter(|device| matches_query(&query, &[&device.name, device.connection]))
                    .collect();
                let count = devices.len();
                let noun = if count == 1 { "device" } else { "devices" };
                ui.horizontal_wrapped(|ui| {
                    ui.label(title(
                        if visible.len() == count {
                            format!("{count} {noun}")
                        } else {
                            format!("{} of {count} {noun}", visible.len())
                        },
                        16.,
                    ));
                    if self.scanning {
                        ui.spinner();
                        ui.label(muted("Refreshing…"));
                    }
                    ui.add_space(6.);
                    workspace::search(ui, &mut self.device_query, "Search devices");
                    for (label, view) in [("Gallery", View::Gallery), ("List", View::List)] {
                        if ui
                            .selectable_label(self.device_view == view, label)
                            .clicked()
                        {
                            self.device_view = view;
                        }
                    }
                });
                if visible.is_empty() {
                    workspace::empty(
                        ui,
                        "No matching devices",
                        "Search by device name or connection.",
                        |ui| {
                            if ui.button("Clear search").clicked() {
                                self.device_query.clear();
                            }
                        },
                    );
                } else {
                    let rail = if self.sources.is_empty() { 0. } else { 40. };
                    let artwork_height = if ui.available_height() < 310. {
                        73.
                    } else {
                        115.
                    };
                    let columns =
                        ((ui.available_width() + 12.) / 264.).floor().clamp(1., 3.) as usize;
                    ScrollArea::vertical()
                        .id_salt("device-inventory")
                        .max_height((ui.available_height() - rail).max(50.))
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            let per_row = match self.device_view {
                                View::Gallery => columns,
                                View::List => 1,
                            };
                            let width = (ui.available_width() - 12. * (per_row - 1) as f32)
                                / per_row as f32;
                            for row in visible.chunks(per_row) {
                                ui.horizontal_top(|ui| {
                                    ui.spacing_mut().item_spacing.x = 12.;
                                    for &device in row {
                                        let key = device.key();
                                        let selected = self.selected_device.as_ref() == Some(&key);
                                        let response = ui
                                            .push_id(&key, |ui| match self.device_view {
                                                View::Gallery => tile(
                                                    ui,
                                                    device,
                                                    width.min(329.),
                                                    artwork_height,
                                                    selected,
                                                ),
                                                View::List => list_row(ui, device, width, selected),
                                            })
                                            .inner;
                                        if let Some(action) = response {
                                            open = Some((device.clone(), action));
                                        }
                                    }
                                });
                            }
                        });
                }
            }
        }
        if retry {
            self.scan(ui.ctx());
        }
        match open {
            Some((device, Open::Workspace)) => self.open_device(device, ui.ctx()),
            Some((device, Open::DeviceInfo)) => self.open_device_details(device, ui.ctx()),
            None => {}
        }
        if !self.sources.is_empty() {
            workspace::divider(ui);
            ui.horizontal(|ui| {
                let count = self.sources.len();
                let noun = if count == 1 { "file" } else { "files" };
                ui.label(muted(format!("{count} settings {noun} available")).size(12.));
                workspace::trailing(ui, |ui| {
                    if labeled_button(ui, "Review imports", Icon::Import, true).clicked() {
                        self.navigate(Page::Import);
                    }
                });
            });
        }
    }
}

fn tile(
    ui: &mut Ui,
    device: &Device,
    width: f32,
    artwork_height: f32,
    selected: bool,
) -> Option<Open> {
    let content_width = (width - 22.).max(0.);
    let mut action = None;
    device_frame(selected).show(ui, |ui| {
        ui.allocate_ui_with_layout(
            vec2(content_width, 0.),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_width(content_width);
                ui.spacing_mut().item_spacing.y = 6.;
                let (rect, _) =
                    ui.allocate_exact_size(vec2(content_width, artwork_height), Sense::hover());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 8, ELEVATED);
                let height = artwork_height - 18.;
                paint_device(
                    &painter,
                    Rect::from_center_size(rect.center(), vec2(height + 6., height)),
                    device,
                );
                ui.add(egui::Label::new(title(&device.name, 17.)).truncate())
                    .on_hover_text(&device.name);
                device_status(ui, device);
                workspace::divider(ui);
                ui.horizontal(|ui| {
                    let customize = ui.button("Customize").on_hover_text(&device.name);
                    paint_focus(ui, &customize);
                    if customize.clicked() {
                        action = Some(Open::Workspace);
                    }
                    workspace::trailing(ui, |ui| {
                        if device_info_button(ui, device) {
                            action = Some(Open::DeviceInfo);
                        }
                    });
                });
            },
        );
    });
    action
}

fn list_row(ui: &mut Ui, device: &Device, width: f32, selected: bool) -> Option<Open> {
    let content_width = (width - 22.).max(0.);
    let mut action = None;
    device_frame(selected).show(ui, |ui| {
        ui.allocate_ui_with_layout(
            vec2(content_width, 56.),
            Layout::left_to_right(Align::Center),
            |ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(63., 56.), Sense::hover());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 8, ELEVATED);
                paint_device(&painter, rect.shrink(6.), device);
                let details_width = (ui.available_width() - 104.).max(0.);
                ui.allocate_ui_with_layout(
                    vec2(details_width, 53.),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_width(details_width);
                        if workspace::row_title(ui, &device.name, device_icon(device)) {
                            action = Some(Open::Workspace);
                        }
                        device_status(ui, device);
                    },
                );
                workspace::trailing(ui, |ui| {
                    if device_info_button(ui, device) {
                        action = Some(Open::DeviceInfo);
                    }
                });
            },
        );
    });
    action
}

fn device_info_button(ui: &mut Ui, device: &Device) -> bool {
    let info = ui.button("Device info").on_hover_text(&device.name);
    paint_focus(ui, &info);
    info.clicked()
}

fn device_frame(selected: bool) -> egui::Frame {
    surface(10).stroke(if selected {
        egui::Stroke::new(1_f32, FOCUS)
    } else {
        line()
    })
}

fn device_status(ui: &mut Ui, device: &Device) {
    ui.horizontal_wrapped(|ui| {
        workspace::badge(
            ui,
            if device.connected {
                "Connected"
            } else {
                "Disconnected"
            },
            device.connected,
        );
        ui.label(muted(device.connection).size(12.));
        if let Ok(Some(battery)) = device.battery {
            ui.label(muted(battery_caption(battery)).size(12.));
        }
    });
}

pub(super) fn battery_caption(battery: Battery) -> String {
    match battery {
        Battery::Percent(value) => format!("Battery {value}%"),
        Battery::Full => "Battery full".into(),
        Battery::Good => "Battery good".into(),
        Battery::Low => "Battery low".into(),
        Battery::Critical => "Battery critical".into(),
        Battery::Charging => "Charging".into(),
    }
}

fn paint_device(painter: &egui::Painter, rect: Rect, device: &Device) {
    if matches!(device.product_id, 0xB034 | 0xB035) {
        let shadow = rect.translate(vec2(1., rect.height() * 0.43));
        painter.add(egui::Shape::ellipse_filled(
            shadow.center(),
            vec2(rect.width() * 0.39, rect.height() * 0.09),
            BORDER,
        ));
        mx_master(painter, rect);
    } else {
        let bounds = Rect::from_center_size(
            rect.center(),
            vec2(rect.height() * 0.75, rect.height() * 0.75),
        );
        paint_icon(painter, bounds, device_icon(device), MUTED);
    }
}

fn mx_master(painter: &egui::Painter, rect: egui::Rect) {
    let center = rect.center();
    let p = |x, y| center + egui::vec2(x, y) * (rect.height() / 100.);
    let body = [
        (-32., -29.),
        (-23., -45.),
        (-10., -50.),
        (7., -50.),
        (21., -45.),
        (32., -35.),
        (38., -19.),
        (40., 0.),
        (37., 19.),
        (31., 34.),
        (18., 44.),
        (2., 49.),
        (-14., 48.),
        (-32., 40.),
        (-44., 25.),
        (-44., 12.),
        (-38., -4.),
        (-35., -17.),
    ];
    let fill = egui::Color32::from_rgb(47, 48, 54);
    let edge = egui::Color32::from_rgb(76, 78, 85);
    painter.add(egui::Shape::convex_polygon(
        body.into_iter().map(|(x, y)| p(x, y)).collect(),
        fill,
        egui::Stroke::new(1.0_f32, edge),
    ));
    painter.add(egui::Shape::line(
        vec![
            p(-33., -10.),
            p(-19., -6.),
            p(-3., -3.),
            p(18., -7.),
            p(37., -15.),
        ],
        egui::Stroke::new(1.0_f32, edge),
    ));
    painter.line_segment(
        [p(-4., -48.), p(-2., -3.)],
        egui::Stroke::new(1.0_f32, edge),
    );
    let wheel = egui::Rect::from_two_pos(p(-9., -39.), p(3., -17.));
    painter.rect_filled(wheel, 5, egui::Color32::from_gray(133));
    for y in [-35., -31., -27., -23.] {
        painter.line_segment([p(-7., y), p(1., y)], egui::Stroke::new(0.9_f32, fill));
    }
    painter.rect_filled(egui::Rect::from_two_pos(p(-7., -13.), p(1., -5.)), 3, edge);
    painter.line_segment(
        [p(-30., 7.), p(-35., 21.)],
        egui::Stroke::new(2.0_f32, edge),
    );
    painter.circle_filled(p(-2., 5.), 1.2, egui::Color32::from_rgb(92, 156, 131));
}

fn device_icon(device: &Device) -> Icon {
    let name = device.name.to_ascii_lowercase();
    if name.contains("keyboard") || name.contains("keys") {
        Icon::Keyboard
    } else if name.contains("mouse") || name.contains("master") || name.contains("anywhere") {
        Icon::Mouse
    } else {
        Icon::Device
    }
}
