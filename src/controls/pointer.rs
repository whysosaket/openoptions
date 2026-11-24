use super::protocol::Controls;
use crate::{devices::Device, migration::WheelMode};

#[derive(Default)]
pub struct Pointer {
    pub dpi: Option<Dpi>,
    pub wheel: Option<Wheel>,
}

pub struct Dpi {
    pub current: u16,
    pub supported: Vec<u16>,
}

#[derive(Clone, Copy)]
pub struct Wheel {
    pub mode: WheelMode,
    pub threshold: u8,
    pub default_threshold: u8,
}

pub enum PointerChange {
    Dpi(u16),
    Wheel(Wheel),
}

pub fn pointer(device: &Device) -> Result<Pointer, String> {
    let channel = Controls::connect(device)?;
    Ok(Pointer {
        dpi: dpi(&channel)?.map(|(_, state)| state),
        wheel: wheel(&channel)?.map(|(_, state)| state),
    })
}

pub fn apply_pointer(device: &Device, change: PointerChange) -> Result<Pointer, String> {
    let channel = Controls::connect(device)?;
    match change {
        PointerChange::Dpi(value) => {
            let (feature, options) = dpi(&channel)?.ok_or("Adjustable DPI is unavailable.")?;
            if !options.supported.contains(&value) {
                return Err("This sensor does not support that DPI.".into());
            }
            let [high, low] = value.to_be_bytes();
            channel.call(feature, 3, &[0, high, low])?;
            if read_dpi(&channel, feature)? != value {
                return Err(
                    "The device did not apply the DPI. Reload its current settings.".into(),
                );
            }
        }
        PointerChange::Wheel(value) => {
            let (feature, _) = wheel(&channel)?.ok_or("SmartShift is unavailable.")?;
            if value.threshold == 0 {
                return Err("Choose a SmartShift threshold greater than zero.".into());
            }
            let mode = match value.mode {
                WheelMode::FreeSpin => 1,
                WheelMode::Ratchet => 2,
            };
            channel.call(
                feature.index,
                feature.read_function + 1,
                &[mode, value.threshold, 0],
            )?;
            let actual = read_wheel(&channel, feature)?;
            if actual.mode != value.mode || actual.threshold != value.threshold {
                return Err(
                    "The device did not apply the wheel settings. Reload its current settings."
                        .into(),
                );
            }
        }
    }
    Ok(Pointer {
        dpi: dpi(&channel)?.map(|(_, state)| state),
        wheel: wheel(&channel)?.map(|(_, state)| state),
    })
}

fn dpi(channel: &Controls) -> Result<Option<(u8, Dpi)>, String> {
    let feature = channel.call(0, 0, &[0x22, 0x01])?[0];
    if feature == 0 || channel.call(feature, 0, &[])?[0] != 1 {
        return Ok(None);
    }
    let report = channel.call(feature, 1, &[0])?;
    if report[0] != 0 {
        return Err("The device reported a different DPI sensor.".into());
    }
    Ok(Some((
        feature,
        Dpi {
            current: read_dpi(channel, feature)?,
            supported: dpi_values(&report[1..])?,
        },
    )))
}

fn read_dpi(channel: &Controls, feature: u8) -> Result<u16, String> {
    let data = channel.call(feature, 2, &[0])?;
    Ok(u16::from_be_bytes([data[1], data[2]]))
}

fn dpi_values(data: &[u8]) -> Result<Vec<u16>, String> {
    let invalid = "The device returned an invalid DPI list.";
    let mut words = data
        .as_chunks::<2>()
        .0
        .iter()
        .copied()
        .map(u16::from_be_bytes);
    let mut values = Vec::new();
    while let Some(value) = words.next() {
        if value == 0 {
            break;
        }
        if value >= 0xE000 {
            let step = usize::from(value & 0x1FFF);
            let start = *values.last().ok_or(invalid)?;
            let end = words
                .next()
                .filter(|end| (*end > start) && (*end < 0xE000))
                .ok_or(invalid)?;
            if step == 0 {
                return Err(invalid.into());
            }
            values.extend((start..end).step_by(step).skip(1));
            values.push(end);
        } else {
            values.push(value);
        }
    }
    values.sort_unstable();
    values.dedup();
    if values.is_empty() {
        return Err(invalid.into());
    }
    Ok(values)
}

#[derive(Clone, Copy)]
struct WheelFeature {
    index: u8,
    read_function: u8,
    default_threshold: Option<u8>,
}

fn wheel(channel: &Controls) -> Result<Option<(WheelFeature, Wheel)>, String> {
    let enhanced = channel.call(0, 0, &[0x21, 0x11])?[0];
    let feature = if enhanced == 0 {
        let index = channel.call(0, 0, &[0x21, 0x10])?[0];
        if index == 0 {
            return Ok(None);
        }
        WheelFeature {
            index,
            read_function: 0,
            default_threshold: None,
        }
    } else {
        let capabilities = channel.call(enhanced, 0, &[])?;
        WheelFeature {
            index: enhanced,
            read_function: 1,
            default_threshold: Some(capabilities[1]),
        }
    };
    Ok(Some((feature, read_wheel(channel, feature)?)))
}

fn read_wheel(channel: &Controls, feature: WheelFeature) -> Result<Wheel, String> {
    let data = channel.call(feature.index, feature.read_function, &[])?;
    let mode = match data[0] {
        1 => WheelMode::FreeSpin,
        2 => WheelMode::Ratchet,
        _ => return Err("The device returned an unknown wheel mode.".into()),
    };
    Ok(Wheel {
        mode,
        threshold: data[1],
        default_threshold: feature.default_threshold.unwrap_or(data[2]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_dpi_range_keeps_the_last_supported_value() {
        assert_eq!(
            dpi_values(&[0, 200, 0xE0, 50, 1, 7, 0, 0]).unwrap(),
            [200, 250, 263]
        );
        assert!(dpi_values(&[0, 200, 0xE0, 0, 1, 7]).is_err());
    }
}
