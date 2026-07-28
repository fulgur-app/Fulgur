//! Applying an in-memory snapshot to the database as per-row changes.

use super::paths::path_to_bytes;
use super::{ApplyStats, StateDb};
use crate::fulgur::state::persistence::{TabState, WindowState, WindowsState};
use anyhow::anyhow;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::collections::{HashMap, HashSet};

/// The persisted identity of a buffer, used to decide whether to rewrite it.
#[derive(PartialEq, Eq, Clone, Copy)]
struct ContentFingerprint {
    hash: u64,
    len: usize,
}

/// The comparable part of a persisted tab row, excluding the buffer text.
#[derive(PartialEq, Eq)]
struct StoredTab {
    position: i64,
    title: String,
    file_path: Option<Vec<u8>>,
    last_saved: Option<String>,
    log_view: bool,
    color_tag: Option<String>,
    remote_host: Option<String>,
    remote_port: Option<u16>,
    remote_user: Option<String>,
    remote_path: Option<String>,
}

/// The comparable part of a persisted window row.
#[derive(PartialEq)]
struct StoredWindow {
    position: i64,
    active_tab_id: Option<i64>,
    bounds_state: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    display_id: Option<u32>,
}

impl StateDb {
    /// Persist a snapshot, writing only the rows that differ from the database.
    ///
    /// ### Arguments
    /// - `snapshot`: The snapshot to persist
    ///
    /// ### Errors
    /// - Returns an error if the transaction cannot be opened, if a statement
    ///   fails, or if the commit fails.
    ///
    /// ### Returns
    /// - `Ok(ApplyStats)`: Counts of the rows actually written
    /// - `Err(anyhow::Error)`: The snapshot could not be persisted
    pub fn apply(&mut self, snapshot: &WindowsState) -> anyhow::Result<ApplyStats> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| anyhow!("Failed to begin the state transaction: {e}"))?;
        let mut stats = ApplyStats::default();
        let mut present: HashSet<i64> = HashSet::with_capacity(snapshot.windows.len());
        for (position, window) in snapshot.windows.iter().enumerate() {
            present.insert(window.window_id);
            let position = i64::try_from(position)
                .map_err(|e| anyhow!("Window position does not fit an integer: {e}"))?;
            apply_window(&tx, window, position, &mut stats)?;
            apply_tabs(&tx, window, &mut stats)?;
        }
        for closed in self.owned_windows.difference(&present) {
            let deleted = tx
                .execute("DELETE FROM windows WHERE id = ?1", [closed])
                .map_err(|e| anyhow!("Failed to delete window {closed}: {e}"))?;
            stats.windows_deleted += deleted;
        }
        tx.commit()
            .map_err(|e| anyhow!("Failed to commit the state transaction: {e}"))?;
        self.owned_windows = present;
        Ok(stats)
    }

    /// Claim every window already persisted as owned by this handle.
    ///
    /// ### Errors
    /// - Returns an error if the window ids cannot be read.
    ///
    /// ### Returns
    /// - `Ok(())`: The persisted windows are now owned by this handle
    /// - `Err(anyhow::Error)`: The window ids could not be read
    pub fn claim_persisted_windows(&mut self) -> anyhow::Result<()> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM windows")
            .map_err(|e| anyhow!("Failed to prepare the window id query: {e}"))?;
        let ids = stmt
            .query_map([], |row| row.get::<_, i64>(0))
            .map_err(|e| anyhow!("Failed to query window ids: {e}"))?
            .collect::<Result<HashSet<_>, _>>()
            .map_err(|e| anyhow!("Failed to decode a window id: {e}"))?;
        self.owned_windows = ids;
        Ok(())
    }
}

/// Insert or update one window row, skipping the write when nothing differs.
///
/// ### Arguments
/// - `conn`: The open transaction
/// - `window`: The window to persist
/// - `position`: Restore order of the window within the snapshot
/// - `stats`: Counters updated when a row is written
///
/// ### Errors
/// - Returns an error if the row cannot be read or written.
///
/// ### Returns
/// - `Ok(())`: The row matches the snapshot
/// - `Err(anyhow::Error)`: The row could not be read or written
fn apply_window(
    conn: &Connection,
    window: &WindowState,
    position: i64,
    stats: &mut ApplyStats,
) -> anyhow::Result<()> {
    let bounds = &window.window_bounds;
    let active_tab_id = window
        .active_tab_index
        .and_then(|index| window.tabs.get(index))
        .map(|tab| tab.tab_id)
        .map(i64::try_from)
        .transpose()
        .map_err(|e| anyhow!("Active tab id does not fit an integer: {e}"))?;
    let desired = StoredWindow {
        position,
        active_tab_id,
        bounds_state: bounds.state.clone(),
        x: bounds.x,
        y: bounds.y,
        width: bounds.width,
        height: bounds.height,
        display_id: bounds.display_id,
    };
    let stored = conn
        .query_row(
            "SELECT position, active_tab_id, bounds_state, bounds_x, bounds_y, bounds_width,
                    bounds_height, display_id
             FROM windows WHERE id = ?1",
            [window.window_id],
            |row| {
                Ok(StoredWindow {
                    position: row.get(0)?,
                    active_tab_id: row.get(1)?,
                    bounds_state: row.get(2)?,
                    x: row.get(3)?,
                    y: row.get(4)?,
                    width: row.get(5)?,
                    height: row.get(6)?,
                    display_id: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(|e| anyhow!("Failed to read window {}: {e}", window.window_id))?;
    if stored.as_ref() == Some(&desired) {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO windows (id, position, active_tab_id, bounds_state, bounds_x, bounds_y,
                              bounds_width, bounds_height, display_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
             position = excluded.position,
             active_tab_id = excluded.active_tab_id,
             bounds_state = excluded.bounds_state,
             bounds_x = excluded.bounds_x,
             bounds_y = excluded.bounds_y,
             bounds_width = excluded.bounds_width,
             bounds_height = excluded.bounds_height,
             display_id = excluded.display_id",
        params![
            window.window_id,
            desired.position,
            desired.active_tab_id,
            desired.bounds_state,
            desired.x,
            desired.y,
            desired.width,
            desired.height,
            desired.display_id,
        ],
    )
    .map_err(|e| anyhow!("Failed to write window {}: {e}", window.window_id))?;
    stats.windows_written += 1;
    Ok(())
}

/// Reconcile the tab rows of one window with the snapshot.
///
/// ### Arguments
/// - `conn`: The open transaction
/// - `window`: The window whose tabs are being persisted
/// - `stats`: Counters updated for each row written or deleted
///
/// ### Errors
/// - Returns an error if the existing rows cannot be read or a write fails.
///
/// ### Returns
/// - `Ok(())`: The tab rows match the snapshot
/// - `Err(anyhow::Error)`: The rows could not be reconciled
fn apply_tabs(
    conn: &Connection,
    window: &WindowState,
    stats: &mut ApplyStats,
) -> anyhow::Result<()> {
    let mut existing = read_stored_tabs(conn, window.window_id)?;
    for (position, tab) in window.tabs.iter().enumerate() {
        let position = i64::try_from(position)
            .map_err(|e| anyhow!("Tab position does not fit an integer: {e}"))?;
        let desired = stored_tab_from_snapshot(tab, position);
        let fingerprint = tab.content.as_ref().map(|content| {
            let (hash, len) = content.fingerprint();
            ContentFingerprint { hash, len }
        });
        match existing.remove(&tab.tab_id) {
            None => {
                insert_tab(conn, window.window_id, tab, &desired, fingerprint)?;
                stats.tabs_inserted += 1;
            }
            Some((stored, stored_fingerprint)) => {
                let content_changed = stored_fingerprint != fingerprint;
                if content_changed {
                    update_tab(conn, window.window_id, tab, &desired, fingerprint)?;
                    stats.tabs_content_written += 1;
                } else if stored != desired {
                    update_tab_metadata(conn, window.window_id, tab.tab_id, &desired)?;
                    stats.tabs_metadata_updated += 1;
                }
            }
        }
    }
    for closed in existing.keys() {
        let id =
            i64::try_from(*closed).map_err(|e| anyhow!("Tab id does not fit an integer: {e}"))?;
        let deleted = conn
            .execute(
                "DELETE FROM tabs WHERE window_id = ?1 AND id = ?2",
                params![window.window_id, id],
            )
            .map_err(|e| anyhow!("Failed to delete tab {closed}: {e}"))?;
        stats.tabs_deleted += deleted;
    }
    Ok(())
}

/// Read the comparable state of every tab row of a window.
///
/// ### Arguments
/// - `conn`: The open transaction
/// - `window_id`: Identity of the owning window
///
/// ### Errors
/// - Returns an error if the query fails or a row cannot be decoded.
///
/// ### Returns
/// - `Ok(HashMap)`: Stored tab state and content fingerprint, keyed by tab id
/// - `Err(anyhow::Error)`: The rows could not be read
fn read_stored_tabs(
    conn: &Connection,
    window_id: i64,
) -> anyhow::Result<HashMap<u64, (StoredTab, Option<ContentFingerprint>)>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, position, title, file_path, last_saved, log_view, color_tag,
                    remote_host, remote_port, remote_user, remote_path, content_hash, content_len
             FROM tabs WHERE window_id = ?1",
        )
        .map_err(|e| anyhow!("Failed to prepare the stored tab query: {e}"))?;
    let rows = stmt
        .query_map([window_id], |row| {
            let id: i64 = row.get(0)?;
            let stored = StoredTab {
                position: row.get(1)?,
                title: row.get(2)?,
                file_path: row.get(3)?,
                last_saved: row.get(4)?,
                log_view: row.get(5)?,
                color_tag: row.get(6)?,
                remote_host: row.get(7)?,
                remote_port: row.get(8)?,
                remote_user: row.get(9)?,
                remote_path: row.get(10)?,
            };
            let hash: Option<i64> = row.get(11)?;
            let len: Option<i64> = row.get(12)?;
            let fingerprint = match (hash, len) {
                (Some(hash), Some(len)) => {
                    usize::try_from(len).ok().map(|len| ContentFingerprint {
                        hash: hash.cast_unsigned(),
                        len,
                    })
                }
                _ => None,
            };
            Ok((id, stored, fingerprint))
        })
        .map_err(|e| anyhow!("Failed to query stored tabs: {e}"))?;
    let mut tabs = HashMap::new();
    for row in rows {
        let (id, stored, fingerprint) =
            row.map_err(|e| anyhow!("Failed to decode a stored tab: {e}"))?;
        if let Ok(id) = u64::try_from(id) {
            tabs.insert(id, (stored, fingerprint));
        }
    }
    Ok(tabs)
}

/// Project a snapshot tab into its comparable persisted form.
///
/// ### Arguments
/// - `tab`: The snapshot tab
/// - `position`: Display order of the tab within its window
///
/// ### Returns
/// - `StoredTab`: The row state the snapshot asks for
fn stored_tab_from_snapshot(tab: &TabState, position: i64) -> StoredTab {
    StoredTab {
        position,
        title: tab.title.clone(),
        file_path: tab.file_path.as_deref().map(path_to_bytes),
        last_saved: tab.last_saved.clone(),
        log_view: tab.log_view,
        color_tag: tab.color_tag.clone(),
        remote_host: tab.remote.as_ref().map(|remote| remote.host.clone()),
        remote_port: tab.remote.as_ref().map(|remote| remote.port),
        remote_user: tab.remote.as_ref().map(|remote| remote.user.clone()),
        remote_path: tab.remote.as_ref().map(|remote| remote.path.clone()),
    }
}

/// Encode a content fingerprint into its two nullable columns.
///
/// ### Arguments
/// - `fingerprint`: The fingerprint to encode, if the tab has content
///
/// ### Errors
/// - Returns an error if the byte length does not fit an `SQLite` integer.
///
/// ### Returns
/// - `Ok((Option<i64>, Option<i64>))`: The `content_hash` and `content_len` values
/// - `Err(anyhow::Error)`: The length could not be encoded
fn fingerprint_columns(
    fingerprint: Option<ContentFingerprint>,
) -> anyhow::Result<(Option<i64>, Option<i64>)> {
    match fingerprint {
        None => Ok((None, None)),
        Some(fingerprint) => {
            let len = i64::try_from(fingerprint.len)
                .map_err(|e| anyhow!("Content length does not fit an integer: {e}"))?;
            Ok((Some(fingerprint.hash.cast_signed()), Some(len)))
        }
    }
}

/// Insert a tab row, including its buffer text.
///
/// ### Arguments
/// - `conn`: The open transaction
/// - `window_id`: Identity of the owning window
/// - `tab`: The snapshot tab, read for its content
/// - `desired`: The row state to write
/// - `fingerprint`: Fingerprint of the content being written
///
/// ### Errors
/// - Returns an error if the insert fails.
///
/// ### Returns
/// - `Ok(())`: The row was inserted
/// - `Err(anyhow::Error)`: The row could not be inserted
fn insert_tab(
    conn: &Connection,
    window_id: i64,
    tab: &TabState,
    desired: &StoredTab,
    fingerprint: Option<ContentFingerprint>,
) -> anyhow::Result<()> {
    let id =
        i64::try_from(tab.tab_id).map_err(|e| anyhow!("Tab id does not fit an integer: {e}"))?;
    let (hash, len) = fingerprint_columns(fingerprint)?;
    conn.execute(
        // An upsert rather than a plain insert: tab identity is unique within a
        // window by construction, and if that ever broke, overwriting one row is
        // far better than failing the save and losing every window's state.
        "INSERT INTO tabs (window_id, id, position, title, file_path, content, content_hash,
                           content_len, last_saved, log_view, color_tag, remote_host, remote_port,
                           remote_user, remote_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(window_id, id) DO UPDATE SET
             position = excluded.position,
             title = excluded.title,
             file_path = excluded.file_path,
             content = excluded.content,
             content_hash = excluded.content_hash,
             content_len = excluded.content_len,
             last_saved = excluded.last_saved,
             log_view = excluded.log_view,
             color_tag = excluded.color_tag,
             remote_host = excluded.remote_host,
             remote_port = excluded.remote_port,
             remote_user = excluded.remote_user,
             remote_path = excluded.remote_path",
        params![
            window_id,
            id,
            desired.position,
            desired.title,
            desired.file_path,
            tab.content.as_ref().map(super::super::TabContent::to_text),
            hash,
            len,
            desired.last_saved,
            desired.log_view,
            desired.color_tag,
            desired.remote_host,
            desired.remote_port,
            desired.remote_user,
            desired.remote_path,
        ],
    )
    .map_err(|e| anyhow!("Failed to insert tab {id}: {e}"))?;
    Ok(())
}

/// Update a tab row including its buffer text.
///
/// ### Arguments
/// - `conn`: The open transaction
/// - `window_id`: Identity of the owning window
/// - `tab`: The snapshot tab, read for its content
/// - `desired`: The row state to write
/// - `fingerprint`: Fingerprint of the content being written
///
/// ### Errors
/// - Returns an error if the update fails.
///
/// ### Returns
/// - `Ok(())`: The row was updated
/// - `Err(anyhow::Error)`: The row could not be updated
fn update_tab(
    conn: &Connection,
    window_id: i64,
    tab: &TabState,
    desired: &StoredTab,
    fingerprint: Option<ContentFingerprint>,
) -> anyhow::Result<()> {
    let id =
        i64::try_from(tab.tab_id).map_err(|e| anyhow!("Tab id does not fit an integer: {e}"))?;
    let (hash, len) = fingerprint_columns(fingerprint)?;
    conn.execute(
        "UPDATE tabs SET position = ?3, title = ?4, file_path = ?5, content = ?6,
                         content_hash = ?7, content_len = ?8, last_saved = ?9, log_view = ?10,
                         color_tag = ?11, remote_host = ?12, remote_port = ?13, remote_user = ?14,
                         remote_path = ?15
         WHERE window_id = ?1 AND id = ?2",
        params![
            window_id,
            id,
            desired.position,
            desired.title,
            desired.file_path,
            tab.content.as_ref().map(super::super::TabContent::to_text),
            hash,
            len,
            desired.last_saved,
            desired.log_view,
            desired.color_tag,
            desired.remote_host,
            desired.remote_port,
            desired.remote_user,
            desired.remote_path,
        ],
    )
    .map_err(|e| anyhow!("Failed to update tab {id}: {e}"))?;
    Ok(())
}

/// Update everything about a tab row except its buffer text.
///
/// ### Arguments
/// - `conn`: The open transaction
/// - `window_id`: Identity of the owning window
/// - `tab_id`: Identity of the tab to update
/// - `desired`: The row state to write
///
/// ### Errors
/// - Returns an error if the update fails.
///
/// ### Returns
/// - `Ok(())`: The row was updated
/// - `Err(anyhow::Error)`: The row could not be updated
fn update_tab_metadata(
    conn: &Connection,
    window_id: i64,
    tab_id: u64,
    desired: &StoredTab,
) -> anyhow::Result<()> {
    let id = i64::try_from(tab_id).map_err(|e| anyhow!("Tab id does not fit an integer: {e}"))?;
    conn.execute(
        "UPDATE tabs SET position = ?3, title = ?4, file_path = ?5, last_saved = ?6,
                         log_view = ?7, color_tag = ?8, remote_host = ?9, remote_port = ?10,
                         remote_user = ?11, remote_path = ?12
         WHERE window_id = ?1 AND id = ?2",
        params![
            window_id,
            id,
            desired.position,
            desired.title,
            desired.file_path,
            desired.last_saved,
            desired.log_view,
            desired.color_tag,
            desired.remote_host,
            desired.remote_port,
            desired.remote_user,
            desired.remote_path,
        ],
    )
    .map_err(|e| anyhow!("Failed to update tab metadata for {id}: {e}"))?;
    Ok(())
}
