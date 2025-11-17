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

#[cfg(test)]
mod tests {
    use super::{get_file_modified_time, is_file_newer};
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn test_get_file_modified_time_existing_file() {
        // Create a temporary file
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_file_modified_time.txt");
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(b"test content").unwrap();
        drop(file);
        let result = get_file_modified_time(&file_path);
        assert!(result.is_some());
        let timestamp = result.unwrap();
        assert!(chrono::DateTime::parse_from_rfc3339(&timestamp).is_ok());
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
