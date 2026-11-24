use super::controls::card;
use super::{Event, OpenOptions, theme::*};
use eframe::egui::{self, Button, Context, Id, RichText};
use openoptions::{
    devices,
    pairing::{self, Passkey, ReceiverDevice},
};
use std::{collections::BTreeMap, time::Duration};

#[derive(Default)]
pub(super) struct Wizard {
    pub receivers: Option<Result<Vec<ReceiverDevice>, String>>,
    connection: Option<ReceiverDevice>,
    session: Option<pairing::Pairing>,
    candidates: BTreeMap<u16, String>,
    searching: bool,
    selected: Option<String>,
    passkey: Option<Passkey>,
    paired: Option<u8>,
    cancelling: bool,
    error: Option<String>,
}

impl Wizard {
    fn poll(&mut self, context: &Context) -> bool {
        let Some(session) = &self.session else {
            return false;
        };
        let mut paired = false;
        while let Some(event) = session.poll() {
            match event {
                pairing::Event::Searching => self.searching = true,
                pairing::Event::Found { id, name } => {
                    self.candidates.insert(id, name);
                }
                pairing::Event::Confirm(passkey) => self.passkey = Some(passkey),
                pairing::Event::Paired(slot) => {
                    self.paired = Some(slot);
                    paired = true;
                }
                pairing::Event::Finished(result) => {
                    self.session = None;
                    self.cancelling = false;
                    self.error = result.err();
                    if self.paired.is_none() && self.error.is_none() {
                        self.connection = None;
                        self.candidates.clear();
                        self.selected = None;
                        self.passkey = None;
                    }
                    return paired;
                }
            }
        }
        context.request_repaint_after(Duration::from_millis(85));
        paired
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
            session.cancel();
            false
        } else {
            true
        }
    }

    fn show(&mut self, context: &Context) -> bool {
        let response = egui::Modal::new(Id::new("add-device"))
            .frame(card().fill(WINDOW).inner_margin(24))
            .show(context, |ui| {
            let width = (context.content_rect().width() - 64.).clamp(260., 470.);
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 12.;
            ui.label(title("Add Device", 23.));
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
                    match devices::open_bluetooth_settings() {
                        Ok(()) => ui.close(),
                        Err(error) => self.error = Some(error),
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
        self.pairing = Some(Wizard::default());
        self.job(context, || Event::ReceiversLoaded(pairing::receivers()));
    }

    pub(super) fn show_pairing(&mut self, context: &Context) {
        let Some(wizard) = &mut self.pairing else {
            return;
        };
        let refresh = wizard.poll(context);
        if wizard.show(context) {
            self.pairing = None;
        }
        if refresh {
            self.scan(context);
        }
    }
}
