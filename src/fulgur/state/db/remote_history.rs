//! Persisting the Open Remote File dialog URL history.

use super::StateDb;
use crate::fulgur::state::persistence::RemoteHistory;
use anyhow::anyhow;
use rusqlite::{TransactionBehavior, params};
use std::collections::HashMap;

impl StateDb {
    /// Read the remote URL history.
    ///
    /// ### Errors
    /// - Returns an error if the query fails or if a row cannot be decoded.
    ///
    /// ### Returns
    /// - `Ok(RemoteHistory)`: The persisted URLs, most recent first
    /// - `Err(anyhow::Error)`: The history could not be read
    pub fn load_remote_history(&self) -> anyhow::Result<RemoteHistory> {
        let mut stmt = self
            .conn
            .prepare("SELECT url FROM remote_history ORDER BY position, url")
            .map_err(|e| anyhow!("Failed to prepare the remote history query: {e}"))?;
        let urls = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| anyhow!("Failed to query the remote history: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("Failed to decode a remote history entry: {e}"))?;
        Ok(RemoteHistory::from_urls(urls))
    }

    /// Persist the remote URL history, writing only the rows that differ.
    ///
    /// ### Arguments
    /// - `history`: The history to persist
    ///
    /// ### Errors
    /// - Returns an error if the transaction cannot be opened, if a statement
    ///   fails, or if the commit fails.
    ///
    /// ### Returns
    /// - `Ok(usize)`: Number of rows inserted, updated or deleted
    /// - `Err(anyhow::Error)`: The history could not be persisted
    pub fn apply_remote_history(&mut self, history: &RemoteHistory) -> anyhow::Result<usize> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| anyhow!("Failed to begin the remote history transaction: {e}"))?;
        let mut stored = {
            let mut stmt = tx
                .prepare("SELECT url, position FROM remote_history")
                .map_err(|e| anyhow!("Failed to prepare the stored remote history query: {e}"))?;
            stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| anyhow!("Failed to query the stored remote history: {e}"))?
            .collect::<Result<HashMap<_, _>, _>>()
            .map_err(|e| anyhow!("Failed to decode a stored remote history entry: {e}"))?
        };
        let mut rows_written = 0;
        for (position, url) in history.urls().iter().enumerate() {
            let position = i64::try_from(position)
                .map_err(|e| anyhow!("Remote history position does not fit an integer: {e}"))?;
            if stored.remove(url) == Some(position) {
                continue;
            }
            tx.execute(
                "INSERT INTO remote_history (url, position) VALUES (?1, ?2)
                 ON CONFLICT(url) DO UPDATE SET position = excluded.position",
                params![url, position],
            )
            .map_err(|e| anyhow!("Failed to write remote history entry: {e}"))?;
            rows_written += 1;
        }
        for forgotten in stored.keys() {
            rows_written += tx
                .execute("DELETE FROM remote_history WHERE url = ?1", [forgotten])
                .map_err(|e| anyhow!("Failed to delete remote history entry: {e}"))?;
        }
        tx.commit()
            .map_err(|e| anyhow!("Failed to commit the remote history transaction: {e}"))?;
        Ok(rows_written)
    }
}
