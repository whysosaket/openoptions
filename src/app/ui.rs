use super::{Notice, OpenOptions, Page, action_labels::action_label, theme::*};
use eframe::egui::{
    self, Align, Button, CentralPanel, Color32, Context, Frame, Label, Layout, Rect, RichText,
    ScrollArea, Stroke, TextEdit, TextureHandle, Ui, Vec2, pos2, vec2,
};
use openoptions::devices::{self, Device};
use openoptions::migration::{
    Action, Binding, Profile, ScrollDirection, Source, SourceKind, WheelMode,
};
use std::path::PathBuf;

impl eframe::App for OpenOptions {
    fn ui(&mut self, root: &mut Ui, _frame: &mut eframe::Frame) {
        self.receive();
        let rail_width = if root.available_width() < 810. {
            176.
        } else {
            202.
        };
        egui::Panel::left("navigation")
            .exact_size(rail_width)
            .resizable(false)
            .frame(Frame::new().fill(RAIL).inner_margin(16))
            .show_inside(root, |ui| {
                ui.add_space(8.);
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.), egui::Sense::hover());
                    mark(ui.painter(), rect.center());
                    ui.label(title("OpenOptions", 19.));
                });
                ui.add_space(28.);
                for (page, label, symbol) in [
                    (Page::Devices, "Devices", Icon::Mouse),
                    (Page::Import, "Import settings", Icon::Import),
                    (Page::Profiles, "Profiles", Icon::Profiles),
                ] {
                    if nav(ui, label, symbol, self.page == page) {
                        self.navigate(page);
                    }
                }
            });

        CentralPanel::default()
            .frame(Frame::new().fill(CANVAS))
            .show_inside(root, |ui| {
                ScrollArea::vertical()
                    .id_salt((
                        self.page as u8,
                        self.page == Page::Import && self.preview.is_some(),
                    ))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let inset = if ui.available_width() < 630. { 20 } else { 32 };
                        Frame::new().inner_margin(inset).show(ui, |ui| {
                            ui.set_width(ui.available_width().min(1180.));
                            match self.page {
                                Page::Devices => self.devices_ui(ui),
                                Page::Import => self.import_ui(ui),
                                Page::Profiles => self.profiles_ui(ui),
                            }
                        });
                    });
            });
    }
}

impl OpenOptions {
    fn navigate(&mut self, page: Page) {
        if self.page != page {
            self.page = page;
            self.notice = None;
        }
    }

    fn devices_ui(&mut self, ui: &mut Ui) {
        header(ui, "Devices", "Your connected Logitech hardware.", |ui| {
            if ui.add(primary("Pair Bluetooth device")).clicked()
                && let Err(error) = devices::open_bluetooth_settings()
            {
                self.notice = Some(Notice::Error(error));
            }
            if ui
                .add_enabled(!self.scanning, Button::new("Refresh"))
                .clicked()
            {
                self.scan(ui.ctx());
            }
        });
        show_notice(ui, self.notice.as_ref());
        match &self.devices {
            Ok(devices) if devices.is_empty() && self.scanning => {
                loading(ui, "Looking for devices…");
            }
            Ok(devices) if devices.is_empty() => {
                empty_state(
                    ui,
                    Icon::Mouse,
                    "No devices connected",
                    "Plug in a receiver or pair a device in Bluetooth settings, then refresh.",
                );
            }
            Err(error) => {
                empty_state(ui, Icon::Device, "Could not read connected devices", error);
            }
            Ok(devices) => {
                for (index, device) in devices.iter().enumerate() {
                    ui.push_id((device.product_id, device.connection), |ui| {
                        let texture = if device.name.to_ascii_lowercase().contains("mx master 3") {
                            self.mouse_art.as_ref()
                        } else {
                            None
                        };
                        if index == 0 {
                            featured_device(ui, device, texture);
                        } else {
                            device_row(ui, device);
                        }
                    });
                    ui.add_space(10.);
                }
            }
        }
        if !self.sources.is_empty() {
            ui.add_space(11.);
            panel(BORDER, 18).show(ui, |ui| {
                ui.horizontal(|ui| {
                    icon_tile(ui, Icon::Import, 42.);
                    ui.vertical(|ui| {
                        ui.label(title("Settings ready to import", 16.));
                        let count = self.sources.len();
                        ui.label(
                            muted(format!(
                                "{count} local settings {} found.",
                                if count == 1 { "file" } else { "files" }
                            ))
                            .size(13.),
                        );
                    });
                    right_actions(ui, |ui| {
                        if ui.button("Review settings").clicked() {
                            self.navigate(Page::Import);
                        }
                    });
                });
            });
        }
    }

    fn import_ui(&mut self, ui: &mut Ui) {
        if self.preview.is_some() {
            self.review_import(ui);
            return;
        }
        header(
            ui,
            "Import settings",
            "Review your existing settings before saving them.",
            |ui| {
                if ui
                    .add_enabled(!self.scanning, Button::new("Scan again"))
                    .clicked()
                {
                    self.scan(ui.ctx());
                }
            },
        );

        let busy = self.reading || self.writing;
        let mut selected = None;
        if self.sources.is_empty() {
            if self.scanning {
                loading(ui, "Looking for local settings…");
            } else {
                empty_state(
                    ui,
                    Icon::Import,
                    "No local settings found",
                    "Enter the path to a saved TOML, SQLite, or Options+ JSON file.",
                );
            }
        } else {
            ui.label(title("Local sources", 17.));
            ui.add_space(4.);
            let columns = if ui.available_width() >= 610. { 2 } else { 1 };
            for sources in self.sources.chunks(columns) {
                ui.columns(sources.len(), |uis| {
                    for (ui, source) in uis.iter_mut().zip(sources) {
                        if source_card(ui, source, busy) {
                            selected = Some(source.clone());
                        }
                    }
                });
            }
        }
        ui.add_space(10.);
        egui::CollapsingHeader::new(title("Use a settings file", 15.)).show(ui, |ui| {
            ui.add_space(3.);
            ui.label(muted("Path to a TOML, SQLite, or JSON file"));
            ui.horizontal(|ui| {
                ui.add(
                    TextEdit::singleline(&mut self.manual_path)
                        .hint_text("File path")
                        .desired_width((ui.available_width() - 129.).max(130.)),
                );
                if ui
                    .add_enabled(
                        !busy && !self.manual_path.trim().is_empty(),
                        Button::new("Preview file"),
                    )
                    .clicked()
                {
                    let path = PathBuf::from(self.manual_path.trim());
                    match SourceKind::from_path(&path) {
                        Ok(kind) => selected = Some(Source { kind, path }),
                        Err(error) => self.notice = Some(Notice::Error(error)),
                    }
                }
            });
        });
        if let Some(source) = selected {
            self.read(source, ui.ctx());
        }
        if self.reading {
            ui.add_space(18.);
            loading(ui, "Reading settings…");
        }

        show_notice(ui, self.notice.as_ref());
    }

    fn review_import(&mut self, ui: &mut Ui) {
        let Some((source, preview)) = &self.preview else {
            return;
        };
        let mut back = false;
        header(
            ui,
            "Settings preview",
            &format!("Source · {}", source.kind.name()),
            |ui| {
                back = ui
                    .add_enabled(!self.writing, Button::new("Back to sources"))
                    .clicked()
            },
        );
        if back {
            self.preview = None;
            self.notice = None;
            ui.ctx().request_repaint();
            return;
        }

        if preview.profiles.is_empty() {
            empty_state(
                ui,
                Icon::Import,
                "No settings to import",
                "This file has no settings supported by the importer.",
            );
        }
        for (index, profile) in preview.profiles.iter().enumerate() {
            let name = self.profile_name(profile, source.kind);
            ui.push_id(index, |ui| profile_card(ui, profile, name, None));
        }
        if !preview.skipped.is_empty() {
            let count: usize = preview.skipped.values().sum();
            let label = RichText::new(format!("Skipped settings ({count})"))
                .color(WARNING)
                .strong();
            egui::CollapsingHeader::new(label)
                .default_open(true)
                .show(ui, |ui| {
                    for (reason, count) in &preview.skipped {
                        ui.label(muted(format!("{reason} ({count})")));
                    }
                });
        }

        let mut save = false;
        if !preview.profiles.is_empty() {
            ui.add_space(11.);
            right_actions(ui, |ui| {
                let label = format!("Import {}", profile_count(preview.profiles.len()));
                save = ui.add_enabled(!self.writing, primary(label)).clicked();
            });
            if self.writing {
                loading(ui, "Saving profiles…");
            }
        }
        if save {
            self.import(ui.ctx());
        }
        show_notice(ui, self.notice.as_ref());
    }

    fn profile_name<'a>(&'a self, profile: &'a Profile, kind: SourceKind) -> &'a str {
        let name = &profile.device_name;
        if kind == SourceKind::Vendor
            && let Some(code) = name
                .split_whitespace()
                .next()
                .and_then(|word| word.strip_prefix('2'))
            && code.len() == 4
            && let Ok(product_id) = u16::from_str_radix(code, 16)
            && let Ok(devices) = &self.devices
            && let Some(device) = devices
                .iter()
                .find(|device| device.product_id == product_id)
        {
            return &device.name;
        }
        name
    }

    fn profiles_ui(&mut self, ui: &mut Ui) {
        page_title(
            ui,
            "Saved profiles",
            "Review the settings you’ve imported to this computer.",
        );
        ui.add_space(23.);
        show_notice(ui, self.notice.as_ref());
        if self.profiles.is_empty() {
            if self.writing {
                loading(ui, "Loading profiles…");
            } else {
                empty_state(
                    ui,
                    Icon::Profiles,
                    "No saved profiles",
                    "Import settings to create your first profile.",
                );
                ui.add_space(10.);
                if ui.add(primary("Import settings")).clicked() {
                    self.navigate(Page::Import);
                }
            }
        }
        for (index, (kind, profile)) in self.profiles.iter().enumerate() {
            let name = self.profile_name(profile, *kind);
            ui.push_id(index, |ui| {
                profile_card(ui, profile, name, Some(kind.name()))
            });
        }
    }
}

fn page_title(ui: &mut Ui, name: &str, detail: &str) {
    ui.label(title(name, 33.));
    ui.label(muted(detail));
}

fn header(ui: &mut Ui, name: &str, detail: &str, actions: impl FnOnce(&mut Ui)) {
    let width = ui.available_width();
    if width >= 650. {
        ui.allocate_ui_with_layout(vec2(width, 62.), Layout::left_to_right(Align::Min), |ui| {
            ui.allocate_ui_with_layout(
                vec2(width - 280., 62.),
                Layout::top_down(Align::Min),
                |ui| page_title(ui, name, detail),
            );
            right_actions(ui, actions);
        });
    } else {
        page_title(ui, name, detail);
        right_actions(ui, actions);
    }
    ui.add_space(23.);
}

fn right_actions(ui: &mut Ui, contents: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), 38.),
        Layout::right_to_left(Align::Center),
        contents,
    );
}

fn show_notice(ui: &mut Ui, notice: Option<&Notice>) {
    let Some(notice) = notice else { return };
    let (text, color) = match notice {
        Notice::Info(text) => (text, ACCENT),
        Notice::Error(text) => (text, WARNING),
    };
    panel(color, 13).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(text).color(color));
    });
    ui.add_space(10.);
}

fn loading(ui: &mut Ui, message: &str) {
    panel(BORDER, 18).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(muted(message));
        });
    });
}

fn empty_state(ui: &mut Ui, symbol: Icon, heading: &str, body: &str) {
    panel(BORDER, 22).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        icon_tile(ui, symbol, 44.);
        ui.add_space(5.);
        ui.label(title(heading, 21.));
        ui.label(muted(body));
    });
}

fn featured_device(ui: &mut Ui, device: &Device, texture: Option<&TextureHandle>) {
    let compact = ui.available_width() < 555.;
    panel(BORDER, if compact { 17 } else { 24 })
        .fill(ELEVATED)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let width = ui.available_width();
            if compact {
                illustration(ui, vec2(width, 170.), device_icon(device), texture);
                ui.add_space(3.);
                device_heading(ui, device, 26.);
            } else {
                let visual_width = (width * 0.47).min(396.);
                let text_width = width - visual_width - 12.;
                ui.allocate_ui_with_layout(
                    vec2(width, 216.),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.allocate_ui_with_layout(
                            vec2(text_width, 96.),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_min_width(text_width);
                                device_heading(ui, device, 33.);
                            },
                        );
                        illustration(ui, vec2(visual_width, 216.), device_icon(device), texture);
                    },
                );
            }
            ui.add_space(8.);
            egui::CollapsingHeader::new(muted("Hardware details"))
                .show(ui, |ui| hardware_details(ui, device));
        });
}

fn device_heading(ui: &mut Ui, device: &Device, size: f32) {
    ui.label(title(&device.name, size));
    ui.horizontal(|ui| {
        if device.connection == "Bluetooth" {
            icon(ui, Icon::Bluetooth, 14., ACCENT);
        }
        ui.label(
            RichText::new(device.connection)
                .size(14.)
                .color(ACCENT)
                .strong(),
        );
        ui.label(muted("·  Connected").size(14.));
    });
}

fn hardware_details(ui: &mut Ui, device: &Device) {
    ui.horizontal_wrapped(|ui| {
        ui.label(muted(format!("Product ID  046D:{:04X}", device.product_id)).monospace());
        ui.label(muted(format!(
            "{} HID {}",
            device.interfaces,
            if device.interfaces == 1 {
                "interface"
            } else {
                "interfaces"
            },
        )));
    });
}

fn device_row(ui: &mut Ui, device: &Device) {
    panel(BORDER, 17).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            icon_tile(ui, device_icon(device), 42.);
            ui.vertical(|ui| {
                device_heading(ui, device, 17.);
                egui::CollapsingHeader::new(muted("Hardware details"))
                    .show(ui, |ui| hardware_details(ui, device));
            });
        });
    });
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

fn illustration(ui: &mut Ui, size: Vec2, fallback: Icon, texture: Option<&TextureHandle>) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter().with_clip_rect(rect);
    let center = rect.center();
    let radius = (rect.width() * 0.37).min(rect.height() * 0.49);
    for step in (1..=16).rev() {
        painter.circle_filled(
            center,
            radius * step as f32 / 16.,
            Color32::from_rgba_unmultiplied(127, 241, 157, 5),
        );
    }
    for scale in [0.69, 0.88, 1.] {
        painter.circle_stroke(
            center,
            radius * scale,
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(174, 240, 184, 31)),
        );
    }
    if let Some(texture) = texture {
        let source = texture.size_vec2();
        let fit = ((size.x * 0.91) / source.x).min((size.y * 0.91) / source.y);
        painter.image(
            texture.id(),
            Rect::from_center_size(center, source * fit),
            Rect::from_min_max(pos2(0., 0.), pos2(1., 1.)),
            Color32::WHITE,
        );
    } else {
        paint_icon(
            &painter,
            Rect::from_center_size(center, Vec2::splat(71.)),
            fallback,
            ACCENT,
        );
    }
}

pub(super) fn load_mouse_art(context: &Context) -> Option<TextureHandle> {
    let rgba = image::load_from_memory_with_format(
        include_bytes!("../../assets/ergonomic-mouse.png"),
        image::ImageFormat::Png,
    )
    .ok()?
    .into_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let texture = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    Some(context.load_texture("ergonomic-mouse", texture, egui::TextureOptions::LINEAR))
}

fn source_card(ui: &mut Ui, source: &Source, busy: bool) -> bool {
    let mut clicked = false;
    panel(BORDER, 15).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            icon_tile(ui, Icon::File, 36.);
            ui.label(title(source.kind.name(), 17.));
        });
        let path = source.path.display().to_string();
        ui.add(Label::new(muted(path.clone()).size(12.)).truncate())
            .on_hover_text(path);
        ui.add_space(3.);
        right_actions(ui, |ui| {
            clicked = ui.add_enabled(!busy, Button::new("Preview")).clicked();
        });
    });
    clicked
}

pub(super) fn profile_count(count: usize) -> String {
    format!(
        "{count} {}",
        if count == 1 { "profile" } else { "profiles" }
    )
}

fn profile_card(ui: &mut Ui, profile: &Profile, name: &str, source: Option<&str>) {
    panel(BORDER, 19).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            icon_tile(ui, Icon::Profiles, 36.);
            ui.vertical(|ui| {
                ui.label(title(name, 19.));
                if let Some(source) = source {
                    ui.label(muted(source).size(13.));
                }
            });
        });
        ui.add_space(6.);
        let settings = &profile.settings;
        egui::Grid::new("settings")
            .num_columns(2)
            .spacing(vec2(20., 8.))
            .show(ui, |ui| {
                if let Some(dpi) = settings.dpi {
                    setting(ui, "Pointer sensitivity", format!("{dpi} DPI"));
                }
                if let Some(smooth) = settings.smooth_scroll {
                    setting(ui, "Smooth scrolling", if smooth { "On" } else { "Off" });
                }
                if let Some(direction) = settings.scroll_direction {
                    use ScrollDirection::*;
                    let label = match direction {
                        Natural => "Natural",
                        Standard => "Standard",
                        InvertedFromSystem => "Inverted from system",
                        MatchSystem => "Match system",
                    };
                    setting(ui, "Scroll direction", label);
                }
                if let Some(mode) = settings.wheel_mode {
                    let label = match mode {
                        WheelMode::Ratchet => "Ratchet",
                        WheelMode::FreeSpin => "Free-spin",
                    };
                    setting(ui, "Wheel mode", label);
                }
                if let Some(threshold) = settings.smart_shift_threshold {
                    setting(ui, "SmartShift threshold", threshold.to_string());
                }
                if let Some(notify) = settings.notify_low_battery {
                    setting(ui, "Low-battery alerts", if notify { "On" } else { "Off" });
                }
            });
        if !settings.buttons.is_empty() {
            ui.add_space(6.);
            ui.separator();
            ui.label(title("Buttons and gestures", 15.));
            for (button, binding) in &settings.buttons {
                match binding {
                    Binding::Click(action) => binding_row(ui, button.name(), action),
                    Binding::Gesture(directions) => {
                        ui.push_id(*button as u8, |ui| {
                            egui::CollapsingHeader::new(format!(
                                "{} · {} gestures",
                                button.name(),
                                directions.len()
                            ))
                            .show(ui, |ui| {
                                for (direction, action) in directions {
                                    binding_row(ui, direction.name(), action);
                                }
                            });
                        });
                    }
                }
            }
        }
    });
    ui.add_space(8.);
}

fn setting(ui: &mut Ui, label: &str, value: impl Into<String>) {
    ui.label(muted(label));
    ui.label(value.into());
    ui.end_row();
}

fn binding_row(ui: &mut Ui, label: &str, action: &Action) {
    ui.horizontal_wrapped(|ui| {
        ui.label(muted(label));
        ui.label(muted("·"));
        if let Action::OpenApplication(path) = action {
            let app = path.rsplit(['/', '\\']).next().unwrap_or(path);
            ui.label(format!("Open {app}")).on_hover_text(path);
        } else {
            ui.label(action_label(action));
        }
    });
}
