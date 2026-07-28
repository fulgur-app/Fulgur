//! `SQLite`-backed store for the window/tab session state.

mod legacy;
mod paths;
mod read;
mod schema;
#[cfg(test)]
mod tests;
mod write;

pub use legacy::{LEGACY_STATE_FILE_NAME, import_legacy_json};

use anyhow::anyhow;
use rusqlite::Connection;
use std::collections::HashSet;
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
    /// Window ids this handle has written, used to scope deletions so a second
    /// process cannot erase windows it does not own.
    owned_windows: HashSet<i64>,
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
            owned_windows: HashSet::new(),
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
            owned_windows: HashSet::new(),
        })
    }

    /// Open the database at `path`, degrading to an ephemeral one on failure.
    ///
    /// ### Arguments
    /// - `path`: Path to the database file
    ///
    /// ### Returns
    /// - `Some(StateDb)`: The file database, or an ephemeral one if it failed
    /// - `None`: Even an in-memory database could not be prepared
    #[must_use]
    pub fn open_or_fallback(path: &Path) -> Option<Self> {
        match Self::open(path) {
            Ok(db) => Some(db),
            Err(file_err) => {
                log::error!(
                    "Falling back to in-memory session state, it will not be saved: {file_err}"
                );
                match Self::open_in_memory() {
                    Ok(db) => Some(db),
                    Err(memory_err) => {
                        log::error!("Failed to open fallback state database: {memory_err}");
                        None
                    }
                }
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

    /// Drop every ownership claim, as if the handle had just been opened.
    ///
    /// Lets a test act as a second process, or as a fresh launch, over rows that
    /// are already present.
    #[cfg(test)]
    fn forget_owned_windows(&mut self) {
        self.owned_windows.clear();
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
