# OpenOptions workflow coverage

## Environment

The local test system runs macOS 27.0.1 with a Bluetooth MX Master 3S. The baseline and a read-only probe detected no Bolt or Unifying receiver. A baseline native app bundle and the probe both enumerated the mouse and received an Input Monitoring error when opening its HID++ interface.

## Executed checks

| Workflow | Evidence | Result |
| --- | --- | --- |
| Baseline devices and receiver scan | Physical Bluetooth MX Master 3S on macOS, device gallery and native receiver picker, read-only backend probe | One connected mouse. Zero supported receivers. Native missing-receiver state and cancel exercised. |
| Baseline import contracts | Native macOS build with the SQLite store redirected to a temporary location and a disposable TOML import | Preview showed two button bindings, five gestures, DPI and an unsupported binding. Preview created no store. Import created one profile and the repeat left one row. The source checksum matched before and after. |
| Baseline automated checks | cargo test --locked --all-targets on macOS | 9 passed. Covers model-key bindings across store reopen, legacy profile JSON, fallback, import WAL snapshot and repeat import, mouse challenge parsing, capabilities, DPI decoding and gesture direction. |
| Phase 1 device contracts | [PR #2](https://github.com/whysosaket/openoptions/pull/2), 54dd922. Native Apple silicon candidate and 14 automated tests | Physical inventory refresh, permission error and retry, missing receiver, Bluetooth Settings handoff and app-side cancel passed. Device identity and fallback, tuning drafts, battery parsing, and late pairing were checked with fixtures. Input Monitoring blocked live HID++ reads/writes. All six macOS, Ubuntu and Windows CI jobs passed. |
| Phase 1 native profile persistence | macOS build with a fixture mouse, actual installed TextEdit and isolated SQLite | Saved Default Copy and TextEdit Paste. Both survived app restart. Clearing and saving the override preserved Default Copy and left zero per-app bindings. |
| Phase 2 Devices, pairing and import | [PR #3](https://github.com/whysosaket/openoptions/pull/3), 5c9a69f, and 15 automated checks | Native actual mouse search and return, receiver absence and retry, and discovered import preview passed. Native fixture verified gallery and list at 650 × 470. All six CI jobs passed. |
| Phase 2 manual import, library and draft save | Native eframe app with scripted input and a fixture inventory and SQLite store. Evidence under openoptions-evidence/phase-2 | Visible invalid-file message, valid preview created no database, repeat import left one profile, restart showed it in the library, stage left zero assignments in storage, Save wrote three controls including five gesture directions. One unsupported assignment was reported. |
| Phase 3 controls, permissions, and Actions Ring | [PR #4](https://github.com/whysosaket/openoptions/pull/4), 5ffc5ae, 17 tests and native OpenGL screenshots | Fixture hotspot, gesture mode, searches, ring editor, range/step and host rows. Actual installed TextEdit and macOS permissions. All six CI jobs passed. Physical input, tuning writes, host changes and ring runtime remain live gaps. |
| Phase 3 fixture interaction | Native macOS app renderer with actual installed app picker and isolated SQLite | Recorded and saved K. Saved different overrides for Calculator and TextEdit, reopened and cleared one to inheritance. Ring preview selected a populated slot without saving, and channel rename cancelled to its old name. |
| Phase 4 responsive and recovery checks | Native OpenGL app at 1080 × 720 and 650 × 470 points, using both physical and synthetic inventory | Reviewed loading, empty, filtered, disconnected, failed scan and retry, long labels, populated and empty slots, compact device sections, pointer tracks and the complete invalid-file error. At the minimum size all eight ring slots were visible. Right Arrow and Enter selected slot 2, and Escape restored Preview focus. |

## Upstream parity and live verification requirements

| Area | OpenOptions backend baseline | Needed to verify or expand |
| --- | --- | --- |
| Flow peer trust and discovery | No peer discovery, address pairing or trust store | Two hosts on a supported network. Peer authorization, setup, manual address, retry and offline states. |
| Flow placement and switching | Easy-Switch commands on supported hardware. No Flow peer layout or edge-trigger engine | Two paired hosts and a compatible device. Placement mapping and cross-host switch confirmation. |
| Flow clipboard and linked keyboard | No clipboard transport or keyboard linking | Two trusted hosts and a compatible keyboard. Verify clipboard formats and independent link behavior. |
| Smart Actions | Single named actions, shortcuts and application launch. No sequences, templates, delays or trigger catalog | A workflow model with validation and cancellation. Hardware and OS checks for each proposed trigger or action. |
| Expanded Actions Ring | Eight action slots per layer with a hardware-triggered live overlay | A compatible programmable device for live testing. Folders, continuous adjustments, plugins and global shortcuts would need their own models and execution paths. |
| Keyboard behavior | HID++ programmable keys where advertised. No Fn Lock, layout or backlight controls | A supported programmable Logitech keyboard. Each additional setting needs capability detection and its own read/write verification. |
| Lighting and cameras | Discovery may list products. No lighting or camera controls | A supported Litra device and camera for power, brightness, color temperature and camera events. Camera controls need a separate capability backend. |
| Haptics, presentation and creative consoles | No dedicated haptic, presenter or creative-console control paths | Compatible hardware for each workflow. Additional HID or product-specific adapters. |
| Additional pointer and scroll settings | Supported DPI values and SmartShift mode and threshold. Some upstream values import as metadata | Hardware-specific branches for pointer speed scales, presets, scroll resolution and inversion, smooth and momentum scrolling, thumb wheel and scroll force. |
| Inventory and receiver management | Gallery and list, reconnection, discovery and pairing | Carousel and gallery aliases need their own UI/data model. Unpairing needs a real backend operation and a compatible receiver. |
| App management | Local settings, profiles and platform access only | Updater, login startup, appearance catalogs, asset downloads, notifications and plugin services would each require separate scope. |
| Windows runtime | Windows backend and CI present | Windows desktop with a compatible device for discovery, receiver pairing, capability reads, saving, foreground identification, live input and focus. |
| Linux runtime | Linux backend and CI present. Runtime desktop actions require X11 | Native X11 desktop with a compatible device and device permissions for the same workflows. Linux Wayland needs a separate backend for system-wide live actions. |

Reference screens are under screenshots/ in this folder. Upstream workflow sources include [OpenLogi](https://github.com/AprilNEA/OpenLogi), [Logitech Flow](https://support.logi.com/hc/en-001/articles/1500005634742-What-is-Logitech-Flow-and-how-do-I-set-it-up-and-troubleshoot-it), [Logitech Actions Ring](https://support.logi.com/hc/en-001/articles/17844647906967-Logi-Options-Actions-Ring), and [Logitech Smart Actions](https://support.logi.com/hc/en-ch/articles/14307858722327-How-to-create-a-new-Smart-Actions-on-Logi-Options).

For the Phase 1 tuning failure fixture, the pending value stayed at 4100 DPI and the last reading stayed at 1500 DPI after the unavailable hardware rejected Apply. The app offered another read. See openoptions-evidence/phase-1/fixture-pointer-failure.png.

## Final comparisons

| Area | Reference screens | OpenOptions captures |
| --- | --- | --- |
| Inventory, device selection and pairing | OpenLogi 01–06 and published multi-device gallery, Options+ dashboard and pairing methods | phase-4/comparison-devices.jpg, fixture-compact-devices.png, fixture-disconnected.png, add-device.png |
| Button and key editing, gestures and app profiles | OpenLogi 07–16 and published keyboard, Options+ device customization and app profiles | phase-3/fixture-gestures.png and fixture-app-search.png, phase-4/fixture-compact-controls.png, fixture-compact-selected.png, fixture-compact-keys.png and fixture-compact-action-picker.png |
| Actions Ring | OpenLogi 17–18 and Options+ 15–17 and 26 | phase-3/fixture-ring-preview.png, phase-4/fixture-compact-ring.png, fixture-ring-keyboard.png and fixture-ring-preview-dismissed.png |
| Point & Scroll and Easy-Switch | OpenLogi 19–21, 33–35 and published SmartShift, Options+ pointer settings | phase-3/fixture-channel-rename.png, phase-4/fixture-compact-pointer.png and fixture-compact-channels.png |
| Settings and recovery | OpenLogi 22–29 and Options+ settings and startup recovery | phase-4/settings.png, fixture-scan-error.png and fixture-scan-retry.png |
| Import and saved profiles | Original OpenOptions workflow | phase-4/comparison-import-error.jpg and phase-2/fixture-import-review.png, fixture-library.png and fixture-staged-assignments.png |

The macOS desktop locked during verification. Subsequent native screenshots came from OpenOptions' own OpenGL renderer in an isolated fixture build. The fixture supplied hardware values where marked and used macOS for live inventory, installed-app discovery and permission preflight where stated. Windows and Ubuntu CI exercised builds and tests. Native input, devices and permission recovery on each platform still require their own checks.
