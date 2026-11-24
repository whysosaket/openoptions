use openoptions::migration::{
    self, Action, Binding, Button, NamedAction, ScrollDirection, Source, SourceKind, WheelMode,
};
use openoptions::store::Store;
use rusqlite::Connection;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn toml_preview_preserves_supported_values_and_skips_scripts() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let text = r#"
[app_settings]
low_battery_notifications = true

[devices."unit:12345678"]
dpi = 1200
smooth_wheel = false
invert_scroll = true

[devices."unit:12345678".identity]
display_name = "Test mouse"

[devices."unit:12345678".smartshift]
mode = "free"
auto_disengage = 18

[devices."unit:12345678".bindings]
MiddleClick = "Paste"
Back = { OpenApplication = { path = "C:\\Program Files\\Test\\Test.exe" } }
Forward = { RunShell = { script = "example" } }

[devices."unit:12345678".bindings.GestureButton]
Click = "Copy"
Up = "MissionControl"
"#;
    std::fs::write(&path, text).unwrap();
    let preview = migration::preview(&Source {
        kind: SourceKind::Community,
        path: path.clone(),
    })
    .unwrap();
    let profile = &preview.profiles[0];
    assert_eq!(profile.device_name, "Test mouse");
    assert_eq!(profile.settings.dpi, Some(1200));
    assert_eq!(
        profile.settings.scroll_direction,
        Some(ScrollDirection::InvertedFromSystem)
    );
    assert_eq!(profile.settings.wheel_mode, Some(WheelMode::FreeSpin));
    assert_eq!(profile.settings.smart_shift_threshold, Some(18));
    assert_eq!(profile.settings.notify_low_battery, Some(true));
    assert_eq!(
        profile.settings.buttons[&Button::Middle],
        Binding::Click(Action::Named(NamedAction::Paste))
    );
    assert!(
        matches!(&profile.settings.buttons[&Button::Back], Binding::Click(Action::OpenApplication(path)) if path.ends_with("Test.exe"))
    );
    assert!(
        matches!(&profile.settings.buttons[&Button::Gesture], Binding::Gesture(map) if map.len() == 2)
    );
    assert!(!profile.settings.buttons.contains_key(&Button::Forward));
    assert_eq!(preview.skipped["Unsupported button binding"], 1);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
}

fn payload() -> serde_json::Value {
    json!({
        "ever_connected_devices": {
            "devices": [
                {"deviceType": "MOUSE", "slotPrefix": "mouse", "deviceModel": "Test Mouse", "serialNumber": "TEST"},
                {"deviceType": "MOUSE", "slotPrefix": "mouse_extra", "deviceModel": "Other Mouse"}
            ]
        },
        "profile_keys": ["default", "editor"],
        "default": {
            "name": "PROFILE_NAME_DEFAULT",
            "assignments": [
                {"slotId": "mouse_c82", "card": {"macro": {"type": "MOUSE", "mouse": {"action": "BUTTON", "hidUsage": 3}}}},
                {"slotId": "mouse_c83", "card": {"macro": {"type": "OPEN_FILE_FOLDER", "openFileFolder": {"path": "/Applications/Test.app"}}}},
                {"slotId": "mouse_c86", "card": {"macro": {"type": "OPEN_FILE_FOLDER", "openFileFolder": {"path": "https://example.com/"}}}},
                {"slotId": "mouse_mouse_settings", "card": {"mouseSettings": {"pointerSpeed": 63}}},
                {"slotId": "mouse_mouse_scroll_wheel_settings", "card": {"mouseScrollWheelSettings": {"dir": "NATURAL", "isSmooth": true, "smartshift": {"mode": "FREESPIN", "sensitivity": 83}}}},
                {"slotId": "mouse_extra_c86", "card": {"macro": {"type": "DO_NOTHING"}}}
            ]
        },
        "editor": {"name": "Editor", "assignments": []}
    })
}

#[test]
fn sqlite_preview_reads_latest_wal_snapshot_and_the_store_is_idempotent() {
    let temp = tempdir().unwrap();
    let source_path = temp.path().join("settings.db");
    let source = Connection::open(&source_path).unwrap();
    source
        .execute_batch(
            "PRAGMA journal_mode=WAL; CREATE TABLE data (_id INTEGER PRIMARY KEY, file BLOB);",
        )
        .unwrap();
    source
        .execute("INSERT INTO data VALUES (1, ?1)", ["invalid old payload"])
        .unwrap();
    source
        .execute(
            "INSERT INTO data VALUES (2, ?1)",
            [serde_json::to_vec(&payload()).unwrap()],
        )
        .unwrap();
    let preview = migration::preview(&Source {
        kind: SourceKind::Vendor,
        path: source_path.clone(),
    })
    .unwrap();
    assert_eq!(preview.profiles.len(), 2);
    let settings = &preview.profiles[0].settings;
    assert_eq!(settings.dpi, None);
    assert_eq!(settings.scroll_direction, Some(ScrollDirection::Natural));
    assert_eq!(settings.buttons.len(), 2);
    assert_eq!(preview.profiles[1].settings.buttons.len(), 1);
    assert_eq!(preview.skipped["Per-app profiles"], 1);
    assert_eq!(preview.skipped["Unsupported button binding or gesture"], 1);
    assert_eq!(
        preview.skipped["Options+ pointer speed needs device DPI read-back"],
        1
    );
    let count: u32 = source
        .query_row("SELECT COUNT(*) FROM data", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 2);

    let storage = temp.path().join("profiles.sqlite3");
    let mut store = Store::open(&storage).unwrap();
    assert_eq!(store.import(SourceKind::Vendor, &preview).unwrap(), 2);
    assert_eq!(store.import(SourceKind::Vendor, &preview).unwrap(), 0);
    drop(store);
    let saved = Store::open(&storage).unwrap().profiles().unwrap();
    assert_eq!(saved[1], (SourceKind::Vendor, preview.profiles[0].clone()));
}
