//! Applying an in-memory snapshot to the database as per-row changes.

mod tabs;
mod window;

use super::{ApplyStats, StateDb};
use crate::fulgur::state::persistence::WindowsState;
use anyhow::anyhow;
use rusqlite::TransactionBehavior;
use std::collections::HashSet;
use tabs::apply_tabs;
use window::apply_window;

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
