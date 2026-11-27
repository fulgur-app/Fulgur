//! Exclusive advisory lock making one process the owner of a session database.
//!
//! Two Fulgur processes sharing `state.db` would restore the same windows, claim
//! the same rows and overwrite each other's snapshots. The process holding the
//! lock owns the database; any other one runs with an in-memory session.

use std::{
    fs::{File, OpenOptions, TryLockError},
    io,
    path::{Path, PathBuf},
};

/// Suffix appended to the database file name to build its lock file name.
const LOCK_FILE_SUFFIX: &str = ".lock";

/// Outcome of trying to become the owner of a session database.
#[derive(Debug)]
pub(super) enum SessionLockOutcome {
    /// This process owns the database for as long as the file stays open.
    Acquired(File),
    /// Another process owns the database.
    HeldElsewhere,
    /// The lock could not be taken for another reason, so ownership is not enforced.
    Failed(io::Error),
}

/// Build the path of the lock file guarding a session database.
///
/// ### Arguments
/// - `db_path`: Path of the session database
///
/// ### Returns
/// - `PathBuf`: The sibling lock file path, e.g. `state.db.lock`
pub(super) fn lock_path_for(db_path: &Path) -> PathBuf {
    let mut file_name = db_path.file_name().unwrap_or_default().to_os_string();
    file_name.push(LOCK_FILE_SUFFIX);
    db_path.with_file_name(file_name)
}

/// Try to take the exclusive lock of a session database without blocking.
///
/// ### Arguments
/// - `db_path`: Path of the session database
///
/// ### Returns
/// - `SessionLockOutcome::Acquired`: The lock is held until the returned file is dropped
/// - `SessionLockOutcome::HeldElsewhere`: Another process holds the lock
/// - `SessionLockOutcome::Failed`: The lock file could not be opened or locked
pub(super) fn try_acquire(db_path: &Path) -> SessionLockOutcome {
    let lock_file = match OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(lock_path_for(db_path))
    {
        Ok(file) => file,
        Err(e) => return SessionLockOutcome::Failed(e),
    };
    match lock_file.try_lock() {
        Ok(()) => SessionLockOutcome::Acquired(lock_file),
        Err(TryLockError::WouldBlock) => SessionLockOutcome::HeldElsewhere,
        Err(TryLockError::Error(e)) => SessionLockOutcome::Failed(e),
    }
}

#[cfg(test)]
mod tests {
    use super::{SessionLockOutcome, lock_path_for, try_acquire};
    use tempfile::TempDir;

    #[test]
    fn lock_file_sits_next_to_the_database() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("state.db");

        assert_eq!(lock_path_for(&db_path), dir.path().join("state.db.lock"));
    }

    #[test]
    fn a_held_lock_refuses_a_second_owner_until_released() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("state.db");

        let first = try_acquire(&db_path);
        assert!(
            matches!(first, SessionLockOutcome::Acquired(_)),
            "{first:?}"
        );
        let second = try_acquire(&db_path);
        assert!(
            matches!(second, SessionLockOutcome::HeldElsewhere),
            "{second:?}"
        );

        drop(first);
        let third = try_acquire(&db_path);
        assert!(
            matches!(third, SessionLockOutcome::Acquired(_)),
            "{third:?}"
        );
    }
}
