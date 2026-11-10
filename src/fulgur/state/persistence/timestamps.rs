use std::fs;
use std::path::Path;
use std::time::SystemTime;

/// Get the last modified time of a file as ISO 8601 string
///
/// ### Arguments
/// - `path`: The path to the file
///
/// ### Returns
/// - `Some(String)`: The last modified time of the file
/// - `None`: If the file could not be found or the last modified time could not be determined
#[must_use]
pub fn get_file_modified_time(path: &Path) -> Option<String> {
    file_modified_system_time(path).and_then(format_modified_time)
}

/// Read the last modified time of a file from the file system.
///
/// ### Arguments
/// - `path`: The path to the file
///
/// ### Returns
/// - `Some(SystemTime)`: The last modified time of the file
/// - `None`: If the file could not be found or the platform reports no modification time
#[must_use]
pub fn file_modified_system_time(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

/// Format a file modification time as the ISO 8601 string persisted in the session state.
///
/// ### Arguments
/// - `modified`: The modification time to format
///
/// ### Returns
/// - `Some(String)`: The RFC 3339 representation, with full sub-second precision
/// - `None`: If the time cannot be represented in RFC 3339
#[must_use]
pub fn format_modified_time(modified: SystemTime) -> Option<String> {
    time::OffsetDateTime::from(modified)
        .format(&time::format_description::well_known::Rfc3339)
        .ok()
}

/// Parse a persisted ISO 8601 file modification time back into a `SystemTime`.
///
/// ### Arguments
/// - `timestamp`: The RFC 3339 string produced by `format_modified_time`
///
/// ### Returns
/// - `Some(SystemTime)`: The parsed modification time
/// - `None`: If the string is not a valid RFC 3339 timestamp
#[must_use]
pub fn parse_modified_time(timestamp: &str) -> Option<SystemTime> {
    time::OffsetDateTime::parse(timestamp, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(SystemTime::from)
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
    use super::{get_file_modified_time, is_file_newer, parse_modified_time};
    use std::fs;
    use std::io::Write;
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    const FIXED_MODIFIED_TIME: &str = "2024-01-01T12:00:00Z";

    /// Parse `FIXED_MODIFIED_TIME` into the instant the timestamp tests stamp on files
    ///
    /// ### Returns
    /// - `SystemTime`: 2024-01-01 at noon UTC
    fn fixed_modified_time() -> SystemTime {
        parse_modified_time(FIXED_MODIFIED_TIME).unwrap()
    }

    /// Create a file whose modification time is set explicitly
    ///
    /// ### Arguments
    /// - `path`: The file to create
    /// - `modified`: The modification time to stamp on the file
    fn create_file_modified_at(path: &Path, modified: SystemTime) {
        let file = fs::File::create(path).unwrap();
        file.set_modified(modified).unwrap();
    }

    #[test]
    fn test_get_file_modified_time_existing_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_file_modified_time.txt");
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
    }

    #[test]
    fn test_get_file_modified_time_nonexistent_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("nonexistent.txt");
        let result = get_file_modified_time(&file_path);
        assert!(result.is_none());
    }

    #[test]
    fn test_get_file_modified_time_valid_rfc3339_format() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_rfc3339_format.txt");
        create_file_modified_at(&file_path, fixed_modified_time());
        let result = get_file_modified_time(&file_path);
        assert_eq!(result.as_deref(), Some(FIXED_MODIFIED_TIME));
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
        let temp_dir = tempfile::tempdir().unwrap();
        let file1_path = temp_dir.path().join("test_file1.txt");
        let file2_path = temp_dir.path().join("test_file2.txt");
        let older = fixed_modified_time();
        create_file_modified_at(&file1_path, older);
        create_file_modified_at(&file2_path, older + Duration::from_secs(1));
        let time1 = get_file_modified_time(&file1_path).unwrap();
        let time2 = get_file_modified_time(&file2_path).unwrap();
        assert!(is_file_newer(&time2, &time1));
        assert!(!is_file_newer(&time1, &time2));
    }
}
