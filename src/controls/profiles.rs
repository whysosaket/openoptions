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
    application_id(&super::focus::current()?.path()?)
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
