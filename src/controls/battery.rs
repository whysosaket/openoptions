use super::protocol::Controls;
use crate::devices::Device;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Battery {
    Percent(u8),
    Full,
    Good,
    Low,
    Critical,
    Charging,
}

pub fn battery(device: &Device) -> Result<Option<Battery>, String> {
    if !device.supports_hidpp() {
        return Ok(None);
    }
    let channel = Controls::connect(device)?;
    let unified = channel.call(0, 0, &[0x10, 0x04])?[0];
    if unified != 0 {
        let caps = channel.call(unified, 0, &[])?;
        let status = channel.call(unified, 1, &[])?;
        return Ok(unified_level(
            caps[0], caps[1], status[0], status[1], status[2],
        ));
    }
    let legacy = channel.call(0, 0, &[0x10, 0x00])?[0];
    if legacy == 0 {
        return Ok(None);
    }
    let status = channel.call(legacy, 0, &[])?;
    Ok(match status[2] {
        0 if status[0] <= 100 => Some(Battery::Percent(status[0])),
        1 | 2 | 4 => Some(Battery::Charging),
        3 => Some(Battery::Full),
        _ => None,
    })
}

fn unified_level(supported: u8, flags: u8, percent: u8, level: u8, status: u8) -> Option<Battery> {
    match status {
        1 | 2 => Some(Battery::Charging),
        3 => Some(Battery::Full),
        _ if flags & 2 != 0 && percent <= 100 => Some(Battery::Percent(percent)),
        _ => match supported & level {
            bits if bits & 8 != 0 => Some(Battery::Full),
            bits if bits & 4 != 0 => Some(Battery::Good),
            bits if bits & 2 != 0 => Some(Battery::Low),
            bits if bits & 1 != 0 => Some(Battery::Critical),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_percent_only_when_reported_as_a_supported_level() {
        assert_eq!(unified_level(0, 2, 60, 0, 0), Some(Battery::Percent(60)));
        assert_eq!(unified_level(7, 0, 60, 4, 0), Some(Battery::Good));
        assert_eq!(unified_level(4, 0, 60, 1, 0), None);
        assert_eq!(unified_level(0, 2, 255, 0, 0), None);
        assert_eq!(unified_level(0, 2, 0, 0, 1), Some(Battery::Charging));
    }
}
