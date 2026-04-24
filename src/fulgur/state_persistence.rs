use crate::fulgur::sync::ssh::url::RemoteSpec;
use crate::fulgur::utils::atomic_write::atomic_write_file;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Persisted SSH/SFTP tab location metadata.
///
/// This representation intentionally excludes any credential material.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SerializedRemoteSpec {
    /// Remote hostname
    pub host: String,
    /// SSH port
    pub port: u16,
    /// Remote username. Empty means "prompt user".
    pub user: String,
    /// Remote file path
    pub path: String,
}

impl SerializedRemoteSpec {
    /// Build a persisted remote spec from a runtime `RemoteSpec`.
    ///
    /// ### Arguments
    /// - `spec`: Runtime remote spec to persist.
    ///
    /// ### Returns
    /// - `SerializedRemoteSpec`: Persistable remote spec with no password field.
    pub fn from_remote_spec(spec: &RemoteSpec) -> Self {
        Self {
            host: spec.host.clone(),
            port: spec.port,
            user: spec.user.clone().unwrap_or_default(),
            path: spec.path.clone(),
        }
    }

    /// Convert persisted remote metadata back into a runtime `RemoteSpec`.
    ///
    /// ### Returns
    /// - `RemoteSpec`: Runtime remote spec with `password_in_url` cleared.
    pub fn to_remote_spec(&self) -> RemoteSpec {
        RemoteSpec {
            host: self.host.clone(),
            port: self.port,
            user: (!self.user.trim().is_empty()).then_some(self.user.clone()),
            path: self.path.clone(),
            password_in_url: None,
        }
    }
}

/// Persisted state of a single editor tab
///
/// Tab IDs are not persisted as they are assigned at runtime based on position.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TabState {
    /// Display title shown in the tab bar (usually the filename)
    pub title: String,
    /// Path to the file on disk, if the tab has an associated file. `None` for unsaved/new tabs.
    pub file_path: Option<PathBuf>,
    /// The text content of the tab, stored for unsaved tabs or when the file may have been modified since last save
    pub content: Option<String>,
    /// ISO 8601 timestamp of when the content was last saved to disk. Used to detect if the file has been modified externally.
    pub last_saved: Option<String>,
    /// Serialized remote location metadata for SSH/SFTP tabs.
    #[serde(default)]
    pub remote: Option<SerializedRemoteSpec>,
}

/// Serialized window bounds that can be saved to JSON
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SerializedWindowBounds {
    /// Window state: "Windowed", "Maximized", or "Fullscreen"
    pub state: String,
    /// X position of window origin in pixels
    pub x: f32,
    /// Y position of window origin in pixels
    pub y: f32,
    /// Window width in pixels
    pub width: f32,
    /// Window height in pixels
    pub height: f32,
    /// Display ID (monitor) where the window was located
    #[serde(default)]
    pub display_id: Option<u32>,
}

impl Default for SerializedWindowBounds {
    /// Default values for serialized window bounds
    ///
    /// ### Returns
    /// - `SerializedWindowBounds`: The default serialized window bounds
    fn default() -> Self {
        Self {
            state: "Windowed".to_string(),
            x: 100.0,
            y: 100.0,
            width: 1200.0,
            height: 800.0,
            display_id: None,
        }
    }
}

impl SerializedWindowBounds {
    /// Convert GPUI WindowBounds to SerializedWindowBounds
    ///
    /// ### Arguments
    /// - `bounds`: The GPUI WindowBounds to convert
    /// - `display_id`: Optional display ID (monitor) for the window
    ///
    /// ### Returns
    /// - `SerializedWindowBounds`: The serialized window bounds
    pub fn from_gpui_bounds(bounds: gpui::WindowBounds, display_id: Option<u32>) -> Self {
        use gpui::WindowBounds;
        match bounds {
            WindowBounds::Windowed(rect) => Self {
                state: "Windowed".to_string(),
                x: rect.origin.x.into(),
                y: rect.origin.y.into(),
                width: rect.size.width.into(),
                height: rect.size.height.into(),
                display_id,
            },
            WindowBounds::Maximized(rect) => Self {
                state: "Maximized".to_string(),
                x: rect.origin.x.into(),
                y: rect.origin.y.into(),
                width: rect.size.width.into(),
                height: rect.size.height.into(),
                display_id,
            },
            WindowBounds::Fullscreen(rect) => Self {
                state: "Fullscreen".to_string(),
                x: rect.origin.x.into(),
                y: rect.origin.y.into(),
                width: rect.size.width.into(),
                height: rect.size.height.into(),
                display_id,
            },
        }
    }

    /// Convert SerializedWindowBounds to GPUI WindowBounds
    ///
    /// ### Returns
    /// - `gpui::WindowBounds`: The GPUI window bounds
    pub fn to_gpui_bounds(&self) -> gpui::WindowBounds {
        use gpui::{Bounds, WindowBounds, point, px, size};
        let bounds = Bounds {
            origin: point(px(self.x), px(self.y)),
            size: size(px(self.width), px(self.height)),
        };
        match self.state.as_str() {
            "Maximized" => WindowBounds::Maximized(bounds),
            "Fullscreen" => WindowBounds::Fullscreen(bounds),
            _ => WindowBounds::Windowed(bounds), // Default to Windowed for unknown states
        }
    }
}

/// Persisted state of a single application window
///
/// Tab IDs are assigned at runtime, so next_tab_id is not persisted.
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
    /// ### Returns
    /// - `Ok(())`: If the app state was saved successfully
    /// - `Err(anyhow::Error)`: If the app state could not be saved
    pub fn save_to_path(&self, path: &Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
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
    /// ### Returns
    /// - `Ok(WindowsState)`: The loaded windows state
    /// - `Err(anyhow::Error)`: If the windows state could not be loaded
    pub fn load_from_path(path: &PathBuf) -> anyhow::Result<Self> {
        let json = fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read state file: {}", e))?;
        let state = serde_json::from_str::<WindowsState>(&json)
            .map_err(|e| anyhow::anyhow!("Failed to parse state: {}", e))?;
        Ok(state)
    }

    /// Save the app state to the default state file location
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
    /// ### Returns
    /// - `Ok(WindowsState)`: The loaded windows state
    /// - `Err(anyhow::Error)`: If the windows state could not be loaded
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::state_file_path()?;
        Self::load_from_path(&path)
    }
}

/// Get the last modified time of a file as ISO 8601 string
///
/// ### Arguments
/// - `path`: The path to the file
///
/// ### Returns
/// - `Some(String)`: The last modified time of the file
/// - `None`: If the file could not be found or the last modified time could not be determined
pub fn get_file_modified_time(path: &PathBuf) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let datetime = time::OffsetDateTime::from(modified);
    datetime
        .format(&time::format_description::well_known::Rfc3339)
        .ok()
}

/// Compare two ISO 8601 timestamps
///
/// ### Arguments
/// - `file_time`: The time of the file
/// - `saved_time`: The time of the saved file
///
/// ### Returns
/// - `True`: If the file is newer than the saved file, `False` otherwise
pub fn is_file_newer(file_time: &str, saved_time: &str) -> bool {
    let file_dt =
        time::OffsetDateTime::parse(file_time, &time::format_description::well_known::Rfc3339).ok();
    let saved_dt =
        time::OffsetDateTime::parse(saved_time, &time::format_description::well_known::Rfc3339)
            .ok();
    match (file_dt, saved_dt) {
        (Some(file), Some(saved)) => file > saved,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SerializedRemoteSpec, SerializedWindowBounds, TabState, WindowState, WindowsState,
        get_file_modified_time, is_file_newer,
    };
    use std::fs;
    use std::io::Write;
    use std::time::Duration;
    use tempfile::TempDir;

    /// Assert the geometry values of a GPUI window bounds rectangle.
    ///
    /// ### Parameters
    /// - `bounds`: The GPUI window bounds to inspect.
    /// - `expected_x`: Expected x origin in pixels.
    /// - `expected_y`: Expected y origin in pixels.
    /// - `expected_width`: Expected width in pixels.
    /// - `expected_height`: Expected height in pixels.
    fn assert_gpui_bounds_geometry(
        bounds: &gpui::WindowBounds,
        expected_x: f32,
        expected_y: f32,
        expected_width: f32,
        expected_height: f32,
    ) {
        use gpui::WindowBounds;
        let rect = match bounds {
            WindowBounds::Windowed(rect)
            | WindowBounds::Maximized(rect)
            | WindowBounds::Fullscreen(rect) => rect,
        };
        assert_eq!(f32::from(rect.origin.x), expected_x);
        assert_eq!(f32::from(rect.origin.y), expected_y);
        assert_eq!(f32::from(rect.size.width), expected_width);
        assert_eq!(f32::from(rect.size.height), expected_height);
    }

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
        }
    }

    #[test]
    fn test_get_file_modified_time_existing_file() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_file_modified_time.txt");
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(b"test content").unwrap();
        drop(file);
        let result = get_file_modified_time(&file_path);
        assert!(result.is_some());
        let timestamp = result.unwrap();
        assert!(
            time::OffsetDateTime::parse(&timestamp, &time::format_description::well_known::Rfc3339)
                .is_ok()
        );
        fs::remove_file(&file_path).ok();
    }

    #[test]
    fn test_get_file_modified_time_nonexistent_file() {
        let file_path = std::env::temp_dir().join("fulgur_nonexistent_modified_time_test.txt");
        fs::remove_file(&file_path).ok();
        let result = get_file_modified_time(&file_path);
        assert!(result.is_none());
    }

    #[test]
    fn test_get_file_modified_time_valid_rfc3339_format() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_rfc3339_format.txt");
        fs::File::create(&file_path).unwrap();
        let result = get_file_modified_time(&file_path);
        assert!(result.is_some());
        let timestamp = result.unwrap();
        assert!(timestamp.contains('T'));
        assert!(timestamp.contains('Z') || timestamp.contains('+') || timestamp.contains('-'));
        fs::remove_file(&file_path).ok();
    }

    #[test]
    fn test_is_file_newer_file_after_saved() {
        let file_time = "2024-01-02T12:00:00Z";
        let saved_time = "2024-01-01T12:00:00Z";
        assert!(is_file_newer(file_time, saved_time));
    }

    #[test]
    fn test_is_file_newer_file_before_saved() {
        let file_time = "2024-01-01T12:00:00Z";
        let saved_time = "2024-01-02T12:00:00Z";
        assert!(!is_file_newer(file_time, saved_time));
    }

    #[test]
    fn test_is_file_newer_file_equal_saved() {
        let file_time = "2024-01-01T12:00:00Z";
        let saved_time = "2024-01-01T12:00:00Z";

        assert!(!is_file_newer(file_time, saved_time));
    }

    #[test]
    fn test_is_file_newer_with_timezone_offset() {
        let file_time = "2024-01-02T12:00:00+01:00";
        let saved_time = "2024-01-02T11:00:00Z";
        assert!(!is_file_newer(file_time, saved_time));
        let saved_time2 = "2024-01-02T10:00:00Z";
        assert!(is_file_newer(file_time, saved_time2));
    }

    #[test]
    fn test_is_file_newer_invalid_file_time() {
        let file_time = "invalid timestamp";
        let saved_time = "2024-01-01T12:00:00Z";
        assert!(!is_file_newer(file_time, saved_time));
    }

    #[test]
    fn test_is_file_newer_invalid_saved_time() {
        let file_time = "2024-01-01T12:00:00Z";
        let saved_time = "invalid timestamp";
        assert!(!is_file_newer(file_time, saved_time));
    }

    #[test]
    fn test_is_file_newer_both_invalid() {
        let file_time = "invalid timestamp 1";
        let saved_time = "invalid timestamp 2";
        assert!(!is_file_newer(file_time, saved_time));
    }

    #[test]
    fn test_is_file_newer_milliseconds_difference() {
        let file_time = "2024-01-01T12:00:00.001Z";
        let saved_time = "2024-01-01T12:00:00.000Z";
        assert!(is_file_newer(file_time, saved_time));
    }

    #[test]
    fn test_is_file_newer_same_day_different_time() {
        let file_time = "2024-01-01T14:30:00Z";
        let saved_time = "2024-01-01T14:29:59Z";
        assert!(is_file_newer(file_time, saved_time));
    }

    #[test]
    fn test_is_file_newer_different_years() {
        let file_time = "2025-01-01T12:00:00Z";
        let saved_time = "2024-12-31T23:59:59Z";
        assert!(is_file_newer(file_time, saved_time));
    }

    #[test]
    fn test_is_file_newer_with_actual_file_times() {
        let temp_dir = std::env::temp_dir();
        let file1_path = temp_dir.join("test_file1.txt");
        let file2_path = temp_dir.join("test_file2.txt");
        fs::File::create(&file1_path).unwrap();
        let time1 = get_file_modified_time(&file1_path).unwrap();
        std::thread::sleep(Duration::from_millis(10));
        fs::File::create(&file2_path).unwrap();
        let time2 = get_file_modified_time(&file2_path).unwrap();
        assert!(is_file_newer(&time2, &time1));
        assert!(!is_file_newer(&time1, &time2));
        fs::remove_file(&file1_path).ok();
        fs::remove_file(&file2_path).ok();
    }

    #[test]
    fn test_serialized_window_bounds_from_gpui_windowed_preserves_geometry_and_display() {
        use gpui::{Bounds, WindowBounds, point, px, size};
        let gpui_bounds = WindowBounds::Windowed(Bounds {
            origin: point(px(120.0), px(80.0)),
            size: size(px(1440.0), px(900.0)),
        });
        let serialized = SerializedWindowBounds::from_gpui_bounds(gpui_bounds, Some(7));
        assert_eq!(serialized.state, "Windowed");
        assert_eq!(serialized.x, 120.0);
        assert_eq!(serialized.y, 80.0);
        assert_eq!(serialized.width, 1440.0);
        assert_eq!(serialized.height, 900.0);
        assert_eq!(serialized.display_id, Some(7));
    }

    #[test]
    fn test_serialized_window_bounds_from_gpui_maximized_preserves_geometry_and_display() {
        use gpui::{Bounds, WindowBounds, point, px, size};
        let gpui_bounds = WindowBounds::Maximized(Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(1920.0), px(1080.0)),
        });
        let serialized = SerializedWindowBounds::from_gpui_bounds(gpui_bounds, Some(2));
        assert_eq!(serialized.state, "Maximized");
        assert_eq!(serialized.x, 0.0);
        assert_eq!(serialized.y, 0.0);
        assert_eq!(serialized.width, 1920.0);
        assert_eq!(serialized.height, 1080.0);
        assert_eq!(serialized.display_id, Some(2));
    }

    #[test]
    fn test_serialized_window_bounds_from_gpui_fullscreen_preserves_geometry_and_display() {
        use gpui::{Bounds, WindowBounds, point, px, size};
        let gpui_bounds = WindowBounds::Fullscreen(Bounds {
            origin: point(px(10.0), px(20.0)),
            size: size(px(2560.0), px(1440.0)),
        });
        let serialized = SerializedWindowBounds::from_gpui_bounds(gpui_bounds, None);
        assert_eq!(serialized.state, "Fullscreen");
        assert_eq!(serialized.x, 10.0);
        assert_eq!(serialized.y, 20.0);
        assert_eq!(serialized.width, 2560.0);
        assert_eq!(serialized.height, 1440.0);
        assert_eq!(serialized.display_id, None);
    }

    #[test]
    fn test_serialized_window_bounds_to_gpui_bounds_preserves_geometry_for_each_state() {
        use gpui::WindowBounds;
        let cases = [
            (
                SerializedWindowBounds {
                    state: "Windowed".to_string(),
                    x: 11.0,
                    y: 22.0,
                    width: 1280.0,
                    height: 720.0,
                    display_id: Some(1),
                },
                "Windowed",
            ),
            (
                SerializedWindowBounds {
                    state: "Maximized".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 1920.0,
                    height: 1080.0,
                    display_id: Some(2),
                },
                "Maximized",
            ),
            (
                SerializedWindowBounds {
                    state: "Fullscreen".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 2560.0,
                    height: 1440.0,
                    display_id: None,
                },
                "Fullscreen",
            ),
        ];
        for (serialized, expected_state) in cases {
            let gpui_bounds = serialized.to_gpui_bounds();
            match (expected_state, &gpui_bounds) {
                ("Windowed", WindowBounds::Windowed(_))
                | ("Maximized", WindowBounds::Maximized(_))
                | ("Fullscreen", WindowBounds::Fullscreen(_)) => {}
                _ => panic!(
                    "unexpected WindowBounds variant for state {}",
                    expected_state
                ),
            }
            assert_gpui_bounds_geometry(
                &gpui_bounds,
                serialized.x,
                serialized.y,
                serialized.width,
                serialized.height,
            );
        }
    }

    #[test]
    fn test_serialized_window_bounds_to_gpui_bounds_unknown_state_defaults_to_windowed() {
        use gpui::WindowBounds;
        let serialized = SerializedWindowBounds {
            state: "UnknownState".to_string(),
            x: 40.0,
            y: 50.0,
            width: 900.0,
            height: 700.0,
            display_id: None,
        };
        let gpui_bounds = serialized.to_gpui_bounds();
        assert!(
            matches!(gpui_bounds, WindowBounds::Windowed(_)),
            "unknown window state should default to Windowed bounds"
        );
        assert_gpui_bounds_geometry(
            &gpui_bounds,
            serialized.x,
            serialized.y,
            serialized.width,
            serialized.height,
        );
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

    #[test]
    fn test_serialized_remote_spec_roundtrip_omits_password() {
        let spec = crate::fulgur::sync::ssh::url::RemoteSpec {
            host: "example.com".to_string(),
            port: 22,
            user: Some("alice".to_string()),
            path: "/tmp/test.txt".to_string(),
            password_in_url: Some(zeroize::Zeroizing::new("secret".to_string())),
        };

        let serialized = SerializedRemoteSpec::from_remote_spec(&spec);
        assert_eq!(serialized.host, "example.com");
        assert_eq!(serialized.user, "alice");

        let json = serde_json::to_string(&serialized).expect("serialize serialized remote spec");
        assert!(
            !json.contains("password"),
            "serialized remote spec must not include a password field"
        );

        let restored = serialized.to_remote_spec();
        assert_eq!(restored.host, "example.com");
        assert_eq!(restored.port, 22);
        assert_eq!(restored.user.as_deref(), Some("alice"));
        assert_eq!(restored.path, "/tmp/test.txt");
        assert!(restored.password_in_url.is_none());
    }
}
