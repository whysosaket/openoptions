use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Target(usize);

impl Target {
    pub(super) fn restore(self) -> bool {
        if current() == Some(self) {
            return true;
        }
        if !self.activate() {
            return false;
        }
        let deadline = Instant::now() + Duration::from_millis(300);
        while Instant::now() < deadline {
            if current() == Some(self) {
                return true;
            }
            thread::sleep(Duration::from_millis(15));
        }
        false
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};

    pub(in crate::controls) fn current() -> Option<Target> {
        objc2::rc::autoreleasepool(|_| {
            let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
            let pid = app.processIdentifier();
            (pid > 0).then_some(Target(pid as usize))
        })
    }

    impl Target {
        pub(in crate::controls) fn path(self) -> Option<PathBuf> {
            objc2::rc::autoreleasepool(|_| {
                let app =
                    NSRunningApplication::runningApplicationWithProcessIdentifier(self.0 as i32)?;
                Some(app.bundleURL()?.path()?.to_string().into())
            })
        }

        pub(super) fn activate(self) -> bool {
            objc2::rc::autoreleasepool(|_| {
                NSRunningApplication::runningApplicationWithProcessIdentifier(self.0 as i32)
                    .is_some_and(|app| {
                        app.activateWithOptions(NSApplicationActivationOptions::empty())
                    })
            })
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HWND},
        System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
        },
        UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
        },
    };

    pub(in crate::controls) fn current() -> Option<Target> {
        // The returned HWND is an opaque identifier and can be zero.
        let window = unsafe { GetForegroundWindow() };
        (!window.is_null()).then_some(Target(window as usize))
    }

    impl Target {
        pub(in crate::controls) fn path(self) -> Option<PathBuf> {
            // OpenProcess returns a nullable handle. Close it after querying the path.
            unsafe {
                let mut pid = 0;
                GetWindowThreadProcessId(self.0 as HWND, &mut pid);
                let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
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

        pub(super) fn activate(self) -> bool {
            // Win32 validates the opaque window identifier.
            unsafe { SetForegroundWindow(self.0 as HWND) != 0 }
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::cell::RefCell;
    use x11rb::{
        connection::Connection,
        errors::ReplyError,
        protocol::xproto::{Atom, AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, Window},
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

        fn active_window(&self) -> Result<Option<u32>, ReplyError> {
            let property = self
                .server
                .get_property(false, self.root, self.active, AtomEnum::WINDOW, 0, 1)?
                .reply()?;
            Ok(property
                .value32()
                .and_then(|mut values| values.next())
                .filter(|window| *window != 0))
        }

        fn pid(&self, window: Window) -> Result<Option<u32>, ReplyError> {
            let property = self
                .server
                .get_property(false, window, self.pid, AtomEnum::CARDINAL, 0, 1)?
                .reply()?;
            Ok(property.value32().and_then(|mut values| values.next()))
        }
    }

    thread_local! { static X11: RefCell<Option<Focus>> = const { RefCell::new(None) }; }

    fn with_focus<T>(work: impl FnOnce(&Focus) -> Result<Option<T>, ReplyError>) -> Option<T> {
        X11.with_borrow_mut(|focus| {
            if focus.is_none() {
                *focus = Focus::connect();
            }
            match work(focus.as_ref()?) {
                Ok(value) => value,
                Err(_) => {
                    *focus = None;
                    None
                }
            }
        })
    }

    pub(in crate::controls) fn current() -> Option<Target> {
        with_focus(Focus::active_window).map(|window| Target(window as usize))
    }

    impl Target {
        pub(in crate::controls) fn path(self) -> Option<PathBuf> {
            let pid = with_focus(|focus| focus.pid(self.0 as Window))?;
            std::fs::read_link(format!("/proc/{pid}/exe")).ok()
        }

        pub(super) fn activate(self) -> bool {
            with_focus(|focus| {
                let event =
                    ClientMessageEvent::new(32, self.0 as Window, focus.active, [2, 0, 0, 0, 0]);
                focus
                    .server
                    .send_event(
                        false,
                        focus.root,
                        EventMask::SUBSTRUCTURE_NOTIFY | EventMask::SUBSTRUCTURE_REDIRECT,
                        event,
                    )?
                    .check()?;
                Ok(Some(()))
            })
            .is_some()
        }
    }
}

pub(super) use platform::current;
