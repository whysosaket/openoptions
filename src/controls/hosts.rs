use super::protocol::Controls;
use crate::devices::Device;

pub struct Hosts {
    pub slots: Vec<Host>,
    pub current: Option<u8>,
    pub can_switch: bool,
}

pub struct Host {
    pub index: u8,
    pub paired: bool,
    pub name: String,
    pub connection: &'static str,
    pub operating_system: Option<&'static str>,
    pub name_capacity: u8,
}

pub fn host_channels(device: &Device) -> Result<Option<Hosts>, String> {
    read_hosts(&Controls::connect(device)?).map(|result| result.map(|(_, hosts)| hosts))
}

fn read_hosts(channel: &Controls) -> Result<Option<(u8, Hosts)>, String> {
    let feature = channel.call(0, 0, &[0x18, 0x15])?;
    let index = feature[0];
    if index == 0 {
        return Ok(None);
    }
    let info = channel.call(index, 0, &[])?;
    let count = info[2];
    if !(1..=8).contains(&count) {
        return Err("The device returned an invalid Easy-Switch channel count.".into());
    }
    let mut slots = Vec::with_capacity(usize::from(count));
    for host in 0..count {
        let data = channel.call(index, 1, &[host])?;
        if data[0] != host || data[1] > 1 || data[4] > data[5] {
            return Err("The device returned invalid Easy-Switch channel information.".into());
        }
        let paired = data[1] == 1;
        let name = if paired && info[0] & 1 != 0 {
            read_name(channel, index, host, data[4])?
        } else {
            String::new()
        };
        let operating_system = if paired && feature[2] >= 2 {
            channel.call(index, 7, &[host]).ok().and_then(|data| {
                if data[0] != host {
                    return None;
                }
                Some(match data[1] {
                    1 | 2 => "Windows",
                    3 => "Linux",
                    4 => "ChromeOS",
                    5 => "Android",
                    6 => "macOS",
                    7 => "iOS or iPadOS",
                    _ => return None,
                })
            })
        } else {
            None
        };
        slots.push(Host {
            index: host,
            paired,
            name,
            connection: match data[2] {
                1 => "Unifying",
                2 => "USB",
                3 | 4 => "Bluetooth",
                5 => "Logi Bolt",
                _ => "",
            },
            operating_system,
            name_capacity: if paired && info[0] & 3 == 3 {
                data[5]
            } else {
                0
            },
        });
    }
    let can_switch = channel
        .call(0, 0, &[0x18, 0x14])
        .is_ok_and(|data| data[0] != 0);
    Ok(Some((
        index,
        Hosts {
            slots,
            current: (info[3] < count).then_some(info[3]),
            can_switch,
        },
    )))
}

fn read_name(channel: &Controls, feature: u8, host: u8, length: u8) -> Result<String, String> {
    let mut name = Vec::with_capacity(length as usize);
    for offset in (0..length).step_by(14) {
        let response = channel.call(feature, 3, &[host, offset])?;
        if response[..2] != [host, offset] {
            return Err("The device returned a name for a different channel.".into());
        }
        let remaining = usize::from(length - offset).min(14);
        name.extend_from_slice(&response[2..2 + remaining]);
    }
    Ok(String::from_utf8_lossy(&name).trim_end_matches('\0').into())
}

pub fn rename_host(device: &Device, host: u8, name: &str) -> Result<Option<Hosts>, String> {
    let channel = Controls::connect(device)?;
    let (feature, hosts) = read_hosts(&channel)?.ok_or("Easy-Switch is unavailable.")?;
    let slot = hosts
        .slots
        .get(usize::from(host))
        .filter(|slot| slot.name_capacity != 0)
        .ok_or("This channel cannot be renamed.")?;
    let name = name.trim();
    if name.is_empty()
        || name.len() > usize::from(slot.name_capacity)
        || name.chars().any(char::is_control)
    {
        return Err(format!(
            "Enter a name up to {} bytes long.",
            slot.name_capacity
        ));
    }
    let write = |name: &str| {
        let mut padded = vec![0; usize::from(slot.name_capacity)];
        padded[..name.len()].copy_from_slice(name.as_bytes());
        for (part, bytes) in padded.chunks(14).enumerate() {
            let mut payload = [0; 16];
            payload[0] = host;
            payload[1] = (part * 14) as u8;
            payload[2..2 + bytes.len()].copy_from_slice(bytes);
            channel.call(feature, 4, &payload)?;
        }
        Ok::<_, String>(())
    };
    if let Err(error) = write(name) {
        if slot.name.len() <= usize::from(slot.name_capacity)
            && let Err(rollback) = write(&slot.name)
        {
            return Err(format!(
                "Could not finish or restore the channel name. {error} {rollback}"
            ));
        }
        return Err(error);
    }
    let readback = read_hosts(&channel)?.map(|(_, hosts)| hosts);
    if !readback
        .as_ref()
        .and_then(|hosts| hosts.slots.get(usize::from(host)))
        .is_some_and(|slot| slot.name == name)
    {
        return Err("The device did not confirm the new name.".into());
    }
    Ok(readback)
}

pub fn switch_host(device: &Device, host: u8) -> Result<(), String> {
    switch_on_channel(&Controls::connect(device)?, host)
}

pub(super) fn switch_on_channel(channel: &Controls, host: u8) -> Result<(), String> {
    let (_, hosts) = read_hosts(channel)?.ok_or("Easy-Switch is unavailable.")?;
    if !hosts
        .slots
        .get(usize::from(host))
        .is_some_and(|slot| slot.paired)
    {
        return Err("Pair a computer with that channel first.".into());
    }
    if hosts.current == Some(host) {
        return Ok(());
    }
    let feature = channel.call(0, 0, &[0x18, 0x14])?[0];
    if feature == 0 {
        return Err("This device does not support software host switching.".into());
    }
    // The peripheral can leave the transport before it has a chance to reply.
    channel.notify(feature, 1, &[host])
}
