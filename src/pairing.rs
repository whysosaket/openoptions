use crate::devices::{Device, with_hid};
use hidapi::{BusType, HidDevice};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ffi::CString;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const DISCOVERY_SECONDS: u8 = 45;
const NOTIFICATIONS: u8 = 0x09;
const RECEIVER: u8 = 0xFF;
const UNIFYING: u8 = 0xB2;
const DISCOVERY: u8 = 0xC0;
const BOLT_PAIR: u8 = 0xC1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiverDevice {
    pub name: String,
    pub bolt: bool,
    long: CString,
    short: Option<CString>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReceiverLink {
    receiver: ReceiverDevice,
    pub slot: u8,
    pub unit: [u8; 4],
}

impl ReceiverLink {
    pub fn open(&self, product: u16) -> Result<(HidDevice, Option<HidDevice>), String> {
        let mut io = ReceiverIo::open(&self.receiver)?;
        let identity = io.slot(self.slot)?;
        if identity.product != product || identity.unit != self.unit || !identity.online {
            return Err(
                "This peripheral is no longer connected to the selected receiver slot.".into(),
            );
        }
        Ok((io.long, io.short))
    }
}

pub(crate) fn connected_peripherals() -> Result<Vec<Device>, String> {
    Ok(receivers()?
        .into_iter()
        .flat_map(|receiver| {
            let Ok(mut io) = ReceiverIo::open(&receiver) else {
                return Vec::new();
            };
            let count = if receiver.bolt {
                6
            } else {
                io.slot_count().unwrap_or(0)
            };
            (1..=count)
                .filter_map(|slot| {
                    let info = io.slot(slot).ok()?;
                    if info.unit == [0; 4] || info.unit == [0xFF; 4] || info.product == 0 {
                        return None;
                    }
                    let name = io
                        .peripheral_name(receiver.bolt, slot)
                        .unwrap_or_else(|| format!("Logitech {:04X}", info.product));
                    Some(Device::paired(
                        name,
                        info.product,
                        if receiver.bolt {
                            "Logi Bolt"
                        } else {
                            "Unifying"
                        },
                        info.online,
                        ReceiverLink {
                            receiver: receiver.clone(),
                            slot,
                            unit: info.unit,
                        },
                    ))
                })
                .collect::<Vec<_>>()
        })
        .collect())
}

pub fn receivers() -> Result<Vec<ReceiverDevice>, String> {
    with_hid(|api| {
        let all: Vec<_> = api
            .device_list()
            .filter(|info| {
                info.vendor_id() == 0x046D
                    && matches!(info.product_id(), 0xC548 | 0xC52B | 0xC532)
                    && matches!(info.bus_type(), BusType::Usb)
                    && info.usage_page() == 0xFF00
            })
            .collect();
        let mut seen = BTreeSet::new();
        let mut found = Vec::new();
        for info in all.iter().copied().filter(|info| info.usage() == 2) {
            if !seen.insert(info.path()) {
                continue;
            }
            let product = info.product_id();
            let unique_product = all
                .iter()
                .filter(|item| item.product_id() == product && item.usage() == 2)
                .count()
                == 1;
            let short = all
                .iter()
                .copied()
                .find(|item| {
                    item.product_id() == product
                        && item.usage() == 1
                        && (unique_product
                            || (info.serial_number().is_some()
                                && info.serial_number() == item.serial_number()))
                })
                .map(|item| item.path().to_owned());
            #[cfg(target_os = "windows")]
            if short.is_none() {
                continue;
            }
            let bolt = product == 0xC548;
            found.push(ReceiverDevice {
                name: if bolt {
                    "Logi Bolt Receiver"
                } else {
                    "Unifying Receiver"
                }
                .into(),
                bolt,
                long: info.path().to_owned(),
                short: short.filter(|path| path.as_c_str() != info.path()),
            });
        }
        Ok(found)
    })
}

pub enum Event {
    Searching,
    Found { id: u16, name: String },
    Confirm(Passkey),
    Paired { slot: u8, name: Option<String> },
    Finished(Result<(), String>),
}

pub enum Passkey {
    Keyboard(String),
    Mouse([bool; 10]),
}

enum Command {
    Pair(u16),
    Cancel,
}

pub struct Pairing {
    commands: Sender<Command>,
    events: Receiver<Event>,
    worker: Option<JoinHandle<()>>,
}

impl Pairing {
    pub fn start(receiver: ReceiverDevice) -> Self {
        let (commands, requests) = mpsc::channel();
        let (send, events) = mpsc::channel();
        let worker = thread::spawn(move || {
            let result = run(receiver, &requests, &send);
            let _ = send.send(Event::Finished(result));
        });
        Self {
            commands,
            events,
            worker: Some(worker),
        }
    }

    pub fn poll(&self) -> Option<Event> {
        match self.events.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Event::Finished(Err(
                "Receiver pairing stopped unexpectedly.".into(),
            ))),
        }
    }

    pub fn pair(&self, id: u16) {
        let _ = self.commands.send(Command::Pair(id));
    }

    pub fn cancel(&self) {
        let _ = self.commands.send(Command::Cancel);
    }
}

impl Drop for Pairing {
    fn drop(&mut self) {
        self.cancel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct ReceiverIo {
    long: HidDevice,
    short: Option<HidDevice>,
    pending: VecDeque<Report>,
    next_short: bool,
}

#[derive(Clone, Copy)]
struct Report {
    length: usize,
    bytes: [u8; 20],
}

enum RegisterError {
    Device(u8),
    Transport(String),
}

impl std::fmt::Display for RegisterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Device(0x05) => write!(f, "The receiver has no free pairing slots."),
            Self::Device(code) => write!(f, "The receiver returned HID++ error {code:02X}."),
            Self::Transport(error) => f.write_str(error),
        }
    }
}

impl From<RegisterError> for String {
    fn from(error: RegisterError) -> Self {
        error.to_string()
    }
}

struct Slot {
    product: u16,
    unit: [u8; 4],
    online: bool,
}

impl ReceiverIo {
    fn open(receiver: &ReceiverDevice) -> Result<Self, String> {
        let receiver = receiver.clone();
        with_hid(move |api| {
            let open =
                |path: &std::ffi::CStr| {
                    api.open_path(path).map_err(|error| {
                format!("Could not open the selected receiver. Check device permissions. {error}")
            })
                };
            Ok(Self {
                long: open(&receiver.long)?,
                short: receiver.short.as_ref().map(|path| open(path)).transpose()?,
                pending: VecDeque::new(),
                next_short: true,
            })
        })
    }

    fn slot_count(&mut self) -> Result<u8, String> {
        let count = self.register(0x83, 0xB5, &[0x03])?.bytes[10];
        if !(1..=15).contains(&count) {
            return Err("The receiver returned an invalid slot count.".into());
        }
        Ok(count)
    }

    fn slot(&mut self, slot: u8) -> Result<Slot, RegisterError> {
        let frame = self.register(0x83, 0xB5, &[0x50 | slot])?.bytes;
        Ok(Slot {
            product: u16::from_le_bytes([frame[6], frame[7]]),
            unit: [frame[8], frame[9], frame[10], frame[11]],
            online: frame[5] & 0x40 == 0,
        })
    }

    fn peripheral_name(&mut self, bolt: bool, slot: u8) -> Option<String> {
        let mut name = Vec::new();
        if bolt {
            let selector = 0x60 | slot;
            let first = self.register(0x83, 0xB5, &[selector, 1]).ok()?.bytes;
            let length = usize::from(first[6]).min(27);
            name.extend_from_slice(&first[7..7 + length.min(13)]);
            if length > 13
                && let Ok(second) = self.register(0x83, 0xB5, &[selector, 2])
                && second.bytes[5] == 2
            {
                name.extend_from_slice(&second.bytes[6..6 + length - 13]);
            }
        } else {
            let data = self.register(0x83, 0xB5, &[0x40 + (slot - 1)]).ok()?.bytes;
            let length = usize::from(data[5]).min(14);
            name.extend_from_slice(&data[6..6 + length]);
        }
        let name = String::from_utf8_lossy(&name)
            .trim_matches(|c: char| c.is_whitespace() || c == '\0')
            .to_owned();
        (!name.is_empty()).then_some(name)
    }

    fn read(&mut self, timeout: i32) -> Result<Option<Report>, String> {
        let mut report = Report {
            length: 0,
            bytes: [0; 20],
        };
        self.next_short = !self.next_short;
        let handle = if self.next_short {
            self.short.as_ref().unwrap_or(&self.long)
        } else {
            &self.long
        };
        report.length = handle
            .read_timeout(&mut report.bytes, timeout)
            .map_err(|error| error.to_string())?;
        Ok(matches!((report.length, report.bytes[0]), (7, 0x10) | (20, 0x11)).then_some(report))
    }

    fn register(
        &mut self,
        operation: u8,
        address: u8,
        data: &[u8],
    ) -> Result<Report, RegisterError> {
        let long = operation == 0x82;
        let length = if long { 20 } else { 7 };
        let handle = if long {
            &self.long
        } else {
            self.short.as_ref().unwrap_or(&self.long)
        };
        let mut frame = [0; 20];
        frame[..4].copy_from_slice(&[if long { 0x11 } else { 0x10 }, RECEIVER, operation, address]);
        frame[4..4 + data.len()].copy_from_slice(data);
        let written = handle
            .write(&frame[..length])
            .map_err(|error| RegisterError::Transport(error.to_string()))?;
        if written < length {
            return Err(RegisterError::Transport(
                "The receiver command was only partially written.".into(),
            ));
        }
        let deadline = Instant::now() + Duration::from_millis(850);
        while Instant::now() < deadline {
            let Some(report) = self.read(35).map_err(RegisterError::Transport)? else {
                continue;
            };
            let bytes = report.bytes;
            if bytes[1..4] == [RECEIVER, operation, address]
                && (operation != 0x83 || report.length == 20 && bytes[4] == data[0])
            {
                return Ok(report);
            }
            if bytes[1..5] == [RECEIVER, 0x8F, operation, address] {
                return Err(RegisterError::Device(bytes[5]));
            }
            if self.pending.len() == 128 {
                self.pending.pop_front();
            }
            self.pending.push_back(report);
        }
        Err(RegisterError::Transport(
            "The receiver did not acknowledge the pairing command.".into(),
        ))
    }
}

#[derive(Default)]
struct Candidate {
    name: Option<String>,
    info: Option<DiscoveryInfo>,
    reported: bool,
}

#[derive(Clone, Copy)]
struct DiscoveryInfo {
    address: [u8; 6],
    authentication: u8,
    kind: u8,
}

enum Phase {
    BoltDiscovery,
    BoltPairing(DiscoveryInfo),
    UnifyingOpening,
    UnifyingSearching,
    Done,
}

fn run(
    receiver: ReceiverDevice,
    commands: &Receiver<Command>,
    events: &Sender<Event>,
) -> Result<(), String> {
    let mut io = ReceiverIo::open(&receiver)?;
    let mut previous_slots = BTreeSet::new();
    if !receiver.bolt {
        let count = io.slot_count()?;
        for slot in 1..=count {
            match io.slot(slot) {
                Ok(_) => {
                    previous_slots.insert(slot);
                }
                Err(RegisterError::Device(0x02 | 0x03 | 0x08)) => {}
                Err(error) => return Err(error.into()),
            }
        }
        if previous_slots.len() == usize::from(count) {
            return Err("The receiver has no free pairing slots.".into());
        }
    }
    let flags = io.register(0x81, 0, &[])?.bytes;
    let added = NOTIFICATIONS & !flags[5];
    let mut opened = false;
    let mut phase = if receiver.bolt {
        Phase::BoltDiscovery
    } else {
        Phase::UnifyingOpening
    };
    let result = (|| {
        if added != 0 {
            io.register(0x80, 0, &[flags[4], flags[5] | added, flags[6]])?;
        }
        if matches!(
            commands.try_recv(),
            Ok(Command::Cancel) | Err(TryRecvError::Disconnected)
        ) {
            return Ok(());
        }
        io.pending.clear();
        opened = true;
        if receiver.bolt {
            io.register(0x80, DISCOVERY, &[DISCOVERY_SECONDS, 1, 0])?;
            let _ = events.send(Event::Searching);
        } else {
            io.register(0x80, UNIFYING, &[1, 0, DISCOVERY_SECONDS])?;
        }
        let mut deadline = Instant::now() + Duration::from_secs(u64::from(DISCOVERY_SECONDS) + 2);
        let confirm_lock = Instant::now() + Duration::from_secs(4);
        let mut candidates = BTreeMap::<u16, Candidate>::new();
        loop {
            if Instant::now() >= deadline {
                return Err(
                    "Pairing timed out. Put the device in pairing mode and try again.".into(),
                );
            }
            if matches!(phase, Phase::UnifyingOpening) && Instant::now() >= confirm_lock {
                return Err("The receiver did not confirm that its pairing window opened.".into());
            }
            match commands.try_recv() {
                Ok(Command::Cancel) | Err(TryRecvError::Disconnected) => return Ok(()),
                Ok(Command::Pair(id)) if matches!(phase, Phase::BoltDiscovery) => {
                    if let Some(info) = candidates.get(&id).and_then(|candidate| candidate.info) {
                        let mut data = [0; 16];
                        data[0] = 1;
                        data[2..8].copy_from_slice(&info.address);
                        data[8] = info.authentication;
                        data[9] = if info.kind == 1 { 20 } else { 10 };
                        phase = Phase::BoltPairing(info);
                        io.register(0x82, BOLT_PAIR, &data)?;
                        deadline = Instant::now() + Duration::from_secs(45);
                    }
                }
                _ => {}
            }
            let report = match io.pending.pop_front() {
                Some(report) => Some(report),
                None => io.read(45)?,
            };
            let Some(report) = report else { continue };
            let b = report.bytes;
            match (&phase, b[2]) {
                (Phase::BoltDiscovery, 0x4F) if report.length == 20 && b[1] == RECEIVER => {
                    let id = u16::from_le_bytes([b[3], b[4]]);
                    if !candidates.contains_key(&id) && candidates.len() >= 64 {
                        continue;
                    }
                    let candidate = candidates.entry(id).or_default();
                    match b[5] {
                        0 => {
                            candidate.info = Some(DiscoveryInfo {
                                address: b[10..16].try_into().expect("fixed report field"),
                                authentication: b[18],
                                kind: b[7],
                            })
                        }
                        1 => {
                            let len = usize::from(b[6]).min(13);
                            candidate.name =
                                Some(String::from_utf8_lossy(&b[7..7 + len]).trim().into());
                        }
                        _ => {}
                    }
                    if !candidate.reported
                        && candidate.info.is_some()
                        && let Some(name) = &candidate.name
                    {
                        candidate.reported = true;
                        let name = if name.is_empty() {
                            "Unnamed Logitech device".into()
                        } else {
                            name.clone()
                        };
                        let _ = events.send(Event::Found { id, name });
                    }
                }
                (Phase::BoltPairing(info), 0x4D)
                    if report.length == 20 && b[1] == RECEIVER && b[10..16] == info.address =>
                {
                    let passkey = passkey(&b[4..10], info.authentication & 1 != 0)?;
                    let _ = events.send(Event::Confirm(passkey));
                }
                (Phase::BoltPairing(_), 0x54) if report.length == 20 && b[1] == RECEIVER => {
                    if b[4] != 0 {
                        return Err(format!("Bolt pairing failed with code {:02X}.", b[4]));
                    }
                    if b[3] == 2 && b[11] != 0 {
                        phase = Phase::Done;
                        let slot = b[11];
                        let name = io.peripheral_name(true, slot);
                        let _ = events.send(Event::Paired { slot, name });
                        return Ok(());
                    }
                }
                (Phase::BoltDiscovery, 0x53) if b[1] == RECEIVER && b[4] != 0 => {
                    return Err(format!("Bolt discovery ended with code {:02X}.", b[4]));
                }
                (Phase::UnifyingOpening | Phase::UnifyingSearching, 0x4A) if b[1] == RECEIVER => {
                    if b[4] != 0 || b[3] & 1 == 0 {
                        return Err(
                            "The Unifying pairing window closed before a new device connected."
                                .into(),
                        );
                    }
                    if matches!(phase, Phase::UnifyingOpening) {
                        phase = Phase::UnifyingSearching;
                        let _ = events.send(Event::Searching);
                    }
                }
                (Phase::UnifyingSearching, 0x41)
                    if (1..=15).contains(&b[1])
                        && b[4] & 0x40 == 0
                        && !previous_slots.contains(&b[1]) =>
                {
                    phase = Phase::Done;
                    let slot = b[1];
                    let name = io.peripheral_name(false, slot);
                    let _ = events.send(Event::Paired { slot, name });
                    return Ok(());
                }
                _ => {}
            }
        }
    })();
    let cancelled = match phase {
        _ if !opened => Ok(()),
        Phase::BoltDiscovery => io
            .register(0x80, DISCOVERY, &[DISCOVERY_SECONDS, 2, 0])
            .map(|_| ()),
        Phase::BoltPairing(_) => io.register(0x82, BOLT_PAIR, &[2]).map(|_| ()),
        Phase::UnifyingOpening | Phase::UnifyingSearching => {
            io.register(0x80, UNIFYING, &[2, 0, 0]).map(|_| ())
        }
        Phase::Done => Ok(()),
    };
    let restored = if added != 0 {
        io.register(0x81, 0, &[])
            .and_then(|current| {
                io.register(
                    0x80,
                    0,
                    &[
                        current.bytes[4],
                        current.bytes[5] & !added,
                        current.bytes[6],
                    ],
                )
            })
            .map(|_| ())
    } else {
        Ok(())
    };
    match (result, cancelled.and(restored)) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(cleanup)) => Err(format!("Could not confirm receiver cleanup. {cleanup}")),
        (Err(error), Err(cleanup)) => Err(format!(
            "{error} Could not confirm receiver cleanup. {cleanup}"
        )),
    }
}

fn passkey(bytes: &[u8], keyboard: bool) -> Result<Passkey, String> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let digits = std::str::from_utf8(&bytes[..end])
        .ok()
        .filter(|text| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
        .ok_or("The receiver sent an invalid pairing passkey.")?;
    if keyboard {
        return Ok(Passkey::Keyboard(digits.into()));
    }
    let value: u16 = digits
        .parse()
        .ok()
        .filter(|value| *value < 1024)
        .ok_or("The receiver sent an invalid mouse pairing sequence.")?;
    Ok(Passkey::Mouse(std::array::from_fn(|index| {
        value & (1 << (9 - index)) != 0
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_passkey_is_a_ten_click_sequence_and_rejects_invalid_values() {
        let Passkey::Mouse(clicks) = passkey(b"000513", false).unwrap() else {
            panic!()
        };
        assert_eq!(
            clicks,
            [
                true, false, false, false, false, false, false, false, false, true
            ]
        );
        assert!(passkey(b"001024", false).is_err());
        assert!(passkey(b"abc", true).is_err());
    }
}
