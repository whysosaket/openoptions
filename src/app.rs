use eframe::egui;
use openoptions::controls::{Control, Hosts, Pointer, Profiles};
use openoptions::devices::{self, Device};
use openoptions::migration::{self, Preview, Profile, Source, SourceKind};
use openoptions::store::Store;
use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver, Sender};

mod action_labels;
mod controls;
mod feature_panels;
mod pairing;
mod theme;
mod ui;
mod workspace;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Devices,
    Import,
    Profiles,
}

#[derive(Clone)]
enum Detail {
    Device(Device),
    Profile(SourceKind, Profile),
}

type Saved = Vec<(SourceKind, Profile)>;

enum Event {
    ControlsLoaded(
        controls::DeviceKey,
        Result<Vec<Control>, String>,
        Result<Profiles, String>,
        Option<Pointer>,
        Result<Option<Hosts>, String>,
    ),
    ControlsSaved(controls::DeviceKey, Result<Profiles, String>),
    HostsRead(controls::DeviceKey, Result<Option<Hosts>, String>),
    HostSwitched(controls::DeviceKey, Result<(), String>),
    ReceiversLoaded(Result<Vec<openoptions::pairing::ReceiverDevice>, String>),
    PointerApplied(controls::DeviceKey, Result<Pointer, String>),
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
    profiles: Saved,
    editors: BTreeMap<controls::DeviceKey, controls::Editor>,
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
            writing: true,
            profiles: Vec::new(),
            editors: BTreeMap::new(),
            live_controls: None,
            actions_ring: None,
            pairing: None,
            rescan: false,
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
                Event::ControlsLoaded(key, available, saved, pointer, hosts) => {
                    if let Some(editor) = self.editors.get_mut(&key) {
                        editor.loaded(available, saved);
                        editor.hosts = Some(hosts);
                        if let Some(pointer) = pointer {
                            editor.pointer_loaded(pointer);
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
                        if result.is_ok() || editor.hosts.is_none() {
                            editor.hosts = Some(result);
                        }
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
                Event::ReceiversLoaded(result) => {
                    if let Some(wizard) = &mut self.pairing {
                        wizard.receivers = Some(result);
                    }
                }
                Event::PointerApplied(key, result) => {
                    if let Some(editor) = self.editors.get_mut(&key) {
                        editor.pointer_busy = false;
                        editor.notice = Some(match result {
                            Ok(pointer) => {
                                editor.pointer_loaded(pointer);
                                ("Device settings applied.".into(), false)
                            }
                            Err(error) => {
                                (format!("Could not apply device settings. {error}"), true)
                            }
                        });
                    }
                }
                Event::Discovery(sources, devices) => {
                    self.sources = sources;
                    self.devices = devices;
                    self.scanning = false;
                }
                Event::Preview(source, result) => {
                    self.reading = false;
                    match result {
                        Ok(preview) => self.preview = Some((source, preview)),
                        Err(error) => self.notice = Some(Notice::Error(error)),
                    }
                }
                Event::Store(result) => {
                    self.writing = false;
                    match result {
                        Ok((count, profiles)) => {
                            self.profiles = profiles;
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
                            self.notice = Some(Notice::Error(format!(
                                "Could not access saved profiles. {error}"
                            )))
                        }
                    }
                }
            }
        }
    }
}
