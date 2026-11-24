use super::controls::card;
use super::{OpenOptions, theme::*};
use eframe::egui::{self, Button, Context, Id, RichText};
use openoptions::{
    devices::{self, Device, DeviceKey},
    pairing::{self, Passkey, ReceiverDevice},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct Wizard {
    pub receivers: Option<Result<Vec<ReceiverDevice>, String>>,
    receiver_scan: Option<Receiver<Result<Vec<ReceiverDevice>, String>>>,
    connection: Option<ReceiverDevice>,
    session: Option<pairing::Pairing>,
    candidates: BTreeMap<u16, String>,
    searching: bool,
    selected: Option<String>,
    passkey: Option<Passkey>,
    paired: Option<u8>,
    cancelling: bool,
    error: Option<String>,
    bluetooth: Option<BluetoothPairing>,
}

struct BluetoothPairing {
    starting_devices: BTreeSet<DeviceKey>,
    found: Option<String>,
    next_scan: Instant,
    deadline: Instant,
}

impl Wizard {
    fn poll(&mut self, context: &Context) -> bool {
        if let Some(receiver_scan) = &self.receiver_scan {
            match receiver_scan.try_recv() {
                Ok(result) => {
                    self.receivers = Some(result);
                    self.receiver_scan = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receivers = Some(Err("Receiver discovery stopped unexpectedly.".into()));
                    self.receiver_scan = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        while let Some(event) = self.session.as_ref().and_then(pairing::Pairing::poll) {
            if self.accept(event) {
                return true;
            }
        }
        if self.session.is_some() {
            context.request_repaint_after(Duration::from_millis(85));
        }
        false
    }

    fn accept(&mut self, event: pairing::Event) -> bool {
        if let pairing::Event::Finished(result) = event {
            self.session = None;
            self.error = result.err();
            if std::mem::take(&mut self.cancelling) {
                self.connection = None;
                self.paired = None;
                self.candidates.clear();
                self.selected = None;
                self.passkey = None;
            }
            return self.paired.is_some();
        }
        if self.cancelling {
            return false;
        }
        match event {
            pairing::Event::Searching => self.searching = true,
            pairing::Event::Found { id, name } => {
                self.candidates.insert(id, name);
            }
            pairing::Event::Confirm(passkey) => self.passkey = Some(passkey),
            pairing::Event::Paired(slot) => self.paired = Some(slot),
            pairing::Event::Finished(_) => unreachable!(),
        }
        false
    }

    fn start(&mut self, receiver: ReceiverDevice, context: &Context) {
        self.searching = false;
        self.selected = None;
        self.candidates.clear();
        self.passkey = None;
        self.error = None;
        self.paired = None;
        self.session = Some(pairing::Pairing::start(receiver.clone()));
        self.connection = Some(receiver);
        context.request_repaint_after(Duration::from_millis(85));
    }

    fn cancel(&mut self) -> bool {
        if let Some(session) = &self.session {
            self.cancelling = true;
            self.paired = None;
            session.cancel();
            false
        } else {
            true
        }
    }

    fn check_bluetooth(
        &mut self,
        context: &Context,
        devices: &Result<Vec<Device>, String>,
        scanning: bool,
    ) -> bool {
        let Some(search) = &mut self.bluetooth else {
            return false;
        };
        if search.found.is_some() {
            return false;
        }
        if let Ok(devices) = devices
            && let Some(device) = devices.iter().find(|device| {
                device.connected
                    && device.connection == "Bluetooth"
                    && !search.starting_devices.contains(&device.key())
            })
        {
            search.found = Some(device.name.clone());
            return false;
        }
        let now = Instant::now();
        if now >= search.deadline {
            self.bluetooth = None;
            self.error = Some("No new Bluetooth device appeared. Finish pairing in system settings and try again.".into());
            return false;
        }
        context.request_repaint_after(Duration::from_secs(1));
        if scanning || now < search.next_scan {
            return false;
        }
        search.next_scan = now + Duration::from_secs(3);
        true
    }

    fn show(&mut self, context: &Context, inventory: &Result<Vec<Device>, String>) -> bool {
        let response = egui::Modal::new(Id::new("add-device"))
            .frame(card().fill(WINDOW).inner_margin(24))
            .show(context, |ui| {
            let width = (context.content_rect().width() - 64.).clamp(260., 470.);
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 12.;
            ui.label(title("Add Device", 23.));
            if let Some(bluetooth) = &self.bluetooth {
                if let Some(name) = &bluetooth.found {
                    ui.label(title("Device connected", 17.));
                    ui.label(name);
                    if ui.add(primary("Done")).clicked() {
                        ui.close();
                    }
                } else {
                    ui.spinner();
                    ui.label(muted("Finish pairing in system Bluetooth settings. OpenOptions will look for the new device here."));
                    if ui.button("Cancel").clicked() {
                        ui.close();
                    }
                }
                return;
            }
            if let Some(slot) = self.paired {
                ui.label(title("Device paired", 17.));
                ui.label(muted(format!("Connected to receiver slot {slot}.")));
                if let Some(error) = &self.error {
                    ui.label(RichText::new(error).color(WARNING));
                }
                if ui.add_enabled(self.session.is_none(), primary("Done")).clicked() {
                    ui.close();
                }
                return;
            }
            if self.cancelling {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Closing the pairing window…");
                });
                return;
            }
            if let Some(error) = &self.error {
                ui.label(RichText::new(error).color(WARNING));
            }
            if let Some(receiver) = &self.connection {
                ui.label(title(&receiver.name, 16.));
                if let Some(passkey) = &self.passkey {
                    ui.label(title("Confirm pairing", 16.));
                    match passkey {
                        Passkey::Keyboard(digits) => {
                            ui.label(muted("Type this code on the new keyboard, then press Enter."));
                            ui.label(RichText::new(digits).monospace().size(25.));
                        }
                        Passkey::Mouse(clicks) => {
                            ui.label(muted("Click the buttons in order on the new mouse. Then press left and right together."));
                            ui.horizontal_wrapped(|ui| {
                                for (index, right) in clicks.iter().enumerate() {
                                    ui.label(format!("{} {}", index + 1, if *right { "R" } else { "L" }));
                                }
                            });
                        }
                    }
                } else if let Some(name) = &self.selected {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("Pairing with {name}…"));
                    });
                } else if self.session.is_some() {
                    ui.label(muted("Select an Easy-Switch channel on the device and hold its pairing button until the light blinks."));
                    if !self.searching || self.candidates.is_empty() {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(if self.searching { "Searching for devices…" } else { "Opening the pairing window…" });
                        });
                    } else {
                        for (&id, name) in &self.candidates {
                            if ui.add_sized([ui.available_width(), 42.], Button::new(name)).clicked()
                                && let Some(session) = &self.session
                            {
                                self.selected = Some(name.clone());
                                session.pair(id);
                            }
                        }
                    }
                }
                if self.session.is_none() && ui.button("Choose another receiver").clicked() {
                    self.connection = None;
                    self.error = None;
                }
            } else {
                ui.label(title("Choose a receiver", 16.));
                ui.label(muted("Put the device in pairing mode, then choose its Bolt or Unifying receiver to start searching."));
                match self.receivers.as_ref() {
                    None => { ui.spinner(); }
                    Some(Err(error)) => { ui.label(RichText::new(error).color(WARNING)); }
                    Some(Ok(receivers)) => {
                        let total = receivers.len();
                        let mut selected = None;
                        for (index, receiver) in receivers.iter().enumerate() {
                            let name = if total > 1 { format!("{} · {}", receiver.name, index + 1) } else { receiver.name.clone() };
                            if ui.add_sized([ui.available_width(), 42.], Button::new(name)).clicked() {
                                selected = Some(receiver.clone());
                            }
                        }
                        if let Some(receiver) = selected {
                            self.start(receiver, context);
                        }
                        if total == 0 {
                            ui.label(muted("No Bolt or Unifying receiver found. Plug in a receiver or use Bluetooth."));
                        }
                    }
                }
                ui.separator();
                if ui.button("Use Bluetooth settings").clicked() {
                    match inventory {
                        Ok(devices) => match devices::open_bluetooth_settings() {
                            Ok(()) => {
                                let now = Instant::now();
                                self.error = None;
                                self.bluetooth = Some(BluetoothPairing {
                                    starting_devices: devices.iter()
                                        .filter(|device| device.connected)
                                        .map(Device::key)
                                        .collect(),
                                    found: None,
                                    next_scan: now,
                                    deadline: now + Duration::from_secs(120),
                                });
                                context.request_repaint();
                            }
                            Err(error) => self.error = Some(error),
                        },
                        Err(_) => {
                            self.error = Some("Refresh the Devices list before starting Bluetooth pairing.".into());
                        }
                    }
                }
            }
            let label = if self.session.is_some() { "Cancel pairing" } else { "Cancel" };
            if ui.button(label).clicked() {
                ui.close();
            }
        });
        response.should_close() && self.cancel()
    }
}

impl OpenOptions {
    pub(super) fn open_pairing(&mut self, context: &Context) {
        let (send, receiver_scan) = mpsc::channel();
        self.pairing = Some(Wizard {
            receiver_scan: Some(receiver_scan),
            ..Wizard::default()
        });
        let context = context.clone();
        std::thread::spawn(move || {
            let _ = send.send(pairing::receivers());
            context.request_repaint();
        });
    }

    pub(super) fn show_pairing(&mut self, context: &Context) {
        let Some(wizard) = &mut self.pairing else {
            return;
        };
        let refresh =
            wizard.poll(context) | wizard.check_bluetooth(context, &self.devices, self.scanning);
        if wizard.show(context, &self.devices) {
            self.pairing = None;
        }
        if refresh {
            self.scan(context);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_ignores_late_receiver_results_and_waits_for_cleanup() {
        let mut wizard = Wizard {
            cancelling: true,
            ..Wizard::default()
        };
        assert!(!wizard.accept(pairing::Event::Found {
            id: 3,
            name: "Late device".into(),
        }));
        assert!(!wizard.accept(pairing::Event::Paired(2)));
        assert!(wizard.cancelling);
        assert!(wizard.candidates.is_empty());
        assert!(wizard.paired.is_none());
        assert!(!wizard.accept(pairing::Event::Finished(Ok(()))));
        assert!(!wizard.cancelling);
        assert!(!wizard.accept(pairing::Event::Paired(1)));
        assert!(wizard.accept(pairing::Event::Finished(Ok(()))));
        assert_eq!(wizard.paired, Some(1));
    }
}
