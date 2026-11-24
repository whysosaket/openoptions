use hidapi::{BusType, HidApi};
use std::collections::BTreeMap;
use std::process::Command;
use std::sync::{OnceLock, mpsc};

const LOGITECH: u16 = 0x046D;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    pub product_id: u16,
    pub connection: &'static str,
    pub interfaces: usize,
}

// hidapi keeps one IOHIDManager attached to the initializing thread's run loop.
type HidJob = Box<dyn FnOnce() + Send>;
static HID_WORKER: OnceLock<mpsc::Sender<HidJob>> = OnceLock::new();

pub(crate) fn with_hid<T: Send + 'static>(
    operation: impl FnOnce(HidApi) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let worker = HID_WORKER.get_or_init(|| {
        let (send, receive) = mpsc::channel::<HidJob>();
        std::thread::spawn(move || {
            for job in receive {
                job();
            }
        });
        send
    });
    let (send, receive) = mpsc::sync_channel(1);
    worker
        .send(Box::new(move || {
            let result = HidApi::new()
                .map_err(|error| format!("Could not scan HID devices: {error}"))
                .and_then(operation);
            let _ = send.send(result);
        }))
        .map_err(|_| "The HID worker stopped.")?;
    receive.recv().map_err(|_| "The HID worker stopped.")?
}

pub fn connected() -> Result<Vec<Device>, String> {
    with_hid(|api| {
        let mut models = BTreeMap::new();
        for interface in api
            .device_list()
            .filter(|device| device.vendor_id() == LOGITECH)
        {
            let product_id = interface.product_id();
            let connection = match interface.bus_type() {
                BusType::Usb => "USB",
                BusType::Bluetooth => "Bluetooth",
                _ => "HID",
            };
            let name = match product_id {
                0xC548 => "Logi Bolt Receiver",
                0xC52B | 0xC532 => "Unifying Receiver",
                _ => interface
                    .product_string()
                    .filter(|name| !name.is_empty())
                    .unwrap_or("Logitech HID device"),
            };

            // List product groups. HID collections aren't physical device identities.
            let device = models
                .entry((connection, product_id))
                .or_insert_with(|| Device {
                    name: name.into(),
                    product_id,
                    connection,
                    interfaces: 0,
                });
            device.interfaces += 1;
        }
        Ok(models.into_values().collect())
    })
}

pub fn open_bluetooth_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("/usr/bin/open");
        command.arg("x-apple.systempreferences:com.apple.BluetoothSettings");
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer.exe");
        command.arg("ms-settings:bluetooth");
        command
    };
    #[cfg(target_os = "linux")]
    let mut command = {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .to_ascii_lowercase();
        if desktop.contains("gnome") {
            let mut command = Command::new("gnome-control-center");
            command.arg("bluetooth");
            command
        } else if desktop.contains("kde") {
            let mut command = Command::new("systemsettings");
            command.arg("kcm_bluetooth");
            command
        } else {
            Command::new("blueman-manager")
        }
    };
    let mut child = command
        .spawn()
        .map_err(|error| format!("Open Bluetooth from your system settings. {error}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
