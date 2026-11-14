use super::mailbox::SnapshotMailbox;
use crate::fulgur::state::db::StateDb;
use crate::fulgur::state::persistence::WindowsState;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

/// Writer-thread bookkeeping for throttling writes.
pub(super) struct WriterState {
    throttle: Duration,
    mailbox: Arc<SnapshotMailbox>,
    db: Option<StateDb>,
    last_write_at: Option<Instant>,
}

impl WriterState {
    /// Create the bookkeeping for a writer thread.
    ///
    /// ### Arguments
    /// - `throttle`: Minimum delay between two consecutive asynchronous writes.
    /// - `mailbox`: Shared mailbox asynchronous snapshots are handed through.
    /// - `db`: The state database, absent when none could be opened.
    ///
    /// ### Returns
    /// - `Self`: Bookkeeping with no write history.
    pub(super) fn new(
        throttle: Duration,
        mailbox: Arc<SnapshotMailbox>,
        db: Option<StateDb>,
    ) -> Self {
        Self {
            throttle,
            mailbox,
            db,
            last_write_at: None,
        }
    }

    /// Persist a snapshot and record when the write happened.
    ///
    /// ### Arguments
    /// - `state`: The snapshot to persist.
    ///
    /// ### Errors
    /// - Returns an error if no database is available or if the rows cannot be
    ///   written.
    ///
    /// ### Returns
    /// - `Ok(())`: The database reflects the snapshot.
    /// - `Err(anyhow::Error)`: The snapshot could not be persisted.
    fn write(&mut self, state: &WindowsState) -> anyhow::Result<()> {
        let result = self.apply(state);
        self.last_write_at = Some(Instant::now());
        result
    }

    /// Reconcile the database with a snapshot.
    ///
    /// ### Arguments
    /// - `snapshot`: The snapshot to persist.
    ///
    /// ### Errors
    /// - Returns an error if no database is available or if the rows cannot be
    ///   written.
    ///
    /// ### Returns
    /// - `Ok(())`: The database reflects the snapshot.
    /// - `Err(anyhow::Error)`: The snapshot could not be persisted.
    fn apply(&mut self, snapshot: &WindowsState) -> anyhow::Result<()> {
        let db = self
            .db
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no state database is available"))?;
        let fallback_error = db.fallback_error().map(str::to_owned);
        let stats = db.apply(snapshot)?;
        if stats.is_empty() {
            log::debug!("State unchanged since the last write, nothing written");
        } else {
            log::debug!(
                "State written ({} window rows, {} tabs inserted, {} tabs updated, {} buffers written, {} tabs removed, {} windows removed)",
                stats.windows_written,
                stats.tabs_inserted,
                stats.tabs_metadata_updated,
                stats.tabs_content_written,
                stats.tabs_deleted,
                stats.windows_deleted
            );
        }
        if let Some(error) = fallback_error {
            return Err(anyhow::anyhow!(
                "{error}; session state was updated only in memory and will be lost when Fulgur exits"
            ));
        }
        Ok(())
    }

    /// Compute the earliest time the mailbox may be flushed.
    ///
    /// ### Returns
    /// - `Some(Instant)`: A snapshot is waiting; the value is when the throttle
    ///   allows writing it, which is in the past when it may be written now.
    /// - `None`: The mailbox is empty, so there is nothing to wait for.
    pub(super) fn mailbox_deadline(&self) -> Option<Instant> {
        if !self.mailbox.has_pending() {
            return None;
        }
        Some(
            self.last_write_at
                .map_or_else(Instant::now, |last| last + self.throttle),
        )
    }

    /// Write the remote history waiting in the mailbox, if any.
    pub(super) fn flush_remote_history(&mut self) {
        let Some(history) = self.mailbox.take_remote_history() else {
            return;
        };
        let Some(db) = self.db.as_mut() else {
            log::error!(
                "State writer failed to save the remote history: no state database is available"
            );
            return;
        };
        match db.apply_remote_history(&history) {
            Ok(rows) => log::debug!("Remote history written ({rows} rows changed)"),
            Err(e) => log::error!("State writer failed to save the remote history: {e}"),
        }
    }

    /// Write the snapshot waiting in the mailbox, ignoring the throttle.
    pub(super) fn flush_mailbox(&mut self) {
        if let Some(state) = self.mailbox.take()
            && let Err(e) = self.write(&state)
        {
            log::error!("State writer failed to save state: {e}");
        }
    }

    /// Write a snapshot whose caller is waiting for the result.
    ///
    /// ### Arguments
    /// - `state`: The snapshot to persist.
    /// - `reply`: Channel the result is reported on.
    pub(super) fn handle_blocking(
        &mut self,
        state: &WindowsState,
        reply: &mpsc::Sender<anyhow::Result<()>>,
    ) {
        self.mailbox.take();
        let result = self.write(state);
        if let Err(ref e) = result {
            log::error!("State writer failed to save state: {e}");
        }
        if reply.send(result).is_err() {
            log::warn!("State writer reply channel dropped before result was read");
        }
    }

    /// Flush anything outstanding and leave the database ready for next launch.
    pub(super) fn shut_down(&mut self) {
        self.flush_remote_history();
        self.flush_mailbox();
        if let Some(db) = self.db.as_ref() {
            db.checkpoint();
        }
    }
}
