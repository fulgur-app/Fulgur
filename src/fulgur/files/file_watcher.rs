use crate::fulgur::Fulgur;
use crate::fulgur::tab::Tab;
use crate::fulgur::utils::utilities::collect_events;
use gpui::{Context, Window};
use notify::{Error as NotifyError, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// File watching state for external file change detection
pub struct FileWatchState {
    pub file_watcher: Option<FileWatcher>,
    pub file_watch_events: Option<Receiver<FileWatchEvent>>,
    pub last_file_events: HashMap<PathBuf, Instant>,
    pub last_file_saves: HashMap<PathBuf, Instant>,
    pub pending_conflicts: HashMap<PathBuf, usize>,
}

impl Default for FileWatchState {
    /// Create a new FileWatchState with all fields initialized to default/empty values
    ///
    /// ### Returns
    /// `Self`: A new FileWatchState
    fn default() -> Self {
        Self::new()
    }
}

impl FileWatchState {
    /// Create a new FileWatchState with all fields initialized to default/empty values
    ///
    /// ### Returns
    /// `Self: a new FileWatchState
    pub fn new() -> Self {
        Self {
            file_watcher: None,
            file_watch_events: None,
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
    pending_rename_from: Arc<Mutex<Option<PathBuf>>>,
}

impl FileWatcher {
    /// Creates a new file watcher
    ///
    /// ### Returns
    /// - `(FileWatcher, Receiver<FileWatchEvent>)`: A tuple containing the file watcher and the event receiver
    ///   - `FileWatcher`: The file watcher instance
    ///   - `Receiver<FileWatchEvent>`: The event receiver to receive the file watch events from the file watcher
    pub fn new() -> (Self, Receiver<FileWatchEvent>) {
        let (event_tx, event_rx) = channel();
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
    /// ### Returns
    /// - `Ok(())`: If the file watcher was started successfully
    /// - `Err(NotifyError)`: If the file watcher could not be started
    pub fn start(&mut self) -> Result<(), NotifyError> {
        if self.watcher.is_some() {
            return Ok(());
        }
        let event_tx = self.event_tx.clone();
        let pending_rename_from = Arc::clone(&self.pending_rename_from);
        let watcher =
            notify::recommended_watcher(move |res: Result<Event, NotifyError>| match res {
                Ok(event) => {
                    Self::handle_notify_event(event, &event_tx, &pending_rename_from);
                }
                Err(e) => {
                    let _ = event_tx.send(FileWatchEvent::Error(e.to_string()));
                }
            })?;
        self.watcher = Some(watcher);
        Ok(())
    }

    /// Handles a notify event and converts it to a FileWatchEvent
    ///
    /// ### Description
    /// - If the event is a modification, it sends a Modified event to the event sender
    /// - If the event is a deletion, it sends a Deleted event to the event sender
    /// - If the event is a creation after a rename or save, it sends a Modified event to the event sender
    /// - If the event is other, it ignores it
    /// - Ignore other event types (access, etc.)
    ///
    /// Rename events come in two shapes depending on the OS backend:
    /// - **macOS/Windows**: A single event with two paths (`RenameMode::Both`)
    /// - **Linux inotify**: Two consecutive single-path events (`RenameMode::From` then
    ///   `RenameMode::To`). The `pending_rename_from` accumulator pairs them up.
    ///
    /// ### Arguments
    /// - `event`: The notify event to handle
    /// - `event_tx`: The event sender to send the events to
    /// - `pending_rename_from`: Accumulator for the Linux split-rename `From` path
    fn handle_notify_event(
        event: Event,
        event_tx: &Sender<FileWatchEvent>,
        pending_rename_from: &Mutex<Option<PathBuf>>,
    ) {
        use notify::event::{ModifyKind, RenameMode};

        match event.kind {
            EventKind::Modify(ModifyKind::Name(rename_mode)) => {
                if event.paths.len() == 2 {
                    // macOS / Windows: both paths arrive in one event
                    let from = event.paths[0].clone();
                    let to = event.paths[1].clone();
                    let _ = event_tx.send(FileWatchEvent::Renamed { from, to });
                } else if event.paths.len() == 1 {
                    match rename_mode {
                        RenameMode::From => {
                            // Linux inotify: first half — store and wait for the To event
                            if let Ok(mut pending) = pending_rename_from.lock() {
                                *pending = Some(event.paths[0].clone());
                            }
                        }
                        RenameMode::To => {
                            // Linux inotify: second half — pair with the stored From path
                            let from = pending_rename_from.lock().ok().and_then(|mut p| p.take());
                            match from {
                                Some(from) => {
                                    let _ = event_tx.send(FileWatchEvent::Renamed {
                                        from,
                                        to: event.paths[0].clone(),
                                    });
                                }
                                None => {
                                    // No matching From; treat as a new file appearing
                                    let _ = event_tx
                                        .send(FileWatchEvent::Modified(event.paths[0].clone()));
                                }
                            }
                        }
                        _ => {
                            // Orphaned or unrecognised single-path rename; flush any pending From
                            if let Ok(mut pending) = pending_rename_from.lock()
                                && let Some(stale) = pending.take()
                            {
                                let _ = event_tx.send(FileWatchEvent::Deleted(stale));
                            }
                        }
                    }
                }
            }
            EventKind::Modify(_) => {
                for path in event.paths {
                    let _ = event_tx.send(FileWatchEvent::Modified(path));
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    let _ = event_tx.send(FileWatchEvent::Deleted(path));
                }
            }
            EventKind::Create(_) => {
                for path in event.paths {
                    let _ = event_tx.send(FileWatchEvent::Modified(path));
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
    /// ### Returns
    /// - `Ok(())`: If the file was watched successfully
    /// - `Err(String)`: If the file could not be watched
    pub fn watch_file(&mut self, path: PathBuf) -> Result<(), String> {
        if self.watched_paths.contains_key(&path) {
            return Ok(());
        }
        if self.watcher.is_none() {
            self.start().map_err(|e| e.to_string())?;
        }
        let modified_time = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if let Some(watcher) = &mut self.watcher {
            watcher
                .watch(&path, RecursiveMode::NonRecursive)
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
    /// Stops the file watcher when the FileWatcher instance is dropped
    fn drop(&mut self) {
        self.stop();
    }
}

impl Fulgur {
    /// Handle file watch events received from the file watcher
    ///
    /// ### Description
    /// - If the event is a modification, it shows a conflict dialog if the file is modified and the tab is active
    /// - If the event is a deletion, it shows a notification that the file was deleted
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
                let now = Instant::now();
                if let Some(&last_time) = self.file_watch_state.last_file_events.get(&path)
                    && now.duration_since(last_time) < Duration::from_millis(500)
                {
                    return;
                }
                if let Some(&save_time) = self.file_watch_state.last_file_saves.get(&path)
                    && now.duration_since(save_time) < Duration::from_millis(500)
                {
                    return;
                }
                self.file_watch_state
                    .last_file_events
                    .insert(path.clone(), now);
                if let Some(tab_index) = self.find_tab_by_path(&path)
                    && let Some(Tab::Editor(editor_tab)) = self.tabs.get(tab_index)
                {
                    if editor_tab.modified {
                        let is_active = self.active_tab_index == Some(tab_index);

                        if is_active {
                            self.show_file_conflict_dialog(path, tab_index, window, cx);
                        } else {
                            self.file_watch_state
                                .pending_conflicts
                                .insert(path, tab_index);
                        }
                    } else {
                        self.reload_tab_from_disk(tab_index, window, cx);
                        self.show_notification_file_reloaded(&path, window, cx);
                    }
                }
            }
            FileWatchEvent::Deleted(path) => {
                self.show_notification_file_deleted(&path, window, cx);
            }
            FileWatchEvent::Renamed { from, to } => {
                if let Some(tab_index) = self.find_tab_by_path(&from) {
                    self.unwatch_file(&from);
                    self.watch_file(&to);
                    if let Some(Tab::Editor(editor_tab)) = self.tabs.get_mut(tab_index) {
                        editor_tab.file_path = Some(to.clone());
                        editor_tab.title = to
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("Untitled")
                            .to_string()
                            .into();
                    }
                    self.show_notification_file_renamed(&from, &to, window, cx);
                }
            }
            FileWatchEvent::Error(msg) => {
                log::error!("File watcher error: {}", msg);
            }
        }
    }

    /// Start the file watcher and watch all open files
    pub fn start_file_watcher(&mut self) {
        let (mut watcher, receiver) = FileWatcher::new();
        if let Err(e) = watcher.start() {
            log::error!("Failed to start file watcher: {}", e);
            return;
        }
        for tab in &self.tabs {
            if let Tab::Editor(editor_tab) = tab
                && let Some(path) = &editor_tab.file_path
                && let Err(e) = watcher.watch_file(path.clone())
            {
                log::warn!("Failed to watch file {}: {}", path.display(), e);
            }
        }
        self.file_watch_state.file_watcher = Some(watcher);
        self.file_watch_state.file_watch_events = Some(receiver);
    }

    /// Stop the file watcher
    pub fn stop_file_watcher(&mut self) {
        if let Some(mut watcher) = self.file_watch_state.file_watcher.take() {
            watcher.stop();
        }
        self.file_watch_state.file_watch_events = None;
    }

    /// Add a file to the watcher
    ///
    /// ### Arguments
    /// - `path`: The path to the file to watch
    pub fn watch_file(&mut self, path: &std::path::Path) {
        if let Some(watcher) = &mut self.file_watch_state.file_watcher
            && let Err(e) = watcher.watch_file(path.to_path_buf())
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
    }

    /// Collect and process file watch events:
    /// - Modified: File content changed externally (may trigger auto-reload or conflict dialog)
    /// - Deleted: File was deleted externally (shows notification)
    /// - Renamed: File was moved/renamed (updates tab path and continues watching)
    ///
    /// ### Arguments
    /// - `window`: The window containing the tabs with watched files
    /// - `cx`: The application context
    pub fn process_file_watch_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let events = collect_events(&self.file_watch_state.file_watch_events);
        for event in events {
            self.handle_file_watch_event(event, window, cx);
        }
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::{FileWatchEvent, FileWatcher};
    use crate::fulgur::{
        Fulgur, settings::Settings, shared_state::SharedAppState, tab::Tab,
        window_manager::WindowManager,
    };
    use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
    use notify::{
        Event, EventKind,
        event::{DataChange, ModifyKind, RemoveKind, RenameMode},
    };
    use parking_lot::Mutex as ParkingMutex;
    use std::{
        cell::RefCell,
        path::PathBuf,
        sync::{
            Arc, Mutex,
            mpsc::{TryRecvError, channel},
        },
        time::Instant,
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
            cx.set_global(SharedAppState::new(settings, pending_files));
            cx.set_global(WindowManager::new());
        });
        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        let window = cx
            .update(|cx| {
                cx.open_window(Default::default(), |window, cx| {
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
        let (event_tx, event_rx) = channel();
        let pending_rename_from = Mutex::new(None);
        let path = temp_test_path("fulgur_notify_modify.txt");
        let event = Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
            .add_path(path.clone());
        FileWatcher::handle_notify_event(event, &event_tx, &pending_rename_from);
        assert!(matches!(
            event_rx.try_recv(),
            Ok(FileWatchEvent::Modified(actual)) if actual == path
        ));
    }

    #[gpui::test]
    fn test_handle_notify_event_maps_remove_to_deleted(_cx: &mut TestAppContext) {
        let (event_tx, event_rx) = channel();
        let pending_rename_from = Mutex::new(None);
        let path = temp_test_path("fulgur_notify_deleted.txt");
        let event = Event::new(EventKind::Remove(RemoveKind::File)).add_path(path.clone());
        FileWatcher::handle_notify_event(event, &event_tx, &pending_rename_from);
        assert!(matches!(
            event_rx.try_recv(),
            Ok(FileWatchEvent::Deleted(actual)) if actual == path
        ));
    }

    #[gpui::test]
    fn test_handle_notify_event_maps_rename_both_to_renamed(_cx: &mut TestAppContext) {
        let (event_tx, event_rx) = channel();
        let pending_rename_from = Mutex::new(None);
        let from = temp_test_path("fulgur_notify_old.txt");
        let to = temp_test_path("fulgur_notify_new.txt");
        let event = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
            .add_path(from.clone())
            .add_path(to.clone());
        FileWatcher::handle_notify_event(event, &event_tx, &pending_rename_from);
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
        let (event_tx, event_rx) = channel();
        let pending_rename_from = Mutex::new(None);
        let from = temp_test_path("fulgur_notify_linux_from.txt");
        let to = temp_test_path("fulgur_notify_linux_to.txt");
        let from_event = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::From)))
            .add_path(from.clone());
        FileWatcher::handle_notify_event(from_event, &event_tx, &pending_rename_from);
        assert!(matches!(event_rx.try_recv(), Err(TryRecvError::Empty)));
        let to_event =
            Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::To))).add_path(to.clone());
        FileWatcher::handle_notify_event(to_event, &event_tx, &pending_rename_from);
        assert!(matches!(
            event_rx.try_recv(),
            Ok(FileWatchEvent::Renamed {
                from: actual_from,
                to: actual_to
            }) if actual_from == from && actual_to == to
        ));
    }

    #[gpui::test]
    fn test_handle_file_watch_event_modified_reloads_unmodified_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("modified_reload_test.txt");
        std::fs::write(&path, "content-from-disk").expect("failed to write test file");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(Tab::Editor(editor_tab)) = this.tabs.first_mut() {
                    editor_tab.file_path = Some(path.clone());
                    editor_tab.content.update(cx, |input_state, cx| {
                        input_state.set_value("stale-content", window, cx);
                    });
                    editor_tab.original_content = "stale-content".to_string();
                    editor_tab.modified = false;
                }
                this.handle_file_watch_event(FileWatchEvent::Modified(path.clone()), window, cx);
                let content = this
                    .tabs
                    .first()
                    .and_then(Tab::as_editor)
                    .map(|editor_tab| editor_tab.content.read(cx).text().to_string())
                    .unwrap_or_default();
                assert_eq!(content, "content-from-disk");
                assert!(
                    this.file_watch_state.last_file_events.contains_key(&path),
                    "modified event should update debounce map"
                );
            });
        });
    }

    #[gpui::test]
    fn test_handle_file_watch_event_modified_is_debounced(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("modified_debounce_test.txt");
        std::fs::write(&path, "content-from-disk").expect("failed to write test file");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(Tab::Editor(editor_tab)) = this.tabs.first_mut() {
                    editor_tab.file_path = Some(path.clone());
                    editor_tab.content.update(cx, |input_state, cx| {
                        input_state.set_value("local-content", window, cx);
                    });
                    editor_tab.original_content = "local-content".to_string();
                    editor_tab.modified = false;
                }
                this.file_watch_state
                    .last_file_events
                    .insert(path.clone(), Instant::now());
                this.handle_file_watch_event(FileWatchEvent::Modified(path.clone()), window, cx);
                let content = this
                    .tabs
                    .first()
                    .and_then(Tab::as_editor)
                    .map(|editor_tab| editor_tab.content.read(cx).text().to_string())
                    .unwrap_or_default();
                assert_eq!(content, "local-content");
            });
        });
    }

    #[gpui::test]
    fn test_handle_file_watch_event_modified_active_tab_does_not_queue_conflict(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let path = temp_test_path("fulgur_conflict_active.txt");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(Tab::Editor(editor_tab)) = this.tabs.first_mut() {
                    editor_tab.file_path = Some(path.clone());
                    editor_tab.modified = true;
                    editor_tab.content.update(cx, |input_state, cx| {
                        input_state.set_value("local-edits", window, cx);
                    });
                }
                this.active_tab_index = Some(0);
                this.handle_file_watch_event(FileWatchEvent::Modified(path.clone()), window, cx);
                assert!(
                    !this.file_watch_state.pending_conflicts.contains_key(&path),
                    "active-tab conflict should prompt immediately, not queue"
                );
            });
        });
    }

    #[gpui::test]
    fn test_handle_file_watch_event_modified_inactive_tab_defers_until_activation(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let deferred_path = temp_test_path("fulgur_conflict_inactive.txt");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                if let Some(Tab::Editor(editor_tab)) = this.tabs.first_mut() {
                    editor_tab.file_path = Some(deferred_path.clone());
                    editor_tab.modified = true;
                }
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
    fn test_handle_file_watch_event_deleted_keeps_editor_state(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let path = temp_test_path("fulgur_deleted_branch.txt");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(Tab::Editor(editor_tab)) = this.tabs.first_mut() {
                    editor_tab.file_path = Some(path.clone());
                    editor_tab.content.update(cx, |input_state, cx| {
                        input_state.set_value("current-content", window, cx);
                    });
                    editor_tab.original_content = "current-content".to_string();
                    editor_tab.title = "deleted_branch.txt".into();
                }
                this.handle_file_watch_event(FileWatchEvent::Deleted(path.clone()), window, cx);
                let (current_path, current_title, current_content) = this
                    .tabs
                    .first()
                    .and_then(Tab::as_editor)
                    .map(|editor_tab| {
                        (
                            editor_tab.file_path.clone(),
                            editor_tab.title.to_string(),
                            editor_tab.content.read(cx).text().to_string(),
                        )
                    })
                    .expect("expected active editor tab");
                assert_eq!(current_path, Some(path));
                assert_eq!(current_title, "deleted_branch.txt");
                assert_eq!(current_content, "current-content");
            });
        });
    }

    #[gpui::test]
    fn test_handle_file_watch_event_renamed_updates_path_and_title(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let from = temp_test_path("fulgur_rename_from.rs");
        let to = temp_test_path("fulgur_rename_to.rs");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(Tab::Editor(editor_tab)) = this.tabs.first_mut() {
                    editor_tab.file_path = Some(from.clone());
                    editor_tab.title = "fulgur_rename_from.rs".into();
                }
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
                    .and_then(Tab::as_editor)
                    .map(|editor_tab| (editor_tab.file_path.clone(), editor_tab.title.to_string()))
                    .expect("expected active editor tab");
                assert_eq!(current_path, Some(to));
                assert_eq!(current_title, "fulgur_rename_to.rs");
            });
        });
    }
}
