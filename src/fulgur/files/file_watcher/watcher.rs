use futures::channel::mpsc::{Receiver, Sender, channel};
use notify::{Error as NotifyError, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

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

/// Size and modification time of a file on disk, used to tell real content
/// changes apart from metadata-only or self-inflicted watch events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DiskSignature {
    modified: SystemTime,
    len: u64,
}

impl DiskSignature {
    /// Read the current signature of a file
    ///
    /// ### Arguments
    /// - `path`: The file to inspect
    ///
    /// ### Returns
    /// - `Some(DiskSignature)`: The file's current modification time and size
    /// - `None`: The file is missing or its metadata cannot be read
    fn read(path: &Path) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        Some(Self {
            modified: metadata.modified().ok()?,
            len: metadata.len(),
        })
    }
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

    /// Expire a pending rename `From` that has waited longer than `PENDING_RENAME_TIMEOUT`
    /// for its matching `To`, emitting it as a `Deleted` event.
    ///
    /// ### Arguments
    /// - `pending_rename_from`: Accumulator holding the `From` path and the instant it was stored
    /// - `event_tx`: The event sender used to emit the synthesized `Deleted` event
    pub(super) fn expire_pending_rename_from(
        pending_rename_from: &Mutex<Option<(PathBuf, Instant)>>,
        event_tx: &mut Sender<FileWatchEvent>,
    ) {
        let stale = pending_rename_from.lock().ok().and_then(|mut pending| {
            let is_stale = pending
                .as_ref()
                .is_some_and(|(_, stored_at)| stored_at.elapsed() >= PENDING_RENAME_TIMEOUT);
            if is_stale { pending.take() } else { None }
        });
        if let Some((path, _)) = stale {
            Self::send_event(event_tx, FileWatchEvent::Deleted(path));
        }
    }

    /// Flush a stale pending rename `From` if it has exceeded `PENDING_RENAME_TIMEOUT`.
    pub fn flush_expired_pending_rename(&mut self) {
        Self::expire_pending_rename_from(&self.pending_rename_from, &mut self.event_tx);
    }

    /// Handles a notify event and converts it to a `FileWatchEvent`
    ///
    /// ### Description
    /// - If the event is a modification, it sends a Modified event to the event sender
    /// - If the event is a deletion, it sends a Deleted event to the event sender
    /// - If the event is a creation after a rename or save, it sends a Modified event to the event sender
    /// - If the event is a metadata-only change (extended attributes, permissions, timestamps), it ignores it
    /// - If the event is other, it ignores it
    /// - Ignore other event types (access, etc.)
    ///
    /// Rename events come in three shapes depending on the OS backend:
    /// - **Windows**: A single event with two paths (`RenameMode::Both`)
    /// - **macOS**: A single-path `RenameMode::Any` event for the source only, since
    ///   `FSEvents` cannot pair the two sides. This also covers a move or delete-to-Trash,
    ///   so it is surfaced as a `Deleted` for the source path.
    /// - **Linux inotify**: Two consecutive single-path events (`RenameMode::From` then
    ///   `RenameMode::To`). The `pending_rename_from` accumulator pairs them up.
    ///
    /// ### Arguments
    /// - `event`: The notify event to handle
    /// - `event_tx`: The event sender to send the events to
    /// - `pending_rename_from`: Accumulator for the Linux split-rename `From` path
    fn handle_notify_event(
        event: Event,
        event_tx: &mut Sender<FileWatchEvent>,
        pending_rename_from: &Mutex<Option<(PathBuf, Instant)>>,
    ) {
        use notify::event::{ModifyKind, RenameMode};

        // Any incoming event is a chance to flush a From whose matching To never came.
        Self::expire_pending_rename_from(pending_rename_from, event_tx);

        match event.kind {
            EventKind::Modify(ModifyKind::Name(rename_mode)) => {
                if event.paths.len() == 2 {
                    // macOS / Windows: both paths arrive in one event
                    let from = event.paths[0].clone();
                    let to = event.paths[1].clone();
                    Self::send_event(event_tx, FileWatchEvent::Renamed { from, to });
                } else if event.paths.len() == 1 {
                    match rename_mode {
                        RenameMode::From => {
                            // Linux inotify: first half - store and wait for the To event
                            if let Ok(mut pending) = pending_rename_from.lock() {
                                *pending = Some((event.paths[0].clone(), Instant::now()));
                            }
                        }
                        RenameMode::To => {
                            // Linux inotify: second half - pair with the stored From path
                            let from = pending_rename_from
                                .lock()
                                .ok()
                                .and_then(|mut p| p.take())
                                .map(|(path, _)| path);
                            match from {
                                Some(from) => {
                                    Self::send_event(
                                        event_tx,
                                        FileWatchEvent::Renamed {
                                            from,
                                            to: event.paths[0].clone(),
                                        },
                                    );
                                }
                                None => {
                                    // No matching From; treat as a new file appearing
                                    Self::send_event(
                                        event_tx,
                                        FileWatchEvent::Modified(event.paths[0].clone()),
                                    );
                                }
                            }
                        }
                        _ => {
                            // Orphaned or unrecognised single-path rename; flush any pending From
                            if let Ok(mut pending) = pending_rename_from.lock()
                                && let Some((stale, _)) = pending.take()
                            {
                                Self::send_event(event_tx, FileWatchEvent::Deleted(stale));
                            }
                            // macOS FSEvents reports a move, rename, or delete-to-Trash of a
                            // watched file as a single-path `RenameMode::Any` with no destination.
                            if rename_mode == RenameMode::Any {
                                Self::send_event(
                                    event_tx,
                                    FileWatchEvent::Deleted(event.paths[0].clone()),
                                );
                            }
                        }
                    }
                }
            }
            // macOS sets extended attributes such as `com.apple.provenance` on files
            // some time after they are written, which must not count as an edit.
            EventKind::Modify(ModifyKind::Metadata(_)) => {}
            EventKind::Modify(_) | EventKind::Create(_) => {
                for path in event.paths {
                    Self::send_event(event_tx, FileWatchEvent::Modified(path));
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    Self::send_event(event_tx, FileWatchEvent::Deleted(path));
                }
            }
            _ => {}
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

    /// Track a file's disk signature without registering it with the OS watcher,
    /// so tests can exercise echo suppression without a live notify thread waking
    /// tasks on gpui's deterministic test scheduler.
    ///
    /// ### Arguments
    /// - `path`: The file whose current signature becomes the known state
    #[cfg(all(test, feature = "gpui-test-support"))]
    pub(super) fn track_without_os_watch(&mut self, path: &Path) {
        self.watched_paths
            .insert(path.to_path_buf(), DiskSignature::read(path));
    }

    /// Record the current on-disk state of a watched file as known to the editor
    ///
    /// ### Arguments
    /// - `path`: The watched file whose signature should be refreshed
    pub fn refresh_disk_signature(&mut self, path: &Path) {
        if let Some(known) = self.watched_paths.get_mut(path) {
            *known = DiskSignature::read(path);
        }
    }

    /// Check whether a watched file changed on disk since its last known state
    ///
    /// ### Arguments
    /// - `path`: The file a watch event refers to
    ///
    /// ### Returns
    /// - `true`: The file changed, is missing, or is not watched
    /// - `false`: The file's size and modification time match the known state
    pub fn acknowledge_disk_change(&mut self, path: &Path) -> bool {
        let current = DiskSignature::read(path);
        let Some(known) = self.watched_paths.get_mut(path) else {
            return true;
        };
        if current.is_some() && *known == current {
            return false;
        }
        *known = current;
        true
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

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::{FILE_WATCH_EVENT_CHANNEL_CAPACITY, FileWatchEvent, FileWatcher};
    use crate::fulgur::files::file_watcher::test_helpers::temp_test_path;
    use futures::channel::mpsc::{TryRecvError, channel};
    use gpui_kit::TestAppContext;
    use notify::{
        Event, EventKind,
        event::{DataChange, MetadataKind, ModifyKind, RemoveKind, RenameMode},
    };
    use std::{
        sync::Mutex,
        time::{Duration, Instant},
    };
    use tempfile::TempDir;

    #[gpui_kit::test]
    fn test_handle_notify_event_maps_modify_to_modified(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let pending_rename_from = Mutex::new(None);
        let path = temp_test_path("fulgur_notify_modify.txt");
        let event = Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
            .add_path(path.clone());
        FileWatcher::handle_notify_event(event, &mut event_tx, &pending_rename_from);
        assert!(matches!(
            event_rx.try_recv(),
            Ok(FileWatchEvent::Modified(actual)) if actual == path
        ));
    }

    #[gpui_kit::test]
    fn test_handle_notify_event_maps_remove_to_deleted(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let pending_rename_from = Mutex::new(None);
        let path = temp_test_path("fulgur_notify_deleted.txt");
        let event = Event::new(EventKind::Remove(RemoveKind::File)).add_path(path.clone());
        FileWatcher::handle_notify_event(event, &mut event_tx, &pending_rename_from);
        assert!(matches!(
            event_rx.try_recv(),
            Ok(FileWatchEvent::Deleted(actual)) if actual == path
        ));
    }

    #[gpui_kit::test]
    fn test_handle_notify_event_maps_macos_rename_any_to_deleted(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let pending_rename_from = Mutex::new(None);
        let path = temp_test_path("fulgur_notify_rename_any.txt");
        // macOS FSEvents reports a delete-to-Trash, move, or rename of a watched
        // file as a single-path `RenameMode::Any` with no destination.
        let event =
            Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Any))).add_path(path.clone());
        FileWatcher::handle_notify_event(event, &mut event_tx, &pending_rename_from);
        assert!(
            matches!(event_rx.try_recv(), Ok(FileWatchEvent::Deleted(actual)) if actual == path),
            "a single-path RenameMode::Any should map to a Deleted event for the source path"
        );
    }

    #[gpui_kit::test]
    fn test_handle_notify_event_maps_rename_both_to_renamed(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let pending_rename_from = Mutex::new(None);
        let from = temp_test_path("fulgur_notify_old.txt");
        let to = temp_test_path("fulgur_notify_new.txt");
        let event = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
            .add_path(from.clone())
            .add_path(to.clone());
        FileWatcher::handle_notify_event(event, &mut event_tx, &pending_rename_from);
        assert!(matches!(
            event_rx.try_recv(),
            Ok(FileWatchEvent::Renamed {
                from: actual_from,
                to: actual_to
            }) if actual_from == from && actual_to == to
        ));
    }

    #[gpui_kit::test]
    fn test_handle_notify_event_pairs_linux_split_rename(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let pending_rename_from = Mutex::new(None);
        let from = temp_test_path("fulgur_notify_linux_from.txt");
        let to = temp_test_path("fulgur_notify_linux_to.txt");
        let from_event = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::From)))
            .add_path(from.clone());
        FileWatcher::handle_notify_event(from_event, &mut event_tx, &pending_rename_from);
        assert!(matches!(event_rx.try_recv(), Err(TryRecvError::Empty)));
        let to_event =
            Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::To))).add_path(to.clone());
        FileWatcher::handle_notify_event(to_event, &mut event_tx, &pending_rename_from);
        assert!(matches!(
            event_rx.try_recv(),
            Ok(FileWatchEvent::Renamed {
                from: actual_from,
                to: actual_to
            }) if actual_from == from && actual_to == to
        ));
    }

    #[gpui_kit::test]
    fn test_handle_notify_event_expires_stale_pending_rename(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let stale_from = temp_test_path("fulgur_notify_stale_from.txt");
        let stored_at = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .expect("instant subtraction should not underflow");
        let pending_rename_from = Mutex::new(Some((stale_from.clone(), stored_at)));
        let unrelated = temp_test_path("fulgur_notify_unrelated_modify.txt");
        let event = Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
            .add_path(unrelated.clone());
        FileWatcher::handle_notify_event(event, &mut event_tx, &pending_rename_from);
        assert!(
            matches!(event_rx.try_recv(), Ok(FileWatchEvent::Deleted(actual)) if actual == stale_from),
            "a stale pending From should be flushed as Deleted on the next event"
        );
        assert!(
            matches!(event_rx.try_recv(), Ok(FileWatchEvent::Modified(actual)) if actual == unrelated)
        );
        assert!(
            pending_rename_from.lock().expect("lock poisoned").is_none(),
            "the stale pending From should be cleared after expiry"
        );
    }

    #[gpui_kit::test]
    fn test_flush_expired_pending_rename_emits_deleted(_cx: &mut TestAppContext) {
        let (mut watcher, mut event_rx) = FileWatcher::new();
        let stale_from = temp_test_path("fulgur_flush_stale_from.txt");
        let stored_at = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .expect("instant subtraction should not underflow");
        *watcher.pending_rename_from.lock().expect("lock poisoned") =
            Some((stale_from.clone(), stored_at));
        watcher.flush_expired_pending_rename();
        assert!(
            matches!(event_rx.try_recv(), Ok(FileWatchEvent::Deleted(actual)) if actual == stale_from),
            "flush should expire a never-completed rename From"
        );
        assert!(
            watcher
                .pending_rename_from
                .lock()
                .expect("lock poisoned")
                .is_none()
        );
    }

    #[gpui_kit::test]
    fn test_handle_notify_event_ignores_metadata_only_changes(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let pending_rename_from = Mutex::new(None);
        let path = temp_test_path("fulgur_notify_metadata.txt");
        let event = Event::new(EventKind::Modify(ModifyKind::Metadata(
            MetadataKind::Extended,
        )))
        .add_path(path);
        FileWatcher::handle_notify_event(event, &mut event_tx, &pending_rename_from);
        assert!(
            matches!(event_rx.try_recv(), Err(TryRecvError::Empty)),
            "an extended-attribute change must not be reported as a modification"
        );
    }

    #[gpui_kit::test]
    fn test_acknowledge_disk_change_reports_each_disk_state_once(_cx: &mut TestAppContext) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("acknowledge_signature.txt");
        std::fs::write(&path, "first").expect("failed to write test file");
        let (mut watcher, _event_rx) = FileWatcher::new();
        watcher
            .watch_file(&path)
            .expect("failed to watch test file");
        assert!(
            !watcher.acknowledge_disk_change(&path),
            "a file untouched since it was watched is not a change"
        );
        std::fs::write(&path, "second, longer").expect("failed to rewrite test file");
        assert!(
            watcher.acknowledge_disk_change(&path),
            "a rewritten file is a change"
        );
        assert!(
            !watcher.acknowledge_disk_change(&path),
            "an acknowledged change must not be reported again"
        );
    }

    #[gpui_kit::test]
    fn test_acknowledge_disk_change_treats_unwatched_path_as_changed(_cx: &mut TestAppContext) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("unwatched_signature.txt");
        std::fs::write(&path, "content").expect("failed to write test file");
        let (mut watcher, _event_rx) = FileWatcher::new();
        assert!(watcher.acknowledge_disk_change(&path));
    }

    #[gpui_kit::test]
    fn test_refresh_disk_signature_absorbs_own_write(_cx: &mut TestAppContext) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("refresh_signature.txt");
        std::fs::write(&path, "before save").expect("failed to write test file");
        let (mut watcher, _event_rx) = FileWatcher::new();
        watcher
            .watch_file(&path)
            .expect("failed to watch test file");
        std::fs::write(&path, "after the editor saved").expect("failed to rewrite test file");
        watcher.refresh_disk_signature(&path);
        assert!(
            !watcher.acknowledge_disk_change(&path),
            "a write recorded through refresh_disk_signature must not count as a change"
        );
    }
}
