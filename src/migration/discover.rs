use super::{Source, SourceKind};
use std::collections::HashSet;
use std::path::PathBuf;

const COMMUNITY_DIRECTORY: &str = "openlogi";

pub fn detect_sources() -> Vec<Source> {
    let mut candidates = Vec::new();
    if let Some(base) = directories::BaseDirs::new() {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
        {
            candidates.push((
                SourceKind::Community,
                xdg.join(COMMUNITY_DIRECTORY).join("config.toml"),
            ));
        }
        candidates.push((
            SourceKind::Community,
            base.home_dir()
                .join(".config")
                .join(COMMUNITY_DIRECTORY)
                .join("config.toml"),
        ));
        #[cfg(target_os = "macos")]
        candidates.push((
            SourceKind::Vendor,
            base.home_dir()
                .join("Library/Application Support/LogiOptionsPlus/settings.db"),
        ));
        #[cfg(target_os = "windows")]
        for base in [base.data_local_dir(), base.config_dir()] {
            let directory = base.join("LogiOptionsPlus");
            candidates.extend([
                (SourceKind::Vendor, directory.join("settings.db")),
                (SourceKind::Vendor, directory.join("settings/settings.db")),
            ]);
        }
    }
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter_map(|(kind, path)| {
            let canonical = path.canonicalize().ok()?;
            (canonical.is_file() && seen.insert(canonical)).then_some(Source { kind, path })
        })
        .collect()
}
