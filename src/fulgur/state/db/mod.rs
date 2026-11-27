//! `SQLite`-backed store for the window/tab session state.

mod device_key_pins;
mod legacy;
mod paths;
mod read;
mod remote_history;
mod schema;
mod session_lock;
#[cfg(test)]
mod tests;
mod write;

pub use legacy::{LEGACY_STATE_FILE_NAME, import_legacy_json};

use crate::fulgur::utils::portable::{self, PortableLayout};
use anyhow::anyhow;
use rusqlite::Connection;
use session_lock::SessionLockOutcome;
use std::collections::HashSet;
use std::fs::File;
use std::path::{Path, PathBuf};

/// File name of the session state database inside the config directory.
pub const STATE_DB_FILE_NAME: &str = "state.db";

/// Counts of the row operations one `apply` performed.
///
/// Exposed so tests can assert that an unchanged snapshot writes nothing and
/// that a reorder does not rewrite buffer text.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ApplyStats {
    /// Window rows inserted or updated.
    pub windows_written: usize,
    /// Window rows deleted because the window closed.
    pub windows_deleted: usize,
    /// Tab rows inserted.
    pub tabs_inserted: usize,
    /// Tab rows updated without rewriting their buffer text.
    pub tabs_metadata_updated: usize,
    /// Tab rows whose buffer text was written.
    pub tabs_content_written: usize,
    /// Tab rows deleted because the tab closed.
    pub tabs_deleted: usize,
}

impl ApplyStats {
    /// Whether the apply left the database untouched.
    ///
    /// ### Returns
    /// - `bool`: `true` when no row was inserted, updated or deleted
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Handle on the session state database.
pub struct StateDb {
    conn: Connection,
    /// `None` for the in-memory fallback database.
    path: Option<PathBuf>,
    /// Why the normal file-backed database could not be used. This is `None`
    /// for file-backed databases and for explicitly-created in-memory databases
    /// used by tests.
    fallback_error: Option<String>,
    /// Window ids this handle has written, used to scope deletions so a second
    /// process cannot erase windows it does not own.
    owned_windows: HashSet<i64>,
    /// Portable layout tab file paths are persisted relative to, `None` outside
    /// portable mode. Captured at open time so tests can inject their own.
    path_layout: Option<PortableLayout>,
    /// Exclusive lock making this process the owner of the database file, held
    /// until the handle is dropped. `None` when the handle was opened without
    /// `open_or_fallback`, or when locking is not supported.
    #[allow(dead_code, reason = "held only so its drop releases the session lock")]
    session_lock: Option<File>,
}

impl StateDb {
    /// Open (creating if needed) the state database at `path`.
    ///
    /// ### Arguments
    /// - `path`: Path to the database file
    ///
    /// ### Errors
    /// - Returns an error if the file cannot be opened, if the pragmas cannot be
    ///   applied, or if the schema cannot be migrated.
    ///
    /// ### Returns
    /// - `Ok(StateDb)`: An open, migrated database
    /// - `Err(anyhow::Error)`: The database could not be opened or migrated
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let existed = path.exists();
        let mut conn = Connection::open(path)
            .map_err(|e| anyhow!("Failed to open state database '{}': {e}", path.display()))?;
        schema::apply_pragmas(&conn)?;
        if existed && schema::schema_version(&conn)? < schema::SCHEMA_VERSION {
            back_up_before_migration(&conn, path);
        }
        schema::migrate(&mut conn)?;
        Ok(Self {
            conn,
            path: Some(path.to_path_buf()),
            fallback_error: None,
            owned_windows: HashSet::new(),
            path_layout: portable::layout().cloned(),
            session_lock: None,
        })
    }

    /// Open a private in-memory database.
    ///
    /// ### Errors
    /// - Returns an error if `SQLite` refuses to open an in-memory database or if
    ///   the schema cannot be created.
    ///
    /// ### Returns
    /// - `Ok(StateDb)`: An open, migrated ephemeral database
    /// - `Err(anyhow::Error)`: The database could not be prepared
    pub fn open_in_memory() -> anyhow::Result<Self> {
        let mut conn = Connection::open_in_memory()
            .map_err(|e| anyhow!("Failed to open in-memory state database: {e}"))?;
        schema::apply_pragmas(&conn)?;
        schema::migrate(&mut conn)?;
        Ok(Self {
            conn,
            path: None,
            fallback_error: None,
            owned_windows: HashSet::new(),
            path_layout: portable::layout().cloned(),
            session_lock: None,
        })
    }

    /// Open the database at `path` as its owner, degrading to an ephemeral one on failure.
    ///
    /// The process first takes the database's exclusive session lock. When
    /// another process holds it, this one gets an ephemeral database instead,
    /// so the two never restore, claim or overwrite the same rows.
    ///
    /// ### Arguments
    /// - `path`: Path to the database file
    ///
    /// ### Returns
    /// - `Some(StateDb)`: The file database, or an ephemeral one if it is owned
    ///   by another process or could not be opened
    /// - `None`: Even an in-memory database could not be prepared
    #[must_use]
    pub fn open_or_fallback(path: &Path) -> Option<Self> {
        let ownership_lock = match session_lock::try_acquire(path) {
            SessionLockOutcome::Acquired(lock) => Some(lock),
            SessionLockOutcome::HeldElsewhere => {
                log::warn!(
                    "Another Fulgur process owns the session database, using in-memory session state"
                );
                return Self::in_memory_fallback(format!(
                    "another Fulgur process is already using session database '{}'",
                    path.display()
                ));
            }
            SessionLockOutcome::Failed(e) => {
                log::warn!(
                    "Could not lock the session database, opening it without exclusive ownership: {e}"
                );
                None
            }
        };
        match Self::open(path) {
            Ok(mut db) => {
                db.session_lock = ownership_lock;
                Some(db)
            }
            Err(file_err) => {
                log::error!(
                    "Falling back to in-memory session state, it will not be saved: {file_err}"
                );
                Self::in_memory_fallback(format!(
                    "could not use session database '{}': {file_err}",
                    path.display()
                ))
            }
        }
    }

    /// Whether another process currently owns the database at `path`.
    ///
    /// ### Arguments
    /// - `path`: Path to the database file
    ///
    /// ### Returns
    /// - `true`: Another process holds the database's session lock
    /// - `false`: Nobody holds it, or the lock could not be checked
    #[must_use]
    pub fn is_owned_by_another_process(path: &Path) -> bool {
        matches!(
            session_lock::try_acquire(path),
            SessionLockOutcome::HeldElsewhere
        )
    }

    /// Open the in-memory database replacing an unusable file database.
    ///
    /// ### Arguments
    /// - `reason`: Why the file database cannot be used, reported to the user
    ///
    /// ### Returns
    /// - `Some(StateDb)`: An ephemeral database carrying `reason` as its fallback error
    /// - `None`: The in-memory database could not be prepared
    fn in_memory_fallback(reason: String) -> Option<Self> {
        match Self::open_in_memory() {
            Ok(mut db) => {
                db.fallback_error = Some(reason);
                Some(db)
            }
            Err(memory_err) => {
                log::error!("Failed to open fallback state database: {memory_err}");
                None
            }
        }
    }

    /// Whether this database only exists in memory and will not be persisted.
    ///
    /// ### Returns
    /// - `bool`: `true` when the handle is the in-memory fallback
    #[must_use]
    pub fn is_ephemeral(&self) -> bool {
        self.path.is_none()
    }

    /// Explain why this database is a non-durable production fallback.
    ///
    /// ### Returns
    /// - `Some(&str)`: The file-backed database failure that caused fallback
    /// - `None`: The database is durable or was explicitly opened in memory
    #[must_use]
    pub(crate) fn fallback_error(&self) -> Option<&str> {
        self.fallback_error.as_deref()
    }

    /// Drop every ownership claim, as if the handle had just been opened.
    ///
    /// Lets a test act as a second process, or as a fresh launch, over rows that
    /// are already present.
    #[cfg(test)]
    fn forget_owned_windows(&mut self) {
        self.owned_windows.clear();
    }

    /// Persist tab file paths relative to a portable layout, as a portable process does.
    ///
    /// ### Arguments
    /// - `layout`: The portable layout to use
    #[cfg(test)]
    fn set_path_layout(&mut self, layout: PortableLayout) {
        self.path_layout = Some(layout);
    }

    /// Flush the write-ahead log into the database file.
    pub fn checkpoint(&self) {
        if self.is_ephemeral() {
            return;
        }
        if let Err(e) = self.conn.pragma_update(None, "wal_checkpoint", "TRUNCATE") {
            log::warn!("Failed to checkpoint the state database WAL: {e}");
        }
    }
}

/// Copy the database aside before its schema is migrated.
///
/// ### Arguments
/// - `conn`: Connection to the database about to be migrated
/// - `path`: Path of that database, used to derive the backup path
fn back_up_before_migration(conn: &Connection, path: &Path) {
    let backup = crate::fulgur::utils::atomic_write::backup_path_for(path);
    if backup.exists()
        && let Err(e) = std::fs::remove_file(&backup)
    {
        log::warn!(
            "Failed to remove the previous state database backup '{}': {}",
            backup.display(),
            e
        );
        return;
    }
    match conn.execute("VACUUM INTO ?1", [&backup.to_string_lossy()]) {
        Ok(_) => log::info!(
            "Backed up the state database to '{}' before migrating",
            backup.display()
        ),
        Err(e) => log::warn!(
            "Failed to back up the state database to '{}': {}",
            backup.display(),
            e
        ),
    }
}
