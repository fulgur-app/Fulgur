use super::FileWatcher;
use std::path::Path;
use std::time::SystemTime;

/// Size and modification time of a file on disk, used to tell real content
/// changes apart from metadata-only or self-inflicted watch events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DiskSignature {
    modified: SystemTime,
    len: u64,
}

impl DiskSignature {
    /// Read the current signature of a file
    ///
    /// ### Arguments
    /// - `path`: The file to inspect
    ///
    /// ### Returns
    /// - `Some(DiskSignature)`: The file's current modification time and size
    /// - `None`: The file is missing or its metadata cannot be read
    pub(super) fn read(path: &Path) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        Some(Self {
            modified: metadata.modified().ok()?,
            len: metadata.len(),
        })
    }
}

impl FileWatcher {
    /// Track a file's disk signature without registering it with the OS watcher,
    /// so tests can exercise echo suppression without a live notify thread waking
    /// tasks on gpui's deterministic test scheduler.
    ///
    /// ### Arguments
    /// - `path`: The file whose current signature becomes the known state
    #[cfg(all(test, feature = "gpui-test-support"))]
    pub(in crate::fulgur::files::file_watcher) fn track_without_os_watch(&mut self, path: &Path) {
        self.watched_paths
            .insert(path.to_path_buf(), DiskSignature::read(path));
    }

    /// Record the current on-disk state of a watched file as known to the editor
    ///
    /// ### Arguments
    /// - `path`: The watched file whose signature should be refreshed
    pub fn refresh_disk_signature(&mut self, path: &Path) {
        if let Some(known) = self.watched_paths.get_mut(path) {
            *known = DiskSignature::read(path);
        }
    }

    /// Check whether a watched file changed on disk since its last known state
    ///
    /// ### Arguments
    /// - `path`: The file a watch event refers to
    ///
    /// ### Returns
    /// - `true`: The file changed, is missing, or is not watched
    /// - `false`: The file's size and modification time match the known state
    pub fn acknowledge_disk_change(&mut self, path: &Path) -> bool {
        let current = DiskSignature::read(path);
        let Some(known) = self.watched_paths.get_mut(path) else {
            return true;
        };
        if current.is_some() && *known == current {
            return false;
        }
        *known = current;
        true
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::super::FileWatcher;
    use gpui_kit::TestAppContext;
    use tempfile::TempDir;

    #[gpui_kit::test]
    fn test_acknowledge_disk_change_reports_each_disk_state_once(_cx: &mut TestAppContext) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("acknowledge_signature.txt");
        std::fs::write(&path, "first").expect("failed to write test file");
        let (mut watcher, _event_rx) = FileWatcher::new();
        watcher
            .watch_file(&path)
            .expect("failed to watch test file");
        assert!(
            !watcher.acknowledge_disk_change(&path),
            "a file untouched since it was watched is not a change"
        );
        std::fs::write(&path, "second, longer").expect("failed to rewrite test file");
        assert!(
            watcher.acknowledge_disk_change(&path),
            "a rewritten file is a change"
        );
        assert!(
            !watcher.acknowledge_disk_change(&path),
            "an acknowledged change must not be reported again"
        );
    }

    #[gpui_kit::test]
    fn test_acknowledge_disk_change_treats_unwatched_path_as_changed(_cx: &mut TestAppContext) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("unwatched_signature.txt");
        std::fs::write(&path, "content").expect("failed to write test file");
        let (mut watcher, _event_rx) = FileWatcher::new();
        assert!(watcher.acknowledge_disk_change(&path));
    }

    #[gpui_kit::test]
    fn test_refresh_disk_signature_absorbs_own_write(_cx: &mut TestAppContext) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("refresh_signature.txt");
        std::fs::write(&path, "before save").expect("failed to write test file");
        let (mut watcher, _event_rx) = FileWatcher::new();
        watcher
            .watch_file(&path)
            .expect("failed to watch test file");
        std::fs::write(&path, "after the editor saved").expect("failed to rewrite test file");
        watcher.refresh_disk_signature(&path);
        assert!(
            !watcher.acknowledge_disk_change(&path),
            "a write recorded through refresh_disk_signature must not count as a change"
        );
    }
}
