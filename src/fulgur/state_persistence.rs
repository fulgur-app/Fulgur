use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Persisted state of a single editor tab
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TabState {
    /// Unique identifier for the tab within its window
    pub id: usize,
    /// Display title shown in the tab bar (usually the filename)
    pub title: String,
    /// Path to the file on disk, if the tab has an associated file. `None` for unsaved/new tabs.
    pub file_path: Option<PathBuf>,
    /// The text content of the tab, stored for unsaved tabs or when the file may have been modified since last save
    pub content: Option<String>,
    /// ISO 8601 timestamp of when the content was last saved to disk. Used to detect if the file has been modified externally.
    pub last_saved: Option<String>,
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
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WindowState {
    /// All tabs in this window, in display order
    pub tabs: Vec<TabState>,
    /// Index of the currently active/visible tab, if any
    pub active_tab_index: Option<usize>,
    /// Counter for generating unique tab IDs within this window
    pub next_tab_id: usize,
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
    fn state_file_path() -> anyhow::Result<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            let app_data = std::env::var("APPDATA")?;
            let mut path = PathBuf::from(app_data);
            path.push("Fulgur");
            fs::create_dir_all(&path)?;
            path.push("state.json");
            Ok(path)
        }

        #[cfg(not(target_os = "windows"))]
        {
            let home = std::env::var("HOME")?;
            let mut path = PathBuf::from(home);
            path.push(".fulgur");
            fs::create_dir_all(&path)?;
            path.push("state.json");
            Ok(path)
        }
    }

    /// Save the app state to disk
    ///
    /// ### Returns
    /// - `Ok(())`: If the app state was saved successfully
    /// - `Err(anyhow::Error)`: If the app state could not be saved
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::state_file_path()?;
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Load the windows state from disk
    ///
    /// ### Returns
    /// - `Ok(WindowsState)`: The loaded windows state
    /// - `Err(anyhow::Error)`: If the windows state could not be loaded
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::state_file_path()?;
        let json = fs::read_to_string(path)?;
        if let Ok(state) = serde_json::from_str::<WindowsState>(&json) {
            return Ok(state);
        }
        Err(anyhow::anyhow!("Failed to parse state file"))
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
    use super::{get_file_modified_time, is_file_newer};
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::Duration;

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
        let file_path = PathBuf::from("/nonexistent/path/file.txt");
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
}
