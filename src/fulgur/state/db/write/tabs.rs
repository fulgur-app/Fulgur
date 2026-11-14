//! Reconciling tab rows, including their buffer text.

use super::super::ApplyStats;
use super::super::paths::path_to_bytes;
use crate::fulgur::state::TabContent;
use crate::fulgur::state::persistence::{TabState, WindowState};
use anyhow::anyhow;
use rusqlite::{Connection, params};
use std::collections::HashMap;
use time::format_description::well_known::Rfc3339;

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
    share_device_name: Option<String>,
    share_shared_at: Option<String>,
    share_size: Option<i64>,
    encoding: Option<String>,
    lossy_decode: bool,
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
pub(super) fn apply_tabs(
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
                    remote_host, remote_port, remote_user, remote_path, content_hash, content_len,
                    share_device_name, share_shared_at, share_size, encoding, lossy_decode
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
                share_device_name: row.get(13)?,
                share_shared_at: row.get(14)?,
                share_size: row.get(15)?,
                encoding: row.get(16)?,
                lossy_decode: row.get(17)?,
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
        share_device_name: tab
            .share
            .as_ref()
            .and_then(|share| share.source_device_name.clone()),
        share_shared_at: tab
            .share
            .as_ref()
            .and_then(|share| share.shared_at)
            .and_then(|shared_at| shared_at.format(&Rfc3339).ok()),
        share_size: tab
            .share
            .as_ref()
            .map(|share| i64::try_from(share.size_bytes).unwrap_or(i64::MAX)),
        encoding: tab.encoding.clone(),
        lossy_decode: tab.lossy_decode,
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
                           remote_user, remote_path, share_device_name, share_shared_at, share_size,
                           encoding, lossy_decode)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                 ?19, ?20)
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
             remote_path = excluded.remote_path,
             share_device_name = excluded.share_device_name,
             share_shared_at = excluded.share_shared_at,
             share_size = excluded.share_size,
             encoding = excluded.encoding,
             lossy_decode = excluded.lossy_decode",
        params![
            window_id,
            id,
            desired.position,
            desired.title,
            desired.file_path,
            tab.content.as_ref().map(TabContent::to_text),
            hash,
            len,
            desired.last_saved,
            desired.log_view,
            desired.color_tag,
            desired.remote_host,
            desired.remote_port,
            desired.remote_user,
            desired.remote_path,
            desired.share_device_name,
            desired.share_shared_at,
            desired.share_size,
            desired.encoding,
            desired.lossy_decode,
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
                         remote_path = ?15, share_device_name = ?16, share_shared_at = ?17,
                         share_size = ?18, encoding = ?19, lossy_decode = ?20
         WHERE window_id = ?1 AND id = ?2",
        params![
            window_id,
            id,
            desired.position,
            desired.title,
            desired.file_path,
            tab.content.as_ref().map(TabContent::to_text),
            hash,
            len,
            desired.last_saved,
            desired.log_view,
            desired.color_tag,
            desired.remote_host,
            desired.remote_port,
            desired.remote_user,
            desired.remote_path,
            desired.share_device_name,
            desired.share_shared_at,
            desired.share_size,
            desired.encoding,
            desired.lossy_decode,
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
                         remote_user = ?11, remote_path = ?12, share_device_name = ?13,
                         share_shared_at = ?14, share_size = ?15, encoding = ?16,
                         lossy_decode = ?17
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
            desired.share_device_name,
            desired.share_shared_at,
            desired.share_size,
            desired.encoding,
            desired.lossy_decode,
        ],
    )
    .map_err(|e| anyhow!("Failed to update tab metadata for {id}: {e}"))?;
    Ok(())
}
