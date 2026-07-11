//! RAII ownership for long-lived background worker threads.
//!
//! Blocking workers (SSE streams, the state writer, the IPC listener) cannot be
//! cancelled by dropping a future, so their lifecycle used to rely on every
//! exit path remembering to set the right `AtomicBool`. `Worker` makes the
//! lifecycle structural: the struct owns the shutdown signal and the thread's
//! `JoinHandle`, and dropping it signals the worker, runs an optional wakeup to
//! unblock a blocked read or accept, and joins with a bounded timeout. Storing
//! the `Worker` in the state that owns the work guarantees that killing the
//! owner kills the worker.

use parking_lot::Mutex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

/// Granularity of the poll loop waiting for a worker thread to finish.
const JOIN_POLL_SLICE: Duration = Duration::from_millis(50);

/// Shared slot holding a worker thread's `JoinHandle`.
///
/// The slot is shared so code that spawns the thread asynchronously (for
/// example the SSE connection, spawned from an orchestration thread after the
/// initial synchronization succeeds) can attach the handle after the `Worker`
/// was created and stored in its owner.
pub type WorkerHandleSlot = Arc<Mutex<Option<thread::JoinHandle<()>>>>;

/// Cheap cloneable handles connecting a thread spawned elsewhere to its `Worker`.
///
/// The thread body polls `shutdown_flag`; the code that spawns the thread
/// stores the resulting `JoinHandle` into `handle_slot` so the owning `Worker`
/// can join it on `Drop`.
#[derive(Clone)]
pub struct WorkerHooks {
    /// Flag the worker thread must poll to notice a shutdown request.
    pub shutdown_flag: Arc<AtomicBool>,
    /// Slot the spawned thread's `JoinHandle` must be stored into.
    pub handle_slot: WorkerHandleSlot,
}

/// Drop-owned lifecycle handle for a long-lived background thread.
///
/// The worker body must poll the shutdown flag (or exit when its input channel
/// closes) so the signal-then-join performed by `Drop` terminates promptly. If
/// the thread does not finish within the join timeout, `Drop` logs a warning
/// and detaches instead of blocking forever.
pub struct Worker {
    name: String,
    shutdown_flag: Arc<AtomicBool>,
    handle_slot: WorkerHandleSlot,
    wakeup: Option<Box<dyn Fn() + Send + Sync>>,
    join_timeout: Duration,
}

impl Worker {
    /// Create a worker whose thread will be attached later through the handle slot.
    ///
    /// ### Arguments
    /// - `name`: Label used in lifecycle log messages.
    /// - `join_timeout`: Maximum time `Drop` waits for the thread to finish.
    ///
    /// ### Returns
    /// - `Self`: A worker with a live shutdown flag and an empty handle slot.
    #[must_use]
    pub fn new(name: impl Into<String>, join_timeout: Duration) -> Self {
        Self {
            name: name.into(),
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            handle_slot: Arc::new(Mutex::new(None)),
            wakeup: None,
            join_timeout,
        }
    }

    /// Create a worker and spawn its thread immediately.
    ///
    /// ### Arguments
    /// - `name`: Label used for the thread name and lifecycle log messages.
    /// - `join_timeout`: Maximum time `Drop` waits for the thread to finish.
    /// - `body`: Thread body; receives the shutdown flag it must poll.
    ///
    /// ### Panics
    /// Panics if the OS refuses to spawn the thread.
    ///
    /// ### Returns
    /// - `Self`: A worker owning the spawned thread.
    #[must_use]
    pub fn spawn<F>(name: impl Into<String>, join_timeout: Duration, body: F) -> Self
    where
        F: FnOnce(Arc<AtomicBool>) + Send + 'static,
    {
        let worker = Self::new(name, join_timeout);
        let flag = Arc::clone(&worker.shutdown_flag);
        let handle = thread::Builder::new()
            .name(worker.name.clone())
            .spawn(move || body(flag))
            .unwrap_or_else(|e| panic!("failed to spawn worker thread '{e}'"));
        *worker.handle_slot.lock() = Some(handle);
        worker
    }

    /// Install a wakeup callback invoked after the shutdown flag is set.
    ///
    /// ### Description
    /// Used when the worker can block outside flag polls (for example a TCP
    /// `accept`), so signaling alone would not unblock it. The callback runs on
    /// `signal_shutdown` and on `Drop`.
    ///
    /// ### Arguments
    /// - `wakeup`: Callback that unblocks the worker's blocking call.
    ///
    /// ### Returns
    /// - `Self`: The worker with the wakeup installed.
    #[must_use]
    pub fn with_wakeup(mut self, wakeup: impl Fn() + Send + Sync + 'static) -> Self {
        self.wakeup = Some(Box::new(wakeup));
        self
    }

    /// Get cheap cloneable hooks for wiring a thread spawned elsewhere to this worker.
    ///
    /// ### Returns
    /// - `WorkerHooks`: The shutdown flag the thread must poll and the slot its
    ///   `JoinHandle` must be stored into.
    #[must_use]
    pub fn hooks(&self) -> WorkerHooks {
        WorkerHooks {
            shutdown_flag: Arc::clone(&self.shutdown_flag),
            handle_slot: Arc::clone(&self.handle_slot),
        }
    }

    /// Signal the worker to stop without waiting for it to finish.
    ///
    /// ### Description
    /// Useful to request shutdown early (for example on the UI thread) while
    /// deferring the bounded join to a later `Drop` on a background thread.
    pub fn signal_shutdown(&self) {
        self.shutdown_flag.store(true, Ordering::Relaxed);
        if let Some(wakeup) = &self.wakeup {
            wakeup();
        }
    }
}

impl Drop for Worker {
    /// Signal the worker, run the wakeup, and join with a bounded timeout.
    fn drop(&mut self) {
        self.signal_shutdown();
        let Some(handle) = self.handle_slot.lock().take() else {
            return;
        };
        let deadline = Instant::now() + self.join_timeout;
        while !handle.is_finished() && Instant::now() < deadline {
            thread::sleep(JOIN_POLL_SLICE);
        }
        if handle.is_finished() {
            if handle.join().is_err() {
                log::warn!("Worker '{}' panicked before shutdown", self.name);
            } else {
                log::debug!("Worker '{}' stopped", self.name);
            }
        } else {
            log::warn!(
                "Worker '{}' still running after {:?}, detaching",
                self.name,
                self.join_timeout
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spawn a worker that loops on its shutdown flag and record when it exits.
    fn spawn_polling_worker(join_timeout: Duration) -> (Worker, Arc<AtomicBool>) {
        let exited = Arc::new(AtomicBool::new(false));
        let exited_clone = Arc::clone(&exited);
        let worker = Worker::spawn("test-worker", join_timeout, move |shutdown| {
            while !shutdown.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(5));
            }
            exited_clone.store(true, Ordering::Release);
        });
        (worker, exited)
    }

    #[test]
    fn drop_signals_and_joins_the_thread() {
        let (worker, exited) = spawn_polling_worker(Duration::from_secs(2));
        drop(worker);
        assert!(exited.load(Ordering::Acquire), "drop must join the worker");
    }

    #[test]
    fn drop_detaches_when_the_thread_outlives_the_timeout() {
        let worker = Worker::spawn(
            "stuck-worker",
            Duration::from_millis(50),
            move |_shutdown| {
                thread::sleep(Duration::from_millis(500));
            },
        );
        let start = Instant::now();
        drop(worker);
        assert!(
            start.elapsed() < Duration::from_millis(400),
            "drop must give up after the join timeout instead of blocking"
        );
    }

    #[test]
    fn drop_joins_a_handle_attached_through_the_slot() {
        let worker = Worker::new("deferred-worker", Duration::from_secs(2));
        let hooks = worker.hooks();
        let exited = Arc::new(AtomicBool::new(false));
        let exited_clone = Arc::clone(&exited);
        let flag = Arc::clone(&hooks.shutdown_flag);
        let handle = thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(5));
            }
            exited_clone.store(true, Ordering::Release);
        });
        *hooks.handle_slot.lock() = Some(handle);
        drop(worker);
        assert!(
            exited.load(Ordering::Acquire),
            "drop must join a late-attached handle"
        );
    }

    #[test]
    fn drop_with_an_empty_slot_only_signals() {
        let worker = Worker::new("never-spawned", Duration::from_secs(2));
        let flag = worker.hooks().shutdown_flag;
        drop(worker);
        assert!(
            flag.load(Ordering::Relaxed),
            "drop must still set the shutdown flag"
        );
    }

    #[test]
    fn signal_shutdown_runs_the_wakeup() {
        let woken = Arc::new(AtomicBool::new(false));
        let woken_clone = Arc::clone(&woken);
        let worker = Worker::new("wakeup-worker", Duration::from_secs(2))
            .with_wakeup(move || woken_clone.store(true, Ordering::Release));
        worker.signal_shutdown();
        assert!(woken.load(Ordering::Acquire), "wakeup must run on signal");
    }
}
