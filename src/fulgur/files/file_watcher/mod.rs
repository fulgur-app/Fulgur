mod event_handling;
mod lifecycle;
mod state;
mod suppression;
mod watcher;

pub use state::FileWatchState;
pub use watcher::{FileWatchEvent, FileWatcher};
