use std::path::{Path, PathBuf};

use super::super::file_browser::{BrowserEntry, build_browser_entry};

/// A single entry in the file browser list.
pub(super) struct PathEntry {
    pub(super) name: String,
    pub(super) is_dir: bool,
    full_path: PathBuf,
}

impl From<&PathEntry> for BrowserEntry {
    fn from(e: &PathEntry) -> Self {
        let full_path = e.full_path.to_string_lossy().into_owned();
        build_browser_entry(
            gpui_kit::SharedString::from(full_path.clone()),
            e.is_dir,
            &e.name,
            &full_path,
            std::path::MAIN_SEPARATOR,
        )
    }
}

/// Read the directory and filter entries by a case-insensitive prefix.
///
/// ### Arguments
/// - `parent`: The directory to read
/// - `filter`: The case-insensitive prefix to filter entries by
///
/// ### Returns
/// - `Vec<PathEntry>`: sorted with directories first, then files,
///   alphabetical within each group. Capped at 500 entries.
pub(super) fn read_and_filter_entries(parent: &Path, filter: &str) -> Vec<PathEntry> {
    let Ok(read_dir) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    let filter_lower = filter.to_lowercase();
    let mut entries: Vec<PathEntry> = read_dir
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if !filter_lower.is_empty() && !name.to_lowercase().starts_with(&filter_lower) {
                return None;
            }
            // Follow symlinks for is_dir detection
            let is_dir = entry.metadata().is_ok_and(|m| m.is_dir());
            Some(PathEntry {
                full_path: entry.path(),
                name,
                is_dir,
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries.truncate(500);
    entries
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::read_and_filter_entries;
    use core::prelude::v1::test;
    use std::path::Path;

    #[test]
    fn test_read_and_filter_entries_nonexistent() {
        let entries = read_and_filter_entries(Path::new("/no_such_dir_abc123"), "");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_read_and_filter_entries_root() {
        let entries = read_and_filter_entries(Path::new("/"), "");
        assert!(!entries.is_empty());
        // Directories should come first
        let first_file_idx = entries.iter().position(|e| !e.is_dir);
        let last_dir_idx = entries.iter().rposition(|e| e.is_dir);
        if let (Some(first_file), Some(last_dir)) = (first_file_idx, last_dir_idx) {
            assert!(last_dir < first_file);
        }
    }

    #[test]
    fn test_read_and_filter_entries_with_filter() {
        let entries = read_and_filter_entries(Path::new("/"), "t");
        for entry in &entries {
            assert!(entry.name.to_lowercase().starts_with('t'));
        }
    }
}
