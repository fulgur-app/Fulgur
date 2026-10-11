use super::watcher::FileWatcher;
use gpui_kit::Task;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

/// A re-check of a watched path that runs once its watch events go quiet
pub struct ScheduledFileCheck {
    /// When the first event of the current burst arrived, on the executor clock
    pub first_event_at: Instant,
    /// Timer task running the check; dropping it cancels the check
    pub task: Task<()>,
}

/// File watching state for external file change detection
pub struct FileWatchState {
    pub file_watcher: Option<FileWatcher>,
    /// Consumer task awaiting file watch events; dropping it cancels the consumer.
    pub consumer_task: Option<Task<()>>,
    pub scheduled_checks: HashMap<PathBuf, ScheduledFileCheck>,
    pub pending_conflicts: HashMap<PathBuf, usize>,
    pub open_conflict_dialogs: HashSet<PathBuf>,
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
            scheduled_checks: HashMap::new(),
            pending_conflicts: HashMap::new(),
            open_conflict_dialogs: HashSet::new(),
        }
    }
}
