mod community;
mod discover;
mod vendor;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub use discover::detect_sources;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Community,
    Vendor,
}

impl SourceKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Community => "OpenLogi",
            Self::Vendor => "Logi Options+",
        }
    }

    pub fn from_path(path: &Path) -> Result<Self, String> {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some(ext) if ext.eq_ignore_ascii_case("toml") => Ok(Self::Community),
            Some(ext)
                if ["db", "sqlite", "sqlite3", "json"]
                    .iter()
                    .any(|candidate| ext.eq_ignore_ascii_case(candidate)) =>
            {
                Ok(Self::Vendor)
            }
            _ => Err("Choose a TOML config, SQLite database, or Options+ JSON export.".into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub kind: SourceKind,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preview {
    pub profiles: Vec<Profile>,
    pub skipped: BTreeMap<&'static str, usize>,
}

impl Preview {
    fn skip(&mut self, reason: &'static str) {
        *self.skipped.entry(reason).or_default() += 1;
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub device_name: String,
    pub device_key: String,
    pub settings: Settings,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub dpi: Option<u16>,
    pub smooth_scroll: Option<bool>,
    pub scroll_direction: Option<ScrollDirection>,
    pub wheel_mode: Option<WheelMode>,
    pub smart_shift_threshold: Option<u8>,
    pub notify_low_battery: Option<bool>,
    pub buttons: BTreeMap<Button, Binding>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollDirection {
    Natural,
    Standard,
    InvertedFromSystem,
    MatchSystem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WheelMode {
    Ratchet,
    FreeSpin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    Middle,
    Back,
    Forward,
    Gesture,
    ModeShift,
}

impl Button {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Middle => "Middle button",
            Self::Back => "Back button",
            Self::Forward => "Forward button",
            Self::Gesture => "Gesture button",
            Self::ModeShift => "Mode Shift button",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Click,
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    fn parse(value: &str) -> Option<Self> {
        Some(match value.to_ascii_lowercase().as_str() {
            "click" => Self::Click,
            "up" => Self::Up,
            "down" => Self::Down,
            "left" => Self::Left,
            "right" => Self::Right,
            _ => return None,
        })
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Click => "Click",
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Binding {
    Click(Action),
    Gesture(BTreeMap<Direction, Action>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Named(NamedAction),
    OpenApplication(String),
    Shortcut(String),
    ShowActionsRing,
    SwitchHost(u8),
}

impl Action {
    pub fn application(path: &str) -> Option<Self> {
        let lower = path.to_ascii_lowercase();
        let unix = path.starts_with('/') && !path.starts_with("//");
        let windows = path.as_bytes().get(0..3).is_some_and(|head| {
            head[0].is_ascii_alphabetic() && head[1] == b':' && matches!(head[2], b'\\' | b'/')
        });
        let local_app = (unix && (lower.ends_with(".app") || lower.ends_with(".desktop")))
            || (windows && lower.ends_with(".exe"));
        (local_app && !path.chars().any(char::is_control))
            .then(|| Self::OpenApplication(path.into()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamedAction {
    NoAction,
    MiddleClick,
    Back,
    Forward,
    Copy,
    Cut,
    Paste,
    Undo,
    Redo,
    MissionControl,
    AppExpose,
    Desktop,
    PreviousDesktop,
    NextDesktop,
    PlayPause,
    PreviousTrack,
    NextTrack,
    VolumeUp,
    VolumeDown,
    Mute,
}

impl NamedAction {
    pub const ALL: [Self; 20] = [
        Self::NoAction,
        Self::MiddleClick,
        Self::Back,
        Self::Forward,
        Self::Copy,
        Self::Cut,
        Self::Paste,
        Self::Undo,
        Self::Redo,
        Self::MissionControl,
        Self::AppExpose,
        Self::Desktop,
        Self::PreviousDesktop,
        Self::NextDesktop,
        Self::PlayPause,
        Self::PreviousTrack,
        Self::NextTrack,
        Self::VolumeUp,
        Self::VolumeDown,
        Self::Mute,
    ];

    fn from_config(value: &str) -> Option<Self> {
        Some(match value {
            "None" => Self::NoAction,
            "MiddleClick" => Self::MiddleClick,
            "Back" => Self::Back,
            "Forward" => Self::Forward,
            "Copy" => Self::Copy,
            "Cut" => Self::Cut,
            "Paste" => Self::Paste,
            "Undo" => Self::Undo,
            "Redo" => Self::Redo,
            "MissionControl" => Self::MissionControl,
            "AppExpose" => Self::AppExpose,
            "ShowDesktop" | "Desktop" => Self::Desktop,
            "PreviousDesktop" => Self::PreviousDesktop,
            "NextDesktop" => Self::NextDesktop,
            "PlayPause" => Self::PlayPause,
            "PrevTrack" => Self::PreviousTrack,
            "NextTrack" => Self::NextTrack,
            "VolumeUp" => Self::VolumeUp,
            "VolumeDown" => Self::VolumeDown,
            "VolumeMute" | "MuteVolume" => Self::Mute,
            _ => return None,
        })
    }
}

pub fn preview(source: &Source) -> Result<Preview, String> {
    const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
    let meta = source
        .path
        .metadata()
        .map_err(|error| format!("Could not open settings: {error}"))?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return Err("Choose a settings file smaller than 128 MB.".into());
    }
    match source.kind {
        SourceKind::Community => community::load(&source.path),
        SourceKind::Vendor => vendor::load(&source.path),
    }
}

fn read_text(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|error| format!("Could not read settings: {error}"))
}
