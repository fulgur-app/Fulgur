//! Path utilities for Fulgur configuration and data files.
//!
//! This module provides functions for determining platform-specific configuration
//! directory paths. The configuration directory is:
//! - Windows: `%APPDATA%\Fulgur`
//! - macOS/Linux: `~/.fulgur`
//! - Portable build: `<folder of the executable>/data`

use super::portable::{self, PortableLayout};
use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(debug_assertions)]
use std::sync::OnceLock;

#[cfg(debug_assertions)]
static CONFIG_DIR_OVERRIDE: OnceLock<PathBuf> = OnceLock::new();

/// Override the configuration directory for the lifetime of the process. Intended for development use only (e.g. the `-d` CLI flag).
///
/// ### Arguments
/// - `path`: Directory to use as the Fulgur configuration root
#[cfg(debug_assertions)]
pub fn set_config_dir_override(path: PathBuf) {
    let _ = CONFIG_DIR_OVERRIDE.set(path);
}

/// Get the Fulgur configuration directory path and ensure it exists by creating it if necessary.
///
/// ### Platform-specific paths
/// - **Windows**: `%APPDATA%\Fulgur` (e.g., `C:\Users\Username\AppData\Roaming\Fulgur`)
/// - **macOS/Linux**: `~/.fulgur` (e.g., `/home/username/.fulgur`)
/// - **Portable**: `<folder of the executable>/data`
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
    let path = resolve_config_dir(
        config_dir_override(),
        portable::layout(),
        platform_config_dir,
    )?;
    fs::create_dir_all(&path)?;
    Ok(path)
}

/// Pick the configuration directory among the candidate locations.
///
/// ### Arguments
/// - `override_dir`: The development override, which always wins
/// - `portable`: The portable layout, used when the process runs in portable mode
/// - `platform_default`: Resolves the platform default, only called as a last resort
///
/// ### Errors
/// - Returns the error of `platform_default` when it is the chosen candidate.
///
/// ### Returns
/// - `Ok(PathBuf)`: The configuration directory, not created yet
/// - `Err(anyhow::Error)`: The platform default could not be resolved
fn resolve_config_dir(
    override_dir: Option<&Path>,
    portable: Option<&PortableLayout>,
    platform_default: impl FnOnce() -> Result<PathBuf>,
) -> Result<PathBuf> {
    if let Some(dir) = override_dir {
        return Ok(dir.to_path_buf());
    }
    if let Some(layout) = portable {
        return Ok(layout.data_dir());
    }
    platform_default()
}

/// The development override of the configuration directory.
///
/// ### Returns
/// - `Some(&Path)`: The directory passed with `-d` in a debug build
/// - `None`: No override is set, or this is a release build
fn config_dir_override() -> Option<&'static Path> {
    #[cfg(debug_assertions)]
    return CONFIG_DIR_OVERRIDE.get().map(PathBuf::as_path);
    #[cfg(not(debug_assertions))]
    return None;
}

/// The platform default configuration directory, inside the user profile.
///
/// ### Errors
/// - Returns an error if `APPDATA` (Windows) or `HOME` (Unix) is not set.
///
/// ### Returns
/// - `Ok(PathBuf)`: The platform default directory, not created yet
/// - `Err(anyhow::Error)`: The user profile could not be located
fn platform_config_dir() -> Result<PathBuf> {
    #[cfg(target_os = "windows")]
    let path = PathBuf::from(std::env::var("APPDATA")?).join("Fulgur");

    #[cfg(not(target_os = "windows"))]
    let path = PathBuf::from(std::env::var("HOME")?).join(".fulgur");

    #[cfg(test)]
    let path = path.join("test");

    Ok(path)
}

/// Get a path to a subdirectory inside the Fulgur configuration directory and
/// ensure it exists.
///
/// ### Arguments
/// - `subdir`: The subdirectory name to create/access
///
/// ### Errors
/// Returns an error if the base config directory cannot be resolved or if the
/// subdirectory cannot be created.
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
/// ### Errors
/// Returns an error if the base config directory cannot be resolved.
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
    use super::{config_dir, config_file, config_subdir, resolve_config_dir};
    use crate::fulgur::utils::portable::PortableLayout;
    use std::path::{Path, PathBuf};

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
            // On Unix, test dir is ~/.fulgur/test
            assert!(dir.to_string_lossy().ends_with(".fulgur/test"));
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

    #[test]
    fn test_resolve_config_dir_prefers_the_override_over_portable_and_default() {
        let portable = PortableLayout::new(PathBuf::from("stick"), PathBuf::from("stick"));

        let resolved = resolve_config_dir(Some(Path::new("dev")), Some(&portable), || {
            panic!("the platform default must not be resolved")
        })
        .unwrap();

        assert_eq!(resolved, PathBuf::from("dev"));
    }

    #[test]
    fn test_resolve_config_dir_prefers_portable_over_default() {
        let root = PathBuf::from("stick").join("Fulgur");
        let portable = PortableLayout::new(root.clone(), PathBuf::from("stick"));

        let resolved = resolve_config_dir(None, Some(&portable), || {
            panic!("the platform default must not be resolved")
        })
        .unwrap();

        assert_eq!(resolved, root.join("data"));
    }

    #[test]
    fn test_resolve_config_dir_falls_back_to_the_platform_default() {
        let resolved = resolve_config_dir(None, None, || Ok(PathBuf::from("profile"))).unwrap();

        assert_eq!(resolved, PathBuf::from("profile"));
    }
}
