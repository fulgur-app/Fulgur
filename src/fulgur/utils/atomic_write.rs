use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Write file contents atomically by writing to a sibling temporary file,
/// syncing it, then renaming it over the destination path.
///
/// ### Arguments
/// - `path`: the path to the file to write
/// - `content`: the content to write in the file
///
/// ### Return
/// - `Ok(())`: the write is successful
/// - `Err()`: error while writing the file
pub fn atomic_write_file(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "Cannot atomically write '{}': destination has no parent directory",
            path.display()
        )
    })?;
    let filename = path.file_name().ok_or_else(|| {
        anyhow::anyhow!(
            "Cannot atomically write '{}': destination has no filename",
            path.display()
        )
    })?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let tmp_name = format!(
        ".{}.{}.{}.tmp",
        filename.to_string_lossy(),
        std::process::id(),
        nonce
    );
    let tmp_path = parent.join(tmp_name);
    let write_result = (|| -> anyhow::Result<()> {
        let mut tmp_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp_path)
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to create temp file '{}' for atomic write: {}",
                    tmp_path.display(),
                    e
                )
            })?;
        tmp_file.write_all(contents).map_err(|e| {
            anyhow::anyhow!("Failed to write temp file '{}': {}", tmp_path.display(), e)
        })?;
        tmp_file.flush().map_err(|e| {
            anyhow::anyhow!("Failed to flush temp file '{}': {}", tmp_path.display(), e)
        })?;
        tmp_file.sync_all().map_err(|e| {
            anyhow::anyhow!(
                "Failed to sync temp file '{}' to disk: {}",
                tmp_path.display(),
                e
            )
        })?;
        fs::rename(&tmp_path, path).map_err(|e| {
            anyhow::anyhow!(
                "Failed to replace '{}' with '{}' atomically: {}",
                path.display(),
                tmp_path.display(),
                e
            )
        })?;
        #[cfg(unix)]
        {
            // Best effort: persist directory metadata (rename) to disk.
            let dir = OpenOptions::new().read(true).open(parent).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to open parent directory '{}' for sync: {}",
                    parent.display(),
                    e
                )
            })?;
            dir.sync_all().map_err(|e| {
                anyhow::anyhow!(
                    "Failed to sync parent directory '{}' to disk: {}",
                    parent.display(),
                    e
                )
            })?;
        }
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    write_result
}

#[cfg(test)]
mod tests {
    use super::atomic_write_file;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn atomic_write_creates_new_file_with_expected_contents() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("settings.json");

        atomic_write_file(&file_path, br#"{"a":1}"#).expect("atomic write should succeed");

        let written = fs::read_to_string(&file_path).expect("file should exist after atomic write");
        assert_eq!(written, r#"{"a":1}"#);
    }

    #[test]
    fn atomic_write_replaces_existing_file_contents() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("state.json");
        fs::write(&file_path, "old contents").expect("failed to write initial file");

        atomic_write_file(&file_path, b"new contents").expect("atomic write should succeed");

        let written = fs::read_to_string(&file_path).expect("file should exist after replacement");
        assert_eq!(written, "new contents");
    }

    #[test]
    fn atomic_write_errors_when_parent_directory_is_missing() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let missing_parent = temp_dir.path().join("missing");
        let file_path = missing_parent.join("settings.json");

        let result = atomic_write_file(&file_path, b"content");

        assert!(
            result.is_err(),
            "write should fail when parent directory is missing"
        );
        assert!(
            !file_path.exists(),
            "destination should not be created on failure"
        );
    }

    #[test]
    fn atomic_write_errors_with_empty_path() {
        let result = atomic_write_file(Path::new(""), b"content");
        assert!(result.is_err(), "empty destination path should fail");
    }
}
