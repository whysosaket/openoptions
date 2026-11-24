mod input;
mod pointer;
pub use pointer::{Pointer, PointerChange, Wheel, apply_pointer, pointer};
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
    Ok(protocol::Controls::open(device)?.available)
}

// The MX Master 3S has these physical controls on its Bluetooth HID++ interface.
// Live enable still checks the actual device before taking over any reporting.
pub fn known_controls(device: &Device) -> Vec<Control> {
    if device.connection == "Bluetooth" && matches!(device.product_id, 0xB034) {
        return [0x52, 0x53, 0x56, 0xC3, 0xC4]
            .map(|id| Control {
                id,
                mouse: true,
                raw_motion: false,
            })
            .to_vec();
    }
    Vec::new()
}

pub fn supports(action: &Action) -> bool {
    input::supports(action)
}

pub enum Status {
    Active,
    Finished(Result<(), String>),
}

pub struct Session {
    stop: Arc<AtomicBool>,
    status: Receiver<Status>,
    worker: Option<JoinHandle<()>>,
}

impl Session {
    pub fn start(device: Device, bindings: Bindings) -> Self {
        let (send, status) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = thread::spawn(move || {
            let result = run(&device, &bindings, &stopping, || {
                let _ = send.send(Status::Active);
            });
            let _ = send.send(Status::Finished(result));
        });
        Self {
            stop,
            status,
            worker: Some(worker),
        }
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

fn run(
    device: &Device,
    bindings: &Bindings,
    stop: &AtomicBool,
    active: impl FnOnce(),
) -> Result<(), String> {
    let mut hardware = protocol::Controls::open(device)?;
    if bindings.is_empty() {
        return Err("Assign an action before enabling controls.".into());
    }
    let mut planned = Vec::new();
    for (&id, binding) in bindings {
        let control = hardware
            .available
            .iter()
            .find(|control| control.id == id)
            .copied()
            .ok_or_else(|| format!("Control {id:04X} is unavailable on this connection."))?;
        match binding {
            Binding::Click(action) if !supports(action) => {
                return Err(format!(
                    "Choose an action supported on this system for {}.",
                    control.name()
                ));
            }
            Binding::Gesture(actions)
                if !control.mouse || actions.values().any(|action| !supports(action)) =>
            {
                return Err(format!("Check the gesture actions for {}.", control.name()));
            }
            _ => {}
        }
        hardware.check_available(control)?;
        planned.push((control, matches!(binding, Binding::Gesture(_))));
    }
    let mut input = input::connect()?;
    if planned
        .iter()
        .any(|(control, gesture)| *gesture && !control.raw_motion)
    {
        input
            .location()
            .map_err(|error| format!("Could not read pointer position. {error}"))?;
    }
    let result = (|| {
        for &(control, gesture) in &planned {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            hardware.divert(control, gesture)?;
        }
        active();
        let mut pressed = BTreeMap::<u16, Press>::new();
        while !stop.load(Ordering::Relaxed) {
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
                            let action = match &bindings[&id] {
                                Binding::Click(action) => Some(action),
                                Binding::Gesture(actions) => actions.get(&direction),
                            };
                            if let Some(action) = action {
                                input::perform(&mut input, action).map_err(|error| {
                                    format!(
                                        "Could not run the action for {}. {error}",
                                        control.name()
                                    )
                                })?;
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
