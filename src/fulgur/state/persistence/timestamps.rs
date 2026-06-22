use std::fs;
use std::path::PathBuf;

/// Get the last modified time of a file as ISO 8601 string
///
/// ### Arguments
/// - `path`: The path to the file
///
/// ### Returns
/// - `Some(String)`: The last modified time of the file
/// - `None`: If the file could not be found or the last modified time could not be determined
#[must_use]
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
#[must_use]
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
}
