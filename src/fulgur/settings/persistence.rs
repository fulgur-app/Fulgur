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
    /// ### Errors
    /// Returns an error if serialization to JSON fails or if the atomic write
    /// to the target path fails.
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
    /// ### Errors
    /// Returns an error if the settings file cannot be read and the backup file
    /// is also unavailable or corrupted, or if JSON deserialization fails on both.
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
                Self::persist_after_legacy_migration(&settings, path);
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
                Self::persist_after_legacy_migration(&settings, path);
                Ok(settings)
            }
        }
    }

    /// Rewrite the settings file when the loaded JSON used the legacy
    /// single-server `synchronization_settings` shape.
    ///
    /// ### Arguments
    /// - `settings`: The freshly migrated settings ready to be saved.
    /// - `path`: The file the settings should be written back to.
    fn persist_after_legacy_migration(settings: &Settings, path: &Path) {
        if !settings
            .app_settings
            .synchronization_settings
            .migrated_from_legacy
        {
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
            if let Some(ref email) = profile.email.clone()
                && !is_valid_email(email.trim())
            {
                log::warn!(
                    "Invalid email for profile '{}', clearing: {}",
                    profile.name,
                    email
                );
                profile.email = None;
            }
        }
    }

    /// Save the settings to the default state file location
    ///
    /// ### Errors
    /// Returns an error if the settings file path cannot be resolved or if the
    /// underlying write fails.
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
    /// ### Errors
    /// Returns an error if the settings file path cannot be resolved or if the
    /// underlying load fails.
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
    #[must_use]
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
    /// ### Errors
    /// Returns an error if persisting the updated settings to disk fails.
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

/// Check whether a string is a plausibly-valid email address.
///
/// ### Arguments
/// - `email`: The candidate email address (already trimmed by the caller).
///
/// ### Returns
/// - `true`: The address passes the structural checks.
/// - `false`: The address is empty, malformed, or contains whitespace.
fn is_valid_email(email: &str) -> bool {
    if email.chars().any(char::is_whitespace) {
        return false;
    }
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    if local.is_empty() || domain.contains('@') {
        return false;
    }
    let mut labels = domain.split('.');
    labels.clone().count() >= 2 && labels.all(|label| !label.is_empty())
}

#[cfg(test)]
mod tests {
    use super::is_valid_email;

    #[test]
    fn accepts_well_formed_addresses() {
        assert!(is_valid_email("user@example.com"));
        assert!(is_valid_email("first.last@sub.example.co.uk"));
        assert!(is_valid_email("a@b.io"));
    }

    #[test]
    fn rejects_malformed_addresses() {
        assert!(!is_valid_email(""));
        assert!(!is_valid_email("plainaddress"));
        assert!(!is_valid_email("@example.com"));
        assert!(!is_valid_email("user@"));
        assert!(!is_valid_email("user@.com"));
        assert!(!is_valid_email("user@exam."));
        assert!(!is_valid_email("user@example"));
        assert!(!is_valid_email("user@exam ple.com"));
        assert!(!is_valid_email("us er@example.com"));
        assert!(!is_valid_email("user@@example.com"));
        assert!(!is_valid_email("user@host@example.com"));
    }
}
