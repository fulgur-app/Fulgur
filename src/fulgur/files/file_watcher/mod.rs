mod event_handling;
mod lifecycle;
mod state;
mod suppression;
mod watcher;

#[cfg(all(test, feature = "gpui-test-support"))]
mod test_helpers;

pub use state::FileWatchState;
pub use watcher::{FileWatchEvent, FileWatcher};
