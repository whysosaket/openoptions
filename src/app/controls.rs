use super::{
    Event, OpenOptions, Saved,
    action_labels::{action_label, action_name},
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

pub(super) type DeviceKey = (&'static str, u16, Option<[u8; 4]>);

fn device_key(device: &Device) -> DeviceKey {
    (device.connection, device.product_id, device.unit_id())
}

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
    pub available: Option<Result<Vec<Control>, String>>,
    pub saved: Result<Profiles, String>,
    pub draft: Profiles,
    application: Option<String>,
    application_path: String,
    pub hosts: Option<Result<Option<Hosts>, String>>,
    pub host_busy: bool,
    host_rename: Option<(u8, String)>,
    host_switch: Option<u8>,
    ring_slot: usize,
    ring_preview: bool,
    pub saving: bool,
    pub pointer: Option<Pointer>,
    pub pointer_busy: bool,
    dpi_choice: Option<u16>,
    wheel_choice: Option<Wheel>,
    pub notice: Option<(String, bool)>,
    section: Section,
    selected: Option<u16>,
    direction: Direction,
    entry: Option<ActionEntry>,
}

pub(super) struct ActionEntry {
    application: bool,
    value: String,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            available: None,
            saved: Ok(Profiles::default()),
            draft: Profiles::default(),
            application: None,
            application_path: String::new(),
            hosts: None,
            host_busy: false,
            host_rename: None,
            host_switch: None,
            ring_slot: 0,
            ring_preview: false,
            saving: false,
            pointer: None,
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
    pub fn loaded(
        &mut self,
        available: Result<Vec<Control>, String>,
        saved: Result<Profiles, String>,
    ) {
        if available.as_ref().is_ok_and(|values| {
            !values.iter().any(|control| control.mouse)
                && values.iter().any(|control| !control.mouse)
        }) {
            self.section = Section::Keys;
        }
        if let Ok(bindings) = &saved {
            self.draft = bindings.clone();
        }
        self.available = Some(available);
        self.saved = saved;
    }

    pub fn pointer_loaded(&mut self, pointer: Pointer) {
        self.dpi_choice = pointer.dpi.as_ref().map(|dpi| dpi.current);
        self.wheel_choice = pointer.wheel;
        self.pointer = Some(pointer);
    }

    fn bindings(&self) -> &Bindings {
        &self.draft.layer(self.application.as_deref()).bindings
    }

    fn bindings_mut(&mut self) -> &mut Bindings {
        &mut self.draft.layer_mut(self.application.as_deref()).bindings
    }

    fn changed(&self) -> bool {
        self.saved.as_ref().is_ok_and(|saved| *saved != self.draft)
    }
}

pub(super) struct LiveControls {
    pub key: DeviceKey,
    pub name: String,
    pub session: Session,
    pub active: bool,
    pub stopping: bool,
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
        if !self.editors.contains_key(&device_key(&device)) {
            self.load_controls(device.clone(), context);
        }
        self.detail = Some(super::Detail::Device(device));
    }

    fn load_controls(&mut self, device: Device, context: &egui::Context) {
        let key = device_key(&device);
        self.editors.insert(key, Editor::default());
        self.job(context, move || {
            let available = controls::discover(&device);
            let saved = Store::location()
                .and_then(|path| Store::open(&path))
                .and_then(|store| store.customizations(&device));
            let pointer = controls::pointer(&device).ok();
            let hosts = controls::host_channels(&device);
            Event::ControlsLoaded(key, available, saved, pointer, hosts)
        });
    }

    pub(super) fn poll_controls(&mut self, context: &egui::Context) {
        let Some(live) = &mut self.live_controls else {
            return;
        };
        while let Some(status) = live.session.poll() {
            match status {
                Status::Active => live.active = true,
                Status::Ring { actions, pointer } => {
                    self.actions_ring =
                        Some(feature_panels::RingWindow::new(context, *actions, pointer));
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
            live.session.stop();
        }
    }

    pub(super) fn device_settings(&mut self, ui: &mut Ui, device: &Device) {
        let key = device_key(device);
        let online = self
            .devices
            .as_ref()
            .is_ok_and(|devices| devices.iter().any(|current| device_key(current) == key));
        let Some(editor) = self.editors.get_mut(&key) else {
            return;
        };
        let live = self.live_controls.as_ref();
        let command = settings_workspace(ui, editor, device, online, &self.profiles, live);
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
                    session: Session::start(device.clone(), editor.draft.clone()),
                    active: false,
                    stopping: false,
                });
                ui.ctx().request_repaint_after(Duration::from_millis(85));
            }
            Some(Command::Pointer(change)) => {
                editor.pointer_busy = true;
                editor.notice = None;
                let device = device.clone();
                self.job(ui.ctx(), move || {
                    Event::PointerApplied(key, controls::apply_pointer(&device, change))
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
    profiles: &Saved,
    live: Option<&LiveControls>,
) -> Option<Command> {
    let mut command = None;
    ui.visuals_mut().selection.stroke.color = TEXT;
    let owns_session = live.is_some_and(|session| session.key == device_key(device));
    let editable = !owns_session && !editor.saving && editor.saved.is_ok();
    let available = match &editor.available {
        Some(Ok(controls)) => controls.clone(),
        Some(Err(_)) => controls::known_controls(device),
        None => Vec::new(),
    };
    let pointer_supported = editor
        .pointer
        .as_ref()
        .is_some_and(|pointer| pointer.dpi.is_some() || pointer.wheel.is_some());
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            vec2(154., ui.available_height()),
            Layout::top_down(Align::Min),
            |ui| {
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
                        Section::Pointer => pointer_supported,
                        Section::Device => true,
                    };
                    if !show {
                        continue;
                    }
                    if ui
                        .add_sized(
                            [154., 33.],
                            Button::selectable(editor.section == section, section.label()),
                        )
                        .clicked()
                    {
                        editor.section = section;
                        editor.selected = None;
                        editor.entry = None;
                    }
                }
            },
        );
        ui.add_space(10.);
        ui.separator();
        ui.add_space(12.);
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            if let Some((notice, error)) = &editor.notice {
                ui.label(RichText::new(notice).size(12.).color(if *error {
                    WARNING
                } else {
                    SUCCESS
                }));
            }
            if editor.section == Section::Device {
                ui.label(title("Connection", 20.));
                workspace::badge(
                    ui,
                    if online { "Connected" } else { "Disconnected" },
                    online,
                );
                ui.label(muted(format!(
                    "{}  ·  046D:{:04X}",
                    device.connection, device.product_id
                )));
                ui.label(muted(format!("{} HID interfaces", device.interfaces)));
                command = host_workspace(ui, editor, online && live.is_none());
                return;
            }
            if editor.section == Section::Pointer {
                command = pointer_workspace(ui, editor, online && !owns_session);
                return;
            }
            match &editor.available {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(muted("Reading device controls…"));
                    });
                    return;
                }
                Some(Ok(_)) if available.is_empty() => {
                    ui.label(title("No configurable controls", 20.));
                    ui.label(muted(
                        "The device did not report any controls available for remapping.",
                    ));
                    return;
                }
                Some(Err(error)) if available.is_empty() => {
                    ui.label(title("Controls unavailable", 20.));
                    ui.label(muted(error));
                    if ui.button("Try again").clicked() {
                        command = Some(Command::Reload);
                    }
                    return;
                }
                Some(_) => {}
            };
            if let Err(error) = &editor.saved {
                ui.label(
                    RichText::new(format!("Could not read assignments. {error}")).color(WARNING),
                );
                if ui.button("Try again").clicked() {
                    command = Some(Command::Reload);
                }
                return;
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(title(editor.section.label(), 23.));
                if owns_session && let Some(live) = live {
                    let text = if live.stopping {
                        "Disabling…"
                    } else if live.active {
                        "Live"
                    } else {
                        "Enabling…"
                    };
                    workspace::badge(ui, text, live.active && !live.stopping);
                }
            });
            let instruction = match editor.section {
                Section::Buttons => "Choose a button to assign a press or a gesture.",
                Section::Keys => "Choose a key to change what it does.",
                Section::Ring => "Choose one of the eight slots to assign an action.",
                Section::Device | Section::Pointer => unreachable!(),
            };
            ui.label(muted(instruction));
            ui.horizontal_wrapped(|ui| {
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
                                && !editor.pointer_busy
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
                if editor.saving {
                    ui.spinner();
                }
            });
            if let Some(Err(error)) = &editor.available {
                ui.label(muted(error).size(12.));
            }
            profile_tabs(ui, editor, editable);
            if editor.section == Section::Ring {
                ui.add_enabled_ui(editable, |ui| ring_workspace(ui, editor));
                return;
            }
            let control_area = ui.available_height().max(100.);
            ScrollArea::vertical()
                .id_salt(("control-settings", device_key(device)))
                .max_height(control_area)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if editor.section == Section::Buttons && editor.application.is_none() {
                        ui.add_enabled_ui(editable, |ui| {
                            profile_picker(ui, editor, device, profiles, &available)
                        });
                    }
                    let controls: Vec<_> = available
                        .into_iter()
                        .filter(|control| editor.section.accepts(*control))
                        .collect();
                    if editor.selected.is_none_or(|selected| {
                        !controls.iter().any(|control| control.id == selected)
                    }) {
                        editor.selected = controls.first().map(|control| control.id);
                    }
                    if ui.available_width() >= 540. {
                        ui.columns(2, |columns| {
                            control_list(&mut columns[0], editor, &controls);
                            columns[1]
                                .add_enabled_ui(editable, |ui| inspector(ui, editor, &controls));
                        });
                    } else {
                        control_list(ui, editor, &controls);
                        ui.add_enabled_ui(editable, |ui| inspector(ui, editor, &controls));
                    }
                });
        });
    });
    command
}

fn pointer_workspace(ui: &mut Ui, editor: &mut Editor, editable: bool) -> Option<Command> {
    let pointer = editor.pointer.as_ref()?;
    let mut command = None;
    ui.label(title("Point & Scroll", 23.));
    if editor.pointer_busy {
        ui.spinner();
    }
    ScrollArea::vertical()
        .id_salt("pointer-settings")
        .show(ui, |ui| {
            if let Some(dpi) = &pointer.dpi {
                card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(title("Pointer speed", 16.));
                    ui.label(muted(format!(
                        "Current device setting · {} DPI",
                        dpi.current
                    )));
                    let Some(choice) = &mut editor.dpi_choice else {
                        return;
                    };
                    let index = dpi
                        .supported
                        .binary_search(choice)
                        .unwrap_or_else(|index| index.min(dpi.supported.len() - 1));
                    let mut index = index;
                    let response = ui.add_enabled(
                        editable && !editor.pointer_busy,
                        egui::Slider::new(&mut index, 0..=(dpi.supported.len() - 1))
                            .show_value(false),
                    );
                    if response.changed() {
                        *choice = dpi.supported[index];
                    }
                    ui.label(format!("{choice} DPI"));
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
                card().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(title("SmartShift", 16.));
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
                                ui.add(
                                    egui::Slider::new(&mut choice.threshold, 1..=254)
                                        .text("Switch threshold"),
                                );
                            }
                        }
                        if ui
                            .add_enabled(
                                current.mode != choice.mode
                                    || current.threshold != choice.threshold,
                                Button::new("Apply wheel settings"),
                            )
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

fn control_list(ui: &mut Ui, editor: &mut Editor, controls: &[Control]) {
    let label = if editor.section == Section::Buttons {
        "Mouse controls"
    } else {
        "Programmable keys"
    };
    card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(title(label, 14.));
        ui.spacing_mut().item_spacing.y = 4.;
        if controls.iter().any(|control| control.id == 0xC3) && ui.available_width() >= 180. {
            mouse_map(ui, editor, controls);
        }
        for &control in controls {
            let name = control.name();
            let assignment = match editor.bindings().get(&control.id) {
                Some(Binding::Click(action)) => action_label(action),
                Some(Binding::Gesture(_)) => "Gesture",
                None if editor.application.is_some() => "Inherit default",
                None => "Device default",
            };
            let selected = editor.selected == Some(control.id);
            let row = Button::new((
                RichText::new(&name).color(TEXT),
                egui::Atom::grow(),
                muted(assignment).size(12.),
            ))
            .selected(selected)
            .frame_when_inactive(selected)
            .truncate();
            if ui
                .add_sized([ui.available_width(), 33.], row)
                .on_hover_text(format!("{name}  ·  {assignment}"))
                .clicked()
            {
                editor.selected = Some(control.id);
                editor.direction = Direction::Click;
                editor.entry = None;
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
    ] {
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
            editor.selected = Some(id);
            editor.direction = Direction::Click;
            editor.entry = None;
        }
        let accent = if selected { SOLID } else { MUTED };
        let radius = if selected || focus.hovered() || focus.has_focus() {
            7.5
        } else {
            5.
        };
        painter.circle(point, radius, WINDOW, egui::Stroke::new(1.4_f32, accent));
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

fn inspector(ui: &mut Ui, editor: &mut Editor, controls: &[Control]) {
    let Some(control) = controls
        .iter()
        .copied()
        .find(|control| Some(control.id) == editor.selected)
    else {
        return;
    };
    let binding = editor.bindings().get(&control.id).cloned();
    let gesture = matches!(binding, Some(Binding::Gesture(_)));
    card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(title(control.name(), 16.));
        if control.mouse {
            ui.horizontal(|ui| {
                if ui.selectable_label(!gesture, "Press").clicked() && gesture {
                    match &binding {
                        Some(Binding::Gesture(actions))
                            if actions.contains_key(&Direction::Click) =>
                        {
                            editor.bindings_mut().insert(
                                control.id,
                                Binding::Click(actions[&Direction::Click].clone()),
                            );
                        }
                        _ => {
                            editor.bindings_mut().remove(&control.id);
                        }
                    }
                    editor.entry = None;
                    editor.notice = None;
                }
                if ui.selectable_label(gesture, "Gesture").clicked() && !gesture {
                    let actions = match &binding {
                        Some(Binding::Click(action)) => {
                            BTreeMap::from([(Direction::Click, action.clone())])
                        }
                        _ => BTreeMap::from([(
                            Direction::Click,
                            Action::Named(NamedAction::NoAction),
                        )]),
                    };
                    editor
                        .bindings_mut()
                        .insert(control.id, Binding::Gesture(actions));
                    editor.direction = Direction::Click;
                    editor.entry = None;
                    editor.notice = None;
                }
            });
        }
        let action = match &binding {
            Some(Binding::Gesture(actions)) => {
                ui.label(muted("Hold, move, then release."));
                gesture_slots(ui, editor, actions);
                actions.get(&editor.direction)
            }
            Some(Binding::Click(action)) => Some(action),
            None => None,
        };
        let default = if gesture {
            "Unassigned"
        } else if editor.application.is_some() {
            "Inherit default"
        } else {
            "Device default"
        };
        let hosts = editor
            .hosts
            .as_ref()
            .and_then(|value| value.as_ref().ok())
            .and_then(Option::as_ref);
        let heading = if gesture {
            format!("{} action", editor.direction.name())
        } else {
            "Action".into()
        };
        let menu = ActionMenu {
            id: Id::new(("action", control.id, editor.direction.name())),
            heading: &heading,
            default,
            show_ring: true,
            hosts,
        };
        if let Some(change) = action_picker(ui, &mut editor.entry, menu, action) {
            if gesture {
                let direction = editor.direction;
                if let Some(Binding::Gesture(actions)) = editor.bindings_mut().get_mut(&control.id)
                {
                    match change {
                        Some(action) => {
                            actions.insert(direction, action);
                        }
                        None => {
                            actions.remove(&direction);
                        }
                    }
                }
            } else {
                match change {
                    Some(action) => {
                        editor
                            .bindings_mut()
                            .insert(control.id, Binding::Click(action));
                    }
                    None => {
                        editor.bindings_mut().remove(&control.id);
                    }
                }
            }
            editor.notice = None;
        }
        if gesture && ui.button("Restore device default").clicked() {
            editor.bindings_mut().remove(&control.id);
            editor.entry = None;
            editor.notice = None;
        }
    });
}

fn gesture_slots(ui: &mut Ui, editor: &mut Editor, actions: &BTreeMap<Direction, Action>) {
    egui::Grid::new("gesture-directions")
        .min_col_width(64.)
        .spacing([4., 4.])
        .show(ui, |ui| {
            ui.label("");
            gesture_slot(ui, editor, actions, Direction::Up, "↑");
            ui.end_row();
            gesture_slot(ui, editor, actions, Direction::Left, "←");
            gesture_slot(ui, editor, actions, Direction::Click, "·");
            gesture_slot(ui, editor, actions, Direction::Right, "→");
            ui.end_row();
            ui.label("");
            gesture_slot(ui, editor, actions, Direction::Down, "↓");
            ui.end_row();
        });
}

fn gesture_slot(
    ui: &mut Ui,
    editor: &mut Editor,
    actions: &BTreeMap<Direction, Action>,
    direction: Direction,
    icon: &str,
) {
    let action = actions.get(&direction);
    let label = format!("{icon} {}", direction.name());
    let description = action.map_or("Unassigned", action_label);
    let text = RichText::new(label).color(if action.is_some() || editor.direction == direction {
        TEXT
    } else {
        MUTED
    });
    if ui
        .add_sized(
            [64., 30.],
            Button::selectable(editor.direction == direction, text),
        )
        .on_hover_text(description)
        .clicked()
    {
        editor.direction = direction;
        editor.entry = None;
    }
}

struct ActionMenu<'a> {
    id: Id,
    heading: &'a str,
    default: &'a str,
    show_ring: bool,
    hosts: Option<&'a Hosts>,
}

fn action_picker(
    ui: &mut Ui,
    entry: &mut Option<ActionEntry>,
    menu: ActionMenu<'_>,
    current: Option<&Action>,
) -> Option<Option<Action>> {
    let mut change = None;
    let ActionMenu {
        id,
        heading,
        default,
        show_ring,
        hosts,
    } = menu;
    ui.label(title(heading, 13.));
    ComboBox::from_id_salt(id)
        .width(ui.available_width().min(290.))
        .selected_text(current.map_or_else(|| default.into(), action_name))
        .show_ui(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 3.;
            if ui.selectable_label(current.is_none(), default).clicked() {
                change = Some(None);
                *entry = None;
                ui.close();
            }
            for name in NamedAction::ALL {
                let action = Action::Named(name);
                if controls::supports(&action)
                    && ui
                        .selectable_label(current == Some(&action), action_label(&action))
                        .clicked()
                {
                    change = Some(Some(action));
                    *entry = None;
                    ui.close();
                }
            }
            if show_ring && ui.button("Show Actions Ring").clicked() {
                change = Some(Some(Action::ShowActionsRing));
                *entry = None;
                ui.close();
            }
            if let Some(hosts) = hosts.filter(|hosts| hosts.can_switch) {
                for host in hosts.slots.iter().filter(|host| host.paired) {
                    if ui
                        .button(format!("Switch to channel {}", host.index + 1))
                        .clicked()
                    {
                        change = Some(Some(Action::SwitchHost(host.index)));
                        *entry = None;
                        ui.close();
                    }
                }
            }
            for (label, application) in [("Keyboard shortcut…", false), ("Open application…", true)]
            {
                if ui.button(label).clicked() {
                    let value = match current {
                        Some(Action::Shortcut(value)) if !application => value.clone(),
                        Some(Action::OpenApplication(value)) if application => value.clone(),
                        _ => String::new(),
                    };
                    *entry = Some(ActionEntry { application, value });
                    ui.close();
                }
            }
        });
    if current.is_some_and(|action| !controls::supports(action)) {
        ui.label(
            RichText::new("Choose an action supported on this system.")
                .size(12.)
                .color(WARNING),
        );
    }
    if let Some(draft) = entry {
        let hint = if draft.application {
            "Absolute application path"
        } else if cfg!(target_os = "macos") {
            "Example: Cmd+Shift+K"
        } else {
            "Example: Ctrl+Shift+K"
        };
        ui.add(
            egui::TextEdit::singleline(&mut draft.value)
                .hint_text(hint)
                .desired_width(ui.available_width()),
        );
        let value = draft.value.trim();
        let candidate = if draft.application {
            Action::application(value)
        } else {
            Some(Action::Shortcut(value.into()))
        };
        let valid = candidate.as_ref().is_some_and(controls::supports);
        if ui
            .add_enabled(valid, Button::new("Use assignment"))
            .clicked()
        {
            change = Some(candidate);
            *entry = None;
        }
    }
    change
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
            .show_ui(ui, |ui| {
                for (index, (kind, profile)) in matching.iter().enumerate() {
                    let label = format!("{} · {}", kind.name(), profile.device_name);
                    ui.push_id(index, |ui| {
                        if ui.button(label).clicked() {
                            let mut changed = 0;
                            let mut skipped = 0;
                            for (&button, binding) in &profile.settings.buttons {
                                if let Some(id) = controls::imported_control(button, available) {
                                    editor.bindings_mut().insert(id, binding.clone());
                                    changed += 1;
                                } else {
                                    skipped += 1;
                                }
                            }
                            editor.entry = None;
                            editor.notice = Some((
                                if skipped == 0 {
                                    format!("Loaded {changed} assignments. Save to keep them.")
                                } else if changed == 0 {
                                    format!("{skipped} imported assignments are unavailable on this device.")
                                } else {
                                    format!("Loaded {changed} assignments and skipped {skipped} unavailable controls. Save to keep your changes.")
                                },
                                changed == 0,
                            ));
                            ui.close();
                        }
                    });
                }
            });
    });
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
    Frame::new()
        .fill(SURFACE)
        .stroke(line())
        .corner_radius(8)
        .inner_margin(8)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(muted("Profile"));
                if ui
                    .selectable_label(editor.application.is_none(), "Default")
                    .clicked()
                {
                    editor.application = None;
                    editor.entry = None;
                }
                for (key, profile) in &editor.draft.applications {
                    if ui
                        .selectable_label(editor.application.as_ref() == Some(key), &profile.name)
                        .on_hover_text(key)
                        .clicked()
                    {
                        editor.application = Some(key.clone());
                        editor.entry = None;
                    }
                }
                ui.add_enabled_ui(editable, |ui| {
                    ui.menu_button("+ Add application", |ui| {
                        ui.set_min_width(340.);
                        let hint = if cfg!(target_os = "macos") {
                            "Path to an installed .app"
                        } else {
                            "Path to the application executable"
                        };
                        ui.label(title("Add application", 15.));
                        ui.add(
                            egui::TextEdit::singleline(&mut editor.application_path)
                                .hint_text(hint)
                                .desired_width(360.),
                        );
                        #[cfg(target_os = "macos")]
                        ComboBox::from_id_salt("installed-apps")
                            .selected_text("Applications")
                            .show_ui(ui, |ui| {
                                for application in installed_mac_apps() {
                                    let name = application
                                        .file_stem()
                                        .and_then(|name| name.to_str())
                                        .unwrap_or_default();
                                    if ui.button(name).clicked() {
                                        editor.application_path =
                                            application.to_string_lossy().into_owned();
                                    }
                                }
                            });
                        if ui
                            .add_enabled(!editor.application_path.trim().is_empty(), primary("Add"))
                            .clicked()
                        {
                            match editor.draft.add(&editor.application_path) {
                                Ok(app) => {
                                    editor.application = Some(app);
                                    editor.application_path.clear();
                                    editor.entry = None;
                                    editor.notice = None;
                                    ui.close();
                                }
                                Err(error) => editor.notice = Some((error, true)),
                            }
                        }
                    });
                    if let Some(app) = editor.application.clone() {
                        ui.menu_button("⋯", |ui| {
                            if ui.button("Remove application profile").clicked() {
                                editor.draft.applications.remove(&app);
                                editor.application = None;
                                editor.entry = None;
                                editor.notice = None;
                                ui.close();
                            }
                        });
                    }
                });
            });
        });
}

#[cfg(target_os = "macos")]
fn installed_mac_apps() -> Vec<std::path::PathBuf> {
    let mut roots = vec![
        std::path::PathBuf::from("/Applications"),
        std::path::PathBuf::from("/System/Applications"),
    ];
    if let Some(user) = directories::UserDirs::new() {
        roots.push(user.home_dir().join("Applications"));
    }
    let mut apps: Vec<_> = roots
        .iter()
        .filter_map(|root| std::fs::read_dir(root).ok())
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "app"))
        .collect();
    apps.sort_by(|left, right| left.file_stem().cmp(&right.file_stem()));
    apps
}

fn host_workspace(ui: &mut Ui, editor: &mut Editor, available: bool) -> Option<Command> {
    let mut command = None;
    let hosts = match editor.hosts.as_ref() {
        Some(Ok(Some(hosts))) => hosts,
        Some(Ok(None)) => return None,
        Some(Err(error)) => {
            card().show(ui, |ui| {
                ui.label(title("Easy-Switch", 16.));
                ui.label(muted(error));
                if ui
                    .add_enabled(available && !editor.host_busy, Button::new("Try again"))
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
                if labeled_button(ui, "Refresh", Icon::Refresh, available && !editor.host_busy)
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
                    ui.vertical(|ui| {
                        let name = match (slot.paired, slot.name.is_empty()) {
                            (false, _) => "No computer paired",
                            (true, true) => "Unnamed computer",
                            (true, false) => &slot.name,
                        };
                        ui.label(title(name, 15.));
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
                        ui.label(muted(details.join("  ·  ")).size(12.));
                    });
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
            let valid =
                !name.trim().is_empty() && name.trim().len() <= usize::from(host.name_capacity);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    ui.close();
                }
                if ui.add_enabled(valid, primary("Save")).clicked() {
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
            ui.label(muted("The device will disconnect from this computer."));
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    ui.close();
                }
                if ui.add(primary("Switch")).clicked() {
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

fn ring_workspace(ui: &mut Ui, editor: &mut Editor) {
    if ui.button("Preview ring").clicked() {
        editor.ring_preview = true;
    }
    let layer = editor.draft.layer_mut(editor.application.as_deref());
    ScrollArea::vertical()
        .id_salt("actions-ring")
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if ui.available_width() >= 600. {
                ui.columns(2, |columns| {
                    let previous = editor.ring_slot;
                    feature_panels::ring_editor(
                        &mut columns[0],
                        &layer.ring,
                        &mut editor.ring_slot,
                    );
                    if editor.ring_slot != previous {
                        editor.entry = None;
                    }
                    edit_ring_slot(
                        &mut columns[1],
                        &mut layer.ring[editor.ring_slot],
                        editor.ring_slot,
                        &mut editor.entry,
                        &editor.hosts,
                    );
                });
            } else {
                let previous = editor.ring_slot;
                feature_panels::ring_editor(ui, &layer.ring, &mut editor.ring_slot);
                if editor.ring_slot != previous {
                    editor.entry = None;
                }
                edit_ring_slot(
                    ui,
                    &mut layer.ring[editor.ring_slot],
                    editor.ring_slot,
                    &mut editor.entry,
                    &editor.hosts,
                );
            }
        });
    if editor.ring_preview {
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
        }
    }
}

fn edit_ring_slot(
    ui: &mut Ui,
    slot: &mut Option<Action>,
    index: usize,
    entry: &mut Option<ActionEntry>,
    hosts: &Option<Result<Option<Hosts>, String>>,
) {
    let hosts = hosts
        .as_ref()
        .and_then(|value| value.as_ref().ok())
        .and_then(Option::as_ref);
    card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(title(format!("Slot {}", index + 1), 16.));
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
