use eframe::egui::{self, Button, Color32, Frame, RichText, ScrollArea, TextEdit};
use openoptions::devices::{self, Device};
use openoptions::migration::{
    self, Action, Binding, NamedAction, Preview, Profile, Source, SourceKind,
};
use openoptions::store::Store;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Devices,
    Import,
    Profiles,
}

type Saved = Vec<(SourceKind, Profile)>;

enum Event {
    Discovery(Vec<Source>, Result<Vec<Device>, String>),
    Preview(Source, Result<Preview, String>),
    Store(Result<(Option<usize>, Saved), String>),
}

pub struct OpenOptions {
    page: Page,
    sender: Sender<Event>,
    events: Receiver<Event>,
    scanning: bool,
    devices: Result<Vec<Device>, String>,
    sources: Vec<Source>,
    manual_path: String,
    reading: bool,
    preview: Option<(Source, Preview)>,
    notice: Option<String>,
    writing: bool,
    profiles: Saved,
}

impl OpenOptions {
    pub fn new(context: &egui::Context) -> Self {
        let (sender, events) = mpsc::channel();
        let mut app = Self {
            page: Page::Devices,
            sender,
            events,
            scanning: false,
            devices: Ok(Vec::new()),
            sources: Vec::new(),
            manual_path: String::new(),
            reading: false,
            preview: None,
            notice: None,
            writing: true,
            profiles: Vec::new(),
        };
        app.scan(context);
        app.job(context, || {
            let result = (|| {
                let path = Store::location()?;
                let profiles = if path.is_file() {
                    Store::open(&path)?.profiles()?
                } else {
                    Vec::new()
                };
                Ok((None, profiles))
            })();
            Event::Store(result)
        });
        app
    }

    fn job(&self, context: &egui::Context, work: impl FnOnce() -> Event + Send + 'static) {
        let sender = self.sender.clone();
        let context = context.clone();
        std::thread::spawn(move || {
            let _ = sender.send(work());
            context.request_repaint();
        });
    }

    fn scan(&mut self, context: &egui::Context) {
        self.scanning = true;
        self.job(context, || {
            Event::Discovery(migration::detect_sources(), devices::connected())
        });
    }

    fn read(&mut self, source: Source, context: &egui::Context) {
        self.reading = true;
        self.preview = None;
        self.notice = None;
        self.job(context, || {
            let preview = migration::preview(&source);
            Event::Preview(source, preview)
        });
    }

    fn import(&mut self, context: &egui::Context) {
        let Some((source, preview)) = self.preview.clone() else {
            return;
        };
        self.writing = true;
        self.notice = None;
        self.job(context, move || {
            let result = (|| {
                let path = Store::location()?;
                let mut store = Store::open(&path)?;
                let count = store.import(source.kind, &preview)?;
                Ok((Some(count), store.profiles()?))
            })();
            Event::Store(result)
        });
    }

    fn receive(&mut self) {
        for event in self.events.try_iter() {
            match event {
                Event::Discovery(sources, devices) => {
                    self.sources = sources;
                    self.devices = devices;
                    self.scanning = false;
                }
                Event::Preview(source, result) => {
                    self.reading = false;
                    match result {
                        Ok(preview) => self.preview = Some((source, preview)),
                        Err(error) => self.notice = Some(error),
                    }
                }
                Event::Store(result) => {
                    self.writing = false;
                    match result {
                        Ok((count, profiles)) => {
                            self.profiles = profiles;
                            if let Some(count) = count {
                                self.notice = Some(if count == 0 {
                                    "These profiles are already saved.".into()
                                } else {
                                    format!(
                                        "Saved {}. View them in Profiles.",
                                        profile_count(count)
                                    )
                                });
                            }
                        }
                        Err(error) => {
                            self.notice = Some(format!("Could not access saved profiles. {error}"))
                        }
                    }
                }
            }
        }
    }

    fn devices(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Connected devices");
            if ui
                .add_enabled(!self.scanning, Button::new("Refresh"))
                .clicked()
            {
                self.scan(ui.ctx());
            }
        });
        ui.label("Logitech HID products reported by this computer.");
        ui.add_space(12.);
        if self.scanning {
            ui.spinner();
        }
        match &self.devices {
            Ok(devices) if devices.is_empty() && !self.scanning => {
                ui.label("No Logitech HID devices found. Connect a receiver or pair a device over Bluetooth, then refresh.");
            }
            Err(error) => {
                ui.label(error);
            }
            Ok(devices) => {
                for device in devices {
                    card(ui, |ui| {
                        ui.strong(&device.name);
                        ui.label(format!(
                            "{}  ·  046D:{:04X}  ·  {} HID {}",
                            device.connection,
                            device.product_id,
                            device.interfaces,
                            if device.interfaces == 1 {
                                "interface"
                            } else {
                                "interfaces"
                            },
                        ));
                    });
                }
            }
        }
        ui.add_space(10.);
        if ui.button("Open Bluetooth settings").clicked()
            && let Err(error) = devices::open_bluetooth_settings()
        {
            self.notice = Some(error);
        }
        ui.label("Pair a Bluetooth device in your system settings, then return here and refresh.");
    }

    fn import_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Import settings");
            if ui
                .add_enabled(!self.scanning, Button::new("Scan again"))
                .clicked()
            {
                self.scan(ui.ctx());
            }
        });
        ui.label("Preview settings from another Logitech manager before saving a profile.");
        ui.add_space(12.);
        let busy = self.reading || self.writing;
        if self.sources.is_empty() {
            ui.label(if self.scanning {
                "Looking for local settings…"
            } else {
                "No settings files found in the standard locations."
            });
        }
        let mut selected = None;
        for source in &self.sources {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(source.kind.name());
                    if ui.add_enabled(!busy, Button::new("Preview")).clicked() {
                        selected = Some(source.clone());
                    }
                });
                ui.label(source.path.display().to_string());
            });
        }
        ui.add_space(10.);
        ui.horizontal(|ui| {
            ui.label("Settings file");
            ui.add(
                TextEdit::singleline(&mut self.manual_path)
                    .hint_text("Path to a TOML, SQLite, or JSON file")
                    .desired_width(280.),
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
                    Err(error) => self.notice = Some(error),
                }
            }
        });
        if let Some(source) = selected {
            self.read(source, ui.ctx());
        }
        if self.reading || self.writing {
            ui.spinner();
        }
        let mut save = false;
        if let Some((source, preview)) = &self.preview {
            ui.add_space(14.);
            ui.separator();
            ui.heading(format!("{} preview", source.kind.name()));
            if preview.profiles.is_empty() {
                ui.label("This file has no settings supported by the importer.");
            }
            for (index, profile) in preview.profiles.iter().enumerate() {
                ui.push_id(index, |ui| profile_card(ui, profile, None));
            }
            if !preview.skipped.is_empty() {
                ui.strong("Skipped");
                for (reason, count) in &preview.skipped {
                    ui.label(format!("{reason} ({count})"));
                }
            }
            if !preview.profiles.is_empty() {
                ui.add_space(8.);
                let label = format!("Import {}", profile_count(preview.profiles.len()));
                save = ui.add_enabled(!busy, Button::new(label)).clicked();
            }
        }
        if save {
            self.import(ui.ctx());
        }
    }

    fn profile_list(&self, ui: &mut egui::Ui) {
        ui.heading("Saved profiles");
        ui.label("Imported settings live on this computer.");
        ui.add_space(12.);
        if self.profiles.is_empty() {
            ui.label("Import settings to create your first profile.");
        }
        for (index, (kind, profile)) in self.profiles.iter().enumerate() {
            ui.push_id(index, |ui| profile_card(ui, profile, Some(kind.name())));
        }
    }
}

impl eframe::App for OpenOptions {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.receive();
        egui::Panel::left("navigation")
            .exact_size(155.)
            .resizable(false)
            .show_inside(root, |ui| {
                ui.add_space(12.);
                ui.heading("OpenOptions");
                ui.add_space(18.);
                for (page, label) in [
                    (Page::Devices, "Devices"),
                    (Page::Import, "Import settings"),
                    (Page::Profiles, "Profiles"),
                ] {
                    if ui
                        .add_sized([132., 32.], Button::selectable(self.page == page, label))
                        .clicked()
                    {
                        self.page = page;
                        self.notice = None;
                    }
                }
            });
        egui::CentralPanel::default().show_inside(root, |ui| {
            ScrollArea::vertical()
                .id_salt(self.page as u8)
                .show(ui, |ui| {
                    ui.add_space(12.);
                    match self.page {
                        Page::Devices => self.devices(ui),
                        Page::Import => self.import_ui(ui),
                        Page::Profiles => self.profile_list(ui),
                    }
                    if let Some(notice) = &self.notice {
                        ui.add_space(10.);
                        ui.label(notice);
                    }
                });
        });
    }
}

fn card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    let visuals = ui.visuals();
    let fill = if visuals.dark_mode {
        Color32::from_gray(31)
    } else {
        Color32::from_gray(248)
    };
    Frame::group(ui.style())
        .fill(fill)
        .inner_margin(12.)
        .corner_radius(8)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            contents(ui);
        });
    ui.add_space(6.);
}

fn profile_count(count: usize) -> String {
    format!(
        "{count} {}",
        if count == 1 { "profile" } else { "profiles" }
    )
}

fn profile_card(ui: &mut egui::Ui, profile: &Profile, source: Option<&str>) {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.strong(&profile.device_name);
            if let Some(source) = source {
                ui.label(RichText::new(source).weak());
            }
        });
        let settings = &profile.settings;
        if let Some(dpi) = settings.dpi {
            ui.label(format!("Pointer sensitivity · {dpi} DPI"));
        }
        if let Some(smooth) = settings.smooth_scroll {
            ui.label(if smooth {
                "Smooth scrolling · On"
            } else {
                "Smooth scrolling · Off"
            });
        }
        if let Some(direction) = settings.scroll_direction {
            use migration::ScrollDirection::*;
            let label = match direction {
                Natural => "Natural",
                Standard => "Standard",
                InvertedFromSystem => "Inverted from system",
                MatchSystem => "Match system",
            };
            ui.label(format!("Scroll direction · {label}"));
        }
        if let Some(mode) = settings.wheel_mode {
            let label = match mode {
                migration::WheelMode::Ratchet => "Ratchet",
                migration::WheelMode::FreeSpin => "Free-spin",
            };
            ui.label(format!("Wheel mode · {label}"));
        }
        if let Some(threshold) = settings.smart_shift_threshold {
            ui.label(format!("SmartShift threshold · {threshold}"));
        }
        if let Some(notify) = settings.notify_low_battery {
            ui.label(if notify {
                "Low-battery alerts · On"
            } else {
                "Low-battery alerts · Off"
            });
        }
        for (button, binding) in &settings.buttons {
            match binding {
                Binding::Click(action) => {
                    ui.label(format!("{} · {}", button.name(), action_label(action)));
                }
                Binding::Gesture(directions) => {
                    ui.push_id(*button as u8, |ui| {
                        ui.collapsing(
                            format!("{} · {} gestures", button.name(), directions.len()),
                            |ui| {
                                for (direction, action) in directions {
                                    ui.label(format!(
                                        "{} · {}",
                                        direction.name(),
                                        action_label(action)
                                    ));
                                }
                            },
                        );
                    });
                }
            }
        }
    });
}

fn action_label(action: &Action) -> &str {
    match action {
        Action::OpenApplication(path) => path,
        Action::Named(action) => match action {
            NamedAction::NoAction => "Do nothing",
            NamedAction::MiddleClick => "Middle click",
            NamedAction::Back => "Back",
            NamedAction::Forward => "Forward",
            NamedAction::Copy => "Copy",
            NamedAction::Cut => "Cut",
            NamedAction::Paste => "Paste",
            NamedAction::Undo => "Undo",
            NamedAction::Redo => "Redo",
            NamedAction::MissionControl => "Mission Control",
            NamedAction::AppExpose => "App Exposé",
            NamedAction::Desktop => "Show desktop",
            NamedAction::PreviousDesktop => "Previous desktop",
            NamedAction::NextDesktop => "Next desktop",
            NamedAction::PlayPause => "Play or pause",
            NamedAction::PreviousTrack => "Previous track",
            NamedAction::NextTrack => "Next track",
            NamedAction::VolumeUp => "Volume up",
            NamedAction::VolumeDown => "Volume down",
            NamedAction::Mute => "Mute",
        },
    }
}
