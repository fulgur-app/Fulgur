//! Path utilities for Fulgur configuration and data files.
//!
//! This module provides functions for determining platform-specific configuration
//! directory paths. The configuration directory is:
//! - Windows: `%APPDATA%\Fulgur`
//! - macOS/Linux: `~/.fulgur`

use anyhow::Result;
use std::fs;
use std::path::PathBuf;

/// Get the Fulgur configuration directory path with platform-specific configuration directory and ensures
/// it exists by creating it if necessary.
///
/// ### Platform-specific paths
/// - **Windows**: `%APPDATA%\Fulgur` (e.g., `C:\Users\Username\AppData\Roaming\Fulgur`)
/// - **macOS/Linux**: `~/.fulgur` (e.g., `/home/username/.fulgur`)
///
/// ### Returns
/// - `Ok(PathBuf)`: The path to the configuration directory
/// - `Err(anyhow::Error)`: If the environment variable is not set or directory creation failed
///
/// ### Errors
/// - On Windows: Returns error if `APPDATA` environment variable is not set
/// - On Unix: Returns error if `HOME` environment variable is not set
/// - Returns error if directory creation fails (permissions, disk full, etc.)
pub fn config_dir() -> Result<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let app_data = std::env::var("APPDATA")?;
        let mut path = PathBuf::from(app_data);
        path.push("Fulgur");
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME")?;
        let mut path = PathBuf::from(home);
        path.push(".fulgur");
        fs::create_dir_all(&path)?;
        Ok(path)
    }
}

/// Get a path to a subdirectory inside the Fulgur configuration directory and
/// ensure it exists.
///
/// ### Arguments
/// - `subdir`: The subdirectory name to create/access
///
/// ### Returns
/// - `Ok(PathBuf)`: Absolute path to the subdirectory
/// - `Err(anyhow::Error)`: If base config path resolution or directory creation fails
pub fn config_subdir(subdir: &str) -> Result<PathBuf> {
    let mut path = config_dir()?;
    path.push(subdir);
    fs::create_dir_all(&path)?;
    Ok(path)
}

/// Get a path to a file inside the Fulgur configuration directory.
///
/// ### Arguments
/// - `filename`: The file name to resolve in the config directory
///
/// ### Returns
/// - `Ok(PathBuf)`: Absolute path to the file location
/// - `Err(anyhow::Error)`: If config directory resolution fails
pub fn config_file(filename: &str) -> Result<PathBuf> {
    let mut path = config_dir()?;
    path.push(filename);
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_dir_exists() {
        // This test verifies that config_dir() returns a path and creates the directory
        let dir = config_dir().expect("Failed to get config dir");
        assert!(
            dir.exists(),
            "Config directory should exist after calling config_dir()"
        );
        assert!(dir.is_dir(), "Config path should be a directory");
    }

    #[test]
    fn test_config_dir_platform_specific() {
        let dir = config_dir().expect("Failed to get config dir");

        #[cfg(target_os = "windows")]
        {
            // On Windows, should contain Fulgur in AppData
            assert!(dir.to_string_lossy().contains("Fulgur"));
        }

        #[cfg(not(target_os = "windows"))]
        {
            // On Unix, should end with .fulgur
            assert!(dir.to_string_lossy().ends_with(".fulgur"));
        }
    }

    #[test]
    fn test_config_dir_idempotent() {
        // Calling config_dir() multiple times should return the same path
        let dir1 = config_dir().expect("Failed to get config dir (1st call)");
        let dir2 = config_dir().expect("Failed to get config dir (2nd call)");
        assert_eq!(dir1, dir2, "config_dir() should be idempotent");
    }

    #[test]
    fn test_config_subdir_creates_subdirectory() {
        let themes_dir = config_subdir("themes").expect("Failed to get themes subdir");
        assert!(themes_dir.exists(), "Subdirectory should exist");
        assert!(
            themes_dir.is_dir(),
            "Subdirectory path should be a directory"
        );
        assert!(
            themes_dir.to_string_lossy().contains("themes"),
            "Subdirectory should include requested name"
        );
    }

    #[test]
    fn test_config_file_resolves_inside_config_dir() {
        let base = config_dir().expect("Failed to get base config dir");
        let log_path = config_file("fulgur.log").expect("Failed to get config file path");
        assert_eq!(log_path.parent(), Some(base.as_path()));
        assert_eq!(
            log_path.file_name().and_then(|n| n.to_str()),
            Some("fulgur.log")
        );
    }
}
