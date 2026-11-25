use super::{
    action_labels::{action_label, action_name},
    theme::*,
};
use eframe::egui::{self, Button, Id, Key, Modifiers, ScrollArea, TextEdit, Ui};
use openoptions::{
    controls::{self, Hosts},
    migration::{Action, NamedAction},
};
use std::path::Path;

#[derive(Default)]
pub(super) struct ActionPicker {
    query: String,
    mode: Mode,
    value: String,
    recording: bool,
    focus: bool,
    error: Option<&'static str>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Catalog,
    Shortcut,
    Application,
}

pub(super) struct ActionMenu<'a> {
    pub id: Id,
    pub heading: &'a str,
    pub default: &'a str,
    pub show_ring: bool,
    pub hosts: Option<&'a Hosts>,
}

pub(super) fn action_picker(
    ui: &mut Ui,
    picker: &mut Option<ActionPicker>,
    menu: ActionMenu<'_>,
    current: Option<&Action>,
) -> Option<Option<Action>> {
    let caption = current.map_or_else(|| menu.default.into(), action_name);
    ui.label(title(menu.heading, 13.));
    if ui
        .add(Button::new(format!("{caption}  ›")).truncate())
        .on_hover_text("Choose an assignment")
        .clicked()
    {
        *picker = Some(ActionPicker {
            focus: true,
            ..ActionPicker::default()
        });
    }
    if current.is_some_and(|action| !controls::supports(action)) {
        ui.colored_label(WARNING, "This assignment is unavailable on this system.");
    }

    let state = picker.as_mut()?;
    let modal = egui::Modal::new(menu.id)
        .frame(surface(18))
        .show(ui.ctx(), |ui| {
            ui.set_width(384.);
            ui.label(title("Choose an action", 19.));
            ui.label(muted(menu.heading));
            let choice = match state.mode {
                Mode::Catalog => state.catalog(ui, &menu, current),
                Mode::Shortcut | Mode::Application => state.form(ui),
            };
            ui.horizontal(|ui| {
                if state.mode != Mode::Catalog && ui.button("Back to actions").clicked() {
                    state.focus = true;
                    state.mode = Mode::Catalog;
                    state.error = None;
                    state.recording = false;
                }
                if ui.button("Cancel").clicked() {
                    ui.close();
                }
            });
            choice
        });
    if modal.inner.is_some() || modal.should_close() {
        *picker = None;
    }
    modal.inner
}

impl ActionPicker {
    fn catalog(
        &mut self,
        ui: &mut Ui,
        menu: &ActionMenu<'_>,
        current: Option<&Action>,
    ) -> Option<Option<Action>> {
        let search = ui.add(
            TextEdit::singleline(&mut self.query)
                .hint_text("Search actions")
                .desired_width(ui.available_width()),
        );
        if std::mem::take(&mut self.focus) {
            search.request_focus();
        }
        let query = self.query.trim().to_lowercase();
        let mut result = None;
        let mut rendered = false;
        let matches = |label: &str| label.to_lowercase().contains(&query);
        ScrollArea::vertical()
            .id_salt(menu.id.with("actions"))
            .max_height((ui.ctx().content_rect().height() - 220.).clamp(140., 290.))
            .show(ui, |ui| {
                let mut option = |label: &str, action: Option<Action>| {
                    if matches(label) {
                        rendered = true;
                        if ui
                            .selectable_label(current == action.as_ref(), label)
                            .clicked()
                        {
                            result = Some(action);
                        }
                    }
                };
                option(menu.default, None);
                for name in NamedAction::ALL {
                    let action = Action::Named(name);
                    if controls::supports(&action) {
                        let label = action_label(&action).to_owned();
                        option(&label, Some(action));
                    }
                }
                if menu.show_ring {
                    option("Show Actions Ring", Some(Action::ShowActionsRing));
                }
                if let Some(hosts) = menu.hosts.filter(|hosts| hosts.can_switch) {
                    for host in hosts.slots.iter().filter(|host| host.paired) {
                        let action = Action::SwitchHost(host.index);
                        option(&action_name(&action), Some(action));
                    }
                }
                for (label, mode) in [
                    ("Keyboard shortcut…", Mode::Shortcut),
                    ("Open application…", Mode::Application),
                ] {
                    if matches(label) {
                        rendered = true;
                        if ui.button(label).clicked() {
                            self.mode = mode;
                            self.value = match current {
                                Some(Action::Shortcut(value)) if mode == Mode::Shortcut => {
                                    value.clone()
                                }
                                Some(Action::OpenApplication(value))
                                    if mode == Mode::Application =>
                                {
                                    value.clone()
                                }
                                _ => String::new(),
                            };
                            self.focus = true;
                        }
                    }
                }
                if !rendered {
                    ui.label(muted("No matching actions."));
                }
            });
        result
    }

    fn form(&mut self, ui: &mut Ui) -> Option<Option<Action>> {
        let application = self.mode == Mode::Application;
        let hint = if application {
            "Absolute application path"
        } else if cfg!(target_os = "macos") {
            "Example: Cmd+Shift+K"
        } else {
            "Example: Ctrl+Shift+K"
        };

        if self.recording {
            ui.label(muted("Press a shortcut. Escape cancels recording."));
            let pressed = ui.input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        repeat: false,
                        modifiers,
                        ..
                    } => Some((*key, *modifiers)),
                    _ => None,
                })
            });
            if let Some((key, modifiers)) = pressed {
                self.recording = false;
                if key == Key::Escape {
                    ui.input_mut(|input| input.consume_key(modifiers, key));
                } else if let Some(value) = recorded_shortcut(key, modifiers) {
                    self.value = value;
                    self.error = None;
                } else {
                    self.error = Some("That key is unavailable for saved shortcuts.");
                }
            }
        } else {
            let field = ui.add(
                TextEdit::singleline(&mut self.value)
                    .hint_text(hint)
                    .desired_width(ui.available_width()),
            );
            if std::mem::take(&mut self.focus) {
                field.request_focus();
            }
            if field.changed() {
                self.error = None;
            }
            if !application && ui.button("Record shortcut").clicked() {
                field.surrender_focus();
                self.recording = true;
                self.error = None;
            }
        }
        if let Some(error) = self.error {
            ui.colored_label(WARNING, error);
        }
        if ui
            .add_enabled(
                !self.recording && !self.value.trim().is_empty(),
                primary("Use assignment"),
            )
            .clicked()
        {
            let value = self.value.trim();
            let candidate = if application {
                Action::application(value)
            } else {
                Some(Action::Shortcut(value.into()))
            };
            if let Some(action) = candidate.filter(controls::supports) {
                if application && !Path::new(value).exists() {
                    self.error =
                        Some("Choose a path to an application installed on this computer.");
                } else {
                    return Some(Some(action));
                }
            } else {
                self.error = Some(if application {
                    "Enter an absolute application path supported on this system."
                } else {
                    "Enter a shortcut with a supported key and optional modifiers."
                });
            }
        }
        None
    }
}

fn recorded_shortcut(key: Key, modifiers: Modifiers) -> Option<String> {
    let mut parts = Vec::with_capacity(5);
    if modifiers.ctrl {
        parts.push("Ctrl");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    if modifiers.shift {
        parts.push("Shift");
    }
    if modifiers.mac_cmd {
        parts.push("Cmd");
    } else if modifiers.command && !modifiers.ctrl {
        parts.push("Super");
    }
    parts.push(key.name());
    let value = parts.join("+");
    controls::supports(&Action::Shortcut(value.clone())).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorded_chords_use_runtime_shortcut_syntax() {
        let modifiers = Modifiers {
            ctrl: true,
            shift: true,
            command: true,
            ..Modifiers::NONE
        };
        assert_eq!(
            recorded_shortcut(Key::K, modifiers).as_deref(),
            Some("Ctrl+Shift+K")
        );
        assert_eq!(recorded_shortcut(Key::Slash, Modifiers::NONE), None);
    }
}
