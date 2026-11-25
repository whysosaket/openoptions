mod battery;
mod focus;
pub use battery::{Battery, battery};
mod hosts;
mod profiles;
pub use hosts::{Host, Hosts, host_channels, rename_host, switch_host};
pub use profiles::{ApplicationProfile, Layer, Profiles, RING_SLOTS, Ring};
mod input;
mod pointer;
pub use pointer::{Dpi, Pointer, PointerChange, Wheel, apply_pointer, pointer};
mod protocol;

use crate::devices::Device;
use crate::migration::{Action, Binding, Button, Direction};
use enigo::Mouse;
use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub type Bindings = BTreeMap<u16, Binding>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Control {
    pub id: u16,
    pub mouse: bool,
    pub raw_motion: bool,
}

impl Control {
    pub fn name(self) -> String {
        match self.id {
            0x52 => Button::Middle.name(),
            0x53 => Button::Back.name(),
            0x56 => Button::Forward.name(),
            0xC3 | 0xD0 => Button::Gesture.name(),
            0xC4 => Button::ModeShift.name(),
            0xBF => "Screenshot",
            0xC2 => "Lock screen",
            0xC5 => "Microphone",
            0xC7 => "Brightness down",
            0xC8 => "Brightness up",
            0xE0 => "Window overview",
            0xE1 => "App launcher",
            0xE2 => "Backlight down",
            0xE3 => "Backlight up",
            0xE4 => "Previous track",
            0xE5 => "Play or pause",
            0xE6 => "Next track",
            0xE7 => "Mute",
            0xE8 => "Volume down",
            0xE9 => "Volume up",
            0xED => "DPI button",
            _ if self.mouse => return format!("Button {:04X}", self.id),
            _ => return format!("Key {:04X}", self.id),
        }
        .into()
    }
}

pub fn imported_control(button: Button, available: &[Control]) -> Option<u16> {
    let ids: &[u16] = match button {
        Button::Middle => &[0x52],
        Button::Back => &[0x53],
        Button::Forward => &[0x56],
        Button::Gesture => &[0xC3, 0xD0],
        Button::ModeShift => &[0xC4],
    };
    ids.iter().copied().find(|id| {
        available
            .iter()
            .any(|control| control.id == *id && control.mouse)
    })
}

pub fn discover(device: &Device) -> Result<Vec<Control>, String> {
    if !device.supports_hidpp() {
        return Ok(Vec::new());
    }
    Ok(protocol::Controls::open(device)?.available)
}

pub(crate) fn read_direct_unit(device: &Device) -> Result<Option<[u8; 4]>, String> {
    protocol::Controls::connect(device)?.unit_id()
}

pub fn supports(action: &Action) -> bool {
    input::supports(action)
}

pub fn live_platform_available() -> bool {
    input::live_platform_available()
}

pub enum Status {
    Active,
    ProfileChanged(Option<String>),
    ActionFailed(String),
    Ring {
        actions: Box<Ring>,
        pointer: (i32, i32),
    },
    Finished(Result<(), String>),
}

pub struct Session {
    stop: Arc<AtomicBool>,
    status: Receiver<Status>,
    ring_commands: mpsc::Sender<RingCommand>,
    worker: Option<JoinHandle<()>>,
}

enum RingCommand {
    Choose(usize),
    Dismiss { restore_focus: bool },
}

impl Session {
    pub fn start(device: Device, profiles: Profiles) -> Self {
        let (send, status) = mpsc::channel();
        let (ring_commands, chosen) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = thread::spawn(move || {
            let result = run(&device, &profiles, &stopping, &send, &chosen);
            let _ = send.send(Status::Finished(result));
        });
        Self {
            stop,
            status,
            ring_commands,
            worker: Some(worker),
        }
    }

    pub fn choose_ring_slot(&self, index: usize) {
        let _ = self.ring_commands.send(RingCommand::Choose(index));
    }

    pub fn dismiss_ring(&self, restore_focus: bool) {
        let _ = self
            .ring_commands
            .send(RingCommand::Dismiss { restore_focus });
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn poll(&self) -> Option<Status> {
        match self.status.try_recv() {
            Ok(status) => Some(status),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Status::Finished(Err(
                "The input session ended unexpectedly.".into(),
            ))),
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Default)]
struct Motion {
    x: i32,
    y: i32,
}

impl Motion {
    fn update(&mut self, x: i32, y: i32) {
        self.x = self.x.saturating_add(x);
        self.y = self.y.saturating_add(y);
    }

    fn direction(&self, threshold: u32) -> Direction {
        let horizontal = self.x.unsigned_abs();
        let vertical = self.y.unsigned_abs();
        if horizontal.max(vertical) < threshold {
            Direction::Click
        } else if horizontal > vertical {
            if self.x < 0 {
                Direction::Left
            } else {
                Direction::Right
            }
        } else if self.y < 0 {
            Direction::Up
        } else {
            Direction::Down
        }
    }
}

struct Press {
    motion: Motion,
    origin: Option<(i32, i32)>,
}

struct OpenRing {
    actions: Ring,
    target: Option<focus::Target>,
}

fn run(
    device: &Device,
    profiles: &Profiles,
    stop: &AtomicBool,
    status: &mpsc::Sender<Status>,
    chosen: &Receiver<RingCommand>,
) -> Result<(), String> {
    device.require_verified_identity()?;
    let mut hardware = protocol::Controls::open(device)?;
    if !profiles.has_bindings() {
        return Err("Assign an action before enabling controls.".into());
    }
    let mut input = input::connect()?;
    for layer in std::iter::once(&profiles.default)
        .chain(profiles.applications.values().map(|profile| &profile.layer))
    {
        for (&id, binding) in &layer.bindings {
            let control = hardware
                .available
                .iter()
                .find(|control| control.id == id)
                .ok_or_else(|| format!("Control {id:04X} is unavailable on this connection."))?;
            match binding {
                Binding::Click(action) if !supports(action) => {
                    return Err(format!("Choose a supported action for {}.", control.name()));
                }
                Binding::Gesture(actions) => {
                    if !control.mouse || actions.values().any(|action| !supports(action)) {
                        return Err(format!("Check the gesture actions for {}.", control.name()));
                    }
                    if !control.raw_motion {
                        input
                            .location()
                            .map_err(|error| format!("Could not read pointer position. {error}"))?;
                    }
                }
                _ => {}
            }
        }
    }
    let mut effective = Bindings::new();
    let mut open_ring = None::<OpenRing>;
    let mut planned = Vec::new();
    let mut app = None;
    let mut next_check = Instant::now();
    let mut ready = false;
    let result = (|| {
        let mut pressed = BTreeMap::<u16, Press>::new();
        while !stop.load(Ordering::Relaxed) {
            if let Ok(command) = chosen.try_recv()
                && let Some(ring) = open_ring.take()
            {
                match command {
                    RingCommand::Dismiss { restore_focus } => {
                        if restore_focus {
                            let _ = ring.target.is_some_and(focus::Target::restore);
                        }
                    }
                    RingCommand::Choose(index) => {
                        if let Some(action) = ring.actions.get(index).and_then(Option::as_ref) {
                            let needs_focus = !matches!(
                                action,
                                Action::SwitchHost(_)
                                    | Action::OpenApplication(_)
                                    | Action::Named(crate::migration::NamedAction::NoAction)
                            );
                            if needs_focus && !ring.target.is_some_and(focus::Target::restore) {
                                let _ = status.send(Status::ActionFailed(
                                    "Could not return to the application that opened the Actions Ring.".into(),
                                ));
                            } else if dispatch(
                                &mut hardware,
                                &mut input,
                                status,
                                action,
                                &ring.actions,
                                &mut open_ring,
                            )? {
                                return Ok(());
                            }
                        }
                    }
                }
            }
            if open_ring.is_none() && pressed.is_empty() && Instant::now() >= next_check {
                next_check = Instant::now() + Duration::from_millis(160);
                let focus = if profiles.applications.is_empty() {
                    None
                } else {
                    profiles::frontmost()
                };
                let focus = focus.filter(|key| profiles.applications.contains_key(key));
                if !ready || focus != app {
                    let next = profiles.effective_bindings(focus.as_deref());
                    planned = arm_layer(&mut hardware, &effective, &next, stop)?;
                    effective = next;
                    app = focus;
                    let _ = status.send(Status::ProfileChanged(app.clone()));
                }
                if !ready {
                    ready = true;
                    let _ = status.send(Status::Active);
                }
            }

            match hardware.event()? {
                Some(protocol::Event::Buttons(ids)) => {
                    for (control, gesture) in &planned {
                        let id = control.id;
                        if ids.contains(&id) {
                            if let std::collections::btree_map::Entry::Vacant(entry) =
                                pressed.entry(id)
                            {
                                let origin = if *gesture && !control.raw_motion {
                                    Some(input.location().map_err(|error| error.to_string())?)
                                } else {
                                    None
                                };
                                entry.insert(Press {
                                    motion: Motion::default(),
                                    origin,
                                });
                            }
                        } else if let Some(press) = pressed.remove(&id) {
                            let direction = if let Some((start_x, start_y)) = press.origin {
                                let (x, y) = input.location().map_err(|error| error.to_string())?;
                                Motion {
                                    x: x.saturating_sub(start_x),
                                    y: y.saturating_sub(start_y),
                                }
                                .direction(20)
                            } else {
                                press.motion.direction(75)
                            };
                            let action = match &effective[&id] {
                                Binding::Click(action) => Some(action),
                                Binding::Gesture(actions) => actions.get(&direction),
                            };
                            if let Some(action) = action
                                && dispatch(
                                    &mut hardware,
                                    &mut input,
                                    status,
                                    action,
                                    &profiles.layer(app.as_deref()).ring,
                                    &mut open_ring,
                                )?
                            {
                                return Ok(());
                            }
                        }
                    }
                }
                Some(protocol::Event::Motion(x, y)) => {
                    // Raw motion carries no control ID. Attribute it only to an unambiguous hold.
                    let mut gestures = planned.iter().filter(|(control, gesture)| {
                        *gesture && control.raw_motion && pressed.contains_key(&control.id)
                    });
                    if let Some((control, _)) = gestures.next()
                        && gestures.next().is_none()
                        && let Some(press) = pressed.get_mut(&control.id)
                    {
                        press.motion.update(i32::from(x), i32::from(y));
                    }
                }
                Some(protocol::Event::ReportingChanged) => {
                    return Err("Another app changed reporting on this device. Close the other app before enabling controls again.".into());
                }
                None => {}
            }
        }
        Ok(())
    })();
    let restored = hardware.restore();
    match (result, restored) {
        (Err(error), Err(restore)) => Err(format!("{error} {restore}")),
        (result, restored) => result.and(restored),
    }
}

fn arm_layer(
    hardware: &mut protocol::Controls,
    previous: &Bindings,
    next: &Bindings,
    stop: &AtomicBool,
) -> Result<Vec<(Control, bool)>, String> {
    let gesture = |binding: Option<&Binding>| matches!(binding, Some(Binding::Gesture(_)));
    for (&id, binding) in previous {
        if !next.contains_key(&id) || gesture(next.get(&id)) != gesture(Some(binding)) {
            hardware.restore_control(id)?;
        }
    }
    let mut armed = Vec::new();
    for control in hardware.available.clone() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if let Some(binding) = next.get(&control.id) {
            let is_gesture = gesture(Some(binding));
            let before = previous.get(&control.id);
            if before.is_none() || gesture(before) != is_gesture {
                hardware.divert(control, is_gesture)?;
            }
            armed.push((control, is_gesture));
        }
    }
    Ok(armed)
}

fn dispatch(
    hardware: &mut protocol::Controls,
    input: &mut enigo::Enigo,
    status: &mpsc::Sender<Status>,
    action: &Action,
    ring: &Ring,
    open_ring: &mut Option<OpenRing>,
) -> Result<bool, String> {
    match action {
        Action::ShowActionsRing => {
            if open_ring.is_none() && ring.iter().any(Option::is_some) {
                let pointer = input.location().map_err(|error| error.to_string())?;
                *open_ring = Some(OpenRing {
                    actions: ring.clone(),
                    target: focus::current(),
                });
                let _ = status.send(Status::Ring {
                    actions: Box::new(ring.clone()),
                    pointer,
                });
            }
        }
        Action::SwitchHost(host) => {
            hardware.restore()?;
            hosts::switch_on_channel(hardware, *host)?;
            return Ok(true);
        }
        action => input::perform(input, action)?,
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gestures_require_motion_and_use_the_dominant_axis() {
        for (x, y, direction) in [
            (19, -19, Direction::Click),
            (-20, 3, Direction::Left),
            (32, -21, Direction::Right),
            (21, -32, Direction::Up),
            (0, 20, Direction::Down),
        ] {
            let mut motion = Motion::default();
            motion.update(x, y);
            assert_eq!(motion.direction(20), direction);
        }
    }
}
