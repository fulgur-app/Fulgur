pub(crate) mod db;
mod operations;
mod persistence;
mod writer;

pub use db::{StateDb, import_legacy_json};
pub(crate) use operations::schedule_session_autosave;
pub use operations::{TabRestoreDecision, determine_tab_restore_strategy};
pub use persistence::{
    MAX_REMOTE_HISTORY_ENTRIES, RemoteHistory, SerializedRemoteSpec, SerializedWindowBounds,
    TabContent, TabState, WindowState, WindowsState, get_file_modified_time, is_file_newer,
};
pub use writer::StateWriter;
