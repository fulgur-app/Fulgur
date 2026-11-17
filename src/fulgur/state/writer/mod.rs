mod mailbox;
mod writer_state;

use super::db::StateDb;
use super::persistence::{DeviceKeyPins, RemoteHistory, WindowsState};
use crate::fulgur::utils::worker::Worker;
use mailbox::SnapshotMailbox;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use writer_state::WriterState;

/// Bounded channel capacity for writer messages.
///
/// Asynchronous snapshots and remote histories never travel through this
/// channel: they are coalesced in the `SnapshotMailbox`, so the queue only ever
/// holds blocking requests,
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
    /// Signals that the mailbox now holds a snapshot, a remote history or
    /// device key pins waiting to be written.
    MailboxFilled,
}

/// Dedicated background writer that serializes all session-state persistence.
pub struct StateWriter {
    sender: mpsc::SyncSender<WriterMessage>,
    mailbox: Arc<SnapshotMailbox>,
    persistence_error: Option<String>,
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
        let persistence_error = db.as_ref().map_or_else(
            || Some("no state database is available".to_string()),
            |db| db.fallback_error().map(str::to_owned),
        );
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
            persistence_error,
            _worker: worker,
        }
    }

    /// Explain why session snapshots cannot be persisted durably.
    ///
    /// ### Returns
    /// - `Some(&str)`: The startup/storage failure preventing durable saves
    /// - `None`: Session snapshots are written to durable storage
    #[must_use]
    pub fn persistence_error(&self) -> Option<&str> {
        self.persistence_error.as_deref()
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
            writer_state.flush_remote_history();
            writer_state.flush_device_key_pins();
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
        if !self.wake_writer() {
            self.mailbox.take();
            log::error!("State writer thread has exited; dropped async save request");
        }
    }

    /// Hand the remote URL history to the writer without waiting for the write.
    ///
    /// ### Arguments
    /// - `history`: The complete history to persist.
    pub fn save_remote_history(&self, history: RemoteHistory) {
        self.mailbox.put_remote_history(history);
        if !self.wake_writer() {
            self.mailbox.take_remote_history();
            log::error!("State writer thread has exited; dropped remote history save request");
        }
    }

    /// Hand the pinned device public keys to the writer without waiting for the write.
    ///
    /// ### Arguments
    /// - `pins`: The complete set of pins to persist.
    pub fn save_device_key_pins(&self, pins: DeviceKeyPins) {
        self.mailbox.put_device_key_pins(pins);
        if !self.wake_writer() {
            self.mailbox.take_device_key_pins();
            log::error!("State writer thread has exited; dropped device key pins save request");
        }
    }

    /// Wake the writer thread so it drains the mailbox.
    ///
    /// ### Returns
    /// - `bool`: `true` when the writer will drain the mailbox, `false` when the
    ///   writer thread has exited
    fn wake_writer(&self) -> bool {
        match self.sender.try_send(WriterMessage::MailboxFilled) {
            Ok(()) | Err(mpsc::TrySendError::Full(_)) => true,
            Err(mpsc::TrySendError::Disconnected(_)) => false,
        }
    }
}

#[cfg(test)]
mod tests;
