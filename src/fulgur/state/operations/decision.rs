use super::super::persistence::{SerializedRemoteSpec, is_file_newer};
use std::path::PathBuf;

/// Decision for how to restore a tab from saved state
#[derive(Debug, PartialEq, Eq)]
pub enum TabRestoreDecision {
    /// Restore a remote tab (SSH/SFTP) from serialized metadata.
    RestoreRemote {
        remote: SerializedRemoteSpec,
        content: Option<String>,
    },
    /// Load content from file on disk
    LoadFromFile { path: PathBuf },
    /// Use saved content with file path. `changed_on_disk` is set when the file
    /// was modified after the content was persisted, so the user must choose
    /// between the recovered edits and the disk version.
    UseSavedContentWithPath {
        path: PathBuf,
        content: String,
        changed_on_disk: bool,
    },
    /// Use saved content without file path (unsaved tab)
    UseSavedContentNoPath { content: String },
    /// Skip this tab (cannot be restored)
    Skip,
}

/// Determine how to restore a tab based on saved state and file system state
///
/// ### Arguments
/// - `saved_path`: The saved file path (if any)
/// - `saved_content`: The saved content (if any)
/// - `last_saved`: The last saved timestamp as ISO 8601 string (if any)
/// - `file_exists`: Whether the file exists on disk
/// - `file_modified_time`: The file's modification time as ISO 8601 string (if it exists)
/// - `can_read_file`: Whether the file can be read successfully
///
/// ### Returns
/// - `TabRestoreDecision`: The decision for how to restore this tab
#[must_use]
pub fn determine_tab_restore_strategy(
    saved_path: Option<PathBuf>,
    saved_remote: Option<SerializedRemoteSpec>,
    saved_content: Option<String>,
    last_saved: Option<String>,
    file_exists: bool,
    file_modified_time: Option<String>,
    can_read_file: bool,
) -> TabRestoreDecision {
    if let Some(remote) = saved_remote {
        return TabRestoreDecision::RestoreRemote {
            remote,
            content: saved_content,
        };
    }

    match (saved_path, saved_content) {
        // Case 1: Has both path and content (modified file). The persisted edits
        // are never dropped: a newer file on disk raises a conflict instead.
        (Some(path), Some(content)) => {
            if file_exists {
                let file_is_newer = matches!(
                    (last_saved, file_modified_time),
                    (Some(saved_time), Some(file_time)) if is_file_newer(&file_time, &saved_time)
                );
                TabRestoreDecision::UseSavedContentWithPath {
                    path,
                    content,
                    changed_on_disk: file_is_newer && can_read_file,
                }
            } else {
                TabRestoreDecision::UseSavedContentNoPath { content }
            }
        }
        (Some(path), None) => {
            if file_exists && can_read_file {
                TabRestoreDecision::LoadFromFile { path }
            } else {
                TabRestoreDecision::Skip
            }
        }
        (None, Some(content)) => TabRestoreDecision::UseSavedContentNoPath { content },
        (None, None) => TabRestoreDecision::Skip,
    }
}

#[cfg(test)]
mod tests {
    use super::{TabRestoreDecision, determine_tab_restore_strategy};
    use crate::fulgur::state::SerializedRemoteSpec;
    use std::path::PathBuf;

    #[test]
    fn test_determine_tab_restore_strategy_keeps_saved_content_and_flags_conflict_when_newer() {
        let decision = determine_tab_restore_strategy(
            Some(PathBuf::from("/tmp/example.md")),
            None,
            Some("saved".to_string()),
            Some("2026-04-07T09:00:00Z".to_string()),
            true,
            Some("2026-04-07T10:00:00Z".to_string()),
            true,
        );
        assert_eq!(
            decision,
            TabRestoreDecision::UseSavedContentWithPath {
                path: PathBuf::from("/tmp/example.md"),
                content: "saved".to_string(),
                changed_on_disk: true,
            }
        );
    }

    #[test]
    fn test_determine_tab_restore_strategy_uses_saved_content_without_conflict_when_not_newer() {
        let decision = determine_tab_restore_strategy(
            Some(PathBuf::from("/tmp/example.md")),
            None,
            Some("saved".to_string()),
            Some("2026-04-07T10:00:00Z".to_string()),
            true,
            Some("2026-04-07T10:00:00Z".to_string()),
            true,
        );
        assert_eq!(
            decision,
            TabRestoreDecision::UseSavedContentWithPath {
                path: PathBuf::from("/tmp/example.md"),
                content: "saved".to_string(),
                changed_on_disk: false,
            }
        );
    }

    #[test]
    fn test_determine_tab_restore_strategy_uses_saved_content_when_newer_but_unreadable() {
        let decision = determine_tab_restore_strategy(
            Some(PathBuf::from("/tmp/example.md")),
            None,
            Some("saved".to_string()),
            Some("2026-04-07T09:00:00Z".to_string()),
            true,
            Some("2026-04-07T10:00:00Z".to_string()),
            false,
        );
        assert_eq!(
            decision,
            TabRestoreDecision::UseSavedContentWithPath {
                path: PathBuf::from("/tmp/example.md"),
                content: "saved".to_string(),
                changed_on_disk: false,
            }
        );
    }

    #[test]
    fn test_determine_tab_restore_strategy_skips_path_only_tab_when_unreadable() {
        let decision = determine_tab_restore_strategy(
            Some(PathBuf::from("/tmp/example.md")),
            None,
            None,
            None,
            true,
            None,
            false,
        );
        assert_eq!(decision, TabRestoreDecision::Skip);
    }

    #[test]
    fn test_determine_tab_restore_strategy_loads_path_only_tab_when_readable() {
        let decision = determine_tab_restore_strategy(
            Some(PathBuf::from("/tmp/example.md")),
            None,
            None,
            None,
            true,
            None,
            true,
        );
        assert_eq!(
            decision,
            TabRestoreDecision::LoadFromFile {
                path: PathBuf::from("/tmp/example.md")
            }
        );
    }

    #[test]
    fn test_determine_tab_restore_strategy_uses_saved_content_without_path_when_missing_file() {
        let decision = determine_tab_restore_strategy(
            Some(PathBuf::from("/tmp/example.md")),
            None,
            Some("saved".to_string()),
            Some("2026-04-07T09:00:00Z".to_string()),
            false,
            None,
            false,
        );
        assert_eq!(
            decision,
            TabRestoreDecision::UseSavedContentNoPath {
                content: "saved".to_string(),
            }
        );
    }

    #[test]
    fn test_determine_tab_restore_strategy_restores_remote_tabs() {
        let remote = SerializedRemoteSpec {
            host: "example.com".to_string(),
            port: 22,
            user: "alice".to_string(),
            path: "/tmp/test.txt".to_string(),
        };

        let decision = determine_tab_restore_strategy(
            None,
            Some(remote.clone()),
            Some("cached".to_string()),
            None,
            false,
            None,
            false,
        );

        assert_eq!(
            decision,
            TabRestoreDecision::RestoreRemote {
                remote,
                content: Some("cached".to_string()),
            }
        );
    }
}
