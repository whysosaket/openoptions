use super::{
    Action, Binding, Button, Direction, NamedAction, Preview, Profile, ScrollDirection, Settings,
    WheelMode, read_text,
};
use std::path::Path;
use toml::Value;

pub(super) fn load(path: &Path) -> Result<Preview, String> {
    let text = read_text(path)?;
    let config: Value =
        toml::from_str(&text).map_err(|_| "Could not parse the TOML settings file.")?;
    let devices = config
        .get("devices")
        .and_then(Value::as_table)
        .ok_or("The TOML file has no device settings table.")?;
    let mut result = Preview::default();
    let alerts = config
        .get("app_settings")
        .and_then(|app| app.get("low_battery_notifications"))
        .and_then(Value::as_bool);
    for (key, device) in devices {
        let Some(fields) = device.as_table() else {
            result.skip("Unreadable device profile");
            continue;
        };
        let mut settings = Settings {
            dpi: device
                .get("dpi")
                .and_then(Value::as_integer)
                .and_then(|dpi| u16::try_from(dpi).ok())
                .filter(|dpi| *dpi > 0),
            smooth_scroll: device.get("smooth_wheel").and_then(Value::as_bool),
            scroll_direction: device.get("invert_scroll").and_then(Value::as_bool).map(
                |inverted| {
                    if inverted {
                        ScrollDirection::InvertedFromSystem
                    } else {
                        ScrollDirection::MatchSystem
                    }
                },
            ),
            notify_low_battery: alerts,
            ..Settings::default()
        };
        if let Some(wheel) = device.get("smartshift") {
            settings.wheel_mode = match wheel.get("mode").and_then(Value::as_str) {
                Some("free") => Some(WheelMode::FreeSpin),
                Some("ratchet") => Some(WheelMode::Ratchet),
                _ => None,
            };
            settings.smart_shift_threshold = wheel
                .get("auto_disengage")
                .and_then(Value::as_integer)
                .and_then(|threshold| u8::try_from(threshold).ok())
                .filter(|value| (8..=254).contains(value));
            if wheel
                .get("tunable_torque")
                .and_then(Value::as_integer)
                .is_some_and(|value| value != 0)
            {
                result.skip("Wheel force");
            }
        }
        if let Some(bindings) = device.get("bindings").and_then(Value::as_table) {
            for (name, value) in bindings {
                if let Some((button, binding)) = button(name).zip(binding(value)) {
                    settings.buttons.insert(button, binding);
                } else {
                    result.skip("Unsupported button binding");
                }
            }
        }
        for (name, value) in fields {
            if !matches!(
                name.as_str(),
                "identity" | "dpi" | "smooth_wheel" | "invert_scroll" | "smartshift" | "bindings"
            ) && value.as_table().is_none_or(|table| !table.is_empty())
            {
                result.skip(match name.as_str() {
                    "action_ring" => "Actions Ring",
                    "per_app_bindings" => "Per-app bindings",
                    "links" => "Connection-specific settings",
                    _ => "Other device settings",
                });
            }
        }
        if settings != Settings::default() {
            let name = device
                .get("identity")
                .and_then(|identity| identity.get("display_name"))
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .unwrap_or("Logitech device");
            result.profiles.push(Profile {
                device_name: name.into(),
                device_key: key.clone(),
                settings,
            });
        }
    }
    if let Some(keyboard) = config
        .get("keyboard")
        .and_then(|keyboard| keyboard.get("bindings"))
        .and_then(Value::as_table)
        && !keyboard.is_empty()
    {
        result.skip("Keyboard bindings");
    }
    Ok(result)
}

fn button(name: &str) -> Option<Button> {
    Some(match name {
        "MiddleClick" => Button::Middle,
        "Back" => Button::Back,
        "Forward" => Button::Forward,
        "GestureButton" => Button::Gesture,
        "DpiToggle" => Button::ModeShift,
        _ => return None,
    })
}

fn binding(value: &Value) -> Option<Binding> {
    if let Some(action) = action(value) {
        return Some(Binding::Click(action));
    }
    let table = value.as_table()?;
    if !table.contains_key("Click") {
        return None;
    }
    table
        .iter()
        .map(|(name, value)| Some((Direction::parse(name)?, action(value)?)))
        .collect::<Option<_>>()
        .map(Binding::Gesture)
}

fn action(value: &Value) -> Option<Action> {
    if let Some(name) = value.as_str() {
        return NamedAction::from_config(name).map(Action::Named);
    }
    let path = value.get("OpenApplication")?.get("path")?.as_str()?;
    Action::application(path)
}
