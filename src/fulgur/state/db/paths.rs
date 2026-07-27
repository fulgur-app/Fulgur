//! Conversion between OS paths and the byte strings stored in the database.

use std::path::{Path, PathBuf};

/// Encode a path into the bytes persisted in a `BLOB` column.
///
/// ### Arguments
/// - `path`: The path to encode
///
/// ### Returns
/// - `Vec<u8>`: The encoded path bytes
pub fn path_to_bytes(path: &Path) -> Vec<u8> {
    path.as_os_str().as_encoded_bytes().to_vec()
}

/// Decode path bytes read back from a `BLOB` column on Unix.
///
/// ### Arguments
/// - `bytes`: The stored path bytes
///
/// ### Returns
/// - `PathBuf`: The decoded path, byte-for-byte identical to the encoded one
#[cfg(unix)]
pub fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
}

/// Decode path bytes read back from a `BLOB` column on Windows.
///
/// ### Arguments
/// - `bytes`: The stored path bytes
///
/// ### Returns
/// - `PathBuf`: The decoded path
#[cfg(windows)]
pub fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    use std::ffi::OsString;
    match std::str::from_utf8(bytes) {
        Ok(text) => PathBuf::from(OsString::from(text)),
        Err(e) => {
            let lossy = String::from_utf8_lossy(bytes);
            log::warn!("Restored tab path is not valid WTF-8 ({e}), decoding lossily: {lossy}");
            PathBuf::from(OsString::from(lossy.into_owned()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{path_from_bytes, path_to_bytes};
    use std::path::PathBuf;

    #[test]
    fn ascii_path_roundtrips() {
        let path = PathBuf::from("/tmp/fulgur/main.rs");
        assert_eq!(path_from_bytes(&path_to_bytes(&path)), path);
    }

    #[test]
    fn multibyte_path_roundtrips() {
        let path = PathBuf::from("/tmp/dossier/héllo 文档 🚀.txt");
        assert_eq!(path_from_bytes(&path_to_bytes(&path)), path);
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_unix_path_roundtrips() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        // A latin-1 encoded "café.txt", which is not valid UTF-8 and which
        // serde refuses to serialize.
        let raw = b"/tmp/caf\xe9.txt";
        let path = PathBuf::from(OsStr::from_bytes(raw));
        let bytes = path_to_bytes(&path);
        assert_eq!(bytes, raw);
        assert_eq!(path_from_bytes(&bytes), path);
    }
}
