mod encoding;
mod open_ops;
mod remote_open;
mod remote_save;
mod remote_types;
mod save_ops;

pub use encoding::detect_encoding_and_decode;
pub use remote_types::{
    PendingRemoteOpenOutcome, RemoteBrowseResult, RemoteFileResult, RemoteOpenResult,
};

#[cfg(test)]
pub mod test_helpers;
