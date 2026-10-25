use super::{MAX_PROFILES, Settings};
use crate::fulgur::utils::atomic_write::{atomic_write_file, backup_path_for};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
use time::{OffsetDateTime, macros::format_description};

const FONT_SIZE_MIN: f32 = 6.0;
const FONT_SIZE_MAX: f32 = 72.0;
const TAB_SIZE_MIN: usize = 1;
const TAB_SIZE_MAX: usize = 16;
const MAX_RECENT_FILES_MAX: usize = 100;

/// Settings resolved at startup, together with what the user must be told about them
pub struct StartupSettings {
    /// The settings the application runs with
    pub settings: Settings,
    /// Whether no settings file existed yet, so first-run defaults such as the theme apply
    pub is_first_run: bool,
    /// Why the settings file could not be used, to show the user once a window exists
    pub load_problem: Option<String>,
}

/// Why a settings file did not yield settings
enum ReadFailure {
    /// The file does not exist
    Missing,
    /// The file exists but could not be read or parsed
    Unusable(anyhow::Error),
}

/// What reading the settings file and, when needed, its backup produced
enum LoadOutcome {
    /// The settings file was read and parsed
    Primary(Settings),
    /// The settings file does not exist
    Missing,
    /// The settings file was unusable and its backup was read instead
    FromBackup {
        settings: Settings,
        primary_error: anyhow::Error,
    },
    /// Neither the settings file nor its backup could be used
    Failed(anyhow::Error),
}

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
            let backup = backup_path_for(path);
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
    /// Returns an error if the settings file does not exist, or if it cannot be
    /// read or parsed and its backup is also missing, unreadable or corrupted.
    ///
    /// ### Returns
    /// - `Ok(Settings)`: The loaded settings
    /// - `Err(anyhow::Error)`: If there was an error loading the settings
    pub fn load_from_path(path: &Path) -> anyhow::Result<Self> {
        match load_outcome(path) {
            LoadOutcome::Primary(settings) | LoadOutcome::FromBackup { settings, .. } => {
                Ok(settings)
            }
            LoadOutcome::Missing => Err(anyhow::anyhow!(
                "Settings file '{}' does not exist",
                path.display()
            )),
            LoadOutcome::Failed(error) => Err(error),
        }
    }

    /// Load the settings to start the application with from a specific path
    ///
    /// ### Arguments
    /// - `path`: The path to load the settings from
    ///
    /// ### Returns
    /// - `StartupSettings`: The settings from the file, its backup or the
    ///   defaults, with the first-run flag and any problem to report
    #[must_use]
    pub fn load_for_startup_from_path(path: &Path) -> StartupSettings {
        match load_outcome(path) {
            LoadOutcome::Primary(settings) => StartupSettings {
                settings,
                is_first_run: false,
                load_problem: None,
            },
            LoadOutcome::Missing => {
                log::info!(
                    "No settings file at '{}', starting with first-run defaults",
                    path.display()
                );
                StartupSettings {
                    settings: Settings::new(),
                    is_first_run: true,
                    load_problem: None,
                }
            }
            LoadOutcome::FromBackup {
                settings,
                primary_error,
            } => {
                log::error!(
                    "Settings file '{}' is unusable, restored from its backup: {primary_error}",
                    path.display()
                );
                startup_settings_after_failure(
                    path,
                    settings,
                    &primary_error,
                    "The last backup was restored.",
                )
            }
            LoadOutcome::Failed(error) => {
                log::error!(
                    "Failed to load settings from '{}', using defaults: {error}",
                    path.display()
                );
                startup_settings_after_failure(
                    path,
                    Settings::new(),
                    &error,
                    "Default settings are in use.",
                )
            }
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

    /// Load the settings to start the application with from the default location
    ///
    /// ### Returns
    /// - `StartupSettings`: The settings to run with, the first-run flag and any
    ///   problem to report
    #[must_use]
    pub fn load_for_startup() -> StartupSettings {
        match Self::settings_file_path() {
            Ok(path) => Self::load_for_startup_from_path(&path),
            Err(error) => {
                log::error!("Failed to locate the settings file, using defaults: {error}");
                StartupSettings {
                    settings: Settings::new(),
                    is_first_run: false,
                    load_problem: Some(format!(
                        "Settings could not be located: {error}. Default settings are in use."
                    )),
                }
            }
        }
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

/// Read, parse and validate one settings file
///
/// ### Arguments
/// - `path`: The settings file, or its backup
///
/// ### Errors
/// - Returns `ReadFailure::Missing` when the file does not exist, and
///   `ReadFailure::Unusable` when it cannot be read or is not valid settings JSON.
///
/// ### Returns
/// - `Ok(Settings)`: The validated settings
/// - `Err(ReadFailure)`: Why the file did not yield settings
fn read_settings_file(path: &Path) -> Result<Settings, ReadFailure> {
    let json = fs::read_to_string(path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => ReadFailure::Missing,
        _ => ReadFailure::Unusable(anyhow::anyhow!("cannot read '{}': {e}", path.display())),
    })?;
    let mut settings = serde_json::from_str::<Settings>(&json).map_err(|e| {
        ReadFailure::Unusable(anyhow::anyhow!("cannot parse '{}': {e}", path.display()))
    })?;
    settings.validate();
    Ok(settings)
}

/// Read the settings file, falling back to its backup when the file is unusable
///
/// ### Arguments
/// - `path`: The settings file
///
/// ### Returns
/// - `LoadOutcome`: The settings and where they came from, or why none could be read
fn load_outcome(path: &Path) -> LoadOutcome {
    let primary_error = match read_settings_file(path) {
        Ok(settings) => return LoadOutcome::Primary(settings),
        Err(ReadFailure::Missing) => return LoadOutcome::Missing,
        Err(ReadFailure::Unusable(error)) => error,
    };
    let backup = backup_path_for(path);
    log::warn!(
        "Settings file is unusable ({primary_error}), attempting recovery from '{}'",
        backup.display()
    );
    match read_settings_file(&backup) {
        Ok(settings) => {
            log::warn!("Settings recovered from backup '{}'", backup.display());
            LoadOutcome::FromBackup {
                settings,
                primary_error,
            }
        }
        Err(ReadFailure::Missing) => LoadOutcome::Failed(primary_error),
        Err(ReadFailure::Unusable(backup_error)) => LoadOutcome::Failed(anyhow::anyhow!(
            "settings and backup are both unusable: {primary_error}; {backup_error}"
        )),
    }
}

/// Move an unusable settings file aside and describe the outcome for the user
///
/// ### Arguments
/// - `path`: The unusable settings file
/// - `settings`: The settings to run with instead, from the backup or the defaults
/// - `error`: Why the settings file could not be used
/// - `recovery`: A sentence telling the user which settings are in use instead
///
/// ### Returns
/// - `StartupSettings`: The replacement settings, flagged as not a first run, with
///   the problem to report
fn startup_settings_after_failure(
    path: &Path,
    settings: Settings,
    error: &anyhow::Error,
    recovery: &str,
) -> StartupSettings {
    let preservation = match move_unusable_file_aside(path) {
        Ok(kept_at) => {
            log::warn!(
                "Moved the unusable settings file to '{}'",
                kept_at.display()
            );
            format!("The unusable file was kept as '{}'.", kept_at.display())
        }
        Err(e) => {
            log::error!(
                "Failed to move the unusable settings file '{}' aside: {e}",
                path.display()
            );
            format!("It could not be set aside ({e}), so the next settings change replaces it.")
        }
    };
    StartupSettings {
        settings,
        is_first_run: false,
        load_problem: Some(format!(
            "Settings could not be loaded: {error}. {recovery} {preservation}"
        )),
    }
}

/// Rename an unusable settings file out of the way of the save and backup rotation
///
/// ### Arguments
/// - `path`: The unusable settings file
///
/// ### Errors
/// - Returns the `io::Error` of the rename, for example when the file is locked by
///   another process or the directory is not writable.
///
/// ### Returns
/// - `Ok(PathBuf)`: Where the file now lives
/// - `Err(io::Error)`: The file could not be renamed and is still in place
fn move_unusable_file_aside(path: &Path) -> io::Result<PathBuf> {
    let kept_at = unusable_path_for(path, OffsetDateTime::now_utc());
    fs::rename(path, &kept_at)?;
    Ok(kept_at)
}

/// Compute the name an unusable file is kept under, stamped with the time it was set aside
///
/// ### Arguments
/// - `path`: The unusable file
/// - `now`: The time to stamp the name with
///
/// ### Returns
/// - `PathBuf`: The sibling path, e.g. `settings.json` -> `settings.json.corrupt-20261025-143012`
fn unusable_path_for(path: &Path, now: OffsetDateTime) -> PathBuf {
    let stamp = now
        .format(format_description!(
            "[year][month][day]-[hour][minute][second]"
        ))
        .unwrap_or_else(|_| now.unix_timestamp().to_string());
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    path.with_file_name(format!("{name}.corrupt-{stamp}"))
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
    use super::{is_valid_email, unusable_path_for};
    use std::path::Path;
    use time::macros::datetime;

    #[test]
    fn unusable_path_for_stamps_the_file_name_next_to_the_original() {
        let path = Path::new("/config/settings.json");
        assert_eq!(
            unusable_path_for(path, datetime!(2026-10-25 14:30:12 UTC)),
            Path::new("/config/settings.json.corrupt-20261025-143012")
        );
    }

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
