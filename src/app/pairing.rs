use super::{OpenOptions, theme::*, workspace};
use eframe::egui::{self, Button, Context, Id, RichText, ScrollArea, Ui};
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
    receivers: Option<Result<Vec<ReceiverDevice>, String>>,
    receiver_scan: Option<Receiver<Result<Vec<ReceiverDevice>, String>>>,
    connection: Option<ReceiverDevice>,
    session: Option<pairing::Pairing>,
    candidates: BTreeMap<u16, String>,
    searching: bool,
    selected: Option<String>,
    passkey: Option<Passkey>,
    paired: Option<PairedDevice>,
    cancelling: bool,
    error: Option<String>,
    bluetooth: Option<BluetoothPairing>,
}

struct PairedDevice {
    slot: u8,
    name: Option<String>,
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
            pairing::Event::Paired { slot, name } => {
                self.paired = Some(PairedDevice {
                    slot,
                    name: name.or_else(|| self.selected.clone()),
                });
            }
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

    fn scan_receivers(&mut self, context: &Context) {
        self.receivers = None;
        let (send, receiver_scan) = mpsc::channel();
        self.receiver_scan = Some(receiver_scan);
        let context = context.clone();
        std::thread::spawn(move || {
            let _ = send.send(pairing::receivers());
            context.request_repaint();
        });
    }

    fn start_bluetooth(&mut self, context: &Context, inventory: &Result<Vec<Device>, String>) {
        let Ok(devices) = inventory else {
            self.error = Some("Refresh Devices before starting Bluetooth pairing.".into());
            return;
        };
        if let Err(error) = devices::open_bluetooth_settings() {
            self.error = Some(error);
            return;
        }
        let now = Instant::now();
        self.error = None;
        self.bluetooth = Some(BluetoothPairing {
            starting_devices: devices
                .iter()
                .filter(|device| device.connected)
                .map(Device::key)
                .collect(),
            found: None,
            next_scan: now,
            deadline: now + Duration::from_secs(120),
        });
        context.request_repaint();
    }

    fn show(
        &mut self,
        context: &Context,
        inventory: &Result<Vec<Device>, String>,
        scanning: bool,
    ) -> bool {
        let size = context.content_rect().size();
        let response = egui::Modal::new(Id::new("add-device"))
            .frame(surface(22))
            .show(context, |ui| {
                ui.set_width((size.x - 64.).clamp(250., 488.));
                ui.spacing_mut().item_spacing.y = 12.;
                ui.label(title("Add Device", 23.));
                workspace::divider(ui);
                ScrollArea::vertical()
                    .id_salt("pairing-content")
                    .max_height((size.y - 175.).max(110.))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        if self.cancelling {
                            progress(ui, "Closing the receiver pairing window…");
                        } else if let Some(search) = &self.bluetooth {
                            if let Some(name) = &search.found {
                                self.success(ui, name, "Connected over Bluetooth");
                            } else {
                                ui.label(title("Waiting for Bluetooth", 17.));
                                ui.label(muted("Finish pairing in system Bluetooth settings. OpenOptions will look for the new device here."));
                                progress(ui, "Checking connected devices…");
                            }
                        } else if let Some(paired) = &self.paired {
                            let name = paired.name.as_deref().unwrap_or("New Logitech device");
                            let receiver = self.connection.as_ref().map_or("receiver", |receiver| receiver.name.as_str());
                            self.success(ui, name, &format!("{receiver} · Slot {}", paired.slot));
                        } else if self.connection.is_some() {
                            self.receiver_steps(ui, context);
                        } else {
                            self.methods(ui, context, inventory, scanning);
                        }
                        if let Some(error) = &self.error {
                            ui.label(RichText::new(error).color(WARNING));
                        }
                    });
                if !self.cancelling {
                    workspace::divider(ui);
                    let done = self.paired.is_some() || self.bluetooth.as_ref().is_some_and(|search| search.found.is_some());
                    workspace::trailing(ui, |ui| {
                        if done {
                            if ui.add_enabled(self.session.is_none(), primary("Done")).clicked() {
                                ui.close();
                            }
                        } else if ui.button(if self.session.is_some() { "Cancel pairing" } else { "Cancel" }).clicked() {
                            ui.close();
                        }
                    });
                }
            });
        response.should_close() && !self.cancelling && self.cancel()
    }

    fn success(&self, ui: &mut Ui, name: &str, connection: &str) {
        workspace::badge(ui, "Device connected", true);
        ui.label(title(name, 19.));
        ui.label(muted(connection));
    }

    fn methods(
        &mut self,
        ui: &mut Ui,
        context: &Context,
        inventory: &Result<Vec<Device>, String>,
        scanning: bool,
    ) {
        ui.label(title("Choose a connection", 17.));
        surface(10).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(title("Bluetooth", 14.));
                    ui.label(muted("Pair in system settings").size(12.));
                });
                workspace::trailing(ui, |ui| {
                    if ui
                        .add_enabled(!scanning, Button::new("Open Bluetooth settings"))
                        .clicked()
                    {
                        self.start_bluetooth(context, inventory);
                    }
                });
            });
        });
        let mut chosen = None;
        for (bolt, title_text) in [(true, "Logi Bolt receiver"), (false, "Unifying receiver")] {
            surface(10).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(title(title_text, 14.));
                    workspace::trailing(ui, |ui| match &self.receivers {
                        None => {
                            ui.spinner();
                            ui.label(muted("Checking…").size(12.));
                        }
                        Some(Err(_)) => {
                            ui.label(RichText::new("Scan failed").color(WARNING).size(12.));
                        }
                        Some(Ok(receivers)) => {
                            let count = receivers
                                .iter()
                                .filter(|receiver| receiver.bolt == bolt)
                                .count();
                            workspace::badge(
                                ui,
                                if count > 0 {
                                    "Available"
                                } else {
                                    "Not detected"
                                },
                                count > 0,
                            );
                        }
                    });
                });
                if let Some(Ok(receivers)) = &self.receivers {
                    for (index, receiver) in receivers
                        .iter()
                        .filter(|receiver| receiver.bolt == bolt)
                        .enumerate()
                    {
                        let label = format!(
                            "Use {} {}",
                            if bolt { "Bolt" } else { "Unifying" },
                            index + 1
                        );
                        if ui.button(label).clicked() {
                            chosen = Some(receiver.clone());
                        }
                    }
                }
            });
        }
        if let Some(receiver) = chosen {
            self.connection = Some(receiver);
            self.error = None;
        }
        if let Some(result) = &self.receivers {
            if let Err(error) = result {
                ui.label(RichText::new(error).color(WARNING));
            } else if result.as_ref().is_ok_and(Vec::is_empty) {
                ui.label(muted(
                    "Plug in a Bolt or Unifying receiver, then check again.",
                ));
            }
            if labeled_button(ui, "Check receivers again", Icon::Refresh, true).clicked() {
                self.scan_receivers(context);
            }
        }
    }

    fn receiver_steps(&mut self, ui: &mut Ui, context: &Context) {
        let receiver = self
            .connection
            .as_ref()
            .expect("receiver step has a device")
            .clone();
        ui.label(muted(&receiver.name).size(12.));
        if self.session.is_none() {
            ui.label(title(
                if self.error.is_some() {
                    "Pairing stopped"
                } else {
                    "Prepare your device"
                },
                17.,
            ));
            pairing_instructions(ui, receiver.bolt);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add(primary(if self.error.is_some() {
                        "Try again"
                    } else {
                        "Start pairing"
                    }))
                    .clicked()
                {
                    self.start(receiver, context);
                }
                if ui.button("Change connection").clicked() {
                    self.connection = None;
                    self.error = None;
                    self.scan_receivers(context);
                }
            });
            return;
        }
        if let Some(passkey) = &self.passkey {
            ui.label(title("Verify your device", 17.));
            if let Some(device) = &self.selected {
                ui.label(title(device, 14.));
            }
            match passkey {
                Passkey::Keyboard(digits) => {
                    ui.label(muted(
                        "Enter this code on the new keyboard, then press Enter on that keyboard.",
                    ));
                    surface(12).show(ui, |ui| {
                        ui.label(RichText::new(digits).monospace().size(25.));
                    });
                }
                Passkey::Mouse(clicks) => {
                    ui.label(muted("Click these buttons in order on the new mouse."));
                    surface(10).show(ui, |ui| {
                        for (row, steps) in clicks.chunks(5).enumerate() {
                            ui.horizontal_wrapped(|ui| {
                                for (index, right) in steps.iter().enumerate() {
                                    workspace::badge(
                                        ui,
                                        &format!(
                                            "{}  {}",
                                            row * 5 + index + 1,
                                            if *right { "Right" } else { "Left" }
                                        ),
                                        false,
                                    );
                                }
                            });
                        }
                    });
                    ui.label(muted("Finish by pressing left and right together."));
                }
            }
        } else if let Some(device) = &self.selected {
            progress(ui, &format!("Pairing {device}…"));
        } else if self.candidates.is_empty() {
            ui.label(title("Find your device", 17.));
            pairing_instructions(ui, receiver.bolt);
            progress(
                ui,
                if self.searching {
                    "Searching for devices…"
                } else {
                    "Opening the receiver pairing window…"
                },
            );
        } else {
            ui.label(title("Choose the device to pair", 17.));
            for (&id, name) in &self.candidates {
                if ui
                    .add_sized([ui.available_width(), 36.], Button::new(name))
                    .clicked()
                    && let Some(session) = &self.session
                {
                    self.selected = Some(name.clone());
                    session.pair(id);
                }
            }
        }
    }
}

fn progress(ui: &mut Ui, label: &str) {
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label(muted(label));
    });
}

fn pairing_instructions(ui: &mut Ui, bolt: bool) {
    ui.label(muted(if bolt {
        "Select a channel on the device, then hold its pairing button until the light blinks."
    } else {
        "Put the device into pairing mode when the receiver starts searching. For devices without a pairing button, turn them off and back on."
    }));
}

impl OpenOptions {
    pub(super) fn open_pairing(&mut self, context: &Context) {
        let mut wizard = Wizard::default();
        wizard.scan_receivers(context);
        self.pairing = Some(wizard);
    }

    pub(super) fn show_pairing(&mut self, context: &Context) {
        let Some(wizard) = &mut self.pairing else {
            return;
        };
        let refresh =
            wizard.poll(context) | wizard.check_bluetooth(context, &self.devices, self.scanning);
        if wizard.show(context, &self.devices, self.scanning) {
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
        assert!(!wizard.accept(pairing::Event::Paired {
            slot: 2,
            name: Some("Late device".into()),
        }));
        assert!(wizard.cancelling);
        assert!(wizard.candidates.is_empty());
        assert!(wizard.paired.is_none());
        assert!(!wizard.accept(pairing::Event::Finished(Ok(()))));
        assert!(!wizard.cancelling);
        assert!(!wizard.accept(pairing::Event::Paired {
            slot: 1,
            name: Some("New mouse".into()),
        }));
        assert!(wizard.accept(pairing::Event::Finished(Ok(()))));
        let paired = wizard.paired.unwrap();
        assert_eq!(paired.slot, 1);
        assert_eq!(paired.name.as_deref(), Some("New mouse"));
    }
}
