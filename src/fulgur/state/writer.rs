use super::persistence::WindowsState;
use crate::fulgur::utils::worker::Worker;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
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
    /// A snapshot whose caller blocks until the writer reports the I/O result.
    Blocking {
        state: WindowsState,
        path: PathBuf,
        reply: mpsc::Sender<anyhow::Result<()>>,
    },
    /// Signals that the mailbox now holds a snapshot waiting to be written.
    MailboxFilled,
}

/// A snapshot waiting in the mailbox, together with its destination.
struct PendingWrite {
    state: WindowsState,
    path: PathBuf,
}

/// Coalescing hand-off for asynchronous snapshots.
#[derive(Default)]
struct SnapshotMailbox {
    pending: Mutex<Vec<PendingWrite>>,
}

impl SnapshotMailbox {
    /// Store a snapshot, replacing any snapshot already waiting for the same path.
    ///
    /// ### Arguments
    /// - `state`: The snapshot to hand to the writer thread.
    /// - `path`: Destination file path.
    fn put(&self, state: WindowsState, path: PathBuf) {
        let mut pending = self.pending.lock();
        match pending.iter_mut().find(|entry| entry.path == path) {
            Some(existing) => existing.state = state,
            None => pending.push(PendingWrite { state, path }),
        }
    }

    /// Drop the snapshot waiting for `path`, if any.
    ///
    /// ### Arguments
    /// - `path`: Destination whose waiting snapshot is superseded.
    fn discard(&self, path: &Path) {
        self.pending.lock().retain(|entry| entry.path != path);
    }

    /// Take every waiting snapshot, oldest destination first.
    ///
    /// ### Returns
    /// - `Vec<PendingWrite>`: The waiting snapshots, leaving the mailbox empty.
    fn drain(&self) -> Vec<PendingWrite> {
        std::mem::take(&mut *self.pending.lock())
    }

    /// Report whether at least one snapshot is waiting.
    ///
    /// ### Returns
    /// - `bool`: `true` when the mailbox holds a snapshot to write.
    fn has_pending(&self) -> bool {
        !self.pending.lock().is_empty()
    }
}

/// Writer-thread bookkeeping for throttling and de-duplicating writes.
struct WriterState {
    throttle: Duration,
    mailbox: Arc<SnapshotMailbox>,
    last_write_at: Option<Instant>,
    last_written: Option<(PathBuf, [u8; 32])>,
}

impl WriterState {
    /// Create the bookkeeping for a writer thread.
    ///
    /// ### Arguments
    /// - `throttle`: Minimum delay between two consecutive asynchronous writes.
    /// - `mailbox`: Shared mailbox asynchronous snapshots are handed through.
    ///
    /// ### Returns
    /// - `Self`: Bookkeeping with no write history.
    fn new(throttle: Duration, mailbox: Arc<SnapshotMailbox>) -> Self {
        Self {
            throttle,
            mailbox,
            last_write_at: None,
            last_written: None,
        }
    }

    /// Persist a snapshot unless it is identical to the last one written.
    ///
    /// ### Arguments
    /// - `state`: The snapshot to persist.
    /// - `path`: Destination file path.
    ///
    /// ### Errors
    /// - Returns an error if the snapshot cannot be serialized or written.
    ///
    /// ### Returns
    /// - `Ok(())`: The file holds the snapshot, whether it was just written or
    ///   already up to date.
    /// - `Err(anyhow::Error)`: Serialization or I/O failed.
    fn write(&mut self, state: &WindowsState, path: &Path) -> anyhow::Result<()> {
        let result = self.write_if_changed(state, path);
        self.last_write_at = Some(Instant::now());
        result
    }

    /// Serialize, compare against the last write, and persist when it differs.
    ///
    /// ### Arguments
    /// - `state`: The snapshot to persist.
    /// - `path`: Destination file path.
    ///
    /// ### Errors
    /// - Returns an error if the snapshot cannot be serialized or written.
    ///
    /// ### Returns
    /// - `Ok(())`: The file holds the snapshot.
    /// - `Err(anyhow::Error)`: Serialization or I/O failed.
    fn write_if_changed(&mut self, state: &WindowsState, path: &Path) -> anyhow::Result<()> {
        let json = state.to_json()?;
        let digest: [u8; 32] = Sha256::digest(json.as_bytes()).into();
        let unchanged = self
            .last_written
            .as_ref()
            .is_some_and(|(written_path, written_digest)| {
                written_path.as_path() == path && *written_digest == digest
            });
        if unchanged {
            log::debug!("State unchanged since the last write, skipping");
            return Ok(());
        }
        WindowsState::write_json_to_path(&json, path)?;
        self.last_written = Some((path.to_path_buf(), digest));
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

    /// Write every snapshot waiting in the mailbox, ignoring the throttle.
    fn flush_mailbox(&mut self) {
        for pending in self.mailbox.drain() {
            if let Err(e) = self.write(&pending.state, &pending.path) {
                log::error!("State writer failed to save state: {e}");
            }
        }
    }

    /// Write a snapshot whose caller is waiting for the result.
    ///
    /// ### Arguments
    /// - `state`: The snapshot to persist.
    /// - `path`: Destination file path.
    /// - `reply`: Channel the I/O result is reported on.
    fn handle_blocking(
        &mut self,
        state: &WindowsState,
        path: &Path,
        reply: &mpsc::Sender<anyhow::Result<()>>,
    ) {
        self.mailbox.discard(path);
        self.flush_mailbox();
        let result = self.write(state, path);
        if let Err(ref e) = result {
            log::error!("State writer failed to save state: {e}");
        }
        if reply.send(result).is_err() {
            log::warn!("State writer reply channel dropped before result was read");
        }
    }
}

/// Dedicated background writer that serializes all `WindowsState` persistence.
pub struct StateWriter {
    sender: mpsc::SyncSender<WriterMessage>,
    mailbox: Arc<SnapshotMailbox>,
    _worker: Worker,
}

impl StateWriter {
    /// Spawn the writer thread and return a handle for submitting save requests.
    ///
    /// ### Returns
    /// - `Self`: A writer handle whose `save_blocking` method dispatches work to
    ///   the worker thread.
    ///
    /// ### Panics
    /// Panics if the OS refuses to spawn the background writer thread.
    #[must_use]
    pub fn new() -> Self {
        Self::with_throttle(SAVE_THROTTLE)
    }

    /// Spawn the writer thread with an explicit throttle period.
    ///
    /// ### Arguments
    /// - `throttle`: Minimum delay between two consecutive asynchronous writes.
    ///
    /// ### Returns
    /// - `Self`: A writer handle bound to the freshly spawned worker thread.
    fn with_throttle(throttle: Duration) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<WriterMessage>(CHANNEL_CAPACITY);
        let mailbox = Arc::new(SnapshotMailbox::default());
        let writer_state = WriterState::new(throttle, Arc::clone(&mailbox));
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
    /// - `writer_state`: Throttle and de-duplication bookkeeping, owning the
    ///   writer's handle on the shared mailbox.
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
                Some(WriterMessage::Blocking { state, path, reply }) => {
                    writer_state.handle_blocking(&state, &path, &reply);
                }
                Some(WriterMessage::MailboxFilled) => {}
                None => writer_state.flush_mailbox(),
            }
        }
        writer_state.flush_mailbox();
        log::debug!("State writer thread exiting (no more senders)");
    }

    /// Enqueue a snapshot and block the caller until the writer has persisted it.
    ///
    /// ### Description
    /// Guarantees FIFO ordering with all other save requests: any save started
    /// before this call completes before this one begins, and any save issued
    /// after this call returns sees the result of this save on disk. Blocking
    /// saves are never throttled, and they supersede any asynchronous save still
    /// held back for the same path, which is by construction older.
    ///
    /// A snapshot identical to the last one written to `path` is reported as
    /// persisted without touching the file, since the file already holds it.
    ///
    /// ### Arguments
    /// - `state`: The fully-assembled windows state snapshot to persist.
    /// - `path`: Destination file path (typically the user config `state.json`).
    ///
    /// ### Errors
    /// Returns an error if the writer thread has exited, if the reply channel is
    /// dropped, or if the underlying write reports an I/O or serialization error.
    ///
    /// ### Returns
    /// - `Ok(())`: The writer successfully persisted the snapshot.
    /// - `Err(anyhow::Error)`: The writer reported an I/O or serialization error,
    ///   or the writer thread has exited before the request could be processed.
    pub fn save_blocking(&self, state: WindowsState, path: PathBuf) -> anyhow::Result<()> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.sender
            .send(WriterMessage::Blocking {
                state,
                path,
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
    /// - `path`: Destination file path (typically the user config `state.json`).
    pub fn save_async(&self, state: WindowsState, path: PathBuf) {
        self.mailbox.put(state, path);
        match self.sender.try_send(WriterMessage::MailboxFilled) {
            Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.mailbox.drain();
                log::error!("State writer thread has exited; dropped async save request");
            }
        }
    }
}

impl Default for StateWriter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::persistence::{
        SerializedWindowBounds, TabContent, TabState, WindowState, WindowsState,
    };
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use tempfile::tempdir;

    fn sample_state(label: &str) -> WindowsState {
        WindowsState {
            windows: vec![WindowState {
                tabs: vec![TabState {
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

    #[test]
    fn writer_persists_snapshot_to_path() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let writer = StateWriter::new();
        writer
            .save_blocking(sample_state("solo"), path.clone())
            .unwrap();
        let reloaded = WindowsState::load_from_path(&path).unwrap();
        assert_eq!(reloaded.windows.len(), 1);
        assert_eq!(reloaded.windows[0].tabs[0].title, "solo");
    }

    #[test]
    fn writer_async_save_persists_snapshot() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let writer = StateWriter::new();
        writer.save_async(sample_state("async"), path.clone());
        // No reply channel is returned, so wait for a subsequent blocking save to
        // flush the FIFO queue and guarantee the async write has landed.
        writer
            .save_blocking(sample_state("flush"), dir.path().join("flush.json"))
            .unwrap();
        let reloaded = WindowsState::load_from_path(&path).unwrap();
        assert_eq!(reloaded.windows[0].tabs[0].title, "async");
    }

    #[test]
    fn writer_flattens_rope_backed_content() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let writer = StateWriter::new();
        let mut state = sample_state("rope");
        let text = "ünsaved\ttext with \"quotes\"\nand a newline";
        state.windows[0].tabs[0].content = Some(TabContent::Rope(ropey::Rope::from_str(text)));
        writer.save_blocking(state, path.clone()).unwrap();
        let reloaded = WindowsState::load_from_path(&path).unwrap();
        assert_eq!(reloaded.windows[0].tabs[0].content.as_ref().unwrap(), text);
    }

    /// Throttle long enough that no held-back save can escape on its own timer,
    /// so the coalescing tests below stay deterministic on slow machines.
    const NEVER_ELAPSES: Duration = Duration::from_mins(1);

    /// Read back the title of the single tab persisted at `path`.
    fn persisted_title(path: &Path) -> String {
        WindowsState::load_from_path(&path.to_path_buf())
            .unwrap()
            .windows[0]
            .tabs[0]
            .title
            .clone()
    }

    #[test]
    fn writer_skips_write_when_snapshot_is_unchanged() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let backup = crate::fulgur::utils::atomic_write::backup_path_for(&path);
        let writer = StateWriter::new();
        writer
            .save_blocking(sample_state("same"), path.clone())
            .unwrap();
        writer
            .save_blocking(sample_state("same"), path.clone())
            .unwrap();
        assert_eq!(persisted_title(&path), "same");
        assert!(
            !backup.exists(),
            "an unchanged snapshot must not be rewritten, so no backup is produced"
        );
    }

    #[test]
    fn writer_coalesces_a_burst_of_async_saves() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let backup = crate::fulgur::utils::atomic_write::backup_path_for(&path);
        let writer = StateWriter::with_throttle(NEVER_ELAPSES);
        // Seed the throttle so no snapshot of the burst can escape on the
        // leading edge, which makes the number of writes deterministic.
        writer
            .save_blocking(sample_state("seed"), path.clone())
            .unwrap();
        for i in 0..10 {
            writer.save_async(sample_state(&format!("burst-{i}")), path.clone());
        }
        // A blocking save to another path flushes whatever is still held back.
        writer
            .save_blocking(sample_state("flush"), dir.path().join("flush.json"))
            .unwrap();
        assert_eq!(persisted_title(&path), "burst-9");
        assert_eq!(
            persisted_title(&backup),
            "seed",
            "the burst must collapse into a single write"
        );
    }

    #[test]
    fn writer_holds_at_most_one_snapshot_per_path() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let writer = StateWriter::with_throttle(NEVER_ELAPSES);
        writer
            .save_blocking(sample_state("seed"), path.clone())
            .unwrap();
        let burst = CHANNEL_CAPACITY * 4;
        for i in 0..burst {
            writer.save_async(sample_state(&format!("burst-{i}")), path.clone());
        }
        let held = writer.mailbox.drain();
        assert_eq!(
            held.len(),
            1,
            "a burst must not queue one snapshot per request"
        );
        assert_eq!(
            held[0].state.windows[0].tabs[0].title,
            format!("burst-{}", burst - 1),
            "the surviving snapshot must be the newest one"
        );
    }

    #[test]
    fn mailbox_keeps_one_entry_per_destination_in_submission_order() {
        let mailbox = SnapshotMailbox::default();
        assert!(!mailbox.has_pending());
        mailbox.put(sample_state("first-a"), PathBuf::from("a.json"));
        mailbox.put(sample_state("b"), PathBuf::from("b.json"));
        mailbox.put(sample_state("second-a"), PathBuf::from("a.json"));
        assert!(mailbox.has_pending());
        let drained = mailbox.drain();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].state.windows[0].tabs[0].title, "second-a");
        assert_eq!(drained[1].state.windows[0].tabs[0].title, "b");
        assert!(!mailbox.has_pending());
    }

    #[test]
    fn mailbox_discards_only_the_named_destination() {
        let mailbox = SnapshotMailbox::default();
        mailbox.put(sample_state("a"), PathBuf::from("a.json"));
        mailbox.put(sample_state("b"), PathBuf::from("b.json"));
        mailbox.discard(Path::new("a.json"));
        let drained = mailbox.drain();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].state.windows[0].tabs[0].title, "b");
    }

    #[test]
    fn writer_throttled_async_save_lands_once_the_period_elapses() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let writer = StateWriter::with_throttle(Duration::from_millis(500));
        writer
            .save_blocking(sample_state("first"), path.clone())
            .unwrap();
        writer.save_async(sample_state("second"), path.clone());
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
        let path = dir.path().join("state.json");
        let backup = crate::fulgur::utils::atomic_write::backup_path_for(&path);
        let writer = StateWriter::with_throttle(NEVER_ELAPSES);
        writer
            .save_blocking(sample_state("first"), path.clone())
            .unwrap();
        writer.save_async(sample_state("superseded"), path.clone());
        writer
            .save_blocking(sample_state("last"), path.clone())
            .unwrap();
        assert_eq!(persisted_title(&path), "last");
        assert_eq!(
            persisted_title(&backup),
            "first",
            "the superseded snapshot must never reach the disk"
        );
    }

    #[test]
    fn writer_flushes_a_held_back_save_on_shutdown() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let writer = StateWriter::with_throttle(NEVER_ELAPSES);
        writer
            .save_blocking(sample_state("first"), path.clone())
            .unwrap();
        writer.save_async(sample_state("held-back"), path.clone());
        drop(writer);
        assert_eq!(persisted_title(&path), "held-back");
    }

    #[test]
    fn writer_serializes_concurrent_save_requests() {
        let dir = tempdir().unwrap();
        let writer = Arc::new(StateWriter::new());
        let mut handles = Vec::new();
        for i in 0..16 {
            let writer = Arc::clone(&writer);
            let path = dir.path().join(format!("state-{i}.json"));
            let label = format!("thread-{i}");
            handles.push(thread::spawn(move || {
                writer.save_blocking(sample_state(&label), path)
            }));
        }
        for h in handles {
            assert!(h.join().unwrap().is_ok());
        }
        for i in 0..16 {
            let path = dir.path().join(format!("state-{i}.json"));
            let reloaded = WindowsState::load_from_path(&path).unwrap();
            assert_eq!(reloaded.windows[0].tabs[0].title, format!("thread-{i}"));
        }
    }

    #[test]
    fn writer_concurrent_writes_to_same_path_produce_a_valid_final_state() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let writer = Arc::new(StateWriter::new());
        let mut handles = Vec::new();
        for i in 0..32 {
            let writer = Arc::clone(&writer);
            let path = path.clone();
            let label = format!("contender-{i}");
            handles.push(thread::spawn(move || {
                writer.save_blocking(sample_state(&label), path)
            }));
        }
        for h in handles {
            assert!(h.join().unwrap().is_ok());
        }
        // The file must be parseable, no torn write, no interleaved JSON.
        let reloaded = WindowsState::load_from_path(&path).unwrap();
        assert_eq!(reloaded.windows.len(), 1);
        assert!(reloaded.windows[0].tabs[0].title.starts_with("contender-"));
    }
}
