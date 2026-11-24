use super::Control;
use crate::devices::{Device, with_hid};
use hidapi::HidDevice;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const LONG_REPORT: u8 = 0x11;
const DIRECT: u8 = 0xFF;
const SOFTWARE_ID: u8 = 0x0D;
const REPROG_CONTROLS: [u8; 2] = [0x1B, 0x04];
const TEMPORARY: u8 = 0x01;
const PERSISTENT: u8 = 0x04;
const RAW_MOTION: u8 = 0x10;
const FORCE_MOTION: u8 = 0x40;

pub(super) enum Event {
    Buttons([u16; 4]),
    Motion(i16, i16),
    ReportingChanged,
}

pub(super) struct Controls {
    handle: HidDevice,
    alternate: Option<HidDevice>,
    target: u8,
    feature: u8,
    pub available: Vec<Control>,
    pending: BTreeMap<u16, u8>,
}

impl Controls {
    pub(super) fn open(device: &Device) -> Result<Self, String> {
        let mut controls = Self::connect(device)?;
        let feature = controls.call(0, 0, &REPROG_CONTROLS)?[0];
        if feature == 0 {
            return Ok(controls);
        }
        controls.feature = feature;
        let count = controls.call(feature, 0, &[])?[0];
        if count > 96 {
            return Err("This device reported an invalid control count.".into());
        }
        for index in 0..count {
            let data = controls.call(feature, 1, &[index])?;
            if let Some(control) = control_from_report(data) {
                controls.available.push(control);
            }
        }
        Ok(controls)
    }

    pub(super) fn connect(device: &Device) -> Result<Self, String> {
        if !device.connected {
            return Err("The selected device is disconnected.".into());
        }
        if let Some(link) = &device.receiver {
            let (handle, alternate) = link.open(device.product_id)?;
            return Ok(Self::new(handle, alternate, link.slot));
        }
        if !device.supports_hidpp() {
            return Err("No direct HID++ interface is available for this device.".into());
        }
        let unit = device.direct_unit();
        let device = device.clone();
        let handle = with_hid(move |api| device.open_direct(api))?;
        let channel = Self::new(handle, None, DIRECT);
        if let Some(unit) = unit
            && channel.unit_id()? != Some(unit)
        {
            return Err(
                "The HID++ interface now belongs to another device. Refresh Devices.".into(),
            );
        }
        Ok(channel)
    }

    pub(super) fn unit_id(&self) -> Result<Option<[u8; 4]>, String> {
        let feature = self.call(0, 0, &[0, 3])?[0];
        if feature == 0 {
            return Ok(None);
        }
        let reply = self.call(feature, 0, &[])?;
        let unit: [u8; 4] = reply[1..5].try_into().expect("fixed unit ID field");
        Ok((unit != [0; 4] && unit != [0xFF; 4]).then_some(unit))
    }

    fn new(handle: HidDevice, alternate: Option<HidDevice>, target: u8) -> Self {
        Self {
            handle,
            alternate,
            target,
            feature: 0,
            available: Vec::new(),
            pending: BTreeMap::new(),
        }
    }

    fn read(&self, report: &mut [u8; 20], timeout: i32) -> Result<usize, String> {
        let delay = if self.alternate.is_some() {
            timeout.min(25)
        } else {
            timeout
        };
        let size = self
            .handle
            .read_timeout(report, delay)
            .map_err(|error| error.to_string())?;
        if size == 0
            && let Some(alternate) = &self.alternate
        {
            return alternate
                .read_timeout(report, delay)
                .map_err(|error| error.to_string());
        }
        Ok(size)
    }

    fn reporting(&self, id: u16) -> Result<u8, String> {
        let cid = id.to_be_bytes();
        let state = self.call(self.feature, 2, &cid)?;
        if state[..2] != cid {
            return Err(
                "The device returned reporting information for a different control.".into(),
            );
        }
        Ok(state[2])
    }

    pub(super) fn divert(&mut self, control: Control, gesture: bool) -> Result<(), String> {
        let initial = self.reporting(control.id)?;
        require_idle(control, initial)?;
        let requested = TEMPORARY
            | if gesture && control.raw_motion {
                RAW_MOTION
            } else {
                0
            };
        let mask = TEMPORARY | requested & RAW_MOTION;
        // Record the intended change before sending it. A timeout can hide a successful write.
        self.pending.insert(control.id, mask);
        self.set_reporting(control.id, requested, mask)
    }

    pub(super) fn restore_control(&mut self, id: u16) -> Result<(), String> {
        if let Some(mask) = self.pending.get(&id).copied() {
            self.set_reporting(id, 0, mask)?;
            self.pending.remove(&id);
        }
        Ok(())
    }

    pub(super) fn restore(&mut self) -> Result<(), String> {
        let mut failure = None;
        for (id, mask) in std::mem::take(&mut self.pending) {
            if let Err(error) = self.set_reporting(id, 0, mask) {
                failure = Some(error);
            }
        }
        failure.map_or(Ok(()), |error| {
            Err(format!("Could not restore default reporting. Power-cycle the device if a control is missing. {error}"))
        })
    }

    fn set_reporting(&self, id: u16, flags: u8, mask: u8) -> Result<(), String> {
        let [high, low] = id.to_be_bytes();
        let reply = self.call(self.feature, 3, &[high, low, flags | (mask << 1)])?;
        if reply[..2] != [high, low] {
            return Err("The device acknowledged a different control.".into());
        }
        if self.reporting(id)? & mask != flags {
            return Err("The device did not accept the requested reporting mode.".into());
        }
        Ok(())
    }

    pub(super) fn event(&mut self) -> Result<Option<Event>, String> {
        let mut report = [0; 20];
        let read = self.read(&mut report, 55)?;
        if read != report.len() || report[..3] != [LONG_REPORT, self.target, self.feature] {
            return Ok(None);
        }
        let event = match report[3] {
            0x00 => Event::Buttons(std::array::from_fn(|index| {
                let index = 4 + index * 2;
                u16::from_be_bytes([report[index], report[index + 1]])
            })),
            0x10 => Event::Motion(
                i16::from_be_bytes([report[4], report[5]]),
                i16::from_be_bytes([report[6], report[7]]),
            ),
            command if command >> 4 == 3 && command & 0x0F != SOFTWARE_ID => {
                let id = u16::from_be_bytes([report[4], report[5]]);
                if self.pending.remove(&id).is_none() {
                    return Ok(None);
                }
                Event::ReportingChanged
            }
            _ => return Ok(None),
        };
        Ok(Some(event))
    }

    pub(super) fn notify(
        &self,
        feature: u8,
        function: u8,
        parameters: &[u8],
    ) -> Result<(), String> {
        self.send(feature, function, parameters).map(|_| ())
    }

    pub(super) fn call(
        &self,
        feature: u8,
        function: u8,
        parameters: &[u8],
    ) -> Result<[u8; 16], String> {
        let request = self.send(feature, function, parameters)?;
        let command = request[3];
        let deadline = Instant::now() + Duration::from_millis(600);
        let mut reply = [0; 20];
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let timeout = remaining.as_millis().max(1) as i32;
            let read = self.read(&mut reply, timeout)?;
            if ((read == 20 && reply[0] == LONG_REPORT) || (read == 7 && reply[0] == 0x10))
                && reply[1..4] == request[1..4]
            {
                let mut payload = [0; 16];
                payload[..read - 4].copy_from_slice(&reply[4..read]);
                return Ok(payload);
            }
            if read >= 6 && reply[1..5] == [self.target, 0xFF, feature, command] {
                return Err(format!(
                    "The device rejected a HID++ request (error {:02X}).",
                    reply[5]
                ));
            }
        }
        Err("The device did not respond to a HID++ request.".into())
    }
    fn send(&self, feature: u8, function: u8, parameters: &[u8]) -> Result<[u8; 20], String> {
        let command = (function << 4) | SOFTWARE_ID;
        let mut request = [0; 20];
        request[..4].copy_from_slice(&[LONG_REPORT, self.target, feature, command]);
        request[4..4 + parameters.len()].copy_from_slice(parameters);
        let written = self
            .handle
            .write(&request)
            .map_err(|error| error.to_string())?;
        if written < request.len() {
            return Err("The HID++ request was only partially written.".into());
        }
        Ok(request)
    }
}

fn control_from_report(data: [u8; 16]) -> Option<Control> {
    let id = u16::from_be_bytes([data[0], data[1]]);
    let flags = data[4];
    // Preserve primary clicks, hardware host-switch keys and virtual controls.
    let editable = id != 0
        && !matches!(id, 0x50 | 0x51 | 0xD1..=0xD3)
        && flags & 0x20 != 0
        && flags & 0x80 == 0;
    editable.then_some(Control {
        id,
        mouse: flags & 0x01 != 0,
        raw_motion: data[8] & 0x01 != 0,
    })
}

fn require_idle(control: Control, flags: u8) -> Result<(), String> {
    if flags & (TEMPORARY | PERSISTENT | RAW_MOTION | FORCE_MOTION) != 0 {
        return Err(format!(
            "{} is already being managed. Quit the other device utility and reconnect the peripheral before enabling controls.",
            control.name()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_discovery_preserves_primary_clicks_and_uses_raw_motion_flag() {
        let mut data = [0; 16];
        data[1] = 0x50;
        data[4] = 0x31;
        assert_eq!(control_from_report(data), None);
        data[1] = 0xC3;
        assert!(!control_from_report(data).unwrap().raw_motion);
        data[8] = 1;
        assert!(control_from_report(data).unwrap().raw_motion);
        data[4] |= 0x80;
        assert_eq!(control_from_report(data), None);
    }
}
