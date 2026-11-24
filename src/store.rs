use crate::controls::{Bindings, Layer, Profiles};
use crate::devices::Device;
use crate::migration::{Preview, Profile, SourceKind};
use rusqlite::{Connection, OptionalExtension, params};
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
            );
            CREATE TABLE IF NOT EXISTS control_profiles (
                connection TEXT NOT NULL,
                product_id INTEGER NOT NULL,
                bindings_json TEXT NOT NULL,
                PRIMARY KEY (connection, product_id)
            );",
            )
            .map_err(|error| error.to_string())?;
        Ok(Self { connection })
    }

    pub fn customizations(&self, device: &Device) -> Result<Profiles, String> {
        let json = self.connection.query_row(
            "SELECT bindings_json FROM control_profiles WHERE connection = ?1 AND product_id = ?2",
            params![device.storage_connection().as_ref(), device.product_id],
            |row| row.get::<_, String>(0),
        ).optional().map_err(|error| error.to_string())?;
        let Some(json) = json else {
            return Ok(Profiles::default());
        };
        let value: serde_json::Value =
            serde_json::from_str(&json).map_err(|error| error.to_string())?;
        if value.get("default").is_some() {
            return serde_json::from_value(value).map_err(|error| error.to_string());
        }
        let bindings = serde_json::from_value(value).map_err(|error| error.to_string())?;
        Ok(Profiles {
            default: Layer {
                bindings,
                ..Layer::default()
            },
            ..Profiles::default()
        })
    }

    pub fn bindings(&self, device: &Device) -> Result<Bindings, String> {
        Ok(self.customizations(device)?.default.bindings)
    }

    pub fn save_bindings(&self, device: &Device, bindings: &Bindings) -> Result<(), String> {
        let mut profiles = self.customizations(device)?;
        profiles.default.bindings = bindings.clone();
        self.save_customizations(device, &profiles)
    }

    pub fn save_customizations(&self, device: &Device, profiles: &Profiles) -> Result<(), String> {
        let json = serde_json::to_string(profiles).map_err(|error| error.to_string())?;
        self.connection.execute(
            "INSERT INTO control_profiles (connection, product_id, bindings_json) VALUES (?1, ?2, ?3)
            ON CONFLICT (connection, product_id) DO UPDATE SET bindings_json = excluded.bindings_json",
            params![device.storage_connection().as_ref(), device.product_id, json],
        ).map_err(|error| error.to_string())?;
        Ok(())
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
