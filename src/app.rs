use eframe::egui;
use openoptions::controls::{Control, Hosts, Pointer, Profiles};
use openoptions::devices::{self, Device, DeviceKey};
use openoptions::migration::{self, Preview, Profile, Source, SourceKind};
use openoptions::store::Store;
use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver, Sender};

mod action_labels;
mod action_picker;
mod applications;
mod controls;
mod feature_panels;
mod gallery;
mod pairing;
mod screen;
mod settings;
mod theme;
mod ui;
mod workspace;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Devices,
    Import,
    Profiles,
    Settings,
}

#[derive(Clone)]
enum Detail {
    Device(Device),
    Profile(SourceKind, Profile),
}

type Saved = Vec<(SourceKind, Profile)>;

struct DeviceSettings {
    available: Result<Vec<Control>, String>,
    pointer: Result<Pointer, String>,
    hosts: Result<Option<Hosts>, String>,
}

enum Event {
    ControlsLoaded(
        DeviceKey,
        Option<DeviceSettings>,
        Option<Result<Profiles, String>>,
    ),
    ControlsSaved(DeviceKey, Result<Profiles, String>),
    HostsRead(DeviceKey, Result<Option<Hosts>, String>),
    HostSwitched(DeviceKey, Result<(), String>),
    PointerApplied(DeviceKey, controls::Tuning, Result<Pointer, String>),
    Discovery(Vec<Source>, Result<Vec<Device>, String>),
    Preview(Source, Result<Preview, String>),
    Store(Result<(Option<usize>, Saved), String>),
}

enum Notice {
    Info(String),
    Error(String),
}

pub struct OpenOptions {
    page: Page,
    detail: Option<Detail>,
    device_query: String,
    device_view: gallery::View,
    selected_device: Option<DeviceKey>,
    profile_query: String,
    sender: Sender<Event>,
    events: Receiver<Event>,
    scanning: bool,
    devices: Result<Vec<Device>, String>,
    sources: Vec<Source>,
    manual_path: String,
    reading: bool,
    preview: Option<(Source, Preview)>,
    notice: Option<Notice>,
    writing: bool,
    store_error: Option<String>,
    profiles: Saved,
    editors: BTreeMap<DeviceKey, controls::Editor>,
    live_controls: Option<controls::LiveControls>,
    actions_ring: Option<feature_panels::RingWindow>,
    pairing: Option<pairing::Wizard>,
    rescan: bool,
}

impl OpenOptions {
    pub fn new(context: &egui::Context) -> Self {
        theme::install(context);
        let (sender, events) = mpsc::channel();
        let mut app = Self {
            page: Page::Devices,
            detail: None,
            device_query: String::new(),
            device_view: gallery::View::Gallery,
            selected_device: None,
            profile_query: String::new(),
            sender,
            events,
            scanning: false,
            devices: Ok(Vec::new()),
            sources: Vec::new(),
            manual_path: String::new(),
            reading: false,
            preview: None,
            notice: None,
            writing: false,
            store_error: None,
            profiles: Vec::new(),
            editors: BTreeMap::new(),
            live_controls: None,
            actions_ring: None,
            pairing: None,
            rescan: false,
        };
        app.scan(context);
        app.load_profiles(context);
        app
    }

    fn load_profiles(&mut self, context: &egui::Context) {
        self.writing = true;
        self.store_error = None;
        self.job(context, || {
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
        if self.scanning {
            self.rescan = true;
            return;
        }
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
        self.store_error = None;
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
                Event::ControlsLoaded(key, hardware, saved) => {
                    if let Some(editor) = self.editors.get_mut(&key) {
                        if let Some(hardware) = hardware {
                            editor.loaded(Some(hardware.available), saved);
                            editor.hosts = Some(hardware.hosts);
                            editor.pointer_loaded(hardware.pointer, None);
                        } else {
                            editor.loaded(None, saved);
                        }
                    }
                }
                Event::ControlsSaved(key, result) => {
                    if let Some(editor) = self.editors.get_mut(&key) {
                        editor.saving = false;
                        editor.notice = Some(match result {
                            Ok(bindings) => {
                                editor.saved = Ok(bindings);
                                ("Assignments saved.".into(), false)
                            }
                            Err(error) => (format!("Could not save assignments. {error}"), true),
                        });
                    }
                }
                Event::HostsRead(key, result) => {
                    if let Some(editor) = self.editors.get_mut(&key) {
                        editor.host_busy = false;
                        editor.notice = result.as_ref().err().map(|error| (error.clone(), true));
                        editor.hosts = Some(result);
                    }
                }
                Event::HostSwitched(key, result) => {
                    if let Some(editor) = self.editors.get_mut(&key) {
                        editor.host_busy = false;
                        editor.notice = Some(match result {
                            Ok(()) => {
                                self.rescan = true;
                                ("Switch request sent to the device.".into(), false)
                            }
                            Err(error) => (error, true),
                        });
                    }
                }
                Event::PointerApplied(key, tuning, result) => {
                    if let Some(editor) = self.editors.get_mut(&key) {
                        editor.pointer_busy = false;
                        editor.notice = Some(match result {
                            Ok(pointer) => {
                                editor.pointer_loaded(Ok(pointer), Some(tuning));
                                ("Device settings applied.".into(), false)
                            }
                            Err(error) => {
                                editor.reload_requested = true;
                                (format!("Could not apply device settings. {error}"), true)
                            }
                        });
                    }
                }
                Event::Discovery(sources, devices) => {
                    self.sources = sources;
                    if let Ok(devices) = &devices {
                        for device in devices.iter().filter(|device| device.connected) {
                            if let Some(editor) = self.editors.get_mut(&device.key()) {
                                editor.reload_requested = true;
                            }
                        }
                        if let Some(Detail::Device(device)) = &mut self.detail {
                            let key = device.key();
                            if let Some(current) = devices.iter().find(|device| device.key() == key)
                            {
                                *device = current.clone();
                            } else {
                                device.connected = false;
                                device.battery = Ok(None);
                            }
                        }
                        if let Some(live) = &mut self.live_controls
                            && !devices
                                .iter()
                                .any(|device| device.connected && device.key() == live.key)
                        {
                            live.stopping = true;
                            live.session.stop();
                            self.actions_ring = None;
                        }
                    }
                    self.devices = devices;
                    self.scanning = false;
                }
                Event::Preview(source, result) => {
                    self.reading = false;
                    match result {
                        Ok(preview) => self.preview = Some((source, preview)),
                        Err(error) => {
                            let name = source.path.file_name().unwrap_or(source.path.as_os_str());
                            self.notice = Some(Notice::Error(format!(
                                "Could not preview {}. {error} Check the file format or choose another file.",
                                name.to_string_lossy(),
                            )));
                        }
                    }
                }
                Event::Store(result) => {
                    self.writing = false;
                    match result {
                        Ok((count, profiles)) => {
                            self.profiles = profiles;
                            self.store_error = None;
                            if let Some(count) = count {
                                self.notice = Some(Notice::Info(if count == 0 {
                                    "These profiles are already saved.".into()
                                } else {
                                    format!(
                                        "Saved {}. View them in Profiles.",
                                        workspace::profile_count(count)
                                    )
                                }));
                            }
                        }
                        Err(error) => {
                            self.store_error =
                                Some(format!("Could not access saved profiles. {error}"));
                        }
                    }
                }
            }
        }
    }
}
