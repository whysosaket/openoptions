use super::{Detail, Notice, OpenOptions, Page, action_labels::action_label, theme::*, workspace};
use eframe::egui::{self, Button, CentralPanel, Frame, RichText, ScrollArea, TextEdit, Ui, Vec2};
use openoptions::devices::Device;
use openoptions::migration::{
    Action, Binding, Profile, ScrollDirection, Source, SourceKind, WheelMode,
};
use std::path::PathBuf;

impl eframe::App for OpenOptions {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.; 4]
    }

    fn ui(&mut self, root: &mut Ui, _frame: &mut eframe::Frame) {
        self.receive();
        self.poll_controls(root.ctx());
        if self.rescan {
            self.rescan = false;
            self.scan(root.ctx());
        }
        let device_detail = matches!(self.detail, Some(Detail::Device(_)));
        if !device_detail {
            egui::Panel::top("navigation")
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::new().fill(SURFACE).inner_margin(12))
                .show_inside(root, |ui| {
                    #[cfg(target_os = "macos")]
                    ui.horizontal(|ui| {
                        ui.add_space(71.);
                        ui.label(title("OpenOptions", 13.));
                    });
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::splat(24.), egui::Sense::hover());
                        mark(ui.painter(), rect.center());
                        for (page, label) in [
                            (Page::Devices, "Devices"),
                            (Page::Import, "Import"),
                            (Page::Profiles, "Profiles"),
                        ] {
                            if ui.selectable_label(self.page == page, label).clicked() {
                                self.navigate(page);
                            }
                        }
                    });
                });
        }
        let mut margin = egui::Margin::same(0);
        if cfg!(target_os = "macos") && device_detail {
            margin.top = 30;
        }
        CentralPanel::default()
            .frame(Frame::new().fill(WINDOW).inner_margin(margin))
            .show_inside(root, |ui| {
                Frame::new().fill(WINDOW).show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    ui.spacing_mut().item_spacing.y = 0.;
                    self.header(ui);
                    let inset = if ui.available_width() < 520. { 16 } else { 22 };
                    Frame::new().inner_margin(inset).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 10.;
                        if let Some(detail) = self.detail.clone() {
                            match detail {
                                Detail::Device(device) => self.device_settings(ui, &device),
                                Detail::Profile(kind, profile) => {
                                    self.profile_detail(ui, kind, &profile)
                                }
                            }
                        } else {
                            match self.page {
                                Page::Devices => self.devices_ui(ui),
                                Page::Import if self.preview.is_some() => self.review_ui(ui),
                                Page::Import => self.import_sources(ui),
                                Page::Profiles => self.profiles_ui(ui),
                            }
                        }
                    });
                });
            });
        self.show_pairing(root.ctx());
        self.show_actions_ring(root);
    }
}

impl OpenOptions {
    fn navigate(&mut self, page: Page) {
        if self.page != page || self.detail.is_some() {
            self.page = page;
            self.detail = None;
            self.notice = None;
        }
    }

    fn header(&mut self, ui: &mut Ui) {
        let review = self.page == Page::Import && self.preview.is_some();
        let title = match (&self.detail, self.page) {
            (Some(Detail::Device(device)), _) => &device.name,
            (Some(Detail::Profile(..)), _) => "Profile details",
            (_, Page::Devices) => "Devices",
            (_, Page::Import) if review => "Review import",
            (_, Page::Import) => "Import settings",
            (_, Page::Profiles) => "Profiles",
        };
        let inventory = self.detail.is_none() && self.page == Page::Devices;
        let sources = self.page == Page::Import && !review;
        let back_enabled = (self.detail.is_some() || review).then_some(!self.writing);
        let title = title.to_owned();
        let back = workspace::header(ui, &title, back_enabled, |ui| {
            if let Some(live) = &self.live_controls
                && ui
                    .add_enabled(
                        !live.stopping,
                        Button::new(if live.stopping {
                            "Disabling…"
                        } else {
                            "Disable live controls"
                        }),
                    )
                    .on_hover_text(&live.name)
                    .clicked()
            {
                self.stop_controls();
            }
            if inventory && ui.add(primary("+ Add Device")).clicked() {
                self.open_pairing(ui.ctx());
            }
            if sources {
                if labeled_button(ui, "Scan again", Icon::Refresh, !self.scanning).clicked() {
                    self.scan(ui.ctx());
                }
                if self.scanning {
                    ui.spinner();
                }
            }
            if review {
                let count = self
                    .preview
                    .as_ref()
                    .map_or(0, |(_, preview)| preview.profiles.len());
                if count > 0
                    && ui
                        .add_enabled(
                            !self.writing,
                            primary(format!("Import {}", workspace::profile_count(count))),
                        )
                        .clicked()
                {
                    self.import(ui.ctx());
                }
                if self.writing {
                    ui.spinner();
                }
            }
        });
        if back {
            if self.detail.is_some() {
                self.detail = None;
            } else {
                self.preview = None;
            }
            self.notice = None;
        }
    }

    fn devices_ui(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if self
                .devices
                .as_ref()
                .is_ok_and(|devices| !devices.is_empty())
            {
                workspace::search(ui, &mut self.device_query, "Search devices");
            }
            workspace::trailing(ui, |ui| {
                if labeled_button(ui, "Refresh", Icon::Refresh, !self.scanning).clicked() {
                    self.scan(ui.ctx());
                }
                if self.scanning {
                    ui.spinner();
                }
            });
        });
        show_notice(ui, self.notice.as_ref());
        let mut selected = None;
        match &self.devices {
            Ok(devices) if devices.is_empty() && self.scanning => {
                skeleton(ui, "devices", ["Device", "Connection", "Status"]);
            }
            Ok(devices) if devices.is_empty() => {
                workspace::empty(
                    ui,
                    "No devices connected",
                    "Connect a Logitech receiver or pair a device over Bluetooth.",
                    |_| {},
                );
            }
            Err(error) => {
                let mut retry = false;
                workspace::empty(
                    ui,
                    "Couldn't scan for devices",
                    "Check your device connection and try again.",
                    |ui| {
                        ui.collapsing("Details", |ui| {
                            ui.label(muted(error));
                        });
                        retry = labeled_button(ui, "Try again", Icon::Refresh, !self.scanning)
                            .clicked();
                    },
                );
                if retry {
                    self.scan(ui.ctx());
                }
            }
            Ok(devices) => {
                let query = self.device_query.trim().to_lowercase();
                let mut rows = devices
                    .iter()
                    .filter(|device| {
                        matches_query(&query, &[device.name.as_str(), device.connection])
                    })
                    .peekable();
                if rows.peek().is_none() {
                    workspace::empty(
                        ui,
                        "No matching devices",
                        "Try a different name or connection.",
                        |ui| {
                            if ui.button("Clear search").clicked() {
                                self.device_query.clear();
                            }
                        },
                    );
                } else {
                    let visible: Vec<_> = rows.collect();
                    let columns = if ui.available_width() >= 950. { 3 } else { 2 };
                    ScrollArea::vertical()
                        .id_salt("device-gallery")
                        .show(ui, |ui| {
                            let edge = ((ui.available_width() - 16. * (columns as f32 - 1.))
                                / columns as f32)
                                .min(326.);
                            for row in visible.chunks(columns) {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 16.;
                                    for device in row {
                                        if device_tile(ui, device, edge) {
                                            selected = Some((*device).clone());
                                        }
                                    }
                                });
                                ui.add_space(7.);
                            }
                        });
                }
            }
        }
        if let Some(device) = selected {
            self.open_device(device, ui.ctx());
        }
        if !self.sources.is_empty() {
            ui.horizontal(|ui| {
                ui.label(muted(format!("{} settings files found", self.sources.len())).size(12.));
                workspace::trailing(ui, |ui| {
                    if labeled_button(ui, "Review imports", Icon::Import, true).clicked() {
                        self.navigate(Page::Import);
                    }
                });
            });
        }
    }

    fn import_sources(&mut self, ui: &mut Ui) {
        ScrollArea::vertical()
            .id_salt("import sources")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let busy = self.reading || self.writing;
                let mut selected = None;
                if self.sources.is_empty() && self.scanning {
                    skeleton(ui, "sources", ["Source", "File", ""]);
                } else if self.sources.is_empty() {
                    workspace::empty(
                        ui,
                        "No settings files found",
                        "Enter the path to a settings file below.",
                        |_| {},
                    );
                } else {
                    workspace::index(
                        ui,
                        "sources",
                        ["Source", "File", ""],
                        126.,
                        |ui, columns| {
                            for (index, source) in self.sources.iter().enumerate() {
                                if index > 0 {
                                    workspace::divider(ui);
                                }
                                let path = source.path.display().to_string();
                                let file = source
                                    .path
                                    .file_name()
                                    .map(|name| name.to_string_lossy())
                                    .unwrap_or((&path).into());
                                columns.row(
                                    ui,
                                    |ui| {
                                        icon(ui, Icon::File, 16., MUTED);
                                        ui.add_space(7.);
                                        clipped(
                                            ui,
                                            title(source.kind.name(), 14.),
                                            source.kind.name(),
                                        );
                                    },
                                    |ui| clipped(ui, muted(file.as_ref()).size(12.), &path),
                                    |ui| {
                                        if ui.add_enabled(!busy, Button::new("Preview")).clicked() {
                                            selected = Some(source.clone());
                                        }
                                    },
                                );
                            }
                        },
                    );
                }
                ui.collapsing("Use a settings file", |ui| {
                    ui.label(muted("Path to a TOML, SQLite, or JSON file"));
                    ui.horizontal(|ui| {
                        ui.add(
                            TextEdit::singleline(&mut self.manual_path)
                                .hint_text("File path")
                                .desired_width((ui.available_width() - 113.).max(80.)),
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
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(muted("Reading settings…"));
                    });
                }
                show_notice(ui, self.notice.as_ref());
            });
    }

    fn review_ui(&self, ui: &mut Ui) {
        let Some((source, preview)) = &self.preview else {
            return;
        };
        show_notice(ui, self.notice.as_ref());
        ScrollArea::vertical()
            .id_salt("review import")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(muted(format!("Source · {}", source.kind.name())).size(12.));
                if preview.profiles.is_empty() {
                    workspace::empty(
                        ui,
                        "No supported settings",
                        "Choose another file to import.",
                        |_| {},
                    );
                }
                for (index, profile) in preview.profiles.iter().enumerate() {
                    ui.push_id(index, |ui| {
                        ui.label(title(self.profile_name(profile, source.kind), 22.));
                        profile_sections(ui, profile);
                    });
                }
                if !preview.skipped.is_empty() {
                    let count: usize = preview.skipped.values().sum();
                    egui::CollapsingHeader::new(
                        RichText::new(format!("Skipped settings ({count})")).color(WARNING),
                    )
                    .default_open(true)
                    .show(ui, |ui| {
                        for (reason, count) in &preview.skipped {
                            ui.label(muted(format!("{reason} ({count})")));
                        }
                    });
                }
            });
    }

    fn profiles_ui(&mut self, ui: &mut Ui) {
        show_notice(ui, self.notice.as_ref());
        if self.profiles.is_empty() {
            if self.writing {
                skeleton(ui, "profiles", ["Device", "Imported from", ""]);
            } else {
                let mut open_import = false;
                workspace::empty(
                    ui,
                    "No saved profiles",
                    "Import settings to create a profile.",
                    |ui| {
                        open_import = ui.add(primary("Import settings")).clicked();
                    },
                );
                if open_import {
                    self.navigate(Page::Import);
                }
            }
            return;
        }
        ui.horizontal(|ui| {
            workspace::search(ui, &mut self.profile_query, "Search profiles");
            workspace::trailing(ui, |ui| {
                if ui.add(primary("Import settings")).clicked() {
                    self.navigate(Page::Import);
                }
            });
        });

        let query = self.profile_query.trim().to_lowercase();
        let mut rows = self
            .profiles
            .iter()
            .filter(|(kind, profile)| {
                matches_query(&query, &[self.profile_name(profile, *kind), kind.name()])
            })
            .peekable();
        if rows.peek().is_none() {
            workspace::empty(
                ui,
                "No matching profiles",
                "Try a different name or source.",
                |ui| {
                    if ui.button("Clear search").clicked() {
                        self.profile_query.clear();
                    }
                },
            );
            return;
        }

        let mut selected = None;
        workspace::index(
            ui,
            "profiles",
            ["Device", "Imported from", ""],
            0.,
            |ui, columns| {
                for (index, (kind, profile)) in rows.enumerate() {
                    if index > 0 {
                        workspace::divider(ui);
                    }
                    columns.row(
                        ui,
                        |ui| {
                            if workspace::row_title(
                                ui,
                                self.profile_name(profile, *kind),
                                Icon::Profiles,
                            ) {
                                selected = Some(Detail::Profile(*kind, profile.clone()));
                            }
                        },
                        |ui| {
                            clipped(ui, muted(kind.name()), kind.name());
                        },
                        |_| {},
                    );
                }
            },
        );
        if let Some(detail) = selected {
            self.detail = Some(detail);
        }
    }

    fn profile_detail(&self, ui: &mut Ui, kind: SourceKind, profile: &Profile) {
        ScrollArea::vertical()
            .id_salt("profile-detail")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(title(self.profile_name(profile, kind), 23.));
                ui.label(muted(format!("Imported from {}", kind.name())));
                profile_sections(ui, profile);
            });
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
}

fn matches_query(query: &str, values: &[&str]) -> bool {
    query.is_empty()
        || values
            .iter()
            .any(|value| value.to_lowercase().contains(query))
}

fn skeleton(ui: &mut Ui, id: &str, headers: [&str; 3]) {
    workspace::index(ui, id, headers, 0., |ui, columns| {
        for index in 0..3 {
            if index > 0 {
                workspace::divider(ui);
            }
            columns.placeholder(ui);
        }
    });
}

fn show_notice(ui: &mut Ui, notice: Option<&Notice>) {
    let Some(notice) = notice else { return };
    let (copy, color) = match notice {
        Notice::Info(copy) => (copy, SUCCESS),
        Notice::Error(copy) => (copy, WARNING),
    };
    surface(10).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(copy).color(color));
    });
}

fn device_tile(ui: &mut Ui, device: &Device, width: f32) -> bool {
    let (_, response) = ui.allocate_exact_size(egui::vec2(width, 230.), egui::Sense::click());
    let rect = response.rect;
    let painter = ui.painter_at(rect);
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &device.name));
    painter.rect(
        rect.shrink(1.),
        10,
        if response.hovered() { HOVER } else { SURFACE },
        line(),
        egui::StrokeKind::Inside,
    );
    let illustration = egui::Rect::from_center_size(
        rect.center_top() + egui::vec2(0., 88.),
        egui::Vec2::splat(107.),
    );
    paint_icon(&painter, illustration, device_icon(device), MUTED);
    let details = rect.left_bottom() + egui::vec2(17., -61.);
    let mut name: String = device.name.chars().take(30).collect();
    if device.name.chars().count() > 30 {
        name.push('…');
    }
    painter.text(
        details,
        egui::Align2::LEFT_TOP,
        name,
        egui::FontId::proportional(16.),
        TEXT,
    );
    painter.circle_filled(details + egui::vec2(4., 34.), 3.5, SUCCESS);
    painter.text(
        details + egui::vec2(14., 34.),
        egui::Align2::LEFT_CENTER,
        format!("Connected · {}", device.connection),
        egui::FontId::proportional(12.),
        MUTED,
    );
    paint_focus(ui, &response);
    response.on_hover_text(&device.name).clicked()
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

fn profile_sections(ui: &mut Ui, profile: &Profile) {
    let settings = &profile.settings;
    let mut values = Vec::new();
    if let Some(dpi) = settings.dpi {
        values.push(("Pointer sensitivity", format!("{dpi} DPI")));
    }
    if let Some(smooth) = settings.smooth_scroll {
        values.push(("Smooth scrolling", on_off(smooth).into()));
    }
    if let Some(direction) = settings.scroll_direction {
        let label = match direction {
            ScrollDirection::Natural => "Natural",
            ScrollDirection::Standard => "Standard",
            ScrollDirection::InvertedFromSystem => "Inverted from system",
            ScrollDirection::MatchSystem => "Match system",
        };
        values.push(("Scroll direction", label.into()));
    }
    if let Some(mode) = settings.wheel_mode {
        let label = match mode {
            WheelMode::Ratchet => "Ratchet",
            WheelMode::FreeSpin => "Free-spin",
        };
        values.push(("Wheel mode", label.into()));
    }
    if let Some(threshold) = settings.smart_shift_threshold {
        values.push(("SmartShift threshold", threshold.to_string()));
    }
    if let Some(notify) = settings.notify_low_battery {
        values.push(("Low-battery alerts", on_off(notify).into()));
    }
    if !values.is_empty() {
        facts(ui, values);
    }
    if settings.buttons.is_empty() {
        return;
    }
    ui.label(title("Buttons and gestures", 15.));
    surface(12).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 4.;
        for (index, (button, binding)) in settings.buttons.iter().enumerate() {
            if index > 0 {
                workspace::divider(ui);
            }
            match binding {
                Binding::Click(action) => binding_row(ui, button.name(), action),
                Binding::Gesture(directions) => {
                    ui.push_id(*button as u8, |ui| {
                        ui.collapsing(
                            format!("{} · {} gestures", button.name(), directions.len()),
                            |ui| {
                                for (direction, action) in directions {
                                    binding_row(ui, direction.name(), action);
                                }
                            },
                        );
                    });
                }
            }
        }
    });
}

fn facts<'a>(ui: &mut Ui, facts: impl IntoIterator<Item = (&'a str, String)>) {
    surface(12).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 4.;
        for (index, (label, value)) in facts.into_iter().enumerate() {
            if index > 0 {
                workspace::divider(ui);
            }
            fact(ui, label, &value, &value);
        }
    });
}

fn fact(ui: &mut Ui, label: &str, value: &str, full: &str) {
    ui.horizontal(|ui| {
        ui.label(muted(label));
        workspace::trailing(ui, |ui| clipped(ui, RichText::new(value).color(TEXT), full));
    });
}

fn on_off(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

fn binding_row(ui: &mut Ui, label: &str, action: &Action) {
    if let Action::OpenApplication(path) = action {
        let app = path.rsplit(['/', '\\']).next().unwrap_or(path);
        fact(ui, label, &format!("Open {app}"), path);
    } else {
        let label_text = action_label(action);
        fact(ui, label, label_text, label_text);
    }
}
