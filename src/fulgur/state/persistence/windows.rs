use super::{SerializedWindowBounds, TabState};
use crate::fulgur::utils::atomic_write::atomic_write_file;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Persisted state of a single application window
///
/// Tab IDs are assigned at runtime, so `next_tab_id` is not persisted.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WindowState {
    /// All tabs in this window, in display order
    pub tabs: Vec<TabState>,
    /// Index of the currently active/visible tab, if any
    pub active_tab_index: Option<usize>,
    /// Window position, size, and display state (windowed/maximized/fullscreen)
    #[serde(default)]
    pub window_bounds: SerializedWindowBounds,
}

/// Top-level container for all persisted application state
///
/// This struct is serialized to `state.json` and contains the complete
/// state of all windows. On startup, each window in this list is restored
/// with its tabs, positions, and content.
///
/// File location:
/// - Windows: `%APPDATA%\Fulgur\state.json`
/// - macOS/Linux: `~/.fulgur/state.json`
#[derive(Serialize, Deserialize, Debug)]
pub struct WindowsState {
    /// All application windows to be restored
    pub windows: Vec<WindowState>,
}

impl WindowsState {
    /// Get the path to the state file
    ///
    /// ### Returns
    /// - `Ok(PathBuf)`: The path to the state file
    /// - `Err(anyhow::Error)`: If the state file path could not be determined
    pub(crate) fn state_file_path() -> anyhow::Result<PathBuf> {
        let mut path = crate::fulgur::utils::paths::config_dir()?;
        path.push("state.json");
        Ok(path)
    }

    /// Save the app state to a specific path
    ///
    /// ### Description
    /// Core implementation for saving window state. Uses atomic file writes to
    /// prevent corruption when multiple windows write simultaneously.
    ///
    /// ### Arguments
    /// - `path`: The path to save the state to
    ///
    /// ### Errors
    /// - Returns an error if JSON serialization fails or if the atomic write to
    ///   the target path fails.
    ///
    /// ### Returns
    /// - `Ok(())`: If the app state was saved successfully
    /// - `Err(anyhow::Error)`: If the app state could not be saved
    pub fn save_to_path(&self, path: &Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        if path.exists() {
            let backup = crate::fulgur::utils::atomic_write::backup_path_for(path);
            if let Err(e) = fs::copy(path, &backup) {
                log::warn!("Failed to back up state to '{}': {}", backup.display(), e);
            }
        }
        atomic_write_file(path, json.as_bytes())
    }

    /// Load the windows state from a specific path
    ///
    /// ### Description
    /// Core implementation for loading window state.
    ///
    /// ### Arguments
    /// - `path`: The path to load the state from
    ///
    /// ### Errors
    /// - Returns an error if the state file cannot be read and the backup file
    ///   is also unavailable or corrupted, or if JSON deserialization fails on both.
    ///
    /// ### Returns
    /// - `Ok(WindowsState)`: The loaded windows state
    /// - `Err(anyhow::Error)`: If the windows state could not be loaded
    pub fn load_from_path(path: &PathBuf) -> anyhow::Result<Self> {
        let json = fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read state file: {e}"))?;
        match serde_json::from_str::<WindowsState>(&json) {
            Ok(state) => Ok(state),
            Err(primary_err) => {
                let backup = crate::fulgur::utils::atomic_write::backup_path_for(path);
                log::warn!(
                    "State file is corrupted ({}), attempting recovery from '{}'",
                    primary_err,
                    backup.display()
                );
                let bak_json = fs::read_to_string(&backup)
                    .map_err(|_| anyhow::anyhow!("Failed to parse state: {primary_err}"))?;
                let state = serde_json::from_str::<WindowsState>(&bak_json).map_err(|bak_err| {
                    anyhow::anyhow!(
                        "State and backup are both corrupted: primary={primary_err}, backup={bak_err}"
                    )
                })?;
                log::warn!("State recovered from backup '{}'", backup.display());
                Ok(state)
            }
        }
    }

    /// Save the app state to the default state file location
    ///
    /// ### Errors
    /// - Returns an error if the state file path cannot be resolved or if the
    ///   underlying write fails.
    ///
    /// ### Returns
    /// - `Ok(())`: If the app state was saved successfully
    /// - `Err(anyhow::Error)`: If the app state could not be saved
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::state_file_path()?;
        self.save_to_path(&path)
    }

    /// Load the windows state from the default state file location
    ///
    /// ### Errors
    /// - Returns an error if the state file path cannot be resolved or if the
    ///   underlying load fails.
    ///
    /// ### Returns
    /// - `Ok(WindowsState)`: The loaded windows state
    /// - `Err(anyhow::Error)`: If the windows state could not be loaded
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::state_file_path()?;
        Self::load_from_path(&path)
    }
}

#[cfg(test)]
mod tests {
    use super::{SerializedWindowBounds, TabState, WindowState, WindowsState};
    use std::fs;
    use tempfile::TempDir;

    /// Build a simple file-backed tab state for persistence tests.
    ///
    /// ### Parameters
    /// - `title`: The tab title.
    /// - `file_name`: The file name used to build a path under the temp directory.
    /// - `content`: Optional in-memory content to persist.
    /// - `last_saved`: Optional ISO 8601 last-saved timestamp.
    ///
    /// ### Returns
    /// - `TabState`: A tab state ready to be serialized.
    fn file_tab_state(
        title: &str,
        file_name: &str,
        content: Option<&str>,
        last_saved: Option<&str>,
    ) -> TabState {
        TabState {
            title: title.to_string(),
            file_path: Some(std::env::temp_dir().join(file_name)),
            content: content.map(std::string::ToString::to_string),
            last_saved: last_saved.map(std::string::ToString::to_string),
            remote: None,
            log_view: false,
            color_tag: None,
        }
    }

    #[test]
    fn save_to_path_creates_backup_of_previous_state_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("state.json");
        let backup = dir.path().join("state.json.bak");

        let state = WindowsState { windows: vec![] };
        state.save_to_path(&path).unwrap();
        assert!(!backup.exists(), "no backup before second save");

        state.save_to_path(&path).unwrap();
        assert!(backup.exists(), "backup created on second save");
    }

    #[test]
    fn load_from_path_recovers_state_from_backup_when_primary_is_corrupted() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("state.json");
        let backup = dir.path().join("state.json.bak");

        let state = WindowsState { windows: vec![] };
        state.save_to_path(&backup).unwrap();

        fs::write(&path, b"not valid json").unwrap();

        let recovered = WindowsState::load_from_path(&path).unwrap();
        assert_eq!(recovered.windows.len(), 0);
    }

    #[test]
    fn load_from_path_returns_error_when_both_state_and_backup_are_corrupted() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("state.json");
        let backup = dir.path().join("state.json.bak");

        fs::write(&path, b"bad primary").unwrap();
        fs::write(&backup, b"bad backup").unwrap();

        let result = WindowsState::load_from_path(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_windows_state_save_load_roundtrip_multi_window_with_mixed_tabs_and_bounds() {
        let temp_dir = TempDir::new().expect("failed to create temporary directory");
        let state_path = temp_dir.path().join("state.json");
        let original = WindowsState {
            windows: vec![
                WindowState {
                    tabs: vec![
                        file_tab_state("main.rs", "fulgur_state_main.rs", None, None),
                        file_tab_state(
                            "notes.md",
                            "fulgur_state_notes.md",
                            Some("# draft"),
                            Some("2026-03-26T10:00:00Z"),
                        ),
                    ],
                    active_tab_index: Some(1),
                    window_bounds: SerializedWindowBounds {
                        state: "Windowed".to_string(),
                        x: 120.0,
                        y: 90.0,
                        width: 1300.0,
                        height: 900.0,
                        display_id: Some(1),
                    },
                },
                WindowState {
                    tabs: vec![TabState {
                        title: "Untitled".to_string(),
                        file_path: None,
                        content: Some("scratch content".to_string()),
                        last_saved: None,
                        remote: None,
                        log_view: false,
                        color_tag: None,
                    }],
                    active_tab_index: Some(0),
                    window_bounds: SerializedWindowBounds {
                        state: "Maximized".to_string(),
                        x: 0.0,
                        y: 0.0,
                        width: 1920.0,
                        height: 1080.0,
                        display_id: Some(2),
                    },
                },
            ],
        };
        original
            .save_to_path(&state_path)
            .expect("failed to save windows state");
        let loaded = WindowsState::load_from_path(&state_path)
            .expect("failed to load windows state after roundtrip");
        assert_eq!(loaded.windows.len(), 2);
        assert_eq!(loaded.windows[0].tabs.len(), 2);
        assert_eq!(loaded.windows[1].tabs.len(), 1);
        assert_eq!(loaded.windows[0].active_tab_index, Some(1));
        assert_eq!(loaded.windows[1].active_tab_index, Some(0));
        assert_eq!(loaded.windows[0].window_bounds.state, "Windowed");
        assert_eq!(loaded.windows[1].window_bounds.state, "Maximized");
        assert_eq!(loaded.windows[0].window_bounds.display_id, Some(1));
        assert_eq!(loaded.windows[1].window_bounds.display_id, Some(2));
        assert_eq!(loaded.windows[0].tabs[0].title, "main.rs");
        assert_eq!(loaded.windows[0].tabs[1].title, "notes.md");
        assert_eq!(loaded.windows[1].tabs[0].title, "Untitled");
        assert_eq!(
            loaded.windows[0].tabs[1].content,
            Some("# draft".to_string())
        );
        assert_eq!(
            loaded.windows[1].tabs[0].content,
            Some("scratch content".to_string())
        );
    }
}
