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
    detect_encoding_and_decode, encode_for_save, looks_binary,
};
pub use remote_types::{
    PendingRemoteOpenOutcome, RemoteBrowseResult, RemoteFileResult, RemoteOpenResult,
};

#[cfg(test)]
pub mod test_helpers;
