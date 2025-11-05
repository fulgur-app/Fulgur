use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TabState {
    pub id: usize,
    pub title: String,
    pub file_path: Option<PathBuf>,
    pub content: Option<String>,
    pub last_saved: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AppState {
    pub tabs: Vec<TabState>,
    pub active_tab_index: Option<usize>,
    pub next_tab_id: usize,
}

impl AppState {
    // Get the path to the state file
    // @return: The path to the state file
    fn state_file_path() -> anyhow::Result<PathBuf> {
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

    // Save the app state to disk
    // @return: The result of the save operation
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::state_file_path()?;
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    // Load the app state from disk
    // @return: The loaded app state
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::state_file_path()?;
        let json = fs::read_to_string(path)?;
        let state: AppState = serde_json::from_str(&json)?;
        Ok(state)
    }
}

// Get the last modified time of a file as ISO 8601 string
// @param path: The path to the file
// @return: The last modified time of the file
pub fn get_file_modified_time(path: &PathBuf) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let datetime: chrono::DateTime<chrono::Utc> = modified.into();
    Some(datetime.to_rfc3339())
}

// Compare two ISO 8601 timestamps
// @param file_time: The time of the file
// @param saved_time: The time of the saved file
// @return: True if the file is newer than the saved file
// Returns true if file_time is after saved_time
pub fn is_file_newer(file_time: &str, saved_time: &str) -> bool {
    let file_dt = chrono::DateTime::parse_from_rfc3339(file_time).ok();
    let saved_dt = chrono::DateTime::parse_from_rfc3339(saved_time).ok();

    match (file_dt, saved_dt) {
        (Some(file), Some(saved)) => file > saved,
        _ => false,
    }
}
