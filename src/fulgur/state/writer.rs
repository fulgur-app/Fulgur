use super::db::StateDb;
use super::persistence::WindowsState;
use crate::fulgur::utils::worker::Worker;
use parking_lot::Mutex;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

/// Bounded channel capacity for writer messages.
///
/// Asynchronous snapshots never travel through this channel: they are coalesced
/// in the `SnapshotMailbox`, so the queue only ever holds blocking requests,
/// whose callers wait on the reply before issuing another one, plus zero-sized
/// wakeups. A full queue therefore indicates an abnormal backlog of blocking
/// saves and makes those callers back off naturally.
const CHANNEL_CAPACITY: usize = 16;

/// Maximum time a dropped `StateWriter` waits for the worker to flush queued
/// saves before detaching it.
const WRITER_JOIN_TIMEOUT: Duration = Duration::from_secs(5);

/// Minimum delay between two consecutive asynchronous state writes.
const SAVE_THROTTLE: Duration = Duration::from_millis(200);

/// A message sent from a UI thread to the writer thread.
enum WriterMessage {
    /// A snapshot whose caller blocks until the writer reports the result.
    Blocking {
        state: WindowsState,
        reply: mpsc::Sender<anyhow::Result<()>>,
    },
    /// Signals that the mailbox now holds a snapshot waiting to be written.
    MailboxFilled,
}

/// Coalescing hand-off for asynchronous snapshots.
///
/// Only the newest snapshot matters, since each one describes the whole session,
/// so a burst collapses into a single write instead of queueing one snapshot per
/// request.
#[derive(Default)]
struct SnapshotMailbox {
    pending: Mutex<Option<WindowsState>>,
}

impl SnapshotMailbox {
    /// Store a snapshot, replacing any snapshot already waiting.
    ///
    /// ### Arguments
    /// - `state`: The snapshot to hand to the writer thread.
    fn put(&self, state: WindowsState) {
        *self.pending.lock() = Some(state);
    }

    /// Take the waiting snapshot, leaving the mailbox empty.
    ///
    /// ### Returns
    /// - `Some(WindowsState)`: The snapshot that was waiting
    /// - `None`: The mailbox was empty
    fn take(&self) -> Option<WindowsState> {
        self.pending.lock().take()
    }

    /// Report whether a snapshot is waiting.
    ///
    /// ### Returns
    /// - `bool`: `true` when the mailbox holds a snapshot to write
    fn has_pending(&self) -> bool {
        self.pending.lock().is_some()
    }
}

/// Writer-thread bookkeeping for throttling writes.
struct WriterState {
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
    fn new(throttle: Duration, mailbox: Arc<SnapshotMailbox>, db: Option<StateDb>) -> Self {
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
        Ok(())
    }

    /// Compute the earliest time the mailbox may be flushed.
    ///
    /// ### Returns
    /// - `Some(Instant)`: A snapshot is waiting; the value is when the throttle
    ///   allows writing it, which is in the past when it may be written now.
    /// - `None`: The mailbox is empty, so there is nothing to wait for.
    fn mailbox_deadline(&self) -> Option<Instant> {
        if !self.mailbox.has_pending() {
            return None;
        }
        Some(
            self.last_write_at
                .map_or_else(Instant::now, |last| last + self.throttle),
        )
    }

    /// Write the snapshot waiting in the mailbox, ignoring the throttle.
    fn flush_mailbox(&mut self) {
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
    fn handle_blocking(&mut self, state: &WindowsState, reply: &mpsc::Sender<anyhow::Result<()>>) {
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
    fn shut_down(&mut self) {
        self.flush_mailbox();
        if let Some(db) = self.db.as_ref() {
            db.checkpoint();
        }
    }
}

/// Dedicated background writer that serializes all session-state persistence.
pub struct StateWriter {
    sender: mpsc::SyncSender<WriterMessage>,
    mailbox: Arc<SnapshotMailbox>,
    _worker: Worker,
}

impl StateWriter {
    /// Spawn the writer thread and return a handle for submitting save requests.
    ///
    /// ### Arguments
    /// - `db`: The state database, absent when none could be opened.
    ///
    /// ### Returns
    /// - `Self`: A writer handle that dispatches work to the worker thread.
    ///
    /// ### Panics
    /// Panics if the OS refuses to spawn the background writer thread.
    #[must_use]
    pub fn new(db: Option<StateDb>) -> Self {
        Self::with_throttle(db, SAVE_THROTTLE)
    }

    /// Spawn the writer thread with an explicit throttle period.
    ///
    /// ### Arguments
    /// - `db`: The state database, absent when none could be opened.
    /// - `throttle`: Minimum delay between two consecutive asynchronous writes.
    ///
    /// ### Returns
    /// - `Self`: A writer handle bound to the freshly spawned worker thread.
    fn with_throttle(db: Option<StateDb>, throttle: Duration) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<WriterMessage>(CHANNEL_CAPACITY);
        let mailbox = Arc::new(SnapshotMailbox::default());
        let writer_state = WriterState::new(throttle, Arc::clone(&mailbox), db);
        let worker = Worker::spawn(
            "fulgur-state-writer",
            WRITER_JOIN_TIMEOUT,
            move |_shutdown| {
                Self::run(&receiver, writer_state);
            },
        );
        Self {
            sender,
            mailbox,
            _worker: worker,
        }
    }

    /// Worker-thread loop that processes save requests one at a time.
    ///
    /// ### Arguments
    /// - `receiver`: Channel of blocking requests and mailbox wakeups.
    /// - `writer_state`: Throttle bookkeeping, owning the database connection
    ///   and the writer's handle on the shared mailbox.
    fn run(receiver: &mpsc::Receiver<WriterMessage>, mut writer_state: WriterState) {
        loop {
            let next_message = match writer_state.mailbox_deadline() {
                Some(deadline) => {
                    let wait = deadline.saturating_duration_since(Instant::now());
                    match receiver.recv_timeout(wait) {
                        Ok(message) => Some(message),
                        Err(mpsc::RecvTimeoutError::Timeout) => None,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                None => match receiver.recv() {
                    Ok(message) => Some(message),
                    Err(mpsc::RecvError) => break,
                },
            };
            match next_message {
                Some(WriterMessage::Blocking { state, reply }) => {
                    writer_state.handle_blocking(&state, &reply);
                }
                Some(WriterMessage::MailboxFilled) => {}
                None => writer_state.flush_mailbox(),
            }
        }
        writer_state.shut_down();
        log::debug!("State writer thread exiting (no more senders)");
    }

    /// Enqueue a snapshot and block the caller until the writer has persisted it.
    ///
    /// ### Description
    /// Guarantees FIFO ordering with all other save requests: any save started
    /// before this call completes before this one begins, and any save issued
    /// after this call returns sees the result of this save on disk. Blocking
    /// saves are never throttled, and they supersede any asynchronous save still
    /// held back, which is by construction older.
    ///
    /// Rows identical to what is already stored are left alone, so a snapshot
    /// that changes nothing reports success without touching the database.
    ///
    /// ### Arguments
    /// - `state`: The fully-assembled windows state snapshot to persist.
    ///
    /// ### Errors
    /// - Returns an error if the writer thread has exited, if the reply channel is
    ///   dropped, or if the underlying write fails.
    ///
    /// ### Returns
    /// - `Ok(())`: The writer successfully persisted the snapshot.
    /// - `Err(anyhow::Error)`: The write failed, or the writer thread has exited
    ///   before the request could be processed.
    pub fn save_blocking(&self, state: WindowsState) -> anyhow::Result<()> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.sender
            .send(WriterMessage::Blocking {
                state,
                reply: reply_tx,
            })
            .map_err(|_| anyhow::anyhow!("state writer thread has exited"))?;
        reply_rx
            .recv()
            .map_err(|_| anyhow::anyhow!("state writer reply channel closed before result"))?
    }

    /// Hand a snapshot to the writer without waiting for the write to complete.
    ///
    /// ### Arguments
    /// - `state`: The fully-assembled windows state snapshot to persist.
    pub fn save_async(&self, state: WindowsState) {
        self.mailbox.put(state);
        match self.sender.try_send(WriterMessage::MailboxFilled) {
            Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.mailbox.take();
                log::error!("State writer thread has exited; dropped async save request");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::persistence::{
        SerializedWindowBounds, TabContent, TabState, WindowState, WindowsState,
    };
    use super::*;
    use std::path::Path;
    use std::sync::Arc;
    use std::thread;
    use tempfile::tempdir;

    fn sample_state(label: &str) -> WindowsState {
        state_with_window_id(label, 1)
    }

    fn state_with_window_id(label: &str, window_id: i64) -> WindowsState {
        WindowsState {
            windows: vec![WindowState {
                window_id,
                tabs: vec![TabState {
                    tab_id: 0,
                    title: label.to_string(),
                    file_path: None,
                    content: Some(TabContent::from(label)),
                    last_saved: None,
                    remote: None,
                    log_view: false,
                    color_tag: None,
                }],
                active_tab_index: Some(0),
                window_bounds: SerializedWindowBounds::default(),
            }],
        }
    }

    /// Open a writer over a fresh database at `path`.
    fn writer_at(path: &Path, throttle: Duration) -> StateWriter {
        let db = StateDb::open(path).expect("open state database");
        StateWriter::with_throttle(Some(db), throttle)
    }

    /// Read back the title of the single tab persisted at `path`.
    fn persisted_title(path: &Path) -> String {
        WindowsState::load_from_path(path)
            .expect("load persisted state")
            .windows[0]
            .tabs[0]
            .title
            .clone()
    }

    /// Read back every persisted window.
    fn persisted_windows(path: &Path) -> Vec<WindowState> {
        WindowsState::load_from_path(path)
            .expect("load persisted state")
            .windows
    }

    #[test]
    fn writer_persists_snapshot() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.db");
        let writer = writer_at(&path, SAVE_THROTTLE);
        writer.save_blocking(sample_state("solo")).unwrap();
        assert_eq!(persisted_title(&path), "solo");
    }

    #[test]
    fn writer_async_save_persists_snapshot_on_shutdown() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.db");
        let writer = writer_at(&path, NEVER_ELAPSES);
        writer.save_async(sample_state("async"));
        // Dropping the writer flushes whatever the throttle is still holding.
        drop(writer);
        assert_eq!(persisted_title(&path), "async");
    }

    #[test]
    fn writer_flattens_rope_backed_content() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.db");
        let writer = writer_at(&path, SAVE_THROTTLE);
        let mut state = sample_state("rope");
        let text = "ünsaved\ttext with \"quotes\"\nand a newline";
        state.windows[0].tabs[0].content = Some(TabContent::Rope(ropey::Rope::from_str(text)));
        writer.save_blocking(state).unwrap();
        let windows = persisted_windows(&path);
        assert_eq!(windows[0].tabs[0].content.as_ref().unwrap(), text);
    }

    /// Throttle long enough that no held-back save can escape on its own timer,
    /// so the coalescing tests below stay deterministic on slow machines.
    const NEVER_ELAPSES: Duration = Duration::from_mins(1);

    #[test]
    fn writer_coalesces_a_burst_of_async_saves() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.db");
        let writer = writer_at(&path, NEVER_ELAPSES);
        // Seed the throttle so no snapshot of the burst can escape on the
        // leading edge, which makes the outcome deterministic.
        writer.save_blocking(sample_state("seed")).unwrap();
        for i in 0..10 {
            writer.save_async(sample_state(&format!("burst-{i}")));
        }
        drop(writer);
        assert_eq!(
            persisted_title(&path),
            "burst-9",
            "the burst must collapse into the newest snapshot"
        );
    }

    #[test]
    fn writer_holds_at_most_one_pending_snapshot() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.db");
        let writer = writer_at(&path, NEVER_ELAPSES);
        writer.save_blocking(sample_state("seed")).unwrap();
        let burst = CHANNEL_CAPACITY * 4;
        for i in 0..burst {
            writer.save_async(sample_state(&format!("burst-{i}")));
        }
        let held = writer.mailbox.take().expect("a snapshot must be waiting");
        assert_eq!(
            held.windows[0].tabs[0].title,
            format!("burst-{}", burst - 1),
            "the surviving snapshot must be the newest one"
        );
        assert!(
            !writer.mailbox.has_pending(),
            "a burst must not queue one snapshot per request"
        );
    }

    #[test]
    fn writer_throttled_async_save_lands_once_the_period_elapses() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.db");
        let writer = writer_at(&path, Duration::from_millis(500));
        writer.save_blocking(sample_state("first")).unwrap();
        writer.save_async(sample_state("second"));
        assert_eq!(
            persisted_title(&path),
            "first",
            "the async save must be held back by the throttle"
        );
        thread::sleep(Duration::from_millis(900));
        assert_eq!(persisted_title(&path), "second");
    }

    #[test]
    fn writer_blocking_save_supersedes_a_held_back_async_save() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.db");
        let writer = writer_at(&path, NEVER_ELAPSES);
        writer.save_blocking(sample_state("first")).unwrap();
        writer.save_async(sample_state("superseded"));
        writer.save_blocking(sample_state("last")).unwrap();
        assert!(
            !writer.mailbox.has_pending(),
            "the superseded snapshot must be dropped, not written later"
        );
        drop(writer);
        assert_eq!(persisted_title(&path), "last");
    }

    #[test]
    fn writer_flushes_a_held_back_save_on_shutdown() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.db");
        let writer = writer_at(&path, NEVER_ELAPSES);
        writer.save_blocking(sample_state("first")).unwrap();
        writer.save_async(sample_state("held-back"));
        drop(writer);
        assert_eq!(persisted_title(&path), "held-back");
    }

    #[test]
    fn writer_reports_an_error_when_no_database_is_available() {
        let writer = StateWriter::with_throttle(None, SAVE_THROTTLE);
        assert!(writer.save_blocking(sample_state("nowhere")).is_err());
    }

    #[test]
    fn writer_serializes_concurrent_save_requests() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.db");
        let writer = Arc::new(writer_at(&path, SAVE_THROTTLE));
        let mut handles = Vec::new();
        for i in 0..16 {
            let writer = Arc::clone(&writer);
            let state = state_with_window_id(&format!("thread-{i}"), i64::from(i) + 1);
            handles.push(thread::spawn(move || writer.save_blocking(state)));
        }
        for handle in handles {
            assert!(handle.join().unwrap().is_ok());
        }
        drop(writer);
        // Each snapshot describes the whole session and owns a single window, so
        // the last writer's window is the one left standing. What matters is that
        // the interleaving produced exactly one coherent window, not a mix.
        let windows = persisted_windows(&path);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].tabs.len(), 1);
        assert!(windows[0].tabs[0].title.starts_with("thread-"));
    }
}
