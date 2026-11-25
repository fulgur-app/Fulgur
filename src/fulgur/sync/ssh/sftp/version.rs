use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Size and modification time identifying one version of a remote file.
///
/// SFTP v3 reports whole seconds, so two writes of the same size within one second
/// cannot be told apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemoteFileVersion {
    /// File size in bytes, when the server reports it.
    pub size: Option<u64>,
    /// Modification time in seconds since the Unix epoch, when the server reports it.
    pub mtime: Option<u64>,
}

impl RemoteFileVersion {
    /// Read the version described by SFTP file attributes.
    ///
    /// ### Arguments
    /// - `stat`: Attributes returned by the server.
    ///
    /// ### Returns
    /// - `RemoteFileVersion`: The reported size and modification time.
    #[must_use]
    pub fn from_stat(stat: &ssh2::FileStat) -> Self {
        Self {
            size: stat.size,
            mtime: stat.mtime,
        }
    }

    /// Describe the version a save has just written.
    ///
    /// ### Arguments
    /// - `stat`: Attributes of the destination after the write, or `None` when unreadable.
    /// - `written_len`: Number of bytes the save wrote.
    ///
    /// ### Returns
    /// - `RemoteFileVersion`: The written size, and its modification time when known.
    #[must_use]
    pub fn after_write(stat: Option<&ssh2::FileStat>, written_len: u64) -> Self {
        Self {
            size: Some(written_len),
            mtime: stat
                .filter(|stat| stat.size.is_none_or(|size| size == written_len))
                .and_then(|stat| stat.mtime),
        }
    }

    /// Convert the modification time to a `SystemTime`.
    ///
    /// ### Returns
    /// - `Some(SystemTime)`: The server's modification time.
    /// - `None`: The server did not report it, or it cannot be represented.
    #[must_use]
    pub fn modified(&self) -> Option<SystemTime> {
        SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(self.mtime?))
    }

    /// Check whether another version is known to be a different one.
    ///
    /// ### Arguments
    /// - `other`: The version to compare with.
    ///
    /// ### Returns
    /// - `true`: An attribute known on both sides differs.
    /// - `false`: Every attribute known on both sides matches.
    #[must_use]
    pub fn differs_from(&self, other: &Self) -> bool {
        let differ = |a: Option<u64>, b: Option<u64>| a.zip(b).is_some_and(|(a, b)| a != b);
        differ(self.size, other.size) || differ(self.mtime, other.mtime)
    }
}

/// The remote version a tab's buffer is based on, shared with its save workers.
///
/// Clones share the same value. A worker reads it when its turn to write comes and
/// advances it once it has written, so a save queued behind another one, or following a
/// save whose result was discarded, does not take Fulgur's own write for a conflict.
#[derive(Clone, Debug, Default)]
pub struct RemoteBaseline(Arc<Mutex<Option<RemoteFileVersion>>>);

impl RemoteBaseline {
    /// Create a baseline.
    ///
    /// ### Arguments
    /// - `version`: The version the buffer is based on, or `None` when unknown.
    ///
    /// ### Returns
    /// - `RemoteBaseline`: A baseline not shared with any other tab.
    #[must_use]
    pub fn new(version: Option<RemoteFileVersion>) -> Self {
        Self(Arc::new(Mutex::new(version)))
    }

    /// Read the current baseline.
    ///
    /// ### Returns
    /// - `Some(RemoteFileVersion)`: The version the buffer is based on.
    /// - `None`: That version is unknown.
    #[must_use]
    pub fn get(&self) -> Option<RemoteFileVersion> {
        *self.0.lock()
    }

    /// Replace the baseline, for every clone.
    ///
    /// ### Arguments
    /// - `version`: The version the buffer is now based on, or `None` when unknown.
    pub fn set(&self, version: Option<RemoteFileVersion>) {
        *self.0.lock() = version;
    }
}

/// Why a remote save was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteConflict {
    /// The file changed on the server since the version the buffer is based on.
    Changed,
    /// The file exists, but the version the buffer is based on is unknown.
    UnknownBaseline,
}

/// What a remote write requires of the file it is about to replace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteWriteGuard {
    /// The file must be missing or still be this version; `None` when it is unknown.
    Unchanged(Option<RemoteFileVersion>),
    /// Nothing: replace whatever version the server holds.
    Overwrite,
}

impl RemoteWriteGuard {
    /// Check the version the server currently holds against this guard.
    ///
    /// ### Arguments
    /// - `current`: The version of the existing destination.
    ///
    /// ### Returns
    /// - `Some(RemoteConflict)`: The destination must not be replaced.
    /// - `None`: The write may go ahead.
    #[must_use]
    pub fn conflict_with(self, current: &RemoteFileVersion) -> Option<RemoteConflict> {
        match self {
            Self::Overwrite => None,
            Self::Unchanged(None) => Some(RemoteConflict::UnknownBaseline),
            Self::Unchanged(Some(baseline)) => baseline
                .differs_from(current)
                .then_some(RemoteConflict::Changed),
        }
    }
}

/// Result of a remote write that did not fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteWriteOutcome {
    /// The file was written; holds the version it now is.
    Written(RemoteFileVersion),
    /// Nothing was written.
    Refused(RemoteConflict),
}

#[cfg(test)]
mod tests {
    use super::{RemoteBaseline, RemoteConflict, RemoteFileVersion, RemoteWriteGuard};
    use std::time::{Duration, SystemTime};

    /// Build SFTP attributes holding only a size and a modification time.
    ///
    /// ### Arguments
    /// - `size`: Reported size, if any.
    /// - `mtime`: Reported modification time, if any.
    ///
    /// ### Returns
    /// - `ssh2::FileStat`: The attributes.
    fn stat(size: Option<u64>, mtime: Option<u64>) -> ssh2::FileStat {
        ssh2::FileStat {
            size,
            uid: None,
            gid: None,
            perm: None,
            atime: None,
            mtime,
        }
    }

    /// Build a version from its size and modification time.
    ///
    /// ### Arguments
    /// - `size`: Known size, if any.
    /// - `mtime`: Known modification time, if any.
    ///
    /// ### Returns
    /// - `RemoteFileVersion`: The version.
    fn version(size: Option<u64>, mtime: Option<u64>) -> RemoteFileVersion {
        RemoteFileVersion { size, mtime }
    }

    #[test]
    fn versions_differ_only_on_attributes_known_on_both_sides() {
        let base = version(Some(10), Some(1_000));
        let cases = [
            (version(Some(10), Some(1_000)), false),
            (version(Some(11), Some(1_000)), true),
            (version(Some(10), Some(1_001)), true),
            // A file restored from a backup goes back in time.
            (version(Some(10), Some(999)), true),
            (version(None, Some(1_000)), false),
            (version(None, Some(1_001)), true),
            (version(Some(10), None), false),
            (version(None, None), false),
        ];
        for (other, expected) in cases {
            assert_eq!(base.differs_from(&other), expected, "{other:?}");
            assert_eq!(other.differs_from(&base), expected, "{other:?}");
        }
    }

    #[test]
    fn guard_refuses_changed_and_unknown_versions_unless_overwriting() {
        let loaded = version(Some(10), Some(1_000));
        let changed = version(Some(10), Some(2_000));

        let known = RemoteWriteGuard::Unchanged(Some(loaded));
        assert_eq!(known.conflict_with(&loaded), None);
        assert_eq!(known.conflict_with(&changed), Some(RemoteConflict::Changed));
        assert_eq!(
            RemoteWriteGuard::Unchanged(None).conflict_with(&loaded),
            Some(RemoteConflict::UnknownBaseline)
        );
        assert_eq!(RemoteWriteGuard::Overwrite.conflict_with(&changed), None);
    }

    #[test]
    fn written_version_adopts_the_server_time_of_the_written_file() {
        let written = RemoteFileVersion::after_write(Some(&stat(Some(7), Some(3_000))), 7);
        assert_eq!(written, version(Some(7), Some(3_000)));
    }

    #[test]
    fn written_version_does_not_adopt_a_file_replaced_right_after_the_write() {
        let replaced = stat(Some(9), Some(3_000));
        let written = RemoteFileVersion::after_write(Some(&replaced), 7);

        assert_eq!(
            RemoteWriteGuard::Unchanged(Some(written))
                .conflict_with(&RemoteFileVersion::from_stat(&replaced)),
            Some(RemoteConflict::Changed)
        );
    }

    #[test]
    fn written_version_keeps_the_size_when_the_file_cannot_be_inspected() {
        assert_eq!(
            RemoteFileVersion::after_write(None, 7),
            version(Some(7), None)
        );
    }

    #[test]
    fn modified_converts_the_server_time() {
        assert_eq!(
            version(None, Some(1_000)).modified(),
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000))
        );
        assert_eq!(version(Some(1), None).modified(), None);
    }

    #[test]
    fn baseline_clones_share_the_version_a_worker_wrote() {
        let tab_baseline = RemoteBaseline::new(Some(version(Some(1), Some(1))));
        let worker_baseline = tab_baseline.clone();

        worker_baseline.set(Some(version(Some(2), Some(2))));

        assert_eq!(tab_baseline.get(), Some(version(Some(2), Some(2))));
    }
}
