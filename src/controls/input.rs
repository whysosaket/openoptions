use crate::migration::{Action, NamedAction::*};
use enigo::{Button, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use std::{path::Path, process::Command, thread};

pub(super) fn live_platform_available() -> bool {
    !cfg!(target_os = "linux")
        || !(std::env::var("XDG_SESSION_TYPE").is_ok_and(|kind| kind == "wayland")
            || std::env::var_os("WAYLAND_DISPLAY").is_some())
            && std::env::var_os("DISPLAY").is_some()
}

pub(super) fn connect() -> Result<Enigo, String> {
    if !live_platform_available() {
        return Err("Live assignments on Linux require an X11 session.".into());
    }
    Enigo::new(&Settings::default()).map_err(|error| {
        #[cfg(target_os = "macos")]
        return format!("Allow OpenOptions in System Settings → Privacy & Security → Accessibility, then try again. {error}");
        #[cfg(not(target_os = "macos"))]
        return format!("Could not start desktop input. {error}");
    })
}

pub(super) fn supports(action: &Action) -> bool {
    match action {
        Action::Shortcut(value) => shortcut(value).is_some(),
        Action::ShowActionsRing | Action::SwitchHost(_) => true,
        Action::OpenApplication(path) => {
            let path = path.to_ascii_lowercase();
            Action::application(&path).is_some()
                && ((cfg!(target_os = "macos") && path.ends_with(".app"))
                    || (cfg!(target_os = "windows") && path.ends_with(".exe"))
                    || (cfg!(target_os = "linux") && path.ends_with(".desktop")))
        }
        Action::Named(AppExpose) => cfg!(target_os = "macos"),
        Action::Named(Desktop) => !cfg!(target_os = "linux"),
        Action::Named(MissionControl | PreviousDesktop | NextDesktop) => !cfg!(target_os = "linux"),
        Action::Named(_) => true,
    }
}

pub(super) fn perform(input: &mut Enigo, action: &Action) -> Result<(), String> {
    let primary = if cfg!(target_os = "macos") {
        Key::Meta
    } else {
        Key::Control
    };
    let ctrl = Key::Control;
    let meta = Key::Meta;
    let result = match action {
        Action::OpenApplication(path) => return launch(path),
        Action::ShowActionsRing | Action::SwitchHost(_) => {
            return Err("This action requires a device session.".into());
        }
        Action::Shortcut(value) => {
            let (modifiers, key) = shortcut(value).ok_or("Invalid shortcut.")?;
            chord(input, &modifiers, key)
        }
        Action::Named(name) => match name {
            NoAction => return Ok(()),
            MiddleClick => input.button(Button::Middle, Direction::Click),
            Back => input.button(Button::Back, Direction::Click),
            Forward => input.button(Button::Forward, Direction::Click),
            Copy => chord(input, &[primary], letter_key('c')),
            Cut => chord(input, &[primary], letter_key('x')),
            Paste => chord(input, &[primary], letter_key('v')),
            Undo => chord(input, &[primary], letter_key('z')),
            Redo => chord(input, &[primary, Key::Shift], letter_key('z')),
            MissionControl if cfg!(target_os = "macos") => chord(input, &[ctrl], Key::UpArrow),
            MissionControl => chord(input, &[meta], Key::Tab),
            AppExpose => chord(input, &[ctrl], Key::DownArrow),
            Desktop if cfg!(target_os = "macos") => input.key(Key::F11, Direction::Click),
            Desktop => chord(input, &[meta], letter_key('d')),
            PreviousDesktop | NextDesktop => {
                let modifiers: &[Key] = if cfg!(target_os = "macos") {
                    &[ctrl]
                } else {
                    &[ctrl, meta]
                };
                let key = if *name == PreviousDesktop {
                    Key::LeftArrow
                } else {
                    Key::RightArrow
                };
                chord(input, modifiers, key)
            }
            PlayPause => input.key(Key::MediaPlayPause, Direction::Click),
            PreviousTrack => input.key(Key::MediaPrevTrack, Direction::Click),
            NextTrack => input.key(Key::MediaNextTrack, Direction::Click),
            VolumeUp => input.key(Key::VolumeUp, Direction::Click),
            VolumeDown => input.key(Key::VolumeDown, Direction::Click),
            Mute => input.key(Key::VolumeMute, Direction::Click),
        },
    };
    result.map_err(|error| error.to_string())
}

fn chord(input: &mut Enigo, modifiers: &[Key], key: Key) -> enigo::InputResult<()> {
    let mut pressed = Vec::with_capacity(modifiers.len());
    let result = (|| {
        for &modifier in modifiers {
            input.key(modifier, Direction::Press)?;
            pressed.push(modifier);
        }
        input.key(key, Direction::Click)
    })();
    let mut cleanup = Ok(());
    for modifier in pressed.into_iter().rev() {
        if let Err(error) = input.key(modifier, Direction::Release) {
            cleanup = Err(error);
        }
    }
    result.and(cleanup)
}

fn shortcut(value: &str) -> Option<(Vec<Key>, Key)> {
    let mut parts = value.rsplit('+');
    let key = match parts.next()?.trim().to_ascii_lowercase().as_str() {
        "tab" => Key::Tab,
        "space" => Key::Space,
        "enter" => Key::Return,
        "escape" | "esc" => Key::Escape,
        "backspace" => Key::Backspace,
        "delete" => Key::Delete,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        text if text.len() == 1 && text.as_bytes()[0].is_ascii_alphanumeric() => {
            letter_key(char::from(text.as_bytes()[0]))
        }
        _ => return None,
    };
    let mut modifiers = Vec::new();
    for part in parts {
        let modifier = match part.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Key::Control,
            "alt" | "option" => Key::Alt,
            "shift" => Key::Shift,
            "cmd" | "command" | "super" | "meta" | "win" => Key::Meta,
            _ => return None,
        };
        if modifiers.contains(&modifier) {
            return None;
        }
        modifiers.push(modifier);
    }
    modifiers.reverse();
    Some((modifiers, key))
}

fn letter_key(character: char) -> Key {
    if cfg!(target_os = "windows") {
        Key::Other(u32::from(character.to_ascii_uppercase()))
    } else {
        Key::Unicode(character)
    }
}

fn launch(path: &str) -> Result<(), String> {
    if !Path::new(path).exists() {
        return Err("The selected application cannot be found.".into());
    }
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("/usr/bin/open");
        command.arg("-a").arg(path);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = Command::new(path);
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = Command::new("gio");
        command.arg("launch").arg(path);
        command
    };
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
