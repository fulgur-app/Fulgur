//! Connection pragmas and versioned schema migrations for the state database.

use anyhow::anyhow;
use rusqlite::{Connection, TransactionBehavior};

/// Schema version this build expects. Bumping it requires appending a step to
/// `MIGRATIONS`; the existing steps must never be edited.
pub const SCHEMA_VERSION: i64 = 1;

/// Ordered schema migrations. Index `n` upgrades `user_version` from `n` to
/// `n + 1`, so a fresh database runs every step in order.
const MIGRATIONS: &[&str] = &[include_str!("migrations/001_initial.sql")];

/// How long a connection waits for a lock held by another connection.
const BUSY_TIMEOUT_MS: i64 = 5_000;

/// Apply the connection-scoped pragmas the state store relies on.
///
/// ### Description
/// `busy_timeout` is set first, before any statement that takes a lock. Switching
/// a database to WAL needs a brief exclusive lock, and without a busy handler
/// already installed `SQLITE_BUSY` is returned immediately instead of being
/// waited out, which happens whenever two processes open a not-yet-WAL database
/// at the same time.
///
/// ### Arguments
/// - `conn`: The connection to configure
///
/// ### Errors
/// - Returns an error if any pragma other than `journal_mode` cannot be applied.
///   Failing to reach WAL is only logged, because the store stays correct in the
///   rollback-journal mode the database already has, and giving up here would
///   cost the whole session its persistence.
///
/// ### Returns
/// - `Ok(())`: The connection is configured
/// - `Err(anyhow::Error)`: A pragma could not be applied
pub fn apply_pragmas(conn: &Connection) -> anyhow::Result<()> {
    conn.pragma_update(None, "busy_timeout", BUSY_TIMEOUT_MS)
        .map_err(|e| anyhow!("Failed to set busy timeout: {e}"))?;
    // `journal_mode` returns the resulting mode as a row, so it needs a query.
    match conn.query_row("PRAGMA journal_mode = WAL", [], |row| {
        row.get::<_, String>(0)
    }) {
        Ok(mode) if mode.eq_ignore_ascii_case("wal") => {}
        Ok(mode) => log::warn!("State database journal mode is '{mode}' instead of WAL"),
        Err(e) => log::warn!("Failed to enable WAL journal mode, keeping the current one: {e}"),
    }
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(|e| anyhow!("Failed to set synchronous mode: {e}"))?;
    conn.pragma_update(None, "foreign_keys", true)
        .map_err(|e| anyhow!("Failed to enable foreign key enforcement: {e}"))?;
    // Checkpoint every ~4 MB of WAL so the sidecar cannot outgrow the database
    // itself in a long-running process.
    conn.pragma_update(None, "wal_autocheckpoint", 1000)
        .map_err(|e| anyhow!("Failed to set WAL autocheckpoint: {e}"))?;
    Ok(())
}

/// Read the schema version recorded in the database.
///
/// ### Arguments
/// - `conn`: The connection to query
///
/// ### Errors
/// - Returns an error if `PRAGMA user_version` cannot be read.
///
/// ### Returns
/// - `Ok(i64)`: The recorded version, `0` for a database that has never been migrated
/// - `Err(anyhow::Error)`: The version could not be read
pub fn schema_version(conn: &Connection) -> anyhow::Result<i64> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| anyhow!("Failed to read schema version: {e}"))
}

/// Bring the database up to `SCHEMA_VERSION`, running every pending step in one
/// transaction.
///
/// ### Arguments
/// - `conn`: The connection to migrate
///
/// ### Errors
/// - Returns an error if the write lock cannot be taken, if the version cannot be
///   read, if the database was written by a newer build, or if a migration step fails.
///
/// ### Returns
/// - `Ok(bool)`: `true` when at least one migration step ran
/// - `Err(anyhow::Error)`: The database could not be migrated
pub fn migrate(conn: &mut Connection) -> anyhow::Result<bool> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| anyhow!("Failed to lock the state database for migration: {e}"))?;
    let current = schema_version(&tx)?;
    if current > SCHEMA_VERSION {
        return Err(anyhow!(
            "State database schema version {current} is newer than the supported version {SCHEMA_VERSION}"
        ));
    }
    if current == SCHEMA_VERSION {
        return Ok(false);
    }
    let pending = usize::try_from(current)
        .map_err(|e| anyhow!("Invalid schema version {current} in state database: {e}"))?;
    for (offset, migration) in MIGRATIONS.iter().enumerate().skip(pending) {
        let version = i64::try_from(offset + 1)
            .map_err(|e| anyhow!("Migration index {offset} does not fit a schema version: {e}"))?;
        log::info!("Migrating state database to schema version {version}");
        tx.execute_batch(migration).map_err(|e| {
            anyhow!("Failed to migrate state database to schema version {version}: {e}")
        })?;
        tx.pragma_update(None, "user_version", version)
            .map_err(|e| anyhow!("Failed to record schema version {version}: {e}"))?;
    }
    tx.commit()
        .map_err(|e| anyhow!("Failed to commit the state database migration: {e}"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{SCHEMA_VERSION, apply_pragmas, migrate, schema_version};
    use rusqlite::Connection;

    fn migrated_connection() -> Connection {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        apply_pragmas(&conn).expect("apply pragmas");
        migrate(&mut conn).expect("migrate");
        conn
    }

    #[test]
    fn migration_count_matches_the_declared_schema_version() {
        assert_eq!(
            super::MIGRATIONS.len(),
            usize::try_from(SCHEMA_VERSION).expect("schema version fits usize"),
            "every schema version must have exactly one migration step"
        );
    }

    #[test]
    fn fresh_database_is_migrated_to_the_current_version() {
        let conn = migrated_connection();
        assert_eq!(schema_version(&conn).expect("read version"), SCHEMA_VERSION);
    }

    #[test]
    fn migrating_an_up_to_date_database_is_a_no_op() {
        let mut conn = migrated_connection();
        assert!(!migrate(&mut conn).expect("second migrate"));
    }

    #[test]
    fn expected_tables_exist_after_migration() {
        let conn = migrated_connection();
        for table in ["windows", "tabs"] {
            let count: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .expect("query sqlite_master");
            assert_eq!(count, 1, "table {table} must exist");
        }
    }

    #[test]
    fn tables_are_declared_strict() {
        let conn = migrated_connection();
        // A STRICT table rejects a value whose type does not match the column.
        conn.execute_batch(
            "INSERT INTO windows (id, position, bounds_state, bounds_x, bounds_y, bounds_width, bounds_height)
             VALUES (1, 0, 'Windowed', 0.0, 0.0, 100.0, 100.0);",
        )
        .expect("insert window");
        let result = conn.execute(
            "INSERT INTO tabs (window_id, id, position, title, log_view) VALUES (1, 1, 'not-an-integer', 'x', 0)",
            [],
        );
        assert!(
            result.is_err(),
            "a STRICT table must reject a text value in an INTEGER column"
        );
    }

    #[test]
    fn deleting_a_window_cascades_to_its_tabs() {
        let conn = migrated_connection();
        conn.execute_batch(
            "INSERT INTO windows (id, position, bounds_state, bounds_x, bounds_y, bounds_width, bounds_height)
             VALUES (1, 0, 'Windowed', 0.0, 0.0, 100.0, 100.0);
             INSERT INTO tabs (window_id, id, position, title, log_view) VALUES (1, 0, 0, 'a.txt', 0);",
        )
        .expect("seed window and tab");
        conn.execute("DELETE FROM windows WHERE id = 1", [])
            .expect("delete window");
        let tabs: i64 = conn
            .query_row("SELECT count(*) FROM tabs", [], |row| row.get(0))
            .expect("count tabs");
        assert_eq!(tabs, 0, "tabs must be removed with their window");
    }

    #[test]
    fn a_newer_schema_version_is_refused_instead_of_downgraded() {
        let mut conn = Connection::open_in_memory().expect("open in-memory database");
        apply_pragmas(&conn).expect("apply pragmas");
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .expect("set future version");
        assert!(migrate(&mut conn).is_err());
    }
}
