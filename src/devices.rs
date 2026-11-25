use crate::pairing::{self, ReceiverLink};
use hidapi::{BusType, DeviceInfo, HidApi, HidDevice};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::CString;
use std::process::Command;
use std::sync::{OnceLock, mpsc};

const LOGITECH: u16 = 0x046D;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Identity {
    Model,
    Serial(String),
    Interface(CString),
    HidppUnit([u8; 4]),
    ReceiverUnit([u8; 4]),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceKey {
    connection: &'static str,
    product: u16,
    identity: Identity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    pub product_id: u16,
    pub connection: &'static str,
    pub interfaces: usize,
    pub connected: bool,
    pub battery: Result<Option<crate::controls::Battery>, String>,
    pub(crate) receiver: Option<ReceiverLink>,
    identity: Identity,
    direct_path: Option<CString>,
}

impl Device {
    pub fn direct(
        name: String,
        product_id: u16,
        connection: &'static str,
        interfaces: usize,
    ) -> Self {
        Self {
            name,
            product_id,
            connection,
            interfaces,
            connected: true,
            battery: Ok(None),
            receiver: None,
            identity: Identity::Model,
            direct_path: None,
        }
    }

    pub(crate) fn paired(
        name: String,
        product_id: u16,
        connection: &'static str,
        connected: bool,
        link: ReceiverLink,
    ) -> Self {
        Self {
            connected,
            identity: Identity::ReceiverUnit(link.unit),
            receiver: Some(link),
            ..Self::direct(name, product_id, connection, 1)
        }
    }

    pub fn unit_id(&self) -> Option<[u8; 4]> {
        self.receiver.as_ref().map(|link| link.unit)
    }

    pub fn key(&self) -> DeviceKey {
        DeviceKey {
            connection: self.connection,
            product: self.product_id,
            identity: self.identity.clone(),
        }
    }

    pub fn supports_hidpp(&self) -> bool {
        self.direct_path.is_some() || self.receiver.is_some()
    }

    pub fn verifies_identity(&self) -> bool {
        matches!(
            self.identity,
            Identity::Serial(_) | Identity::HidppUnit(_) | Identity::ReceiverUnit(_)
        )
    }

    pub(crate) fn require_verified_identity(&self) -> Result<(), String> {
        if self.verifies_identity() {
            Ok(())
        } else {
            Err("This device did not supply a stable identity for changes.".into())
        }
    }

    pub(crate) fn direct_unit(&self) -> Option<[u8; 4]> {
        if let Identity::HidppUnit(unit) = self.identity {
            Some(unit)
        } else {
            None
        }
    }

    pub(crate) fn storage_connection(&self) -> std::borrow::Cow<'static, str> {
        match &self.identity {
            Identity::ReceiverUnit(unit) => {
                format!("{}:{:08X}", self.connection, u32::from_be_bytes(*unit)).into()
            }
            Identity::Serial(serial) => format!("{}:serial:{serial}", self.connection).into(),
            Identity::HidppUnit(unit) => {
                format!("{}:unit:{:08X}", self.connection, u32::from_be_bytes(*unit)).into()
            }
            Identity::Interface(_) | Identity::Model => self.connection.into(),
        }
    }

    pub(crate) fn legacy_storage_connection(&self) -> Option<&'static str> {
        matches!(self.identity, Identity::Serial(_) | Identity::HidppUnit(_))
            .then_some(self.connection)
    }

    pub(crate) fn open_direct(&self, api: HidApi) -> Result<HidDevice, String> {
        let paths = api
            .device_list()
            .filter(|info| {
                info.vendor_id() == LOGITECH
                    && info.product_id() == self.product_id
                    && connection(info) == self.connection
                    && is_hidpp(info)
                    && match &self.identity {
                        Identity::Serial(serial) => info.serial_number() == Some(serial),
                        Identity::Interface(_) | Identity::HidppUnit(_) => {
                            info.serial_number().is_none_or(str::is_empty)
                                && self.direct_path.as_deref() == Some(info.path())
                        }
                        _ => false,
                    }
            })
            .map(DeviceInfo::path)
            .collect::<BTreeSet<_>>();
        let path = match paths.len() {
            0 => return Err("The selected device is no longer connected.".into()),
            1 => *paths.first().unwrap(),
            _ => return Err("More than one HID++ interface reports this device identity.".into()),
        };
        api.open_path(path).map_err(|error| {
            if cfg!(target_os = "macos") && error.to_string().contains("0xE00002E2") {
                return "Allow OpenOptions in System Settings → Privacy & Security → Input Monitoring, then reopen OpenOptions.".into();
            }
            format!("Could not open the controls interface. {error}")
        })
    }
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
    let mut devices = with_hid(|api| {
        let mut direct = BTreeMap::new();
        let mut anonymous = BTreeMap::new();
        for interface in api
            .device_list()
            .filter(|device| device.vendor_id() == LOGITECH)
        {
            let product_id = interface.product_id();
            let connection = connection(interface);
            let name = match product_id {
                0xC548 => "Logi Bolt Receiver",
                0xC52B | 0xC532 => "Unifying Receiver",
                _ => interface
                    .product_string()
                    .filter(|name| !name.is_empty())
                    .unwrap_or("Logitech HID device"),
            };

            let hidpp = !is_receiver(product_id) && is_hidpp(interface);
            let identity = match interface
                .serial_number()
                .filter(|serial| !serial.is_empty())
            {
                Some(serial) => Identity::Serial(serial.into()),
                None if hidpp => Identity::Interface(interface.path().into()),
                None => Identity::Model,
            };
            let devices = if identity == Identity::Model {
                &mut anonymous
            } else {
                &mut direct
            };
            let mut device = Device::direct(name.into(), product_id, connection, 0);
            device.identity = identity;
            let device = devices.entry(device.key()).or_insert(device);
            device.interfaces += 1;
            if hidpp {
                device.direct_path = Some(interface.path().into());
            }
        }
        let models: BTreeSet<_> = direct
            .values()
            .map(|device| (device.connection, device.product_id))
            .collect();
        Ok(direct
            .into_values()
            .chain(
                anonymous
                    .into_values()
                    .filter(|device| !models.contains(&(device.connection, device.product_id))),
            )
            .collect::<Vec<_>>())
    })?;
    if devices.iter().any(|device| is_receiver(device.product_id)) {
        devices.extend(pairing::connected_peripherals()?);
    }
    for device in &mut devices {
        if device.connected && device.supports_hidpp() {
            if matches!(device.identity, Identity::Interface(_))
                && let Ok(Some(unit)) = crate::controls::read_direct_unit(device)
            {
                device.identity = Identity::HidppUnit(unit);
            }
            device.battery = crate::controls::battery(device);
        }
    }
    Ok(devices)
}

fn connection(info: &DeviceInfo) -> &'static str {
    match info.bus_type() {
        BusType::Usb => "USB",
        BusType::Bluetooth => "Bluetooth",
        _ => "HID",
    }
}

fn is_hidpp(info: &DeviceInfo) -> bool {
    matches!(
        (info.usage_page(), info.usage()),
        (0xFF00, 0x0002) | (0xFF43, 0x0202 | 0x0602 | 0x1A02)
    )
}

pub(crate) fn is_receiver(product: u16) -> bool {
    matches!(product, 0xC548 | 0xC52B | 0xC532)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        migration::{Action, Binding, NamedAction},
        store::Store,
    };

    #[test]
    fn same_model_devices_get_independent_saved_settings_after_reconnect() {
        let path = tempfile::tempdir().unwrap();
        let database = path.path().join("profiles.db");
        let mut first = Device::direct("Mouse".into(), 0xB034, "Bluetooth", 3);
        let mut second = first.clone();
        let legacy = first.clone();
        first.identity = Identity::Serial("fixture-a".into());
        second.identity = Identity::Serial("fixture-b".into());
        first.direct_path = Some(c"before".into());
        second.direct_path = Some(c"other".into());
        let copy = Binding::Click(Action::Named(NamedAction::Copy));
        let paste = Binding::Click(Action::Named(NamedAction::Paste));
        let store = Store::open(&database).unwrap();
        store
            .save_bindings(&legacy, &BTreeMap::from([(0x53, copy.clone())]))
            .unwrap();
        assert_eq!(store.bindings(&first).unwrap()[&0x53], copy);
        store
            .save_bindings(&first, &BTreeMap::from([(0x53, paste.clone())]))
            .unwrap();
        drop(store);

        let previous_key = first.key();
        first.direct_path = Some(c"after".into());
        assert_eq!(first.key(), previous_key);
        assert_ne!(first.key(), second.key());
        let store = Store::open(&database).unwrap();
        assert_eq!(store.bindings(&first).unwrap()[&0x53], paste);
        assert_eq!(store.bindings(&second).unwrap()[&0x53], copy);
    }

    #[test]
    fn an_anonymous_hid_path_cannot_save_to_the_legacy_model_row() {
        let path = tempfile::tempdir().unwrap();
        let store = Store::open(&path.path().join("profiles.db")).unwrap();
        let mut device = Device::direct("Mouse".into(), 0xB034, "USB", 1);
        device.identity = Identity::Interface(c"volatile".into());
        device.direct_path = Some(c"volatile".into());
        assert!(store.save_bindings(&device, &BTreeMap::new()).is_err());
        assert!(device.require_verified_identity().is_err());
    }
}
