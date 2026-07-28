use super::{SerializedWindowBounds, TabState};
use crate::fulgur::state::db::{LEGACY_STATE_FILE_NAME, STATE_DB_FILE_NAME, StateDb};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};

/// Last window identity handed out by `WindowState::allocate_id`.
static LAST_WINDOW_ID: AtomicI64 = AtomicI64::new(0);

/// Persisted state of a single application window
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WindowState {
    #[serde(default)]
    pub window_id: i64,
    /// All tabs in this window, in display order
    pub tabs: Vec<TabState>,
    /// Index of the currently active/visible tab, if any
    pub active_tab_index: Option<usize>,
    /// Window position, size, and display state (windowed/maximized/fullscreen)
    #[serde(default)]
    pub window_bounds: SerializedWindowBounds,
}

impl WindowState {
    /// Allocate an identity for a window that has never been persisted.
    ///
    /// ### Returns
    /// - `i64`: An identity not handed out before by this process
    #[must_use]
    pub fn allocate_id() -> i64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| i64::try_from(elapsed.as_nanos()).unwrap_or(0));
        LAST_WINDOW_ID.fetch_max(now, Ordering::Relaxed);
        LAST_WINDOW_ID.fetch_add(1, Ordering::Relaxed) + 1
    }
}

/// Top-level container for all persisted application state
///
/// Holds the complete state of all windows. On startup, each window in this list
/// is restored with its tabs, positions, and content. The list is persisted as
/// rows in a `SQLite` database rather than as one document, so a save costs what
/// changed instead of everything that is open.
///
/// File location:
/// - Windows: `%APPDATA%\Fulgur\state.db`
/// - macOS/Linux: `~/.fulgur/state.db`
///
/// `Serialize`/`Deserialize` remain only to read the legacy `state.json`
/// document written by Fulgur 0.10 and earlier.
#[derive(Serialize, Deserialize, Debug)]
pub struct WindowsState {
    /// All application windows to be restored
    pub windows: Vec<WindowState>,
}

impl WindowsState {
    /// Get the path to the state database
    ///
    /// ### Returns
    /// - `Ok(PathBuf)`: The path to the state database
    /// - `Err(anyhow::Error)`: If the state database path could not be determined
    pub(crate) fn state_file_path() -> anyhow::Result<PathBuf> {
        let mut path = crate::fulgur::utils::paths::config_dir()?;
        path.push(STATE_DB_FILE_NAME);
        Ok(path)
    }

    /// Get the path to the legacy JSON state document
    ///
    /// ### Returns
    /// - `Ok(PathBuf)`: The path to the legacy state document
    /// - `Err(anyhow::Error)`: If the path could not be determined
    fn legacy_state_file_path() -> anyhow::Result<PathBuf> {
        let mut path = crate::fulgur::utils::paths::config_dir()?;
        path.push(LEGACY_STATE_FILE_NAME);
        Ok(path)
    }

    /// Save the app state to a database at a specific path
    ///
    /// ### Arguments
    /// - `path`: The path of the database to save into
    ///
    /// ### Errors
    /// - Returns an error if the database cannot be opened or migrated, or if
    ///   the rows cannot be written.
    ///
    /// ### Returns
    /// - `Ok(())`: If the app state was saved successfully
    /// - `Err(anyhow::Error)`: If the app state could not be saved
    pub fn save_to_path(&self, path: &Path) -> anyhow::Result<()> {
        let mut db = StateDb::open(path)?;
        db.claim_persisted_windows()?;
        db.apply(self)?;
        Ok(())
    }

    /// Load the windows state from a database at a specific path
    ///
    /// ### Arguments
    /// - `path`: The path of the database to load from
    ///
    /// ### Errors
    /// - Returns an error if the database cannot be opened or migrated, or if
    ///   the rows cannot be decoded.
    ///
    /// ### Returns
    /// - `Ok(WindowsState)`: The loaded windows state
    /// - `Err(anyhow::Error)`: If the windows state could not be loaded
    pub fn load_from_path(path: &Path) -> anyhow::Result<Self> {
        StateDb::open(path)?.load()
    }

    /// Save the app state to the default state database location
    ///
    /// ### Errors
    /// - Returns an error if the database path cannot be resolved or if the
    ///   underlying write fails.
    ///
    /// ### Returns
    /// - `Ok(())`: If the app state was saved successfully
    /// - `Err(anyhow::Error)`: If the app state could not be saved
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::state_file_path()?;
        self.save_to_path(&path)
    }

    /// Load the windows state for this session from the default location
    ///
    /// ### Errors
    /// - Returns an error if the database path cannot be resolved, or if the
    ///   database can neither be opened nor replaced by an ephemeral one.
    ///
    /// ### Returns
    /// - `Ok((WindowsState, StateDb))`: The restored session and the open database
    /// - `Err(anyhow::Error)`: If the session could not be restored
    pub fn load_with_db() -> anyhow::Result<(Self, StateDb)> {
        let path = Self::state_file_path()?;
        let needs_legacy_import = !path.exists();
        let mut db = StateDb::open_or_fallback(&path)
            .ok_or_else(|| anyhow::anyhow!("Failed to open any state database"))?;
        if needs_legacy_import && !db.is_ephemeral() {
            match Self::legacy_state_file_path() {
                Ok(legacy_path) if legacy_path.exists() => {
                    if let Err(e) =
                        crate::fulgur::state::db::import_legacy_json(&legacy_path, &mut db)
                    {
                        log::warn!("Failed to import the legacy state document: {e}");
                    }
                }
                Ok(_) => log::debug!("No legacy state document to import"),
                Err(e) => log::warn!("Failed to resolve the legacy state document path: {e}"),
            }
        }
        let state = db.load()?;
        // Windows restored now are this process's responsibility, so closing one
        // deletes its row instead of leaving it to reappear on the next launch.
        db.claim_persisted_windows()?;
        Ok((state, db))
    }
}

#[cfg(test)]
mod tests {
    use super::super::TabContent;
    use super::{SerializedWindowBounds, TabState, WindowState, WindowsState};
    use std::fs;
    use tempfile::TempDir;

    /// Build a simple file-backed tab state for persistence tests.
    ///
    /// ### Parameters
    /// - `tab_id`: Identity of the tab within its window.
    /// - `title`: The tab title.
    /// - `file_name`: The file name used to build a path under the temp directory.
    /// - `content`: Optional in-memory content to persist.
    /// - `last_saved`: Optional ISO 8601 last-saved timestamp.
    ///
    /// ### Returns
    /// - `TabState`: A tab state ready to be persisted.
    fn file_tab_state(
        tab_id: u64,
        title: &str,
        file_name: &str,
        content: Option<&str>,
        last_saved: Option<&str>,
    ) -> TabState {
        TabState {
            tab_id,
            title: title.to_string(),
            file_path: Some(std::env::temp_dir().join(file_name)),
            content: content.map(TabContent::from),
            last_saved: last_saved.map(std::string::ToString::to_string),
            remote: None,
            log_view: false,
            color_tag: None,
        }
    }

    #[test]
    fn load_from_path_rejects_a_file_that_is_not_a_database() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("state.db");
        fs::write(&path, b"not a database at all").unwrap();

        assert!(WindowsState::load_from_path(&path).is_err());
    }

    #[test]
    fn an_unusable_database_degrades_to_an_ephemeral_one() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("state.db");
        fs::write(&path, b"not a database at all").unwrap();

        // Failing to start is worse than losing session restore for one run, so
        // an unreadable database is replaced by an in-memory one.
        let db = crate::fulgur::state::db::StateDb::open_or_fallback(&path)
            .expect("a fallback database must always be available");
        assert!(db.is_ephemeral());
        assert!(db.load().expect("load from fallback").windows.is_empty());
    }

    #[test]
    fn save_to_path_removes_windows_that_are_gone_from_the_snapshot() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("state.db");
        let original = WindowsState {
            windows: vec![WindowState {
                window_id: 1,
                tabs: vec![file_tab_state(0, "a.rs", "fulgur_state_a.rs", None, None)],
                active_tab_index: Some(0),
                window_bounds: SerializedWindowBounds::default(),
            }],
        };
        original.save_to_path(&path).unwrap();

        WindowsState { windows: vec![] }
            .save_to_path(&path)
            .unwrap();

        let loaded = WindowsState::load_from_path(&path).unwrap();
        assert!(loaded.windows.is_empty());
    }

    #[test]
    fn test_windows_state_save_load_roundtrip_multi_window_with_mixed_tabs_and_bounds() {
        let temp_dir = TempDir::new().expect("failed to create temporary directory");
        let state_path = temp_dir.path().join("state.db");
        let original = WindowsState {
            windows: vec![
                WindowState {
                    window_id: 1,
                    tabs: vec![
                        file_tab_state(0, "main.rs", "fulgur_state_main.rs", None, None),
                        file_tab_state(
                            1,
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
                    window_id: 2,
                    tabs: vec![TabState {
                        tab_id: 0,
                        title: "Untitled".to_string(),
                        file_path: None,
                        content: Some(TabContent::from("scratch content")),
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
            Some(TabContent::from("# draft"))
        );
        assert_eq!(
            loaded.windows[1].tabs[0].content,
            Some(TabContent::from("scratch content"))
        );
    }
}
