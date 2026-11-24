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

This build supports migration of common mouse button and gesture mappings,
application launch paths, scrolling, and available DPI and SmartShift values.
Options+ uses its own scales for pointer speed and SmartShift sensitivity.
Those values are reported as skipped until they can be validated on the device.
Linux users can import a database copied from macOS or Windows.

Live remapping, applying saved profiles, and in-app receiver pairing are under
development. OpenOptions runs as one desktop process with no background agent
or network service.

## Build

Use stable Rust. On Debian or Ubuntu, install the development libraries for
OpenGL, Wayland, X11, and xkbcommon.

```sh
cargo run --locked
cargo test --locked --all-targets
```

## Credits

[OpenLogi](https://github.com/AprilNEA/OpenLogi) inspired this project.
