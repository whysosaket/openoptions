use eframe::egui;
use openoptions::devices::{self, Device};
use openoptions::migration::{self, Preview, Profile, Source, SourceKind};
use openoptions::store::Store;
use std::sync::mpsc::{self, Receiver, Sender};

mod action_labels;
mod theme;
mod ui;

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

enum Notice {
    Info(String),
    Error(String),
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
    notice: Option<Notice>,
    mouse_art: Option<egui::TextureHandle>,
    writing: bool,
    profiles: Saved,
}

impl OpenOptions {
    pub fn new(context: &egui::Context) -> Self {
        theme::install(context);
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
            mouse_art: ui::load_mouse_art(context),
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
                                        ui::profile_count(count)
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
