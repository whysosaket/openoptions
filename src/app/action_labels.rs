use openoptions::migration::{Action, NamedAction};

pub(super) fn action_label(action: &Action) -> &str {
    match action {
        Action::OpenApplication(path) => path,
        Action::Named(action) => match action {
            NamedAction::NoAction => "Do nothing",
            NamedAction::MiddleClick => "Middle click",
            NamedAction::Back => "Back",
            NamedAction::Forward => "Forward",
            NamedAction::Copy => "Copy",
            NamedAction::Cut => "Cut",
            NamedAction::Paste => "Paste",
            NamedAction::Undo => "Undo",
            NamedAction::Redo => "Redo",
            NamedAction::MissionControl => "Mission Control",
            NamedAction::AppExpose => "App Exposé",
            NamedAction::Desktop => "Show desktop",
            NamedAction::PreviousDesktop => "Previous desktop",
            NamedAction::NextDesktop => "Next desktop",
            NamedAction::PlayPause => "Play or pause",
            NamedAction::PreviousTrack => "Previous track",
            NamedAction::NextTrack => "Next track",
            NamedAction::VolumeUp => "Volume up",
            NamedAction::VolumeDown => "Volume down",
            NamedAction::Mute => "Mute",
        },
    }
}
