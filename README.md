# OpenOptions

A local desktop app for Logitech peripherals on macOS, Windows, and Linux.
OpenOptions is an independent implementation in early development.

- **Devices** lists connected Logitech HID products, including Bolt and
  Unifying receivers. Bluetooth pairing opens the operating system’s settings.
- **Import settings** looks for config files from supported apps in their
  standard locations. Select **Preview**, review the mapped settings and
  skipped items, then choose **Import** to save local profiles. You can also
  supply a copied TOML, SQLite, or Options+ JSON settings file.
- **Profiles** shows saved imports. Reimporting the same settings is safe.
  OpenOptions reads source files and saves profiles to its own local database.
- **Buttons and Keys** lists programmable controls on directly connected devices.
  Choose a control, assign an action or keyboard shortcut, and save. Mouse
  gestures have separate assignments for a press and four swipe directions.
  Use **Load imported assignments** to copy supported mappings into the editor.
- **Point & Scroll** reads and applies DPI and SmartShift settings on compatible
  devices. OpenOptions shows these controls when the connected device reports
  the corresponding capabilities.

To use saved button or key mappings, choose **Enable live controls** and keep
the desktop app running. OpenOptions releases temporary reporting when you
disable live controls or quit. On macOS, grant Input Monitoring for device
access and Accessibility for desktop actions. Live desktop actions use X11 on
Linux. On devices without raw gesture movement, swipes use the cursor's
displacement and can be limited by screen edges.

Import covers common mouse bindings, gestures, application launch paths,
scrolling, and available DPI and SmartShift values. Options+ uses proprietary
pointer speed and sensitivity scales, which the preview reports as skipped.
Linux users can import a settings database copied from macOS or Windows.

## Options+ parity

| Area | Current coverage |
| --- | --- |
| Button and key customization | Direct HID++ controls, system and media actions, shortcuts, application launch, and five-way mouse gestures. |
| Device tuning | Adjustable DPI and SmartShift on compatible devices. Model-specific modes, scroll tuning, backlight, camera, and lighting are pending. |
| Connection | Direct Bluetooth and USB device discovery and system Bluetooth pairing. Receiver-attached controls, in-app receiver pairing, and Easy-Switch are pending. |
| Profiles | Local import and per-model assignments. Per-app switching, device settings restore, and backup are pending. |
| Workflow features | Actions Ring, Smart Actions, and Flow are pending. |
| Linux live input | X11. System-wide Wayland actions are pending. |

## Build

Use stable Rust. On Debian or Ubuntu, install the development libraries for
OpenGL, Wayland, X11, and xkbcommon.

```sh
cargo run --locked
cargo test --locked --all-targets
```

## Desktop UI

Use the shared palette, typography, and controls in `src/app/theme.rs`. New
lists can use the workspace header, toolbar, and index rows in
`src/app/workspace.rs`. Keep collection navigation in the sidebar and place
item actions in the workspace header or the relevant row.

## Credits

[OpenLogi](https://github.com/AprilNEA/OpenLogi) inspired this project.
The device workspace uses OpenLogi’s control-and-inspector layout as its reference.
The app shell takes inspiration from Prathamesh’s
[shared index](https://github.com/quaylabshq/orka/pull/198) and
[detail layouts](https://github.com/quaylabshq/orka/pull/72) for Orka.
