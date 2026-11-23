use super::FileWatcher;
use std::fs::Metadata;
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
        Self::from_metadata(&std::fs::metadata(path).ok()?)
    }

    /// Build the signature described by already-read file metadata
    ///
    /// ### Arguments
    /// - `metadata`: The metadata of the file
    ///
    /// ### Returns
    /// - `Some(DiskSignature)`: The modification time and size held by `metadata`
    /// - `None`: The platform reports no modification time
    pub(super) fn from_metadata(metadata: &Metadata) -> Option<Self> {
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
    pub(crate) fn track_without_os_watch(&mut self, path: &Path) {
        self.watched_paths
            .insert(path.to_path_buf(), DiskSignature::read(path));
    }

    /// Record the disk state of a watched file that the editor's buffer matches,
    /// such as the file the editor just wrote or the version it read
    ///
    /// ### Arguments
    /// - `path`: The watched file
    /// - `known`: The metadata of the version the editor's buffer matches
    ///
    /// ### Returns
    /// - `true`: The path is watched and the file on disk already differs from `known`
    /// - `false`: The file still matches `known`, the path is not watched, or the
    ///   platform reports no modification time (the current state is then adopted)
    pub fn record_known_state(&mut self, path: &Path, known: &Metadata) -> bool {
        let Some(tracked) = self.watched_paths.get_mut(path) else {
            return false;
        };
        let current = DiskSignature::read(path);
        let Some(known) = DiskSignature::from_metadata(known) else {
            *tracked = current;
            return false;
        };
        *tracked = Some(known);
        current != Some(known)
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
    use crate::fulgur::utils::atomic_write::atomic_write_file;
    use crate::test_support::write_with_mtime;
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
    fn test_record_known_state_absorbs_own_write(_cx: &mut TestAppContext) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("refresh_signature.txt");
        std::fs::write(&path, "before save").expect("failed to write test file");
        let (mut watcher, _event_rx) = FileWatcher::new();
        watcher.track_without_os_watch(&path);
        let written = atomic_write_file(&path, b"after the editor saved")
            .expect("failed to rewrite test file");
        assert!(
            !watcher.record_known_state(&path, &written),
            "the file still matches the editor's own write"
        );
        assert!(
            !watcher.acknowledge_disk_change(&path),
            "a write recorded through record_known_state must not count as a change"
        );
    }

    #[gpui_kit::test]
    fn test_record_known_state_reports_a_write_that_landed_after_it(_cx: &mut TestAppContext) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("raced_signature.txt");
        std::fs::write(&path, "before save").expect("failed to write test file");
        let (mut watcher, _event_rx) = FileWatcher::new();
        watcher.track_without_os_watch(&path);
        let written = atomic_write_file(&path, b"editor").expect("failed to rewrite test file");
        write_with_mtime(&path, "external, longer", 0);

        assert!(
            watcher.record_known_state(&path, &written),
            "a write landing after the editor's must not be adopted as the editor's own"
        );
        assert!(
            watcher.acknowledge_disk_change(&path),
            "the external write must still be reported as a change"
        );
    }

    #[gpui_kit::test]
    fn test_record_known_state_ignores_unwatched_path(_cx: &mut TestAppContext) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("unwatched_known_state.txt");
        let written = atomic_write_file(&path, b"content").expect("failed to write test file");
        write_with_mtime(&path, "external", 0);
        let (mut watcher, _event_rx) = FileWatcher::new();
        assert!(!watcher.record_known_state(&path, &written));
    }
}
