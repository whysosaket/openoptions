use super::{Notice, OpenOptions, theme::*, workspace};
use eframe::egui::{ScrollArea, Ui};
use openoptions::controls;

impl OpenOptions {
    pub(super) fn settings_ui(&mut self, ui: &mut Ui) {
        ScrollArea::vertical().show(ui, |ui| {
            if let Some(Notice::Error(error)) = &self.notice {
                ui.colored_label(WARNING, error);
            }
            #[cfg(target_os = "macos")]
            self.permissions(ui);

            surface(14).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(title("Live controls", 17.));
                let status = match self.live_controls.as_ref() {
                    Some(live) if live.stopping => "Disabling",
                    Some(live) if live.active => "Enabled",
                    Some(_) => "Enabling",
                    None => "Disabled",
                };
                ui.horizontal_wrapped(|ui| {
                    workspace::badge(ui, status, status == "Enabled");
                    if let Some(live) = &self.live_controls {
                        ui.label(&live.name);
                    }
                });
                if controls::live_platform_available() {
                    ui.label(muted("Enable saved assignments in a device workspace. OpenOptions applies them while it is running."));
                } else {
                    ui.label(muted("Linux live controls require an X11 session."));
                }
            });
            surface(14).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(title("About OpenOptions", 17.));
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.label(muted("Device assignments and imported profiles are stored on this computer."));
            });
        });
    }

    #[cfg(target_os = "macos")]
    fn permissions(&mut self, ui: &mut Ui) {
        surface(14).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(title("macOS permissions", 17.));
            for (name, description, granted, pane) in macos::permissions() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(title(name, 14.));
                    workspace::badge(
                        ui,
                        if granted { "Granted" } else { "Access needed" },
                        granted,
                    );
                    if ui.button(format!("Open {name} settings")).clicked() {
                        self.notice = macos::open_permissions(pane).err().map(Notice::Error);
                    }
                });
                ui.label(muted(description));
            }
            if labeled_button(
                ui,
                "Recheck and refresh devices",
                Icon::Refresh,
                !self.scanning,
            )
            .clicked()
            {
                self.notice = None;
                self.scan(ui.ctx());
            }
            ui.label(muted(
                "If Input Monitoring still fails after you grant it, reopen OpenOptions.",
            ));
        });
    }
}

#[cfg(target_os = "macos")]
mod macos {
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> u8;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightListenEventAccess() -> bool;
    }

    pub(super) fn permissions() -> [(&'static str, &'static str, bool, &'static str); 2] {
        // These non-prompting queries read the current process's access.
        let (accessibility, input_monitoring) =
            unsafe { (AXIsProcessTrusted() != 0, CGPreflightListenEventAccess()) };
        [
            (
                "Accessibility",
                "Sends shortcuts and other desktop actions you assign to your device.",
                accessibility,
                "Privacy_Accessibility",
            ),
            (
                "Input Monitoring",
                "Reads supported HID++ buttons and settings from your device.",
                input_monitoring,
                "Privacy_ListenEvent",
            ),
        ]
    }

    pub(super) fn open_permissions(pane: &str) -> Result<(), String> {
        let mut child = std::process::Command::new("/usr/bin/open")
            .arg(format!(
                "x-apple.systempreferences:com.apple.preference.security?{pane}"
            ))
            .spawn()
            .map_err(|error| format!("Couldn't open System Settings. {error}"))?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}
