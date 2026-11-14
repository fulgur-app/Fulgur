mod disk_signature;
mod notify_events;

use disk_signature::DiskSignature;
use futures::channel::mpsc::{Receiver, Sender, channel};
use notify::{Error as NotifyError, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Bound for the file watch event channel.
const FILE_WATCH_EVENT_CHANNEL_CAPACITY: usize = 256;

/// Maximum time a Linux inotify rename `From` event waits for its matching `To` before it is treated as a deletion.
pub(super) const PENDING_RENAME_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone)]
pub enum FileWatchEvent {
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
    Error(String),
}

pub struct FileWatcher {
    watcher: Option<RecommendedWatcher>,
    /// Watched paths with the last disk signature the editor knows about.
    watched_paths: HashMap<PathBuf, Option<DiskSignature>>,
    pub(super) event_tx: Sender<FileWatchEvent>,
    /// Pending rename source path for Linux inotify, which splits rename events
    /// into separate From and To notifications rather than a single two-path event.
    pub(super) pending_rename_from: Arc<Mutex<Option<(PathBuf, Instant)>>>,
}

impl FileWatcher {
    /// Creates a new file watcher
    ///
    /// ### Returns
    /// - `(FileWatcher, Receiver<FileWatchEvent>)`: A tuple containing the file watcher and the event receiver
    ///   - `FileWatcher`: The file watcher instance
    ///   - `Receiver<FileWatchEvent>`: The event receiver to receive the file watch events from the file watcher
    #[must_use]
    pub fn new() -> (Self, Receiver<FileWatchEvent>) {
        let (event_tx, event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let watcher = Self {
            watcher: None,
            watched_paths: HashMap::new(),
            event_tx,
            pending_rename_from: Arc::new(Mutex::new(None)),
        };
        (watcher, event_rx)
    }

    /// Starts the file watcher
    ///
    /// ### Errors
    /// - Returns a `NotifyError` if the underlying file system watcher could not be created.
    ///
    /// ### Returns
    /// - `Ok(())`: If the file watcher was started successfully
    /// - `Err(NotifyError)`: If the file watcher could not be started
    pub fn start(&mut self) -> Result<(), NotifyError> {
        if self.watcher.is_some() {
            return Ok(());
        }
        let mut event_tx = self.event_tx.clone();
        let pending_rename_from = Arc::clone(&self.pending_rename_from);
        let watcher =
            notify::recommended_watcher(move |res: Result<Event, NotifyError>| match res {
                Ok(event) => {
                    Self::handle_notify_event(event, &mut event_tx, &pending_rename_from);
                }
                Err(e) => {
                    Self::send_event(&mut event_tx, FileWatchEvent::Error(e.to_string()));
                }
            })?;
        self.watcher = Some(watcher);
        Ok(())
    }

    /// Send a file watch event over the bounded channel, dropping it if the channel is full.
    ///
    /// ### Arguments
    /// - `event_tx`: The bounded event sender
    /// - `event`: The file watch event to send
    fn send_event(event_tx: &mut Sender<FileWatchEvent>, event: FileWatchEvent) {
        if let Err(e) = event_tx.try_send(event)
            && e.is_full()
        {
            log::warn!(
                "File watch event channel full, dropping event: {:?}",
                e.into_inner()
            );
        }
    }

    /// Starts watching a file
    ///
    /// ### Arguments
    /// - `path`: The path to the file to watch
    ///
    /// ### Errors
    /// - Returns an error if the watcher cannot be started, the file metadata cannot be read,
    ///   or the watcher fails to register the path.
    ///
    /// ### Returns
    /// - `Ok(())`: If the file was watched successfully
    /// - `Err(String)`: If the file could not be watched
    pub fn watch_file(&mut self, path: &PathBuf) -> Result<(), String> {
        if self.watched_paths.contains_key(path) {
            return Ok(());
        }
        if self.watcher.is_none() {
            self.start().map_err(|e| e.to_string())?;
        }
        let signature = DiskSignature::read(path);
        if let Some(watcher) = &mut self.watcher {
            watcher
                .watch(path, RecursiveMode::NonRecursive)
                .map_err(|e| format!("Failed to watch file {}: {}", path.display(), e))?;
        }
        self.watched_paths.insert(path.clone(), signature);
        log::debug!("Started watching file: {}", path.display());
        Ok(())
    }

    /// Re-register the OS watch of a watched file whose inode may have been
    /// replaced (atomic save by another program), keeping its known disk signature
    ///
    /// ### Arguments
    /// - `path`: The watched file to re-register
    ///
    /// ### Errors
    /// - Returns an error if the watcher fails to register the path again.
    ///
    /// ### Returns
    /// - `Ok(())`: The path is watched again, or it is not tracked by this watcher
    /// - `Err(String)`: The path could not be watched again
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub fn rewatch_file(&mut self, path: &Path) -> Result<(), String> {
        if !self.watched_paths.contains_key(path) {
            return Ok(());
        }
        let Some(watcher) = &mut self.watcher else {
            return Ok(());
        };
        // The watch on the replaced inode may already be gone, so a failed unwatch is expected.
        let _ = watcher.unwatch(path);
        watcher
            .watch(path, RecursiveMode::NonRecursive)
            .map_err(|e| format!("Failed to watch file {} again: {}", path.display(), e))
    }

    /// Stops watching a file
    ///
    /// ### Arguments
    /// - `path`: The path to the file to stop watching
    pub fn unwatch_file(&mut self, path: &PathBuf) {
        if !self.watched_paths.contains_key(path) {
            return;
        }
        if let Some(watcher) = &mut self.watcher
            && let Err(e) = watcher.unwatch(path)
        {
            log::warn!("Failed to unwatch file {}: {}", path.display(), e);
        }

        self.watched_paths.remove(path);
        log::debug!("Stopped watching file: {}", path.display());
    }

    /// Stops the file watcher completely
    pub fn stop(&mut self) {
        if let Some(mut watcher) = self.watcher.take() {
            for path in self.watched_paths.keys() {
                let _ = watcher.unwatch(path);
            }
        }
        self.watched_paths.clear();
        log::debug!("File watcher stopped");
    }
}

impl Drop for FileWatcher {
    /// Stops the file watcher when the `FileWatcher` instance is dropped
    fn drop(&mut self) {
        self.stop();
    }
}
