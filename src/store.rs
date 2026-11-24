use crate::migration::{Preview, Profile, SourceKind};
use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn location() -> Result<PathBuf, String> {
        let dirs = directories::ProjectDirs::from("io.github", "whysosaket", "OpenOptions")
            .ok_or("Could not locate the application data directory.")?;
        Ok(dirs.data_local_dir().join("profiles.sqlite3"))
    }

    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let connection = Connection::open(path).map_err(|error| error.to_string())?;
        connection
            .busy_timeout(Duration::from_secs(2))
            .map_err(|error| error.to_string())?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS profiles (
                id INTEGER PRIMARY KEY,
                source TEXT NOT NULL,
                profile_json TEXT NOT NULL,
                UNIQUE (source, profile_json)
            );",
            )
            .map_err(|error| error.to_string())?;
        Ok(Self { connection })
    }

    pub fn import(&mut self, source: SourceKind, preview: &Preview) -> Result<usize, String> {
        let source = serde_json::to_string(&source).map_err(|error| error.to_string())?;
        let transaction = self
            .connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let mut imported = 0;
        for profile in &preview.profiles {
            let data = serde_json::to_string(profile).map_err(|error| error.to_string())?;
            imported += transaction
                .execute(
                    "INSERT INTO profiles (source, profile_json) VALUES (?1, ?2)
                 ON CONFLICT (source, profile_json) DO NOTHING",
                    params![source, data],
                )
                .map_err(|error| error.to_string())?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(imported)
    }

    pub fn profiles(&self) -> Result<Vec<(SourceKind, Profile)>, String> {
        let mut query = self
            .connection
            .prepare("SELECT source, profile_json FROM profiles ORDER BY id DESC")
            .map_err(|error| error.to_string())?;
        let mut rows = query.query([]).map_err(|error| error.to_string())?;
        let mut profiles = Vec::new();
        while let Some(row) = rows.next().map_err(|error| error.to_string())? {
            let kind: String = row.get(0).map_err(|error| error.to_string())?;
            let profile: String = row.get(1).map_err(|error| error.to_string())?;
            profiles.push((
                serde_json::from_str(&kind).map_err(|error| error.to_string())?,
                serde_json::from_str(&profile).map_err(|error| error.to_string())?,
            ));
        }
        Ok(profiles)
    }
}
