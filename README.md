# OpenOptions

A local desktop app for Logitech peripherals on macOS, Windows, and Linux.
OpenOptions is an independent implementation in early development.

- **Devices** shows connected Logitech peripherals. **Add Device** opens Bolt
  and Unifying receiver pairing and offers Bluetooth pairing through the operating
  system. Bolt pairing shows the receiver’s keyboard code or mouse click sequence.
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
  The **Profile** bar adds overrides for individual applications. OpenOptions
  follows the foreground app and inherits default bindings for other controls.
- **Actions Ring** gives each profile eight configurable slots. Assign
  **Show Actions Ring** to a button, key, or gesture to open the ring at the
  pointer. The editor includes a preview.
- **Easy-Switch** in the **Device** section shows paired host channels and their
  available connection and OS details. Supported devices can rename channels
  or switch directly, including from a button assignment or an Actions Ring slot.
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
| Button and key customization | HID++ controls, system and media actions, shortcuts, application launch, and five-way mouse gestures. Direct and supported receiver-attached devices. |
| Device tuning | Adjustable DPI and SmartShift on compatible devices. Model-specific modes, scroll tuning, backlight, camera, and lighting are pending. |
| Connection | Bluetooth and USB discovery, Bolt and Unifying pairing, and Easy-Switch read, rename, and switch on supported hardware. Receiver model coverage and physical verification are pending. |
| Profiles | Local import, default mappings, and foreground-app overrides on macOS, Windows, and Linux X11. Device settings restore and backup are pending. |
| Workflow features | Eight-slot Actions Ring with per-app layouts. Nested ring folders, Smart Actions, and Flow are pending. |
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
The device gallery, detail workspace, profile bar, pairing flow, and Actions Ring
use OpenLogi’s layout and light palette as their design reference. OpenOptions
uses its own name and illustrations.
