use super::watcher::FileWatcher;
use gpui::Task;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

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
    /// - `Self`: A new `FileWatchState`
    fn default() -> Self {
        Self::new()
    }
}

impl FileWatchState {
    /// Create a new `FileWatchState` with all fields initialized to default/empty values
    ///
    /// ### Returns
    /// - `Self`: A new `FileWatchState`
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
