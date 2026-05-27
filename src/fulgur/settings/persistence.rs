use super::{MAX_PROFILES, Settings};
use crate::fulgur::utils::atomic_write::atomic_write_file;
use std::{
    fs,
    path::{Path, PathBuf},
};

const FONT_SIZE_MIN: f32 = 6.0;
const FONT_SIZE_MAX: f32 = 72.0;
const TAB_SIZE_MIN: usize = 1;
const TAB_SIZE_MAX: usize = 16;
const MAX_RECENT_FILES_MAX: usize = 100;

impl Settings {
    /// Get the path to the settings file
    ///
    /// ### Returns
    /// - `Ok(PathBuf)`: The path to the settings file
    /// - `Err(anyhow::Error)`: If there was an error getting the path
    fn settings_file_path() -> anyhow::Result<PathBuf> {
        let mut path = crate::fulgur::utils::paths::config_dir()?;
        path.push("settings.json");
        Ok(path)
    }

    /// Save the settings to a specific path
    ///
    /// ### Description
    /// Core implementation for saving settings. Can be used with custom paths
    /// for testing or alternative storage locations. Uses atomic file writes to
    /// prevent corruption when multiple windows write simultaneously.
    ///
    /// ### Arguments
    /// - `path`: The path to save the settings to
    ///
    /// ### Returns
    /// - `Ok(())`: The result of the operation
    /// - `Err(anyhow::Error)`: If there was an error saving the settings
    pub fn save_to_path(&self, path: &Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(&self)?;
        if path.exists() {
            let backup = crate::fulgur::utils::atomic_write::backup_path_for(path);
            if let Err(e) = fs::copy(path, &backup) {
                log::warn!(
                    "Failed to back up settings to '{}': {}",
                    backup.display(),
                    e
                );
            }
        }
        atomic_write_file(path, json.as_bytes())
    }

    /// Load the settings from a specific path
    ///
    /// ### Description
    /// Core implementation for loading settings. Can be used with custom paths
    /// for testing or alternative storage locations. Applies invariant validation
    /// after deserialization to clamp numeric fields and discard malformed strings.
    ///
    /// ### Arguments
    /// - `path`: The path to load the settings from
    ///
    /// ### Returns
    /// - `Ok(Settings)`: The loaded settings
    /// - `Err(anyhow::Error)`: If there was an error loading the settings
    pub fn load_from_path(path: &PathBuf) -> anyhow::Result<Self> {
        let json = fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read settings file: {e}"))?;
        match serde_json::from_str::<Settings>(&json) {
            Ok(mut settings) => {
                settings.validate();
                Self::persist_after_legacy_migration(&settings, &json, path);
                Ok(settings)
            }
            Err(primary_err) => {
                let backup = crate::fulgur::utils::atomic_write::backup_path_for(path);
                log::warn!(
                    "Settings file is corrupted ({}), attempting recovery from '{}'",
                    primary_err,
                    backup.display()
                );
                let bak_json = fs::read_to_string(&backup)
                    .map_err(|_| anyhow::anyhow!("Failed to parse settings: {primary_err}"))?;
                let mut settings =
                    serde_json::from_str::<Settings>(&bak_json).map_err(|bak_err| {
                        anyhow::anyhow!(
                            "Settings and backup are both corrupted: primary={primary_err}, backup={bak_err}"
                        )
                    })?;
                settings.validate();
                log::warn!("Settings recovered from backup '{}'", backup.display());
                Self::persist_after_legacy_migration(&settings, &bak_json, path);
                Ok(settings)
            }
        }
    }

    /// Rewrite the settings file when the loaded JSON used the legacy
    /// single-server `synchronization_settings` shape.
    ///
    /// ### Arguments
    /// - `settings`: The freshly migrated settings ready to be saved.
    /// - `source_json`: The raw JSON that was just deserialized.
    /// - `path`: The file the settings should be written back to.
    fn persist_after_legacy_migration(settings: &Settings, source_json: &str, path: &Path) {
        if !json_has_legacy_synchronization_shape(source_json) {
            return;
        }
        log::info!(
            "Persisting settings after legacy synchronization migration to '{}'",
            path.display()
        );
        if let Err(e) = settings.save_to_path(path) {
            log::warn!("Failed to persist settings after legacy synchronization migration: {e}");
        }
    }

    /// Clamp numeric fields into safe ranges and discard malformed optional strings
    pub fn validate(&mut self) {
        let es = &mut self.editor_settings;
        if es.font_size < FONT_SIZE_MIN || es.font_size > FONT_SIZE_MAX || !es.font_size.is_finite()
        {
            log::warn!(
                "font_size {} is out of range [{}, {}], clamping",
                es.font_size,
                FONT_SIZE_MIN,
                FONT_SIZE_MAX
            );
            es.font_size = es.font_size.clamp(FONT_SIZE_MIN, FONT_SIZE_MAX);
            if !es.font_size.is_finite() {
                es.font_size = 14.0;
            }
        }
        if es.tab_size < TAB_SIZE_MIN || es.tab_size > TAB_SIZE_MAX {
            log::warn!(
                "tab_size {} is out of range [{}, {}], clamping",
                es.tab_size,
                TAB_SIZE_MIN,
                TAB_SIZE_MAX
            );
            es.tab_size = es.tab_size.clamp(TAB_SIZE_MIN, TAB_SIZE_MAX);
        }

        if self.recent_files.max_files > MAX_RECENT_FILES_MAX {
            log::warn!(
                "max_recent_files {} exceeds maximum {}, clamping",
                self.recent_files.max_files,
                MAX_RECENT_FILES_MAX
            );
            self.recent_files.max_files = MAX_RECENT_FILES_MAX;
        }

        let sync = &mut self.app_settings.synchronization_settings;
        if sync.profiles.len() > MAX_PROFILES {
            log::warn!(
                "synchronization_settings.profiles count {} exceeds maximum {}, truncating",
                sync.profiles.len(),
                MAX_PROFILES
            );
            sync.profiles.truncate(MAX_PROFILES);
        }
        for profile in &mut sync.profiles {
            if let Some(ref url_str) = profile.server_url.clone()
                && url::Url::parse(url_str).is_err()
            {
                log::warn!(
                    "Invalid server_url for profile '{}', clearing: {}",
                    profile.name,
                    url_str
                );
                profile.server_url = None;
            }
            if let Some(ref email) = profile.email.clone() {
                let trimmed = email.trim();
                let at_pos = trimmed.find('@');
                let is_valid = at_pos.is_some_and(|pos| {
                    pos > 0 && pos < trimmed.len() - 1 && trimmed[pos + 1..].contains('.')
                });
                if !is_valid {
                    log::warn!(
                        "Invalid email for profile '{}', clearing: {}",
                        profile.name,
                        email
                    );
                    profile.email = None;
                }
            }
        }
    }

    /// Save the settings to the default state file location
    ///
    /// ### Returns
    /// - `Ok(())`: The result of the operation
    /// - `Err(anyhow::Error)`: If there was an error saving the settings
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::settings_file_path()?;
        self.save_to_path(&path)
    }

    /// Load the settings from the default state file location
    ///
    /// ### Returns
    /// - `Ok(Settings)`: The loaded settings
    /// - `Err(anyhow::Error)`: If there was an error loading the settings
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::settings_file_path()?;
        Self::load_from_path(&path)
    }

    /// Get the recent files
    ///
    /// ### Returns
    /// - `Vec<PathBuf>`: The recent files
    pub fn get_recent_files(&self) -> Vec<PathBuf> {
        let mut files = self.recent_files.get_files().clone();
        files.reverse();
        files
    }

    /// Add a file to the recent files
    ///
    /// ### Arguments
    /// - `file`: The file to add
    ///
    /// ### Returns
    /// - `Ok(())`: The result of the operation
    /// - `Err(anyhow::Error)`: If there was an error adding the file
    pub fn add_file(&mut self, file: PathBuf) -> anyhow::Result<()> {
        if self.recent_files.get_files().contains(&file) {
            self.recent_files.remove_file(&file);
        }
        self.recent_files.add_file(file);
        self.save()
    }
}

/// Detect whether a settings JSON document still uses the legacy
/// `synchronization_settings` shape (single-server fields, no `profiles`).
///
/// ### Arguments
/// - `json`: Raw JSON contents of the settings file.
///
/// ### Returns
/// - `true`: The document has legacy single-server fields and no `profiles`.
/// - `false`: The document is in the new multi-profile shape, has no
///   synchronization configuration, or could not be parsed as JSON.
fn json_has_legacy_synchronization_shape(json: &str) -> bool {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let Some(sync) = value
        .get("app_settings")
        .and_then(|v| v.get("synchronization_settings"))
    else {
        return false;
    };
    if sync.get("profiles").is_some() {
        return false;
    }
    sync.get("server_url").is_some()
        || sync.get("email").is_some()
        || sync.get("public_key").is_some()
        || sync.get("is_deduplication").is_some()
}
