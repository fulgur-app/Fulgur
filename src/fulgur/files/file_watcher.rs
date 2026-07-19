use crate::fulgur::Fulgur;
use crate::fulgur::editor_tab::TabLocation;
use crate::fulgur::tab::Tab;
use futures::StreamExt;
use futures::channel::mpsc::{Receiver, Sender, channel};
use futures::future::Either;
use gpui::{Context, Task, Window};
use notify::{Error as NotifyError, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// Bound for the file watch event channel.
const FILE_WATCH_EVENT_CHANNEL_CAPACITY: usize = 256;

/// Maximum time a Linux inotify rename `From` event waits for its matching `To` before it is treated as a deletion.
const PENDING_RENAME_TIMEOUT: Duration = Duration::from_millis(500);

/// File watching state for external file change detection
pub struct FileWatchState {
    pub file_watcher: Option<FileWatcher>,
    /// Consumer task awaiting file watch events; dropping it cancels the consumer.
    pub consumer_task: Option<Task<()>>,
    pub last_file_events: HashMap<PathBuf, Instant>,
    pub last_file_saves: HashMap<PathBuf, Instant>,
    pub pending_conflicts: HashMap<PathBuf, usize>,
}

impl Default for FileWatchState {
    /// Create a new `FileWatchState` with all fields initialized to default/empty values
    ///
    /// ### Returns
    /// `Self`: A new `FileWatchState`
    fn default() -> Self {
        Self::new()
    }
}

impl FileWatchState {
    /// Create a new `FileWatchState` with all fields initialized to default/empty values
    ///
    /// ### Returns
    /// `Self`: A new `FileWatchState`
    #[must_use]
    pub fn new() -> Self {
        Self {
            file_watcher: None,
            consumer_task: None,
            last_file_events: HashMap::new(),
            last_file_saves: HashMap::new(),
            pending_conflicts: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum FileWatchEvent {
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
    Error(String),
}

pub struct FileWatcher {
    watcher: Option<RecommendedWatcher>,
    watched_paths: HashMap<PathBuf, SystemTime>,
    event_tx: Sender<FileWatchEvent>,
    /// Pending rename source path for Linux inotify, which splits rename events
    /// into separate From and To notifications rather than a single two-path event.
    pending_rename_from: Arc<Mutex<Option<(PathBuf, Instant)>>>,
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
    /// Returns a `NotifyError` if the underlying file system watcher could not be created.
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
    fn expire_pending_rename_from(
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
    /// Returns an error if the watcher cannot be started, the file metadata cannot be read,
    /// or the watcher fails to register the path.
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
        let modified_time = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if let Some(watcher) = &mut self.watcher {
            watcher
                .watch(path, RecursiveMode::NonRecursive)
                .map_err(|e| format!("Failed to watch file {}: {}", path.display(), e))?;
        }
        self.watched_paths.insert(path.clone(), modified_time);
        log::debug!("Started watching file: {}", path.display());
        Ok(())
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

impl Fulgur {
    /// Remove file-watcher bookkeeping entries for one file path.
    ///
    /// ### Arguments
    /// - `path`: File path to remove from debounce/save/conflict maps
    fn prune_file_watch_bookkeeping_for_path(&mut self, path: &PathBuf) {
        self.file_watch_state.last_file_events.remove(path);
        self.file_watch_state.last_file_saves.remove(path);
        self.file_watch_state.pending_conflicts.remove(path);
    }

    /// Clear all file-watcher bookkeeping maps.
    fn clear_file_watch_bookkeeping(&mut self) {
        self.file_watch_state.last_file_events.clear();
        self.file_watch_state.last_file_saves.clear();
        self.file_watch_state.pending_conflicts.clear();
    }

    /// Determine whether a watch event for a path should be ignored as a
    /// self-save echo (completed or still in flight) or a duplicate within the
    /// debounce window.
    ///
    /// ### Arguments
    /// - `path`: The file path the event refers to
    ///
    /// ### Returns
    /// - `true`: The event is a self-save echo or duplicate and should be ignored
    /// - `false`: The event is new and the debounce timestamp has been recorded
    fn should_suppress_file_watch_event(&mut self, path: &PathBuf) -> bool {
        let now = Instant::now();
        if let Some(&last_time) = self.file_watch_state.last_file_events.get(path)
            && now.duration_since(last_time) < Duration::from_millis(500)
        {
            return true;
        }
        if let Some(&save_time) = self.file_watch_state.last_file_saves.get(path)
            && now.duration_since(save_time) < Duration::from_millis(500)
        {
            return true;
        }
        // A background save may have already renamed the file into place while
        // its completion handler (which records `last_file_saves`) has not run
        // yet; treat events for such paths as self-save echoes too.
        if self
            .inflight_saves
            .values()
            .any(|save_path| save_path == path)
        {
            return true;
        }
        self.file_watch_state
            .last_file_events
            .insert(path.clone(), now);
        false
    }

    /// Apply an external-modification event to the tab backing a path.
    ///
    /// ### Arguments
    /// - `path`: The path of the externally modified file
    /// - `window`: The window context
    /// - `cx`: The application context
    fn apply_external_modification(
        &mut self,
        path: &PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab_index) = self.find_tab_by_path(path, cx)
            && let Some(Tab::Editor(editor_tab)) = self.tabs.get(tab_index).map(|t| t.read(cx))
        {
            if editor_tab.modified {
                let tab_id = editor_tab.id;
                let is_active = self.active_tab_index(cx) == Some(tab_index);

                if is_active {
                    self.show_file_conflict_dialog(path, tab_id, window, cx);
                } else {
                    self.file_watch_state
                        .pending_conflicts
                        .insert(path.clone(), tab_index);
                }
            } else {
                self.reload_tab_from_disk(tab_index, window, cx);
                Self::show_notification_file_reloaded(path, window, cx);
            }
        }
    }

    /// Mark the tab backing an externally deleted file as modified.
    ///
    /// ### Arguments
    /// - `path`: The path of the file that was deleted externally
    /// - `cx`: The application context
    fn mark_tab_deleted_externally(&mut self, path: &PathBuf, cx: &mut gpui::App) {
        self.file_watch_state.pending_conflicts.remove(path);
        if let Some(tab_entity) = self
            .find_tab_by_path(path, cx)
            .and_then(|tab_index| self.tabs.get(tab_index).cloned())
        {
            tab_entity.update(cx, |tab, cx| {
                if let Some(editor_tab) = tab.as_editor_mut() {
                    editor_tab.modified = true;
                    cx.notify();
                }
            });
        }
    }

    /// Handle file watch events received from the file watcher
    ///
    /// ### Description
    /// - If the event is a modification, it shows a conflict dialog if the file is modified and the tab is active
    /// - If the event is a deletion whose path still exists, it is treated as an atomic-rename replacement (re-watch and reload)
    /// - If the event is a deletion whose path is gone, it shows a notification that the file was deleted
    /// - If the event is a rename, it shows a notification that the file was renamed
    /// - If the event is an error, it logs the error
    ///
    /// ### Arguments
    /// - `event`: The file watch event to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn handle_file_watch_event(
        &mut self,
        event: FileWatchEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            FileWatchEvent::Modified(path) => {
                if self.should_suppress_file_watch_event(&path) {
                    return;
                }
                self.apply_external_modification(&path, window, cx);
            }
            FileWatchEvent::Deleted(path) => {
                if self.should_suppress_file_watch_event(&path) {
                    return;
                }
                // A "deleted" event whose path still exists on disk is an atomic save.
                if path.exists() {
                    // Re-register only on inode-based backends..
                    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                    {
                        self.unwatch_file(&path);
                        self.watch_file(&path);
                        self.file_watch_state
                            .last_file_events
                            .insert(path.clone(), Instant::now());
                    }
                    self.apply_external_modification(&path, window, cx);
                    return;
                }
                self.mark_tab_deleted_externally(&path, cx);
                Self::show_notification_file_deleted(&path, window, cx);
            }
            FileWatchEvent::Renamed { from, to } => {
                if self.should_suppress_file_watch_event(&from) {
                    return;
                }
                if let Some(tab_entity) = self
                    .find_tab_by_path(&from, cx)
                    .and_then(|tab_index| self.tabs.get(tab_index).cloned())
                {
                    self.unwatch_file(&from);
                    self.watch_file(&to);
                    tab_entity.update(cx, |tab, cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(to.clone());
                            editor_tab.title = to
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("Untitled")
                                .to_string()
                                .into();
                            cx.notify();
                        }
                    });
                    Self::show_notification_file_renamed(&from, &to, window, cx);
                }
            }
            FileWatchEvent::Error(msg) => {
                log::error!("File watcher error: {msg}");
            }
        }
    }

    /// Start the file watcher, watch all open files, and spawn the event consumer task
    ///
    /// ### Arguments
    /// - `cx`: The application context used to spawn the consumer task
    pub fn start_file_watcher(&mut self, cx: &mut Context<Self>) {
        let (mut watcher, receiver) = FileWatcher::new();
        if let Err(e) = watcher.start() {
            log::error!("Failed to start file watcher: {e}");
            return;
        }
        for tab in &self.tabs {
            if let Tab::Editor(editor_tab) = tab.read(cx)
                && let Some(path) = editor_tab.file_path()
                && let Err(e) = watcher.watch_file(path)
            {
                log::warn!("Failed to watch file {}: {}", path.display(), e);
            }
        }
        let pending_rename_from = Arc::clone(&watcher.pending_rename_from);
        let flush_tx = watcher.event_tx.clone();
        self.file_watch_state.file_watcher = Some(watcher);
        self.file_watch_state.consumer_task =
            Some(self.spawn_file_watch_consumer(receiver, pending_rename_from, flush_tx, cx));
    }

    /// Stop the file watcher and cancel the event consumer task
    pub fn stop_file_watcher(&mut self) {
        if let Some(mut watcher) = self.file_watch_state.file_watcher.take() {
            watcher.stop();
        }
        self.file_watch_state.consumer_task = None;
        self.clear_file_watch_bookkeeping();
    }

    /// Add a file to the watcher
    ///
    /// ### Arguments
    /// - `path`: The path to the file to watch
    pub fn watch_file(&mut self, path: &std::path::Path) {
        if let Some(watcher) = &mut self.file_watch_state.file_watcher
            && let Err(e) = watcher.watch_file(&path.to_path_buf())
        {
            log::warn!("Failed to watch file {}: {}", path.display(), e);
        }
    }

    /// Remove a file from the watcher
    ///
    /// ### Arguments
    /// - `path`: The path to the file to unwatch
    pub fn unwatch_file(&mut self, path: &PathBuf) {
        if let Some(watcher) = &mut self.file_watch_state.file_watcher {
            watcher.unwatch_file(path);
        }
        self.prune_file_watch_bookkeeping_for_path(path);
    }

    /// Spawn the task that consumes file watch events for this window
    ///
    /// ### Arguments
    /// - `events`: The receiver side of the watcher's event channel
    /// - `pending_rename_from`: The watcher's pending split-rename accumulator
    /// - `flush_tx`: Sender used to emit the synthesized `Deleted` event on expiry
    /// - `cx`: The application context used to spawn the task
    ///
    /// ### Returns
    /// - `Task<()>`: The consumer task; dropping it cancels the consumer
    fn spawn_file_watch_consumer(
        &self,
        mut events: Receiver<FileWatchEvent>,
        pending_rename_from: Arc<Mutex<Option<(PathBuf, Instant)>>>,
        mut flush_tx: Sender<FileWatchEvent>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let window_id = self.window_id;
        cx.spawn(async move |view, cx| {
            loop {
                let has_pending_rename = pending_rename_from
                    .lock()
                    .is_ok_and(|pending| pending.is_some());
                let event = if has_pending_rename {
                    let timer = cx.background_executor().timer(PENDING_RENAME_TIMEOUT);
                    match futures::future::select(events.next(), std::pin::pin!(timer)).await {
                        Either::Left((event, _)) => event,
                        Either::Right(((), _)) => {
                            FileWatcher::expire_pending_rename_from(
                                &pending_rename_from,
                                &mut flush_tx,
                            );
                            continue;
                        }
                    }
                } else {
                    events.next().await
                };
                let Some(event) = event else {
                    break;
                };
                let handle = cx.update(|cx| {
                    cx.windows()
                        .into_iter()
                        .find(|handle| handle.window_id() == window_id)
                });
                let Some(handle) = handle else {
                    break;
                };
                let delivered = handle.update(cx, |_, window, cx| {
                    view.update(cx, |this, cx| {
                        this.handle_file_watch_event(event, window, cx);
                        cx.notify();
                    })
                });
                if !matches!(delivered, Ok(Ok(()))) {
                    break;
                }
            }
        })
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::{FileWatchEvent, FileWatcher};
    use crate::fulgur::{
        Fulgur, editor_tab::TabLocation, settings::Settings, shared_state::SharedAppState,
        window_manager::WindowManager,
    };
    use futures::channel::mpsc::{TryRecvError, channel};
    use gpui::{AppContext, Entity, TestAppContext, VisualTestContext, WindowOptions};
    use notify::{
        Event, EventKind,
        event::{DataChange, ModifyKind, RemoveKind, RenameMode},
    };
    use parking_lot::Mutex as ParkingMutex;
    use std::{
        cell::RefCell,
        path::PathBuf,
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };
    use tempfile::TempDir;

    /// Build an OS-agnostic temporary test path.
    ///
    /// ### Arguments
    /// - `file_name`: The file name to append to the platform temp directory.
    ///
    /// ### Returns
    /// - `PathBuf`: A path under `std::env::temp_dir()` suitable for cross-platform tests.
    fn temp_test_path(file_name: &str) -> PathBuf {
        std::env::temp_dir().join(file_name)
    }

    fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let mut settings = Settings::new();
            settings.editor_settings.watch_files = false;
            let pending_files: Arc<ParkingMutex<Vec<PathBuf>>> =
                Arc::new(ParkingMutex::new(Vec::new()));
            cx.set_global(SharedAppState::new(settings, pending_files, None));
            cx.set_global(WindowManager::new());
        });
        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        let window = cx
            .update(|cx| {
                cx.open_window(WindowOptions::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                    *fulgur_slot.borrow_mut() = Some(fulgur.clone());
                    cx.new(|cx| gpui_component::Root::new(fulgur, window, cx))
                })
            })
            .expect("failed to open test window");
        let visual_cx = VisualTestContext::from_window(window.into(), cx);
        visual_cx.run_until_parked();
        let fulgur = fulgur_slot
            .into_inner()
            .expect("failed to capture Fulgur entity");
        (fulgur, visual_cx)
    }

    #[gpui::test]
    fn test_handle_notify_event_maps_modify_to_modified(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(super::FILE_WATCH_EVENT_CHANNEL_CAPACITY);
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

    #[gpui::test]
    fn test_handle_notify_event_maps_remove_to_deleted(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(super::FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let pending_rename_from = Mutex::new(None);
        let path = temp_test_path("fulgur_notify_deleted.txt");
        let event = Event::new(EventKind::Remove(RemoveKind::File)).add_path(path.clone());
        FileWatcher::handle_notify_event(event, &mut event_tx, &pending_rename_from);
        assert!(matches!(
            event_rx.try_recv(),
            Ok(FileWatchEvent::Deleted(actual)) if actual == path
        ));
    }

    #[gpui::test]
    fn test_handle_notify_event_maps_macos_rename_any_to_deleted(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(super::FILE_WATCH_EVENT_CHANNEL_CAPACITY);
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

    #[gpui::test]
    fn test_handle_notify_event_maps_rename_both_to_renamed(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(super::FILE_WATCH_EVENT_CHANNEL_CAPACITY);
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

    #[gpui::test]
    fn test_handle_notify_event_pairs_linux_split_rename(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(super::FILE_WATCH_EVENT_CHANNEL_CAPACITY);
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

    #[gpui::test]
    fn test_handle_notify_event_expires_stale_pending_rename(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(super::FILE_WATCH_EVENT_CHANNEL_CAPACITY);
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

    #[gpui::test]
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

    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
    fn test_handle_file_watch_event_modified_reloads_unmodified_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("modified_reload_test.txt");
        std::fs::write(&path, "content-from-disk").expect("failed to write test file");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                            editor_tab.content.update(cx, |input_state, cx| {
                                input_state.set_value("stale-content", window, cx);
                            });
                            editor_tab.set_original_content_from_str("stale-content");
                            editor_tab.modified = false;
                        }
                    });
                this.handle_file_watch_event(FileWatchEvent::Modified(path.clone()), window, cx);
                assert!(
                    this.file_watch_state.last_file_events.contains_key(&path),
                    "modified event should update debounce map"
                );
            });
        });
        // Reloading runs on the background executor, so wait for it to apply.
        visual_cx.run_until_parked();

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let content = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .map(|editor_tab| editor_tab.content.read(cx).text().to_string())
                    .unwrap_or_default();
                assert_eq!(content, "content-from-disk");
            });
        });
    }

    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
    fn test_handle_file_watch_event_modified_is_debounced(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("modified_debounce_test.txt");
        std::fs::write(&path, "content-from-disk").expect("failed to write test file");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                            editor_tab.content.update(cx, |input_state, cx| {
                                input_state.set_value("local-content", window, cx);
                            });
                            editor_tab.set_original_content_from_str("local-content");
                            editor_tab.modified = false;
                        }
                    });
                this.file_watch_state
                    .last_file_events
                    .insert(path.clone(), Instant::now());
                this.handle_file_watch_event(FileWatchEvent::Modified(path.clone()), window, cx);
                let content = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .map(|editor_tab| editor_tab.content.read(cx).text().to_string())
                    .unwrap_or_default();
                assert_eq!(content, "local-content");
            });
        });
    }

    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
    fn test_handle_file_watch_event_modified_active_tab_does_not_queue_conflict(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let path = temp_test_path("fulgur_conflict_active.txt");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                            editor_tab.modified = true;
                            editor_tab.content.update(cx, |input_state, cx| {
                                input_state.set_value("local-edits", window, cx);
                            });
                        }
                    });
                this.active_tab_id = this.tabs.first().map(|t| t.read(cx).id());
                this.handle_file_watch_event(FileWatchEvent::Modified(path.clone()), window, cx);
                assert!(
                    !this.file_watch_state.pending_conflicts.contains_key(&path),
                    "active-tab conflict should prompt immediately, not queue"
                );
            });
        });
    }

    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
    fn test_handle_file_watch_event_modified_inactive_tab_defers_until_activation(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let deferred_path = temp_test_path("fulgur_conflict_inactive.txt");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, _cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(deferred_path.clone());
                            editor_tab.modified = true;
                        }
                    });
                this.set_active_tab(1, window, cx);
                this.handle_file_watch_event(
                    FileWatchEvent::Modified(deferred_path.clone()),
                    window,
                    cx,
                );
                assert_eq!(
                    this.file_watch_state.pending_conflicts.get(&deferred_path),
                    Some(&0),
                    "inactive modified tab should queue deferred conflict"
                );
                this.set_active_tab(0, window, cx);
                assert!(
                    !this
                        .file_watch_state
                        .pending_conflicts
                        .contains_key(&deferred_path),
                    "deferred conflict should be consumed when tab is activated"
                );
            });
        });
    }

    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
    fn test_handle_file_watch_event_deleted_keeps_editor_state(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let path = temp_test_path("fulgur_deleted_branch.txt");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs.first().expect("expected at least one tab").clone().update(cx, |tab, cx| {
                    if let Some(editor_tab) = tab.as_editor_mut() {
                        editor_tab.location = TabLocation::Local(path.clone());
                        editor_tab.content.update(cx, |input_state, cx| {
                            input_state.set_value("current-content", window, cx);
                        });
                        editor_tab.set_original_content_from_str("current-content");
                        editor_tab.title = "deleted_branch.txt".into();
                    }
                });
                this.handle_file_watch_event(FileWatchEvent::Deleted(path.clone()), window, cx);
                let (current_path, current_title, current_content, current_modified) = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .map(|editor_tab| {
                        (
                            editor_tab.file_path().cloned(),
                            editor_tab.title.to_string(),
                            editor_tab.content.read(cx).text().to_string(),
                            editor_tab.modified,
                        )
                    })
                    .expect("expected active editor tab");
                assert_eq!(current_path, Some(path));
                assert_eq!(current_title, "deleted_branch.txt");
                assert_eq!(current_content, "current-content");
                assert!(
                    current_modified,
                    "a genuine external deletion should mark the tab modified so closing prompts to save"
                );
            });
        });
    }

    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
    fn test_handle_file_watch_event_deleted_existing_path_reloads(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("atomic_rename_reload.txt");
        std::fs::write(&path, "content-from-disk").expect("failed to write test file");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                            editor_tab.content.update(cx, |input_state, cx| {
                                input_state.set_value("stale-content", window, cx);
                            });
                            editor_tab.set_original_content_from_str("stale-content");
                            editor_tab.modified = false;
                        }
                    });
                this.handle_file_watch_event(FileWatchEvent::Deleted(path.clone()), window, cx);
            });
        });
        // Reloading runs on the background executor, so wait for it to apply.
        visual_cx.run_until_parked();

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let content = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .map(|editor_tab| editor_tab.content.read(cx).text().to_string())
                    .unwrap_or_default();
                assert_eq!(
                    content, "content-from-disk",
                    "a delete whose path still exists is an atomic-rename replacement and should reload"
                );
            });
        });
    }

    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
    fn test_handle_file_watch_event_deleted_is_suppressed_after_self_save(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("self_save_suppressed.txt");
        std::fs::write(&path, "content-from-disk").expect("failed to write test file");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                            editor_tab.content.update(cx, |input_state, cx| {
                                input_state.set_value("local-content", window, cx);
                            });
                            editor_tab.set_original_content_from_str("local-content");
                            editor_tab.modified = false;
                        }
                    });
                this.file_watch_state
                    .last_file_saves
                    .insert(path.clone(), Instant::now());
                this.handle_file_watch_event(FileWatchEvent::Deleted(path.clone()), window, cx);
                let content = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .map(|editor_tab| editor_tab.content.read(cx).text().to_string())
                    .unwrap_or_default();
                assert_eq!(
                    content, "local-content",
                    "a delete echoing the user's own atomic save must be suppressed, not reloaded"
                );
            });
        });
    }

    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
    fn test_handle_file_watch_event_renamed_updates_path_and_title(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let from = temp_test_path("fulgur_rename_from.rs");
        let to = temp_test_path("fulgur_rename_to.rs");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, _cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(from.clone());
                            editor_tab.title = "fulgur_rename_from.rs".into();
                        }
                    });
                // Seed stale bookkeeping (older than the 500 ms suppression
                // window): a genuine external rename happens long after any
                // prior save, so it must still be processed and prune these.
                let stale = Instant::now()
                    .checked_sub(Duration::from_secs(1))
                    .expect("instant subtraction should not underflow");
                this.file_watch_state
                    .last_file_events
                    .insert(from.clone(), stale);
                this.file_watch_state
                    .last_file_saves
                    .insert(from.clone(), stale);
                this.file_watch_state
                    .pending_conflicts
                    .insert(from.clone(), 0);
                this.handle_file_watch_event(
                    FileWatchEvent::Renamed {
                        from: from.clone(),
                        to: to.clone(),
                    },
                    window,
                    cx,
                );
                let (current_path, current_title) = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .map(|editor_tab| {
                        (
                            editor_tab.file_path().cloned(),
                            editor_tab.title.to_string(),
                        )
                    })
                    .expect("expected active editor tab");
                assert_eq!(current_path, Some(to));
                assert_eq!(current_title, "fulgur_rename_to.rs");
                assert!(
                    !this.file_watch_state.last_file_events.contains_key(&from),
                    "rename should prune old-path debounce bookkeeping"
                );
                assert!(
                    !this.file_watch_state.last_file_saves.contains_key(&from),
                    "rename should prune old-path save bookkeeping"
                );
                assert!(
                    !this.file_watch_state.pending_conflicts.contains_key(&from),
                    "rename should prune old-path deferred conflict bookkeeping"
                );
            });
        });
    }

    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
    fn test_unwatch_file_prunes_bookkeeping_maps(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let path = temp_test_path("fulgur_unwatch_cleanup.txt");
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, _cx| {
                this.file_watch_state
                    .last_file_events
                    .insert(path.clone(), Instant::now());
                this.file_watch_state
                    .last_file_saves
                    .insert(path.clone(), Instant::now());
                this.file_watch_state
                    .pending_conflicts
                    .insert(path.clone(), 0);
                this.unwatch_file(&path);
                assert!(
                    !this.file_watch_state.last_file_events.contains_key(&path),
                    "unwatch must prune debounce bookkeeping"
                );
                assert!(
                    !this.file_watch_state.last_file_saves.contains_key(&path),
                    "unwatch must prune save bookkeeping"
                );
                assert!(
                    !this.file_watch_state.pending_conflicts.contains_key(&path),
                    "unwatch must prune deferred conflict bookkeeping"
                );
            });
        });
    }

    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
    fn test_stop_file_watcher_clears_bookkeeping_maps(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let path = temp_test_path("fulgur_stop_watcher_cleanup.txt");
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, _cx| {
                this.file_watch_state
                    .last_file_events
                    .insert(path.clone(), Instant::now());
                this.file_watch_state
                    .last_file_saves
                    .insert(path.clone(), Instant::now());
                this.file_watch_state
                    .pending_conflicts
                    .insert(path.clone(), 0);
                this.stop_file_watcher();
                assert!(
                    this.file_watch_state.last_file_events.is_empty(),
                    "stopping watcher must clear debounce bookkeeping"
                );
                assert!(
                    this.file_watch_state.last_file_saves.is_empty(),
                    "stopping watcher must clear save bookkeeping"
                );
                assert!(
                    this.file_watch_state.pending_conflicts.is_empty(),
                    "stopping watcher must clear deferred conflict bookkeeping"
                );
            });
        });
    }
}
