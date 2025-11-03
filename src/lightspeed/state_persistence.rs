use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Represents the state of a single tab
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TabState {
    /// The ID of the tab
    pub id: usize,
    /// The title of the tab
    pub title: String,
    /// The file path if the tab is associated with a file
    pub file_path: Option<PathBuf>,
    /// The content of the tab (only saved if modified or unsaved)
    pub content: Option<String>,
    /// The last saved timestamp (ISO 8601 format)
    pub last_saved: Option<String>,
}

/// Represents the entire app state
#[derive(Serialize, Deserialize, Debug)]
pub struct AppState {
    /// All tabs in the app
    pub tabs: Vec<TabState>,
    /// The index of the active tab
    pub active_tab_index: Option<usize>,
    /// The next tab ID to use
    pub next_tab_id: usize,
}

impl AppState {
    /// Get the path to the state file
    fn state_file_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        #[cfg(target_os = "windows")]
        {
            let app_data = std::env::var("APPDATA")?;
            let mut path = PathBuf::from(app_data);
            path.push("Lightspeed");
            fs::create_dir_all(&path)?;
            path.push("state.json");
            Ok(path)
        }

        #[cfg(not(target_os = "windows"))]
        {
            let home = std::env::var("HOME")?;
            let mut path = PathBuf::from(home);
            path.push(".lightspeed");
            fs::create_dir_all(&path)?;
            path.push("state.json");
            Ok(path)
        }
    }

    /// Save the app state to disk
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::state_file_path()?;
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Load the app state from disk
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let path = Self::state_file_path()?;

        if !path.exists() {
            return Err("State file does not exist".into());
        }

        let json = fs::read_to_string(path)?;
        let state: AppState = serde_json::from_str(&json)?;
        Ok(state)
    }
}

/// Get the last modified time of a file as ISO 8601 string
pub fn get_file_modified_time(path: &PathBuf) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let datetime: chrono::DateTime<chrono::Utc> = modified.into();
    Some(datetime.to_rfc3339())
}

/// Compare two ISO 8601 timestamps
/// Returns true if file_time is after saved_time
pub fn is_file_newer(file_time: &str, saved_time: &str) -> bool {
    let file_dt = chrono::DateTime::parse_from_rfc3339(file_time).ok();
    let saved_dt = chrono::DateTime::parse_from_rfc3339(saved_time).ok();

    match (file_dt, saved_dt) {
        (Some(file), Some(saved)) => file > saved,
        _ => false,
    }
}
