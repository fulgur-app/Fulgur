use crate::fulgur::state_persistence::WindowsState;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

/// Bounded channel capacity for pending save requests.
///
/// Kept small because writes are serialized through a single thread and blocking
/// callers typically wait on the reply before issuing the next save. A full queue
/// indicates an abnormal backlog and makes callers back off naturally.
const CHANNEL_CAPACITY: usize = 16;

/// A request sent from a UI thread to the writer thread: a serialized snapshot,
/// the destination path, and a reply channel the writer uses to report the I/O
/// result.
struct WriteRequest {
    state: WindowsState,
    path: PathBuf,
    reply: mpsc::Sender<anyhow::Result<()>>,
}

/// Dedicated background writer that serializes all `WindowsState` persistence.
pub struct StateWriter {
    sender: mpsc::SyncSender<WriteRequest>,
}

impl StateWriter {
    /// Spawn the writer thread and return a handle for submitting save requests.
    ///
    /// ### Returns
    /// - `Self`: A writer handle whose `save_blocking` method dispatches work to
    ///   the worker thread.
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::sync_channel::<WriteRequest>(CHANNEL_CAPACITY);
        thread::Builder::new()
            .name("fulgur-state-writer".to_string())
            .spawn(move || Self::run(&receiver))
            .expect("failed to spawn state writer thread");
        Self { sender }
    }

    /// Worker-thread loop that processes save requests one at a time.
    ///
    /// ### Arguments
    /// - `receiver`: Channel of pending write requests.
    fn run(receiver: &mpsc::Receiver<WriteRequest>) {
        while let Ok(req) = receiver.recv() {
            let result = req.state.save_to_path(&req.path);
            if let Err(ref e) = result {
                log::error!("State writer failed to save state: {e}");
            }
            if req.reply.send(result).is_err() {
                log::warn!("State writer reply channel dropped before result was read");
            }
        }
        log::debug!("State writer thread exiting (no more senders)");
    }

    /// Enqueue a snapshot and block the caller until the writer has persisted it.
    ///
    /// ### Description
    /// Guarantees FIFO ordering with all other save requests: any save started
    /// before this call completes before this one begins, and any save issued
    /// after this call returns sees the result of this save on disk.
    ///
    /// ### Arguments
    /// - `state`: The fully-assembled windows state snapshot to persist.
    /// - `path`: Destination file path (typically the user config `state.json`).
    ///
    /// ### Returns
    /// - `Ok(())`: The writer successfully persisted the snapshot.
    /// - `Err(anyhow::Error)`: The writer reported an I/O or serialization error,
    ///   or the writer thread has exited before the request could be processed.
    pub fn save_blocking(&self, state: WindowsState, path: PathBuf) -> anyhow::Result<()> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.sender
            .send(WriteRequest {
                state,
                path,
                reply: reply_tx,
            })
            .map_err(|_| anyhow::anyhow!("state writer thread has exited"))?;
        reply_rx
            .recv()
            .map_err(|_| anyhow::anyhow!("state writer reply channel closed before result"))?
    }
}

impl Default for StateWriter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fulgur::state_persistence::{
        SerializedWindowBounds, TabState, WindowState, WindowsState,
    };
    use std::sync::Arc;
    use std::thread;
    use tempfile::tempdir;

    fn sample_state(label: &str) -> WindowsState {
        WindowsState {
            windows: vec![WindowState {
                tabs: vec![TabState {
                    title: label.to_string(),
                    file_path: None,
                    content: Some(label.to_string()),
                    last_saved: None,
                    remote: None,
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
