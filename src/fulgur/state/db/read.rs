//! Reading the persisted session back into an in-memory snapshot.

use super::StateDb;
use super::paths::path_from_bytes;
use crate::fulgur::state::persistence::{
    SerializedRemoteSpec, SerializedWindowBounds, TabContent, TabState, WindowState, WindowsState,
};
use anyhow::anyhow;
use rusqlite::Row;

impl StateDb {
    /// Read every persisted window, in restore order, with its tabs.
    ///
    /// ### Errors
    /// - Returns an error if a query fails or if a stored row cannot be decoded.
    ///
    /// ### Returns
    /// - `Ok(WindowsState)`: The persisted session, empty when nothing is stored
    /// - `Err(anyhow::Error)`: The session could not be read
    pub fn load(&self) -> anyhow::Result<WindowsState> {
        let mut window_stmt = self
            .conn
            .prepare(
                "SELECT id, active_tab_id, bounds_state, bounds_x, bounds_y, bounds_width,
                        bounds_height, display_id
                 FROM windows
                 ORDER BY position, id",
            )
            .map_err(|e| anyhow!("Failed to prepare the window query: {e}"))?;
        let windows: Vec<(i64, Option<i64>, SerializedWindowBounds)> = window_stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, window_bounds_from_row(row)?))
            })
            .map_err(|e| anyhow!("Failed to query persisted windows: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("Failed to decode a persisted window: {e}"))?;

        let mut state = WindowsState {
            windows: Vec::with_capacity(windows.len()),
        };
        for (window_id, active_tab_id, window_bounds) in windows {
            let tabs = self.load_tabs(window_id)?;
            let active_tab_index = active_tab_id.and_then(|active| {
                let active = u64::try_from(active).ok()?;
                tabs.iter().position(|tab| tab.tab_id == active)
            });
            state.windows.push(WindowState {
                window_id,
                tabs,
                active_tab_index,
                window_bounds,
            });
        }
        Ok(state)
    }

    /// Read the tabs of one window, in display order.
    ///
    /// ### Arguments
    /// - `window_id`: Identity of the owning window
    ///
    /// ### Errors
    /// - Returns an error if the query fails or a row cannot be decoded.
    ///
    /// ### Returns
    /// - `Ok(Vec<TabState>)`: The window's tabs in display order
    /// - `Err(anyhow::Error)`: The tabs could not be read
    fn load_tabs(&self, window_id: i64) -> anyhow::Result<Vec<TabState>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, title, file_path, content, last_saved, log_view, color_tag,
                        remote_host, remote_port, remote_user, remote_path
                 FROM tabs
                 WHERE window_id = ?1
                 ORDER BY position, id",
            )
            .map_err(|e| anyhow!("Failed to prepare the tab query: {e}"))?;
        let tabs = stmt
            .query_map([window_id], tab_state_from_row)
            .map_err(|e| anyhow!("Failed to query persisted tabs: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("Failed to decode a persisted tab: {e}"))?;
        Ok(tabs)
    }
}

/// Decode the window bounds columns of a window row.
///
/// ### Arguments
/// - `row`: The window row, positioned on the bounds columns
///
/// ### Errors
/// - Returns a `rusqlite::Error` if a column cannot be read.
///
/// ### Returns
/// - `Ok(SerializedWindowBounds)`: The decoded bounds
/// - `Err(rusqlite::Error)`: A column could not be read
fn window_bounds_from_row(row: &Row<'_>) -> rusqlite::Result<SerializedWindowBounds> {
    Ok(SerializedWindowBounds {
        state: row.get(2)?,
        x: row.get(3)?,
        y: row.get(4)?,
        width: row.get(5)?,
        height: row.get(6)?,
        display_id: row.get(7)?,
    })
}

/// Decode one tab row into a `TabState`.
///
/// ### Arguments
/// - `row`: The tab row
///
/// ### Errors
/// - Returns a `rusqlite::Error` if a column cannot be read.
///
/// ### Returns
/// - `Ok(TabState)`: The decoded tab
/// - `Err(rusqlite::Error)`: A column could not be read
fn tab_state_from_row(row: &Row<'_>) -> rusqlite::Result<TabState> {
    let stored_id: i64 = row.get(0)?;
    let file_path: Option<Vec<u8>> = row.get(2)?;
    let content: Option<String> = row.get(3)?;
    let remote_host: Option<String> = row.get(7)?;
    let remote = match remote_host {
        Some(host) => Some(SerializedRemoteSpec {
            host,
            port: row.get(8)?,
            user: row.get::<_, Option<String>>(9)?.unwrap_or_default(),
            path: row.get::<_, Option<String>>(10)?.unwrap_or_default(),
        }),
        None => None,
    };
    Ok(TabState {
        tab_id: u64::try_from(stored_id).unwrap_or_default(),
        title: row.get(1)?,
        file_path: file_path.as_deref().map(path_from_bytes),
        content: content.map(TabContent::Text),
        last_saved: row.get(4)?,
        remote,
        log_view: row.get(5)?,
        color_tag: row.get(6)?,
    })
}
