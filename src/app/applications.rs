use super::theme::*;
use eframe::egui::{self, Id, TextEdit, Ui};
#[cfg(target_os = "macos")]
use eframe::egui::{Button, ScrollArea};
use openoptions::controls::Profiles;

pub(super) struct AppPicker {
    path: String,
    error: Option<String>,
    #[cfg(target_os = "macos")]
    query: String,
    #[cfg(target_os = "macos")]
    apps: Vec<InstalledApp>,
    #[cfg(target_os = "macos")]
    focus_search: bool,
}

#[cfg(target_os = "macos")]
struct InstalledApp {
    name: String,
    path: String,
    identity: String,
}

impl AppPicker {
    pub fn new() -> Self {
        Self {
            path: String::new(),
            error: None,
            #[cfg(target_os = "macos")]
            query: String::new(),
            #[cfg(target_os = "macos")]
            apps: installed_mac_apps(),
            #[cfg(target_os = "macos")]
            focus_search: true,
        }
    }
}

pub(super) fn app_picker(
    ui: &mut Ui,
    picker: &mut Option<AppPicker>,
    profiles: &mut Profiles,
) -> Option<String> {
    let state = picker.as_mut()?;
    let response = egui::Modal::new(Id::new("add-application-profile"))
        .frame(surface(18))
        .show(ui.ctx(), |ui| {
            ui.set_width(410.);
            ui.label(title("Add application", 19.));
            #[cfg(target_os = "macos")]
            state.installed(ui, profiles);

            let hint = if cfg!(target_os = "macos") {
                "Path to an installed .app"
            } else {
                "Path to the application executable"
            };
            ui.label(title("Application path", 13.));
            if ui
                .add(
                    TextEdit::singleline(&mut state.path)
                        .hint_text(hint)
                        .desired_width(ui.available_width()),
                )
                .changed()
            {
                state.error = None;
            }
            if let Some(error) = &state.error {
                ui.colored_label(WARNING, error);
            }
            let mut selected = None;
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    ui.close();
                }
                if ui
                    .add_enabled(!state.path.trim().is_empty(), primary("Add profile"))
                    .clicked()
                {
                    match profiles.add(&state.path) {
                        Ok(app) => selected = Some(app),
                        Err(error) => state.error = Some(error),
                    }
                }
            });
            selected
        });
    if response.should_close() || response.inner.is_some() {
        *picker = None;
    }
    response.inner
}

#[cfg(target_os = "macos")]
impl AppPicker {
    fn installed(&mut self, ui: &mut Ui, profiles: &Profiles) {
        let search = ui.add(
            TextEdit::singleline(&mut self.query)
                .hint_text("Search installed applications")
                .desired_width(ui.available_width()),
        );
        if std::mem::take(&mut self.focus_search) {
            search.request_focus();
        }

        let query = self.query.trim().to_lowercase();
        ScrollArea::vertical()
            .id_salt("installed-apps")
            .max_height((ui.ctx().content_rect().height() - 254.).clamp(96., 236.))
            .show(ui, |ui| {
                let mut found = false;
                for app in &self.apps {
                    if !super::ui::matches_query(&query, &[&app.name, &app.path]) {
                        continue;
                    }
                    found = true;
                    let configured = profiles.applications.contains_key(&app.identity);
                    let available = std::path::Path::new(&app.path).is_dir();
                    let caption = if configured {
                        format!("{} · Already configured", app.name)
                    } else if !available {
                        format!("{} · Unavailable", app.name)
                    } else {
                        app.name.clone()
                    };
                    ui.push_id(&app.identity, |ui| {
                        if ui
                            .add_enabled(
                                available && !configured,
                                Button::new(caption)
                                    .selected(self.path == app.path)
                                    .truncate(),
                            )
                            .on_hover_text(&app.path)
                            .clicked()
                        {
                            self.path = app.path.clone();
                            self.error = None;
                        }
                        ui.add(egui::Label::new(muted(&app.path).size(11.)).truncate())
                            .on_hover_text(&app.path);
                    });
                }
                if !found {
                    ui.label(muted(if self.apps.is_empty() {
                        "No applications found. Enter the path below."
                    } else {
                        "No matching applications."
                    }));
                }
            });
    }
}

#[cfg(target_os = "macos")]
fn installed_mac_apps() -> Vec<InstalledApp> {
    use openoptions::controls::application_id;
    use std::{collections::BTreeSet, path::PathBuf};

    fn is_mac_app(path: &std::path::Path) -> bool {
        path.is_dir()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
    }

    fn installed_app(path: PathBuf, seen: &mut BTreeSet<String>) -> Option<InstalledApp> {
        let identity = application_id(&path)?;
        seen.insert(identity.clone()).then(|| InstalledApp {
            name: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            path: path.to_string_lossy().into_owned(),
            identity,
        })
    }

    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
    ];
    if let Some(user) = directories::UserDirs::new() {
        roots.push(user.home_dir().join("Applications"));
    }

    let mut seen = BTreeSet::new();
    let mut apps = Vec::new();
    for root in roots {
        for entry in std::fs::read_dir(&root).into_iter().flatten().flatten() {
            let path = entry.path();
            if is_mac_app(&path) {
                if let Some(app) = installed_app(path, &mut seen) {
                    apps.push(app);
                }
            } else if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                for child in std::fs::read_dir(path).into_iter().flatten().flatten() {
                    let path = child.path();
                    if is_mac_app(&path)
                        && let Some(app) = installed_app(path, &mut seen)
                    {
                        apps.push(app);
                    }
                }
            }
        }
    }
    apps.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then(left.path.cmp(&right.path))
    });
    apps
}
