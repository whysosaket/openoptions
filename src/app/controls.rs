use super::{
    Event, OpenOptions, Saved,
    action_labels::action_name,
    action_picker::{ActionMenu, ActionPicker, action_picker},
    applications::{AppPicker, app_picker},
    feature_panels,
    theme::*,
    workspace,
};
use eframe::egui::{
    self, Align, Button, ComboBox, Frame, Id, Layout, RichText, ScrollArea, Ui, vec2,
};
use openoptions::controls::{
    self, Bindings, Control, Hosts, Pointer, PointerChange, Profiles, Session, Status, Wheel,
};
use openoptions::devices::Device;
use openoptions::migration::{
    Action, Binding, Direction, NamedAction, Profile, SourceKind, WheelMode,
};
use openoptions::store::Store;
use std::{collections::BTreeMap, time::Duration};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Buttons,
    Keys,
    Ring,
    Pointer,
    Device,
}

impl Section {
    fn accepts(self, control: Control) -> bool {
        match self {
            Self::Buttons => control.mouse,
            Self::Keys => !control.mouse,
            Self::Ring | Self::Pointer | Self::Device => false,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Buttons => "Buttons",
            Self::Keys => "Keys",
            Self::Ring => "Actions Ring",
            Self::Pointer => "Point & Scroll",
            Self::Device => "Device",
        }
    }
}

pub(super) struct Editor {
    pub loading: bool,
    pub reload_requested: bool,
    pub available: Option<Vec<Control>>,
    pub controls_error: Option<String>,
    pub saved: Result<Profiles, String>,
    pub draft: Profiles,
    application: Option<String>,
    app_picker: Option<AppPicker>,
    pub hosts: Option<Result<Option<Hosts>, String>>,
    pub host_busy: bool,
    host_rename: Option<(u8, String)>,
    host_switch: Option<u8>,
    ring_slot: usize,
    ring_preview: bool,
    pub saving: bool,
    pub pointer: Option<Pointer>,
    pub pointer_error: Option<String>,
    pub pointer_busy: bool,
    dpi_choice: Option<u16>,
    wheel_choice: Option<Wheel>,
    pub notice: Option<(String, bool)>,
    section: Section,
    selected: Option<u16>,
    direction: Direction,
    entry: Option<ActionPicker>,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            loading: false,
            reload_requested: false,
            available: None,
            controls_error: None,
            saved: Ok(Profiles::default()),
            draft: Profiles::default(),
            application: None,
            app_picker: None,
            hosts: None,
            host_busy: false,
            host_rename: None,
            host_switch: None,
            ring_slot: 0,
            ring_preview: false,
            saving: false,
            pointer: None,
            pointer_error: None,
            pointer_busy: false,
            dpi_choice: None,
            wheel_choice: None,
            notice: None,
            section: Section::Buttons,
            selected: None,
            direction: Direction::Click,
            entry: None,
        }
    }
}

impl Editor {
    fn busy(&self) -> bool {
        self.loading || self.saving || self.pointer_busy || self.host_busy
    }

    pub fn loaded(
        &mut self,
        available: Option<Result<Vec<Control>, String>>,
        saved: Option<Result<Profiles, String>>,
    ) {
        self.loading = false;
        if let Some(Err(error)) = &available {
            self.controls_error = Some(error.clone());
        }
        if let Some(Ok(available)) = available {
            self.controls_error = None;
            if self.section == Section::Buttons
                && self.selected.is_none()
                && !available.is_empty()
                && available.iter().all(|control| !control.mouse)
            {
                self.section = Section::Keys;
            }
            self.available = Some(available);
        }
        if let Some(saved) = saved {
            if let Ok(bindings) = &saved {
                self.draft = bindings.clone();
            }
            self.saved = saved;
        }
    }

    pub fn pointer_loaded(&mut self, result: Result<Pointer, String>, applied: Option<Tuning>) {
        let mut pointer = match result {
            Ok(pointer) => pointer,
            Err(error) => {
                self.pointer_error = Some(error);
                return;
            }
        };
        self.pointer_error = None;
        let mut previous = self.pointer.take().unwrap_or_default();
        if pointer.dpi_error.is_some() {
            pointer.dpi = previous.dpi.take();
        }
        if pointer.wheel_error.is_some() {
            pointer.wheel = previous.wheel;
        }
        let previous = &previous;
        if pointer.dpi_error.is_none() {
            self.dpi_choice = pointer.dpi.as_ref().map(|dpi| {
                if applied != Some(Tuning::Dpi)
                    && let Some(choice) = self.dpi_choice
                    && previous
                        .dpi
                        .as_ref()
                        .is_some_and(|old| old.current != choice)
                    && dpi.supported.contains(&choice)
                {
                    choice
                } else {
                    dpi.current
                }
            });
        }
        if pointer.wheel_error.is_none() {
            self.wheel_choice = pointer.wheel.map(|wheel| {
                if applied != Some(Tuning::Wheel)
                    && let Some(choice) = self.wheel_choice
                    && previous.wheel.is_some_and(|old| {
                        old.mode != choice.mode || old.threshold != choice.threshold
                    })
                {
                    Wheel {
                        mode: choice.mode,
                        threshold: choice.threshold,
                        ..wheel
                    }
                } else {
                    wheel
                }
            });
        }
        self.pointer = Some(pointer);
    }

    fn bindings(&self) -> &Bindings {
        &self.draft.layer(self.application.as_deref()).bindings
    }

    fn bindings_mut(&mut self) -> &mut Bindings {
        &mut self.draft.layer_mut(self.application.as_deref()).bindings
    }

    fn select_control(&mut self, id: u16) {
        if self.selected != Some(id) {
            self.selected = Some(id);
            self.direction = Direction::Click;
            self.entry = None;
        }
    }

    fn changed(&self) -> bool {
        self.saved.as_ref().is_ok_and(|saved| *saved != self.draft)
    }
}

pub(super) struct LiveControls {
    pub key: openoptions::devices::DeviceKey,
    pub name: String,
    pub session: Session,
    pub active: bool,
    pub stopping: bool,
    active_application: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Tuning {
    Dpi,
    Wheel,
}

enum Command {
    Reload,
    Save,
    Enable,
    Pointer(PointerChange),
    ReloadHosts,
    RenameHost(u8, String),
    SwitchHost(u8),
}

impl OpenOptions {
    pub(super) fn open_device(&mut self, device: Device, context: &egui::Context) {
        self.selected_device = Some(device.key());
        if !self.editors.contains_key(&device.key()) {
            self.load_controls(device.clone(), context);
        }
        self.detail = Some(super::Detail::Device(device));
    }

    pub(super) fn open_device_details(&mut self, device: Device, context: &egui::Context) {
        let key = device.key();
        self.open_device(device, context);
        if let Some(editor) = self.editors.get_mut(&key) {
            editor.section = Section::Device;
        }
    }

    fn load_controls(&mut self, device: Device, context: &egui::Context) {
        let key = device.key();
        let editor = self.editors.entry(key.clone()).or_default();
        editor.loading = true;
        editor.reload_requested = false;
        let read_saved = editor.available.is_none() || editor.saved.is_err();
        self.job(context, move || {
            let saved = read_saved.then(|| {
                Store::location()
                    .and_then(|path| Store::open(&path))
                    .and_then(|store| store.customizations(&device))
            });
            let hardware = device.connected.then(|| super::DeviceSettings {
                available: controls::discover(&device),
                pointer: controls::pointer(&device),
                hosts: controls::host_channels(&device),
            });
            Event::ControlsLoaded(key, hardware, saved)
        });
    }

    pub(super) fn poll_controls(&mut self, context: &egui::Context) {
        let Some(live) = &mut self.live_controls else {
            return;
        };
        while let Some(status) = live.session.poll() {
            match status {
                Status::Active => live.active = true,
                Status::ProfileChanged(application) => live.active_application = application,
                Status::ActionFailed(error) => {
                    if let Some(editor) = self.editors.get_mut(&live.key) {
                        editor.notice = Some((error, true));
                    }
                }

                Status::Ring { actions, pointer } => {
                    if !live.stopping {
                        self.actions_ring =
                            Some(feature_panels::RingWindow::new(context, *actions, pointer));
                    }
                }
                Status::Finished(result) => {
                    if let Some(editor) = self.editors.get_mut(&live.key) {
                        editor.notice = Some(match result {
                            Ok(()) => ("Live controls disabled.".into(), false),
                            Err(error) => (error, true),
                        });
                    }
                    self.live_controls = None;
                    self.actions_ring = None;
                    return;
                }
            }
        }
        context.request_repaint_after(Duration::from_millis(85));
    }

    pub(super) fn stop_controls(&mut self) {
        if let Some(live) = &mut self.live_controls {
            live.stopping = true;
            self.actions_ring = None;
            live.session.stop();
        }
    }

    pub(super) fn device_settings(&mut self, ui: &mut Ui, device: &Device) {
        let key = device.key();
        let online = self.devices.is_ok() && device.connected;
        if online
            && self
                .editors
                .get(&key)
                .is_some_and(|editor| editor.reload_requested && !editor.busy())
            && self
                .live_controls
                .as_ref()
                .is_none_or(|live| live.key != key)
        {
            self.load_controls(device.clone(), ui.ctx());
        }
        let Some(editor) = self.editors.get_mut(&key) else {
            return;
        };
        let command = settings_workspace(
            ui,
            editor,
            device,
            online,
            self.devices.is_err(),
            &self.profiles,
            self.live_controls.as_ref(),
        );
        match command {
            Some(Command::Reload) => self.load_controls(device.clone(), ui.ctx()),
            Some(Command::Save) => {
                editor.saving = true;
                editor.notice = None;
                let bindings = editor.draft.clone();
                let device = device.clone();
                self.job(ui.ctx(), move || {
                    let result = Store::location()
                        .and_then(|path| Store::open(&path))
                        .and_then(|store| store.save_customizations(&device, &bindings))
                        .map(|()| bindings);
                    Event::ControlsSaved(key, result)
                });
            }
            Some(Command::Enable) => {
                editor.notice = None;
                self.live_controls = Some(LiveControls {
                    key,
                    name: device.name.clone(),
                    session: Session::start(
                        device.clone(),
                        editor
                            .saved
                            .as_ref()
                            .expect("enable requires saved profiles")
                            .clone(),
                    ),
                    active: false,
                    stopping: false,
                    active_application: None,
                });
                ui.ctx().request_repaint_after(Duration::from_millis(85));
            }
            Some(Command::Pointer(change)) => {
                editor.pointer_busy = true;
                editor.notice = None;
                let device = device.clone();
                let tuning = match change {
                    PointerChange::Dpi(_) => Tuning::Dpi,
                    PointerChange::Wheel(_) => Tuning::Wheel,
                };
                self.job(ui.ctx(), move || {
                    Event::PointerApplied(key, tuning, controls::apply_pointer(&device, change))
                });
            }
            Some(Command::ReloadHosts) => {
                editor.host_busy = true;
                let device = device.clone();
                self.job(ui.ctx(), move || {
                    Event::HostsRead(key, controls::host_channels(&device))
                });
            }
            Some(Command::RenameHost(index, name)) => {
                editor.host_busy = true;
                let device = device.clone();
                self.job(ui.ctx(), move || {
                    Event::HostsRead(key, controls::rename_host(&device, index, &name))
                });
            }
            Some(Command::SwitchHost(index)) => {
                editor.host_busy = true;
                let device = device.clone();
                self.job(ui.ctx(), move || {
                    Event::HostSwitched(key, controls::switch_host(&device, index))
                });
            }
            None => {}
        }
    }
}

fn settings_workspace(
    ui: &mut Ui,
    editor: &mut Editor,
    device: &Device,
    online: bool,
    scan_failed: bool,
    profiles: &Saved,
    live: Option<&LiveControls>,
) -> Option<Command> {
    let mut command = None;
    ui.visuals_mut().selection.stroke.color = TEXT;
    let owns_session = live.is_some_and(|session| session.key == device.key());
    let idle = !owns_session && !editor.busy();
    let stable = device.verifies_identity();
    let editable = idle && stable && editor.saved.is_ok();
    let available = editor.available.clone().unwrap_or_default();
    let pointer_supported = editor.pointer.as_ref().is_some_and(|pointer| {
        pointer.dpi.is_some()
            || pointer.wheel.is_some()
            || pointer.dpi_error.is_some()
            || pointer.wheel_error.is_some()
    });
    let compact = ui.available_width() < 690.;
    if compact {
        ui.horizontal_wrapped(|ui| {
            section_navigation(ui, editor, &available, pointer_supported, false)
        });
        workspace::divider(ui);
    }
    ui.horizontal_top(|ui| {
        if !compact {
            ui.allocate_ui_with_layout(
                vec2(144., ui.available_height()),
                Layout::top_down(Align::Min),
                |ui| section_navigation(ui, editor, &available, pointer_supported, true),
            );
            ui.add_space(8.);
            ui.separator();
            ui.add_space(8.);
        }
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            if editor.section != Section::Device {
                ui.horizontal_wrapped(|ui| {
                    workspace::badge(
                        ui,
                        if online {
                            "Connected"
                        } else if scan_failed {
                            "Status unavailable"
                        } else {
                            "Disconnected"
                        },
                        online,
                    );
                    ui.label(muted(device.connection));
                    if let Ok(Some(battery)) = device.battery {
                        ui.label(muted(super::gallery::battery_caption(battery)));
                    }
                });
            }
            if device.supports_hidpp() && !stable {
                ui.label(muted(
                    "The device has no verified identity. Settings are read-only.",
                ));
            }
            if let Some((notice, error)) = &editor.notice {
                ui.label(RichText::new(notice).size(12.).color(if *error {
                    WARNING
                } else {
                    SUCCESS
                }));
            }
            if editor.section == Section::Device {
                ScrollArea::vertical()
                    .id_salt("device-information")
                    .show(ui, |ui| {
                        ui.label(title("Connection", 20.));
                        workspace::badge(
                            ui,
                            if online {
                                "Connected"
                            } else if scan_failed {
                                "Status unavailable"
                            } else {
                                "Disconnected"
                            },
                            online,
                        );
                        ui.label(muted(format!(
                            "{}  ·  046D:{:04X}",
                            device.connection, device.product_id
                        )));
                        ui.label(muted(format!("{} HID interfaces", device.interfaces)));
                        match &device.battery {
                            Ok(Some(battery)) => {
                                ui.label(muted(super::gallery::battery_caption(*battery)));
                            }
                            Err(error) if online => {
                                ui.label(muted(format!("Couldn't read battery. {error}")));
                            }
                            _ => {}
                        }
                        if editor.loading {
                            ui.spinner();
                        }
                        if let Some(error) = &editor.pointer_error
                            && ui
                                .collapsing("Point & Scroll read error", |ui| {
                                    ui.label(muted(error));
                                    labeled_button(ui, "Try again", Icon::Refresh, online && idle)
                                        .clicked()
                                })
                                .body_returned
                                == Some(true)
                        {
                            command = Some(Command::Reload);
                            return;
                        }
                        let may_read = online && idle && live.is_none();
                        command = host_workspace(ui, editor, may_read && stable, may_read);
                    });
                return;
            }
            if editor.section == Section::Pointer {
                command = pointer_workspace(ui, editor, online && stable && idle, online && idle);
                return;
            }
            if let Some(error) = &editor.controls_error {
                if available.is_empty() {
                    ui.label(title("Controls unavailable", 20.));
                }
                ui.label(muted(error));
                if ui
                    .add_enabled(online && idle, Button::new("Try again"))
                    .clicked()
                {
                    command = Some(Command::Reload);
                }
                if available.is_empty() {
                    return;
                }
            }
            match &editor.available {
                None if !online && !editor.loading => return,
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(muted("Reading device controls…"));
                    });
                    return;
                }
                Some(_) if available.is_empty() => {
                    ui.label(title("No configurable controls", 20.));
                    ui.label(muted(
                        "The device did not report any controls available for remapping.",
                    ));
                    return;
                }
                Some(_) => {}
            };
            if let Err(error) = &editor.saved {
                ui.label(
                    RichText::new(format!("Could not read assignments. {error}")).color(WARNING),
                );
                if ui.add_enabled(idle, Button::new("Try again")).clicked() {
                    command = Some(Command::Reload);
                }
                return;
            }
            let preview = ui
                .horizontal_wrapped(|ui| {
                    ui.label(title(editor.section.label(), 23.));
                    if ui
                        .add_enabled(editable && editor.changed(), primary("Save"))
                        .clicked()
                    {
                        command = Some(Command::Save);
                    }
                    if !owns_session
                    && ui
                        .add_enabled(
                            editable
                                && online
                                && live.is_none()
                                && controls::live_platform_available()
                                && editor.controls_error.is_none()
                                && editor.draft.has_bindings()
                                && !editor.changed(),
                            Button::new("Enable live controls"),
                        )
                        .on_hover_text(
                            "Use the saved assignments until you disable them or quit OpenOptions.",
                        )
                        .clicked()
                {
                    command = Some(Command::Enable);
                }
                    if editor.saving || editor.loading {
                        ui.spinner();
                    }
                    if editor.section == Section::Ring {
                        let preview = ui.button("Preview ring");
                        paint_focus(ui, &preview);
                        if preview.clicked() {
                            editor.ring_preview = true;
                        }
                        if editor.ring_preview {
                            return Some(preview);
                        }
                    }
                    None
                })
                .inner;
            if owns_session && let Some(live) = live {
                let text = if live.stopping {
                    "Disabling…"
                } else if live.active {
                    "Live"
                } else {
                    "Enabling…"
                };
                ui.horizontal_wrapped(|ui| {
                    workspace::badge(ui, text, live.active && !live.stopping);
                    if live.active && !live.stopping {
                        let name = live
                            .active_application
                            .as_ref()
                            .and_then(|key| editor.draft.applications.get(key))
                            .map_or("Default", |profile| profile.name.as_str());
                        clipped(ui, muted(format!("Active profile · {name}")), name);
                    }
                });
            }
            if !controls::live_platform_available() {
                ui.label(muted("Live controls require an X11 session on Linux."));
            }
            profile_tabs(ui, editor, editable);
            if editor.section == Section::Ring {
                ring_workspace(ui, editor, editable, preview);
                return;
            }
            if editor.section == Section::Buttons && editor.application.is_none() {
                ui.add_enabled_ui(editable, |ui| {
                    profile_picker(ui, editor, device, profiles, &available)
                });
            }
            let controls: Vec<_> = available
                .iter()
                .copied()
                .filter(|control| editor.section.accepts(*control))
                .collect();
            if editor
                .selected
                .is_some_and(|selected| !controls.iter().any(|control| control.id == selected))
            {
                editor.selected = None;
                editor.entry = None;
            }
            let control_area = ui.available_height().max(60.);
            let section = editor.section.label();
            let key = device.key();
            if ui.available_width() >= 500. {
                let diagram =
                    control_area >= 315. && device.name.to_lowercase().contains("mx master 3");
                let selected = editor.selected;
                ui.columns(2, |columns| {
                    ScrollArea::vertical()
                        .id_salt(("control-list", &key, section))
                        .max_height(control_area)
                        .auto_shrink([false, false])
                        .show(&mut columns[0], |ui| {
                            control_list(ui, editor, &controls, diagram)
                        });
                    let mut inspector_scroll = ScrollArea::vertical()
                        .id_salt(("control-inspector", &key, section))
                        .max_height(control_area)
                        .auto_shrink([false, false]);
                    if editor.selected != selected {
                        inspector_scroll = inspector_scroll.vertical_scroll_offset(0.);
                    }
                    inspector_scroll.show(&mut columns[1], |ui| {
                        ui.add_enabled_ui(editable, |ui| inspector(ui, editor, &controls));
                    });
                });
            } else {
                ScrollArea::vertical()
                    .id_salt(("control-settings", &key, section))
                    .max_height(control_area)
                    .show(ui, |ui| {
                        control_list(ui, editor, &controls, false);
                        ui.add_enabled_ui(editable, |ui| inspector(ui, editor, &controls));
                    });
            }
        });
    });
    command
}

fn section_navigation(
    ui: &mut Ui,
    editor: &mut Editor,
    available: &[Control],
    pointer_supported: bool,
    sidebar: bool,
) {
    ui.spacing_mut().item_spacing.y = 5.;
    for section in [
        Section::Buttons,
        Section::Keys,
        Section::Ring,
        Section::Pointer,
        Section::Device,
    ] {
        let show = match section {
            Section::Buttons => {
                available.is_empty() || available.iter().any(|control| control.mouse)
            }
            Section::Keys => available.iter().any(|control| !control.mouse),
            Section::Ring => !available.is_empty(),
            Section::Pointer => pointer_supported || editor.section == Section::Pointer,
            Section::Device => true,
        };
        if !show {
            continue;
        }
        let button = Button::selectable(editor.section == section, section.label());
        let response = if sidebar {
            ui.add_sized([ui.available_width(), 33.], button)
        } else {
            ui.add(button)
        };
        paint_focus(ui, &response);
        if response.clicked() && editor.section != section {
            editor.section = section;
            editor.selected = None;
            editor.entry = None;
        }
    }
}

fn pointer_workspace(
    ui: &mut Ui,
    editor: &mut Editor,
    editable: bool,
    retry: bool,
) -> Option<Command> {
    let mut command = None;
    let reload = ui.horizontal_wrapped(|ui| {
        ui.label(title("Point & Scroll", 23.));
        if editor.pointer_busy || editor.loading {
            ui.spinner();
        }
        labeled_button(ui, "Read current settings", Icon::Refresh, retry).clicked()
    });
    if let Some(error) = &editor.pointer_error {
        ui.label(RichText::new(format!("Couldn't read device settings. {error}")).color(WARNING));
    }
    if reload.inner {
        return Some(Command::Reload);
    }
    let pointer = editor.pointer.as_ref()?;
    let editable = editable && editor.pointer_error.is_none();
    ScrollArea::vertical()
        .id_salt("pointer-settings")
        .show(ui, |ui| {
            if let Some(error) = &pointer.dpi_error {
                ui.label(
                    RichText::new(format!("Couldn't read pointer speed. {error}")).color(WARNING),
                );
            }
            if let Some(error) = &pointer.wheel_error {
                ui.label(
                    RichText::new(format!("Couldn't read SmartShift. {error}")).color(WARNING),
                );
            }
            if let Some(dpi) = &pointer.dpi {
                let editable = editable && pointer.dpi_error.is_none();
                card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let heading = ui.label(title("Pointer speed", 16.));
                    ui.label(muted(format!("Last device reading · {} DPI", dpi.current)));
                    if let Some((first, last)) = dpi.supported.first().zip(dpi.supported.last()) {
                        let mut range = format!("{first}–{last} DPI");
                        if let Some(step) = dpi.supported.get(1).map(|next| next - first)
                            && dpi
                                .supported
                                .windows(2)
                                .all(|values| values[1] - values[0] == step)
                        {
                            range.push_str(&format!(" · {step} DPI step"));
                        }
                        ui.label(muted(range).size(12.));
                    }
                    let Some(choice) = &mut editor.dpi_choice else {
                        return;
                    };
                    let index = dpi
                        .supported
                        .binary_search(choice)
                        .unwrap_or_else(|index| index.min(dpi.supported.len() - 1));
                    let mut index = index;
                    let response = tuning_slider(
                        ui,
                        egui::Slider::new(&mut index, 0..=(dpi.supported.len() - 1))
                            .show_value(false),
                        editable && !editor.pointer_busy,
                    )
                    .labelled_by(heading.id);
                    if response.changed() {
                        *choice = dpi.supported[index];
                    }
                    if *choice != dpi.current {
                        ui.label(title(format!("Pending · {choice} DPI"), 13.));
                    }
                    if ui
                        .add_enabled(
                            editable && !editor.pointer_busy && *choice != dpi.current,
                            Button::new("Apply pointer speed"),
                        )
                        .clicked()
                    {
                        command = Some(Command::Pointer(PointerChange::Dpi(*choice)));
                    }
                });
                ui.add_space(12.);
            }
            if let (Some(current), Some(choice)) = (pointer.wheel, &mut editor.wheel_choice) {
                let editable = editable && pointer.wheel_error.is_none();
                card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(title("SmartShift", 16.));
                    let current_mode = match current.mode {
                        WheelMode::Ratchet => "Ratchet",
                        WheelMode::FreeSpin => "Free spin",
                    };
                    let current_setting =
                        if current.mode == WheelMode::Ratchet && current.threshold < 255 {
                            format!("{current_mode} · threshold {}", current.threshold)
                        } else {
                            current_mode.into()
                        };
                    ui.label(muted(format!("Last device reading · {current_setting}")));
                    ui.add_enabled_ui(editable && !editor.pointer_busy, |ui| {
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut choice.mode, WheelMode::Ratchet, "Ratchet");
                            ui.selectable_value(&mut choice.mode, WheelMode::FreeSpin, "Free spin");
                        });
                        if choice.mode == WheelMode::Ratchet {
                            let mut automatic = choice.threshold < 255;
                            if ui
                                .checkbox(&mut automatic, "Switch to free spin automatically")
                                .changed()
                            {
                                choice.threshold = if automatic {
                                    choice.default_threshold.clamp(1, 254)
                                } else {
                                    255
                                };
                            }
                            if automatic {
                                let heading = ui.label(title("Switch threshold", 13.));
                                tuning_slider(
                                    ui,
                                    egui::Slider::new(&mut choice.threshold, 1..=254),
                                    true,
                                )
                                .labelled_by(heading.id);
                            }
                        }
                        let pending =
                            current.mode != choice.mode || current.threshold != choice.threshold;
                        if pending {
                            ui.label(title("Wheel changes pending", 13.));
                        }
                        if ui
                            .add_enabled(pending, Button::new("Apply wheel settings"))
                            .clicked()
                        {
                            command = Some(Command::Pointer(PointerChange::Wheel(*choice)));
                        }
                    });
                });
            }
        });
    command
}

fn tuning_slider(ui: &mut Ui, slider: egui::Slider<'_>, enabled: bool) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 60.).clamp(80., 460.);
        ui.spacing_mut().slider_rail_height = 5.;
        ui.visuals_mut().widgets.inactive.bg_fill = BORDER;
        ui.visuals_mut().selection.bg_fill = SOLID;
        let response = ui.add_enabled(
            enabled,
            slider
                .trailing_fill(true)
                .handle_shape(egui::style::HandleShape::Circle),
        );
        paint_focus(ui, &response);
        response
    })
    .inner
}

fn control_list(ui: &mut Ui, editor: &mut Editor, controls: &[Control], diagram: bool) {
    let label = if editor.section == Section::Buttons {
        "Mouse controls"
    } else {
        "Programmable keys"
    };
    card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(title(label, 14.));
        ui.spacing_mut().item_spacing.y = 4.;
        if diagram && editor.section == Section::Buttons && ui.available_width() >= 180. {
            mouse_map(ui, editor, controls);
        }
        for &control in controls {
            let name = control.name();
            let local = editor.bindings().get(&control.id);
            let assignment = if local.is_none() && editor.application.is_some() {
                format!(
                    "Inherited · {}",
                    binding_summary(editor.draft.default.bindings.get(&control.id))
                )
            } else {
                binding_summary(local)
            };
            let selected = editor.selected == Some(control.id);
            let row = Button::new((
                RichText::new(&name).color(TEXT),
                egui::Atom::grow(),
                muted(&assignment).size(12.),
            ))
            .selected(selected)
            .frame_when_inactive(selected)
            .truncate();
            let row = ui
                .add_sized([ui.available_width(), 33.], row)
                .on_hover_text(format!("{name}  ·  {assignment}"));
            paint_focus(ui, &row);
            if row.clicked() {
                editor.select_control(control.id);
            }
        }
    });
}

fn mouse_map(ui: &mut Ui, editor: &mut Editor, controls: &[Control]) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 182.), egui::Sense::hover());
    let center = rect.center();
    let at = |x: f32, y: f32| center + vec2(x, y);
    let painter = ui.painter_at(rect);
    let body = egui::Rect::from_center_size(at(9., 0.), vec2(108., 165.));
    painter.rect_filled(rect.shrink(1.), 6, SURFACE);
    let shell = egui::Color32::from_rgb(225, 226, 231);
    painter.rect_filled(body, 49, shell);
    painter.rect_filled(
        egui::Rect::from_center_size(at(-29., 35.), vec2(76., 79.)),
        34,
        shell,
    );
    painter.rect_stroke(
        body,
        49,
        egui::Stroke::new(1.0_f32, BORDER),
        egui::StrokeKind::Inside,
    );
    painter.line_segment([at(9., -81.), at(9., -28.)], line());
    painter.rect_filled(
        egui::Rect::from_center_size(at(9., -48.), vec2(11., 27.)),
        5,
        BORDER,
    );
    painter.line_segment(
        [at(-39., -27.), at(-30., -15.)],
        egui::Stroke::new(2.0_f32, MUTED),
    );
    for (id, x, y) in [
        (0x52, 9., -49.),
        (0xC4, 9., -20.),
        (0x56, -39., -2.),
        (0x53, -39., 19.),
        (0xC3, -48., 47.),
        (0xD0, -48., 47.),
    ] {
        if id == 0xD0 && controls.iter().any(|control| control.id == 0xC3) {
            continue;
        }
        let Some(control) = controls.iter().find(|control| control.id == id).copied() else {
            continue;
        };
        let point = at(x, y);
        let selected = editor.selected == Some(id);
        let focus = ui.interact(
            egui::Rect::from_center_size(point, vec2(22., 21.)),
            ui.id().with(("mouse-hotspot", id)),
            egui::Sense::click(),
        );
        focus.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, control.name())
        });
        let focus = focus.on_hover_text(control.name());
        if focus.clicked() {
            editor.select_control(id);
        }
        let accent = if selected { SOLID } else { MUTED };
        let radius = if selected || focus.hovered() || focus.has_focus() {
            7.5
        } else {
            5.
        };
        painter.circle(point, radius, WINDOW, egui::Stroke::new(1.4_f32, accent));
        if focus.has_focus() {
            painter.circle_stroke(point, radius + 3., egui::Stroke::new(2_f32, FOCUS));
        }
        if selected {
            painter.circle_filled(point, 3.2, accent);
        }
    }
}

pub(super) fn card() -> Frame {
    Frame::new()
        .fill(ELEVATED)
        .stroke(line())
        .corner_radius(8)
        .inner_margin(12)
}

fn binding_summary(binding: Option<&Binding>) -> String {
    match binding {
        Some(Binding::Click(action)) => action_name(action),
        Some(Binding::Gesture(actions)) => format!("{} mapped gestures", actions.len()),
        None => "Device default".into(),
    }
}

fn factory_action(control: Control) -> &'static str {
    match (control.mouse, control.id) {
        (true, 0x52) => "Middle click",
        (true, 0x53) => "Back navigation",
        (true, 0x56) => "Forward navigation",
        (true, 0xC4) => "Switch scroll wheel mode",
        (true, 0xED) => "Switch pointer sensitivity",
        _ => "Device default",
    }
}

fn inspector(ui: &mut Ui, editor: &mut Editor, controls: &[Control]) {
    let Some(control) = controls
        .iter()
        .copied()
        .find(|control| Some(control.id) == editor.selected)
    else {
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(title("Control inspector", 16.));
            ui.label(muted("Select a control to see its current assignment."));
        });
        return;
    };

    let assigned = editor.bindings().get(&control.id).cloned();
    let per_app = editor.application.is_some();
    let inherited = per_app && assigned.is_none();
    let binding = assigned
        .as_ref()
        .or_else(|| {
            inherited
                .then(|| editor.draft.default.bindings.get(&control.id))
                .flatten()
        })
        .cloned();
    let gesture = matches!(binding, Some(Binding::Gesture(_)));
    card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(title(control.name(), 16.));
        ui.label(muted(format!("Factory action · {}", factory_action(control))).size(12.));
        ui.label(muted(if inherited {
            "Inherited from Default"
        } else if per_app {
            "Application override"
        } else {
            "Default profile"
        }).size(12.));

        if control.mouse {
            let mut changed = false;
            ui.horizontal(|ui| {
                if ui.selectable_label(!gesture, "Press").clicked() && gesture {
                    let press = match &binding {
                        Some(Binding::Gesture(actions)) => actions.get(&Direction::Click).cloned(),
                        _ => None,
                    };
                    if let Some(action) = press.or_else(|| inherited.then_some(Action::Named(NamedAction::NoAction))) {
                        editor.bindings_mut().insert(control.id, Binding::Click(action));
                    } else {
                        editor.bindings_mut().remove(&control.id);
                    }
                    changed = true;
                }
                if ui.selectable_label(gesture, "Gesture").clicked() && !gesture {
                    let press = match &binding {
                        Some(Binding::Click(action)) => action.clone(),
                        _ => Action::Named(NamedAction::NoAction),
                    };
                    editor.bindings_mut().insert(
                        control.id,
                        Binding::Gesture(BTreeMap::from([(Direction::Click, press)])),
                    );
                    editor.direction = Direction::Click;
                    changed = true;
                }
            });
            if changed {
                editor.entry = None;
                editor.notice = None;
                return;
            }
        }

        let action = match &binding {
            Some(Binding::Gesture(actions)) => {
                ui.label(muted("Hold the button, move in a direction, then release. Release without moving for Press."));
                if !control.raw_motion {
                    ui.label(muted("This device measures movement from the pointer, which can be limited at a screen edge.").size(12.));
                }
                gesture_slots(ui, editor, actions);
                actions.get(&editor.direction)
            }
            Some(Binding::Click(action)) => Some(action),
            None => None,
        };
        let default = if gesture {
            "Unassign direction"
        } else if per_app {
            "Inherit default"
        } else {
            "Device default"
        };
        let hosts = editor.hosts.as_ref().and_then(|value| value.as_ref().ok()).and_then(Option::as_ref);
        let heading = if gesture {
            format!("{} action", gesture_name(editor.direction))
        } else {
            "Action".into()
        };
        let menu = ActionMenu {
            id: ui.id().with(("action", control.id, editor.direction.name())),
            heading: &heading,
            default,
            show_ring: true,
            hosts,
        };
        if let Some(change) = action_picker(ui, &mut editor.entry, menu, action) {
            let next = match binding {
                Some(Binding::Gesture(mut actions)) => {
                    match change {
                        Some(action) => {
                            actions.insert(editor.direction, action);
                        }
                        None => {
                            actions.remove(&editor.direction);
                        }
                    }
                    (!actions.is_empty()).then_some(Binding::Gesture(actions))
                }
                _ => change.map(Binding::Click),
            };
            let bindings = editor.bindings_mut();
            if let Some(next) = next {
                bindings.insert(control.id, next);
            } else {
                bindings.remove(&control.id);
            }
            editor.notice = None;
        }
        if assigned.is_some()
            && ui.button(if per_app { "Clear override" } else { "Restore device default" }).clicked()
        {
            editor.bindings_mut().remove(&control.id);
            editor.entry = None;
            editor.notice = None;
        }
    });
}

fn gesture_name(direction: Direction) -> &'static str {
    if direction == Direction::Click {
        "Press"
    } else {
        direction.name()
    }
}

fn gesture_slots(ui: &mut Ui, editor: &mut Editor, actions: &BTreeMap<Direction, Action>) {
    let slots = [
        (Direction::Click, "•"),
        (Direction::Up, "↑"),
        (Direction::Down, "↓"),
        (Direction::Left, "←"),
        (Direction::Right, "→"),
    ];
    let columns = if ui.available_width() >= 248. { 2 } else { 1 };
    let width = (ui.available_width() - (columns - 1) as f32 * ui.spacing().item_spacing.x)
        / columns as f32;
    for row in slots.chunks(columns) {
        ui.horizontal(|ui| {
            for &(direction, icon) in row {
                let action = actions
                    .get(&direction)
                    .map_or_else(|| "Unassigned".into(), action_name);
                let label = format!("{icon} {} · {action}", gesture_name(direction));
                let button = ui
                    .add_sized(
                        [width, 28.],
                        Button::selectable(editor.direction == direction, &label).truncate(),
                    )
                    .on_hover_text(&label);
                paint_focus(ui, &button);
                if button.clicked() && editor.direction != direction {
                    editor.direction = direction;
                    editor.entry = None;
                }
            }
        });
    }
}

fn profile_picker(
    ui: &mut Ui,
    editor: &mut Editor,
    device: &Device,
    profiles: &Saved,
    available: &[Control],
) {
    let matching = profiles
        .iter()
        .filter(|(kind, profile)| {
            profile_matches(*kind, profile, device) && !profile.settings.buttons.is_empty()
        })
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ComboBox::from_id_salt("copy-imported-bindings")
            .selected_text("Load imported assignments")
            .width(ui.available_width().min(320.))
            .show_ui(ui, |ui| {
                for (index, (kind, profile)) in matching.iter().enumerate() {
                    let label = format!("{} · {}", kind.name(), profile.device_name);
                    ui.push_id(index, |ui| {
                        if ui
                            .add(Button::new(label).truncate())
                            .on_hover_text(&profile.device_key)
                            .clicked()
                        {
                            let (loaded, skipped) = stage_imported_assignments(
                                editor.bindings_mut(),
                                profile,
                                available,
                            );
                            editor.entry = None;
                            let mut notice = if loaded == 0 {
                                "No compatible assignments loaded.".to_owned()
                            } else {
                                let noun = if loaded == 1 {
                                    "assignment"
                                } else {
                                    "assignments"
                                };
                                format!(
                                    "Loaded {loaded} {noun} into Default. Use Save to keep them."
                                )
                            };
                            if !skipped.is_empty() {
                                notice.push_str(&format!(" Skipped {}.", skipped.join(", ")));
                            }
                            editor.notice = Some((notice, loaded == 0));
                            ui.close();
                        }
                    });
                }
            });
    });
}

fn stage_imported_assignments(
    draft: &mut Bindings,
    profile: &Profile,
    available: &[Control],
) -> (usize, Vec<String>) {
    let mut loaded = 0;
    let mut skipped = Vec::new();
    for (&button, binding) in &profile.settings.buttons {
        let label = button.name();
        let Some(id) = controls::imported_control(button, available) else {
            skipped.push(format!("{label} (control unavailable)"));
            continue;
        };
        match binding {
            Binding::Click(action) if controls::supports(action) => {
                draft.insert(id, binding.clone());
                loaded += 1;
            }
            Binding::Click(_) => skipped.push(format!("{label} (action unsupported here)")),
            Binding::Gesture(directions) => {
                let mut supported = BTreeMap::new();
                for (&direction, action) in directions {
                    if controls::supports(action) {
                        supported.insert(direction, action.clone());
                    } else {
                        skipped.push(format!(
                            "{label} {} (action unsupported here)",
                            direction.name()
                        ));
                    }
                }
                loaded += supported.len();
                if !supported.is_empty() {
                    if let Some(Binding::Gesture(current)) = draft.get_mut(&id) {
                        current.extend(supported);
                    } else {
                        draft.insert(id, Binding::Gesture(supported));
                    }
                }
            }
        }
    }
    (loaded, skipped)
}

fn profile_matches(kind: SourceKind, profile: &Profile, device: &Device) -> bool {
    if kind == SourceKind::Vendor
        && let Some(model) = profile.device_name.split_whitespace().next()
        && let Some(pid) = model.strip_prefix('2')
        && pid.len() == 4
        && let Ok(pid) = u16::from_str_radix(pid, 16)
    {
        return pid == device.product_id;
    }
    let normalize = |name: &str| {
        name.to_lowercase()
            .replace("logitech ", "")
            .trim_end_matches(" m")
            .split_whitespace()
            .collect::<String>()
    };
    normalize(&profile.device_name) == normalize(&device.name)
}

fn profile_tabs(ui: &mut Ui, editor: &mut Editor, editable: bool) {
    let mut add_application = None;
    surface(8).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let editing = editor
            .application
            .as_deref()
            .and_then(|key| editor.draft.applications.get(key))
            .map_or("Default", |profile| profile.name.as_str());
        ui.add(egui::Label::new(title(format!("Editing profile · {editing}"), 13.)).truncate())
            .on_hover_text(editing);
        ui.horizontal(|ui| {
            let add = ui.add_enabled(editable, Button::new("+ Add application"));
            paint_focus(ui, &add);
            if add.clicked() {
                editor.app_picker = Some(AppPicker::new());
            }
            add_application = Some(add);
            if let Some(app) = editor.application.clone() {
                ui.add_enabled_ui(editable, |ui| {
                    ui.menu_button("Profile options", |ui| {
                        if ui.button("Remove application profile").clicked() {
                            editor.draft.applications.remove(&app);
                            editor.application = None;
                            editor.entry = None;
                            editor.notice = None;
                            ui.close();
                        }
                    });
                });
            }
            ScrollArea::horizontal()
                .id_salt("application-profiles")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(editor.application.is_none(), "Default")
                            .clicked()
                        {
                            editor.application = None;
                            editor.entry = None;
                        }
                        for (key, profile) in &editor.draft.applications {
                            let text = ui.painter().layout_no_wrap(
                                profile.name.clone(),
                                egui::TextStyle::Button.resolve(ui.style()),
                                TEXT,
                            );
                            let tab_width =
                                (text.size().x + ui.spacing().button_padding.x * 2. + 2.)
                                    .clamp(64., 192.);
                            let tab = ui
                                .add_sized(
                                    [tab_width, CONTROL_HEIGHT],
                                    Button::selectable(
                                        editor.application.as_ref() == Some(key),
                                        &profile.name,
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(format!("{}\n{key}", profile.name));
                            paint_focus(ui, &tab);
                            if tab.clicked() {
                                editor.application = Some(key.clone());
                                editor.entry = None;
                            }
                        }
                    });
                });
        });
    });

    let picker_was_open = editor.app_picker.is_some();
    if let Some(app) = app_picker(ui, &mut editor.app_picker, &mut editor.draft) {
        editor.application = Some(app);
        editor.entry = None;
        editor.notice = None;
    }
    if picker_was_open
        && editor.app_picker.is_none()
        && let Some(add) = add_application
    {
        add.request_focus();
    }
}

fn host_workspace(
    ui: &mut Ui,
    editor: &mut Editor,
    available: bool,
    retry: bool,
) -> Option<Command> {
    let mut command = None;
    let hosts = match editor.hosts.as_ref() {
        Some(Ok(Some(hosts))) => hosts,
        Some(Ok(None)) => return None,
        Some(Err(error)) => {
            card().show(ui, |ui| {
                ui.label(title("Easy-Switch", 16.));
                ui.label(muted(error));
                if ui
                    .add_enabled(retry && !editor.host_busy, Button::new("Try again"))
                    .clicked()
                {
                    command = Some(Command::ReloadHosts);
                }
            });
            return command;
        }
        None => return None,
    };
    card().show(ui, |ui| {
        ui.set_width(ui.available_width().min(680.));
        ui.horizontal(|ui| {
            ui.label(title("Easy-Switch", 16.));
            workspace::trailing(ui, |ui| {
                if labeled_button(ui, "Refresh", Icon::Refresh, retry && !editor.host_busy)
                    .clicked()
                {
                    command = Some(Command::ReloadHosts);
                }
                if editor.host_busy {
                    ui.spinner();
                }
            });
        });
        ui.separator();
        for slot in &hosts.slots {
            ui.push_id(slot.index, |ui| {
                ui.horizontal(|ui| {
                    let actions = if slot.name_capacity != 0 { 92. } else { 0. }
                        + if hosts.can_switch && slot.paired && hosts.current != Some(slot.index) {
                            72.
                        } else {
                            0.
                        };
                    let description_width = (ui.available_width() - actions).max(120.);
                    ui.allocate_ui_with_layout(
                        vec2(description_width, 40.),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(description_width);
                            let name = match (slot.paired, slot.name.is_empty()) {
                                (false, _) => "No computer paired",
                                (true, true) => "Unnamed computer",
                                (true, false) => &slot.name,
                            };
                            ui.add(egui::Label::new(title(name, 15.)).truncate())
                                .on_hover_text(name);
                            let mut details = vec![format!("Channel {}", slot.index + 1)];
                            if hosts.current == Some(slot.index) {
                                details.push("This computer".into());
                            }
                            if let Some(os) = slot.operating_system {
                                details.push(os.into());
                            }
                            if !slot.connection.is_empty() {
                                details.push(slot.connection.into());
                            }
                            let details = details.join(" · ");
                            ui.add(egui::Label::new(muted(&details).size(12.)).truncate())
                                .on_hover_text(&details);
                        },
                    );
                    workspace::trailing(ui, |ui| {
                        let editable = available && !editor.host_busy;
                        if slot.name_capacity != 0
                            && ui.add_enabled(editable, Button::new("Rename…")).clicked()
                        {
                            editor.host_rename = Some((slot.index, slot.name.clone()));
                        }
                        if hosts.can_switch
                            && slot.paired
                            && hosts.current != Some(slot.index)
                            && ui.add_enabled(editable, Button::new("Switch")).clicked()
                        {
                            editor.host_switch = Some(slot.index);
                        }
                    });
                });
                ui.add_space(5.);
            });
        }
    });
    if let Some((index, name)) = &mut editor.host_rename
        && let Some(host) = hosts.slots.get(usize::from(*index))
    {
        let response = egui::Modal::new(Id::new("rename-channel")).show(ui.ctx(), |ui| {
            ui.set_min_width(320.);
            ui.label(title(format!("Rename channel {}", *index + 1), 18.));
            ui.add(egui::TextEdit::singleline(name).desired_width(320.));
            let remaining = usize::from(host.name_capacity);
            let used = name.trim().len();
            let valid = used > 0 && used <= remaining && !name.trim().chars().any(char::is_control);
            ui.label(muted(format!("{used} of {remaining} bytes")));
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    ui.close();
                }
                if ui
                    .add_enabled(valid && available, primary("Save"))
                    .clicked()
                {
                    command = Some(Command::RenameHost(*index, name.trim().into()));
                    ui.close();
                }
            });
        });
        if response.should_close() {
            editor.host_rename = None;
        }
    }
    if let Some(index) = editor.host_switch {
        let response = egui::Modal::new(Id::new("switch-channel")).show(ui.ctx(), |ui| {
            ui.set_min_width(320.);
            ui.label(title(format!("Switch to channel {}?", index + 1), 18.));
            if let Some(host) = hosts.slots.iter().find(|host| host.index == index)
                && !host.name.is_empty()
            {
                ui.label(muted(format!("Destination · {}", host.name)));
            }
            ui.label(muted("The device will disconnect from this computer."));
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    ui.close();
                }
                if ui.add_enabled(available, primary("Switch")).clicked() {
                    command = Some(Command::SwitchHost(index));
                    ui.close();
                }
            });
        });
        if response.should_close() {
            editor.host_switch = None;
        }
    }
    command
}

fn ring_workspace(
    ui: &mut Ui,
    editor: &mut Editor,
    editable: bool,
    preview: Option<egui::Response>,
) {
    let layer = editor.draft.layer_mut(editor.application.as_deref());
    let ring_edge = (ui.available_height() - 30.).clamp(142., 288.);
    ScrollArea::vertical()
        .id_salt("actions-ring")
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if ui.available_width() >= 500. {
                ui.columns(2, |columns| {
                    let previous = editor.ring_slot;
                    feature_panels::ring_editor(
                        &mut columns[0],
                        &layer.ring,
                        &mut editor.ring_slot,
                        ring_edge,
                    );
                    if editor.ring_slot != previous {
                        editor.entry = None;
                    }
                    columns[1].add_enabled_ui(editable, |ui| {
                        edit_ring_slot(
                            ui,
                            &mut layer.ring[editor.ring_slot],
                            editor.ring_slot,
                            &mut editor.entry,
                            &editor.hosts,
                        );
                    });
                });
            } else {
                let previous = editor.ring_slot;
                feature_panels::ring_editor(ui, &layer.ring, &mut editor.ring_slot, ring_edge);
                if editor.ring_slot != previous {
                    editor.entry = None;
                }
                ui.add_enabled_ui(editable, |ui| {
                    edit_ring_slot(
                        ui,
                        &mut layer.ring[editor.ring_slot],
                        editor.ring_slot,
                        &mut editor.entry,
                        &editor.hosts,
                    );
                });
            }
        });
    if let Some(preview) = preview {
        let response = egui::Modal::new(Id::new("preview-actions-ring")).show(ui.ctx(), |ui| {
            ui.label(title("Actions Ring preview", 18.));
            let clicked = feature_panels::ring_preview(ui, &layer.ring);
            if ui.button("Done").clicked() {
                ui.close();
            }
            clicked
        });
        if response.should_close() || response.inner {
            editor.ring_preview = false;
            preview.request_focus();
        }
    }
}

fn edit_ring_slot(
    ui: &mut Ui,
    slot: &mut Option<Action>,
    index: usize,
    entry: &mut Option<ActionPicker>,
    hosts: &Option<Result<Option<Hosts>, String>>,
) {
    let hosts = hosts
        .as_ref()
        .and_then(|value| value.as_ref().ok())
        .and_then(Option::as_ref);
    card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        let position = [
            "Top",
            "Upper right",
            "Right",
            "Lower right",
            "Bottom",
            "Lower left",
            "Left",
            "Upper left",
        ][index];
        ui.horizontal_wrapped(|ui| {
            ui.label(title(format!("Slot {} · {position}", index + 1), 16.));
            if ui
                .add_enabled(slot.is_some(), Button::new("Clear slot"))
                .clicked()
            {
                *slot = None;
                *entry = None;
            }
        });
        ui.push_id(index, |ui| {
            let menu = ActionMenu {
                id: ui.id().with("action"),
                heading: "Action",
                default: "Unassigned",
                show_ring: false,
                hosts,
            };
            if let Some(change) = action_picker(ui, entry, menu, slot.as_ref()) {
                *slot = change;
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pointer(dpi: u16, threshold: u8) -> Pointer {
        Pointer {
            dpi: Some(controls::Dpi {
                current: dpi,
                supported: vec![1000, 1100, 1200],
            }),
            wheel: Some(Wheel {
                mode: WheelMode::Ratchet,
                threshold,
                default_threshold: 30,
            }),
            ..Pointer::default()
        }
    }

    #[test]
    fn applying_dpi_keeps_the_independent_wheel_draft() {
        let mut editor = Editor::default();
        editor.pointer_loaded(Ok(pointer(1000, 30)), None);
        editor.dpi_choice = Some(1100);
        editor.wheel_choice.as_mut().unwrap().threshold = 40;

        editor.pointer_loaded(Ok(pointer(1100, 35)), Some(Tuning::Dpi));
        assert_eq!(editor.dpi_choice, Some(1100));
        assert_eq!(
            editor.pointer.as_ref().unwrap().wheel.unwrap().threshold,
            35
        );
        assert_eq!(editor.wheel_choice.unwrap().threshold, 40);

        editor.pointer_loaded(Err("Disconnected during read".into()), None);
        assert!(editor.pointer_error.is_some());
        assert_eq!(editor.wheel_choice.unwrap().threshold, 40);

        editor.pointer_loaded(Ok(pointer(1200, 35)), None);
        assert!(editor.pointer_error.is_none());
        assert_eq!(editor.dpi_choice, Some(1200));
        assert_eq!(editor.wheel_choice.unwrap().threshold, 40);
    }

    #[test]
    fn imported_assignments_stage_only_compatible_controls_and_directions() {
        use openoptions::migration::Button as DeviceButton;

        let action = Action::Named(NamedAction::Copy);
        let profile = Profile {
            settings: openoptions::migration::Settings {
                buttons: BTreeMap::from([
                    (DeviceButton::Forward, Binding::Click(action.clone())),
                    (
                        DeviceButton::Gesture,
                        Binding::Gesture(BTreeMap::from([
                            (Direction::Up, action.clone()),
                            (Direction::Down, Action::Shortcut("invalid key".into())),
                        ])),
                    ),
                ]),
                ..Default::default()
            },
            ..Profile::default()
        };
        let mut draft = BTreeMap::from([(
            0xC3,
            Binding::Gesture(BTreeMap::from([(Direction::Down, action.clone())])),
        )]);
        let (count, skipped) = stage_imported_assignments(
            &mut draft,
            &profile,
            &[Control {
                id: 0xC3,
                mouse: true,
                raw_motion: true,
            }],
        );
        assert_eq!(count, 1);
        assert_eq!(skipped.len(), 2);
        assert!(skipped[0].contains("Forward button"));
        assert!(skipped[1].contains("Down"));
        assert_eq!(
            draft[&0xC3],
            Binding::Gesture(BTreeMap::from([
                (Direction::Down, action.clone()),
                (Direction::Up, action),
            ])),
        );
    }
}
