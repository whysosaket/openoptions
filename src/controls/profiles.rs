use super::Bindings;
use crate::migration::Action;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

pub const RING_SLOTS: usize = 8;
pub type Ring = [Option<Action>; RING_SLOTS];

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layer {
    pub bindings: Bindings,
    pub ring: Ring,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profiles {
    pub default: Layer,
    #[serde(default)]
    pub applications: BTreeMap<String, ApplicationProfile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationProfile {
    pub name: String,
    pub layer: Layer,
}

impl Profiles {
    pub fn add(&mut self, path: &str) -> Result<String, String> {
        let path = Path::new(path.trim());
        let application = if cfg!(target_os = "macos") {
            path.is_dir()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
        } else {
            path.is_file()
                && (!cfg!(target_os = "windows")
                    || path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe")))
        };
        if !path.is_absolute() || !application {
            return Err("Choose the absolute path of an installed application.".into());
        }
        let key = application_id(path).ok_or("The application path has no name.")?;
        if self.applications.contains_key(&key) {
            return Err("This application already has a profile.".into());
        }
        let name = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or(&key);
        self.applications.insert(
            key.clone(),
            ApplicationProfile {
                name: name.into(),
                layer: Layer {
                    ring: self.default.ring.clone(),
                    ..Layer::default()
                },
            },
        );
        Ok(key)
    }

    pub fn layer(&self, app: Option<&str>) -> &Layer {
        app.and_then(|key| self.applications.get(key))
            .map_or(&self.default, |profile| &profile.layer)
    }

    pub fn layer_mut(&mut self, app: Option<&str>) -> &mut Layer {
        app.and_then(|key| self.applications.get_mut(key))
            .map_or(&mut self.default, |profile| &mut profile.layer)
    }

    pub fn effective_bindings(&self, app: Option<&str>) -> Bindings {
        let mut bindings = self.default.bindings.clone();
        if let Some(profile) = app.and_then(|key| self.applications.get(key)) {
            bindings.extend(profile.layer.bindings.clone());
        }
        bindings
    }

    pub fn has_bindings(&self) -> bool {
        !self.default.bindings.is_empty()
            || self
                .applications
                .values()
                .any(|profile| !profile.layer.bindings.is_empty())
    }
}

fn application_id(path: &Path) -> Option<String> {
    if !path.is_absolute() {
        return None;
    }
    let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let key = normalized.to_str()?.to_owned();
    let key = if cfg!(target_os = "windows") {
        key.trim_start_matches(r"\\?\").to_lowercase()
    } else {
        key
    };
    Some(key)
}

pub(super) fn frontmost() -> Option<String> {
    application_id(&foreground_path()?)
}

#[cfg(target_os = "macos")]
fn foreground_path() -> Option<std::path::PathBuf> {
    objc2::rc::autoreleasepool(|_| {
        let app = objc2_app_kit::NSWorkspace::sharedWorkspace().frontmostApplication()?;
        Some(app.bundleURL()?.path()?.to_string().into())
    })
}

#[cfg(target_os = "windows")]
fn foreground_path() -> Option<std::path::PathBuf> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    };
    // These functions return nullable handles. Close the process handle on every path.
    unsafe {
        let window = GetForegroundWindow();
        if window.is_null() {
            return None;
        }
        let mut process_id = 0;
        GetWindowThreadProcessId(window, &mut process_id);
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
        if process.is_null() {
            return None;
        }
        let mut path = vec![0u16; 32768];
        let mut length = path.len() as u32;
        let status = QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length);
        CloseHandle(process);
        (status != 0).then(|| {
            use std::os::windows::ffi::OsStringExt;
            std::ffi::OsString::from_wide(&path[..length as usize]).into()
        })
    }
}

#[cfg(target_os = "linux")]
fn foreground_path() -> Option<std::path::PathBuf> {
    use std::cell::RefCell;
    use x11rb::{
        connection::Connection,
        protocol::xproto::{Atom, AtomEnum, ConnectionExt, Window},
        rust_connection::RustConnection,
    };

    struct Focus {
        server: RustConnection,
        root: Window,
        active: Atom,
        pid: Atom,
    }
    impl Focus {
        fn connect() -> Option<Self> {
            let (server, screen) = x11rb::connect(None).ok()?;
            let root = server.setup().roots.get(screen)?.root;
            let active = server
                .intern_atom(false, b"_NET_ACTIVE_WINDOW")
                .ok()?
                .reply()
                .ok()?
                .atom;
            let pid = server
                .intern_atom(false, b"_NET_WM_PID")
                .ok()?
                .reply()
                .ok()?
                .atom;
            Some(Self {
                server,
                root,
                active,
                pid,
            })
        }
        fn pid(&self) -> Result<Option<u32>, x11rb::errors::ReplyError> {
            let active = self
                .server
                .get_property(false, self.root, self.active, AtomEnum::WINDOW, 0, 1)?
                .reply()?;
            let Some(window) = active
                .value32()
                .and_then(|mut values| values.next())
                .filter(|window| *window != 0)
            else {
                return Ok(None);
            };
            let property = self
                .server
                .get_property(false, window, self.pid, AtomEnum::CARDINAL, 0, 1)?
                .reply()?;
            Ok(property.value32().and_then(|mut values| values.next()))
        }
    }
    thread_local! { static X11: RefCell<Option<Focus>> = const { RefCell::new(None) }; }
    X11.with_borrow_mut(|focus| {
        if focus.is_none() {
            *focus = Focus::connect();
        }
        match focus.as_ref()?.pid() {
            Ok(Some(pid)) => std::fs::read_link(format!("/proc/{pid}/exe")).ok(),
            Ok(None) => None,
            Err(_) => {
                *focus = None;
                None
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::{Binding, NamedAction};

    #[test]
    fn profiles_override_only_the_controls_that_they_define() {
        let mut profiles = Profiles::default();
        profiles
            .default
            .bindings
            .insert(1, Binding::Click(Action::Named(NamedAction::Copy)));
        profiles.applications.insert(
            "mypath".into(),
            ApplicationProfile {
                name: "Demo".into(),
                layer: Layer::default(),
            },
        );
        profiles
            .layer_mut(Some("mypath"))
            .bindings
            .insert(2, Binding::Click(Action::Named(NamedAction::Paste)));
        assert_eq!(profiles.effective_bindings(Some("mypath")).len(), 2);
        assert_eq!(profiles.layer(Some("unknown")), &profiles.default);
    }
}
