use std::path::{Path, PathBuf};

use super::super::file_browser::truncate_to_parent_folder;

/// Parse the raw input string into a `(parent_directory, filter_prefix)` pair.
///
/// ### Arguments
/// - `raw`: The raw input string from the user
///
/// ### Returns
/// - `Some((parent_directory, filter_prefix))`: if the parent directory exists
/// - `None`: if the parent directory doesn't exist
pub(super) fn parse_input_path(raw: &str) -> Option<(PathBuf, String)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let expanded = if trimmed == "~" || trimmed.starts_with("~/") || trimmed.starts_with("~\\") {
        let home = home_dir()?;
        if trimmed == "~" {
            home.to_string_lossy().to_string()
        } else {
            home.join(&trimmed[2..]).to_string_lossy().to_string()
        }
    } else {
        trimmed.to_string()
    };
    // Special handling for paths ending with "/." or "\." to filter dotfiles
    // Without this, PathBuf::parent() treats "." as current directory
    if (expanded.ends_with("/.") || expanded.ends_with("\\.")) && expanded.len() > 2 {
        let parent_str = &expanded[..expanded.len() - 1]; // Remove the dot, keep the separator
        let parent = PathBuf::from(parent_str);
        if parent.is_dir() {
            return Some((parent, ".".to_string()));
        }
        return None;
    }
    let path = PathBuf::from(&expanded);
    if expanded.ends_with('/') || expanded.ends_with(std::path::MAIN_SEPARATOR) {
        if path.is_dir() {
            return Some((path, String::new()));
        }
        return None;
    }
    let parent = path.parent()?;
    if !parent.is_dir() {
        return None;
    }
    let filter = path
        .file_name()
        .map_or_default(|f| f.to_string_lossy().to_string());
    Some((parent.to_path_buf(), filter))
}

/// Truncate a local browser input to its parent folder.
///
/// ### Description
/// A bare `~` stands for the home folder, so its parent is the folder holding
/// the home folder. Roots such as `/`, `C:\` or `\\server\share` have no parent.
///
/// ### Arguments
/// - `raw`: The raw input string from the user
///
/// ### Returns
/// - `Some(String)`: The input up to and including its previous separator
/// - `None`: When the input names a root, or a single relative segment
pub(super) fn local_parent_folder(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let path = if trimmed.trim_end_matches(std::path::is_separator) == "~" {
        home_dir()?.to_string_lossy().into_owned()
    } else {
        trimmed.to_owned()
    };
    // The platform decides where the roots are: `\\server\share` holds separators
    // but has no parent folder.
    let folder = Path::new(path.trim_end_matches(std::path::is_separator));
    folder
        .parent()
        .and(truncate_to_parent_folder(&path, std::path::is_separator))
        .map(str::to_owned)
}

/// Get the user's home directory.
///
/// ### Returns
/// - `Some<PathBuf>`: The user's home directory.
/// - `None`: If the user's home directory could not be determined.
pub(super) fn home_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::{home_dir, local_parent_folder, parse_input_path};
    use core::prelude::v1::test;
    use std::path::{MAIN_SEPARATOR, PathBuf};

    #[test]
    fn test_local_parent_folder_drops_the_last_folder_of_a_folder_path() {
        assert_eq!(local_parent_folder("/var/log/").as_deref(), Some("/var/"));
        assert_eq!(local_parent_folder("/var/").as_deref(), Some("/"));
    }

    #[test]
    fn test_local_parent_folder_drops_only_a_partial_name() {
        assert_eq!(
            local_parent_folder("/var/log/sys").as_deref(),
            Some("/var/log/")
        );
    }

    #[test]
    fn test_local_parent_folder_trims_whitespace() {
        assert_eq!(
            local_parent_folder("  /var/log/  ").as_deref(),
            Some("/var/")
        );
    }

    #[test]
    fn test_local_parent_folder_stops_at_the_root() {
        assert_eq!(local_parent_folder("/"), None);
        assert_eq!(local_parent_folder(""), None);
        assert_eq!(local_parent_folder("notes.txt"), None);
    }

    #[test]
    fn test_local_parent_folder_keeps_the_tilde_below_home() {
        assert_eq!(local_parent_folder("~/Documents/").as_deref(), Some("~/"));
    }

    #[test]
    fn test_local_parent_folder_expands_a_bare_tilde() {
        let home = home_dir().expect("the test environment must define a home folder");
        let home_parent = local_parent_folder(&home.to_string_lossy());
        assert!(home_parent.is_some(), "the home folder must have a parent");
        assert_eq!(local_parent_folder("~"), home_parent);
        assert_eq!(
            local_parent_folder(&format!("~{MAIN_SEPARATOR}")),
            home_parent
        );
    }

    #[cfg(windows)]
    #[test]
    fn test_local_parent_folder_stops_at_windows_roots() {
        assert_eq!(local_parent_folder(r"C:\Users\").as_deref(), Some(r"C:\"));
        assert_eq!(local_parent_folder(r"C:\"), None);
        assert_eq!(local_parent_folder(r"\\server\share\"), None);
        assert_eq!(
            local_parent_folder(r"\\server\share\dir").as_deref(),
            Some(r"\\server\share\")
        );
    }

    #[test]
    fn test_empty_input() {
        assert!(parse_input_path("").is_none());
        assert!(parse_input_path("   ").is_none());
    }

    #[test]
    fn test_root_slash() {
        let result = parse_input_path("/");
        assert!(result.is_some());
        let (parent, filter) = result.unwrap();
        assert_eq!(parent, PathBuf::from("/"));
        assert_eq!(filter, "");
    }

    #[test]
    fn test_directory_with_trailing_slash() {
        let temp_dir = std::env::temp_dir();
        let temp_str = temp_dir.to_string_lossy().to_string() + "/";
        let result = parse_input_path(&temp_str);
        assert!(result.is_some());
        let (parent, filter) = result.unwrap();
        assert!(parent.is_dir());
        assert_eq!(filter, "");
    }

    #[test]
    fn test_partial_name_filter() {
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.join("test_nonexistent_prefix_xyz");
        let result = parse_input_path(&test_path.to_string_lossy());
        assert!(result.is_some());
        let (parent, filter) = result.unwrap();
        assert_eq!(parent, temp_dir);
        assert_eq!(filter, "test_nonexistent_prefix_xyz");
    }

    #[test]
    fn test_nonexistent_parent() {
        let result = parse_input_path("/definitely_not_a_real_directory_abc123/foo");
        assert!(result.is_none());
    }

    #[test]
    fn test_tilde_expansion() {
        let result = parse_input_path("~/");
        assert!(result.is_some());
        let (parent, filter) = result.unwrap();
        // Should expand to home directory, not literally "~/"
        assert_ne!(parent, PathBuf::from("~/"));
        assert!(parent.is_dir());
        assert_eq!(filter, "");
    }

    #[test]
    fn test_dotfile_filter() {
        // Test that "/path/." correctly filters for dotfiles
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.to_string_lossy().to_string() + "/.";
        let result = parse_input_path(&test_path);
        assert!(result.is_some());
        let (parent, filter) = result.unwrap();
        assert!(parent.is_dir());
        assert_eq!(filter, ".");
    }
}
