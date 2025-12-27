use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant, SystemTime};

use gpui::{Context, Window};
use notify::{Error as NotifyError, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::fulgur::Fulgur;
use crate::fulgur::tab::Tab;

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
        let watcher =
            notify::recommended_watcher(move |res: Result<Event, NotifyError>| match res {
                Ok(event) => {
                    Self::handle_notify_event(event, &event_tx);
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
    /// ### Arguments
    /// - `event`: The notify event to handle
    /// - `event_tx`: The event sender to send the events to
    fn handle_notify_event(event: Event, event_tx: &Sender<FileWatchEvent>) {
        use notify::event::ModifyKind;

        match event.kind {
            EventKind::Modify(ModifyKind::Name(_)) => {
                if event.paths.len() == 2 {
                    let from = event.paths[0].clone();
                    let to = event.paths[1].clone();
                    let _ = event_tx.send(FileWatchEvent::Renamed { from, to });
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
        if let Some(watcher) = &mut self.watcher {
            if let Err(e) = watcher.unwatch(path) {
                log::warn!("Failed to unwatch file {}: {}", path.display(), e);
            }
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
                if let Some(&last_time) = self.last_file_events.get(&path) {
                    if now.duration_since(last_time) < Duration::from_millis(500) {
                        return;
                    }
                }
                self.last_file_events.insert(path.clone(), now);
                if let Some(tab_index) = self.find_tab_by_path(&path) {
                    if let Some(Tab::Editor(editor_tab)) = self.tabs.get(tab_index) {
                        if editor_tab.modified {
                            let is_active = self.active_tab_index == Some(tab_index);

                            if is_active {
                                self.show_file_conflict_dialog(path, tab_index, window, cx);
                            } else {
                                self.pending_conflicts.insert(path, tab_index);
                            }
                        } else {
                            self.reload_tab_from_disk(tab_index, window, cx);
                            self.show_notification_file_reloaded(&path, window, cx);
                        }
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
            if let Tab::Editor(editor_tab) = tab {
                if let Some(path) = &editor_tab.file_path {
                    if let Err(e) = watcher.watch_file(path.clone()) {
                        log::warn!("Failed to watch file {}: {}", path.display(), e);
                    }
                }
            }
        }
        self.file_watcher = Some(watcher);
        self.file_watch_events = Some(receiver);
    }

    /// Stop the file watcher
    pub fn stop_file_watcher(&mut self) {
        if let Some(mut watcher) = self.file_watcher.take() {
            watcher.stop();
        }
        self.file_watch_events = None;
    }

    /// Add a file to the watcher
    ///
    /// ### Arguments
    /// - `path`: The path to the file to watch
    pub fn watch_file(&mut self, path: &PathBuf) {
        if let Some(watcher) = &mut self.file_watcher {
            if let Err(e) = watcher.watch_file(path.clone()) {
                log::warn!("Failed to watch file {}: {}", path.display(), e);
            }
        }
    }

    /// Remove a file from the watcher
    ///
    /// ### Arguments
    /// - `path`: The path to the file to unwatch
    pub fn unwatch_file(&mut self, path: &PathBuf) {
        if let Some(watcher) = &mut self.file_watcher {
            watcher.unwatch_file(path);
        }
    }
}
