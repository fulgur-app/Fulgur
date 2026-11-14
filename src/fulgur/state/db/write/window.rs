//! Writing window rows.

use super::super::ApplyStats;
use crate::fulgur::state::persistence::WindowState;
use anyhow::anyhow;
use rusqlite::{Connection, OptionalExtension, params};

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
pub(super) fn apply_window(
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
