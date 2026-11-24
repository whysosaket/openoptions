use super::{
    Action, Binding, Button, Direction, NamedAction, Preview, Profile, ScrollDirection, Settings,
    WheelMode, read_text,
};
use rusqlite::{Connection, OpenFlags, types::Value as SqlValue};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

const MAX_PAYLOAD_BYTES: i64 = 16 * 1024 * 1024;

pub(super) fn load(path: &Path) -> Result<Preview, String> {
    let data = if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        let text = read_text(path)?;
        serde_json::from_str(text.trim_start_matches('\u{feff}'))
    } else {
        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| format!("Could not open the Options+ database: {error}"))?;
        connection
            .busy_timeout(Duration::from_secs(2))
            .map_err(|error| error.to_string())?;
        let payload: SqlValue = connection.query_row(
            "SELECT CASE WHEN length(file) <= ?1 THEN file END FROM data ORDER BY _id DESC LIMIT 1",
            [MAX_PAYLOAD_BYTES],
            |row| row.get(0),
        ).map_err(|error| format!("Could not read the latest Options+ settings: {error}"))?;
        match payload {
            SqlValue::Blob(bytes) => serde_json::from_slice(&bytes),
            SqlValue::Text(text) => serde_json::from_str(&text),
            _ => return Err("The latest Options+ profile is empty or exceeds 16 MB.".into()),
        }
    }
    .map_err(|_| "Could not parse the Options+ settings payload.")?;
    convert(&data)
}

fn convert(data: &Value) -> Result<Preview, String> {
    let devices = data["ever_connected_devices"]["devices"]
        .as_array()
        .ok_or("The Options+ file has no device list.")?;
    let keys = data["profile_keys"]
        .as_array()
        .ok_or("The Options+ file has no profile index.")?;
    let mut result = Preview::default();
    let mut defaults = Vec::new();
    for key in keys.iter().filter_map(Value::as_str) {
        let profile = &data[key];
        if profile["activeForApplication"].as_bool() == Some(false) {
            continue;
        }
        if profile["name"].as_str() == Some("PROFILE_NAME_DEFAULT") {
            if let Some(assignments) = profile["assignments"].as_array() {
                defaults.push(assignments);
            }
        } else {
            result.skip("Per-app profiles");
        }
    }
    let mut seen = std::collections::HashSet::new();
    for device in devices {
        if !device["deviceType"]
            .as_str()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("mouse"))
        {
            result.skip("Non-mouse device profile");
            continue;
        }
        let Some(prefix) = device["slotPrefix"]
            .as_str()
            .filter(|prefix| !prefix.is_empty())
        else {
            result.skip("Mouse profile without an Options+ identifier");
            continue;
        };
        if !seen.insert(prefix) {
            result.skip("Duplicate device record");
            continue;
        }
        let mut settings = Settings {
            notify_low_battery: data["low_battery_notifications_enabled"].as_bool(),
            ..Settings::default()
        };
        let mut matched = false;
        for assignment in defaults.iter().flat_map(|items| *items) {
            let Some(slot) = assignment["slotId"]
                .as_str()
                .and_then(|id| id.strip_prefix(prefix))
                .and_then(|tail| tail.strip_prefix('_'))
            else {
                continue;
            };
            matched = true;
            apply_assignment(slot, &assignment["card"], &mut settings, &mut result);
        }
        if matched && settings != Settings::default() {
            let name = device["deviceModel"]
                .as_str()
                .filter(|name| !name.is_empty())
                .unwrap_or("Logitech mouse")
                .replace('_', " ");
            let key = device["serialNumber"]
                .as_str()
                .filter(|serial| !serial.trim().is_empty())
                .map_or_else(
                    || format!("vendor:{prefix}"),
                    |serial| format!("serial:{}", serial.to_ascii_lowercase()),
                );
            result.profiles.push(Profile {
                device_name: name,
                device_key: key,
                settings,
            });
        }
    }
    Ok(result)
}

fn apply_assignment(slot: &str, card: &Value, settings: &mut Settings, result: &mut Preview) {
    if slot == "mouse_scroll_wheel_settings" {
        let wheel = &card["mouseScrollWheelSettings"];
        settings.smooth_scroll = wheel["isSmooth"].as_bool();
        settings.scroll_direction = match wheel["dir"].as_str() {
            Some("STANDARD") => Some(ScrollDirection::Standard),
            Some("NATURAL") => Some(ScrollDirection::Natural),
            _ => None,
        };
        settings.wheel_mode = match wheel["smartshift"]["mode"].as_str() {
            Some("FREESPIN") => Some(WheelMode::FreeSpin),
            Some("RATCHET") => Some(WheelMode::Ratchet),
            _ => None,
        };
        if wheel["smartshift"]["sensitivity"].is_number() {
            result.skip("Options+ SmartShift sensitivity needs device calibration");
        }
        return;
    }
    let button = match slot {
        "c82" => Button::Middle,
        "c83" => Button::Back,
        "c86" => Button::Forward,
        "c195" => Button::Gesture,
        "c196" => Button::ModeShift,
        other => {
            result.skip(match other {
                "mouse_settings" => "Options+ pointer speed needs device DPI read-back",
                "mouse_thumb_wheel_settings" | "thumb_wheel_adapter" => "Thumb-wheel settings",
                _ => "Unsupported Options+ control",
            });
            return;
        }
    };
    if let Some(binding) = binding(card) {
        settings.buttons.insert(button, binding);
    } else {
        result.skip("Unsupported button binding or gesture");
    }
}

fn binding(card: &Value) -> Option<Binding> {
    if let Some(selected) = card["selectedNestedCard"].as_str()
        && let Some(gestures) = card["nestedCards"][selected]["nestedCards"].as_object()
    {
        let directions = gestures
            .iter()
            .map(|(name, entry)| Some((Direction::parse(name)?, action(entry)?)))
            .collect::<Option<std::collections::BTreeMap<_, _>>>()?;
        return (directions.len() == 5).then_some(Binding::Gesture(directions));
    }
    action(card).map(Binding::Click)
}

fn action(card: &Value) -> Option<Action> {
    let macro_data = &card["macro"];
    match macro_data["type"].as_str() {
        Some("OPEN_FILE_FOLDER") => {
            return Action::application(macro_data["openFileFolder"]["path"].as_str()?);
        }
        Some("DO_NOTHING") => return Some(Action::Named(NamedAction::NoAction)),
        Some("MOUSE")
            if macro_data["mouse"]["action"].as_str() == Some("BUTTON")
                && macro_data["mouse"]["hidUsage"].as_u64() == Some(3) =>
        {
            return Some(Action::Named(NamedAction::MiddleClick));
        }
        _ => {}
    }
    let name = macro_data["quickLaunch"]["action"]
        .as_str()
        .or_else(|| macro_data["system"]["action"].as_str())
        .or_else(|| macro_data["media"]["usage"].as_str());
    let action = match name {
        Some("MISSION_CONTROL") => NamedAction::MissionControl,
        Some("APP_EXPOSE") => NamedAction::AppExpose,
        Some("SHOW_DESKTOP") => NamedAction::Desktop,
        Some("SWITCH_BETWEEN_DESKTOPS_LEFT") => NamedAction::PreviousDesktop,
        Some("SWITCH_BETWEEN_DESKTOPS_RIGHT") => NamedAction::NextDesktop,
        Some("PLAY_PAUSE") => NamedAction::PlayPause,
        Some("NEXT_TRACK") => NamedAction::NextTrack,
        Some("PREVIOUS_TRACK") => NamedAction::PreviousTrack,
        Some("VOLUME_UP") => NamedAction::VolumeUp,
        Some("VOLUME_DOWN") => NamedAction::VolumeDown,
        Some("VOLUME_MUTE") => NamedAction::Mute,
        _ => {
            let id = card["id"].as_str()?;
            match id.strip_prefix("card_global_presets_osx_")? {
                "copy" => NamedAction::Copy,
                "cut" => NamedAction::Cut,
                "paste" => NamedAction::Paste,
                "undo" => NamedAction::Undo,
                "redo" => NamedAction::Redo,
                _ => return None,
            }
        }
    };
    Some(Action::Named(action))
}
