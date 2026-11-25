mod encoding;
mod open_ops;
mod remote_open;
mod remote_processing;
mod remote_save;
mod remote_ssh_task;
mod remote_types;
mod save_ops;

pub use encoding::{
    DecodedContents, EncodedContents, binary_file_notice, decode_text_file,
    detect_encoding_and_decode, encode_for_save, encode_owned_for_save, large_file_notice,
    looks_binary,
};
pub use remote_types::{
    PendingRemoteOpenOutcome, RemoteBrowseResult, RemoteFileResult, RemoteOpenResult,
    RemoteSaveContinuation, RemoteSaveRequest,
};
pub(crate) use save_ops::{DiskChangePolicy, EncodedSave, SnapshotFingerprint, disk_changed_since};

#[cfg(test)]
pub mod test_helpers;
