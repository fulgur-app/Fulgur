//! Identity of a Fulgur installation for single-instance coordination.
//!
//! Two Fulgur processes forward files to each other only when they share the
//! same configuration directory. An installed Fulgur, a portable Fulgur and a
//! development instance started with `-d` each get their own IPC channel, derived
//! from a hash of their configuration directory. Platform-neutral so its tests
//! run on every OS.

use sha2::{Digest, Sha256};
use std::path::Path;

/// Short, stable identifier of a configuration directory.
///
/// ### Arguments
/// - `config_dir`: The resolved configuration directory
///
/// ### Returns
/// - `[u8; 8]`: The first eight bytes of the SHA-256 of the directory path
fn instance_hash(config_dir: &Path) -> [u8; 8] {
    let digest = Sha256::digest(config_dir.as_os_str().as_encoded_bytes());
    let mut hash = [0; 8];
    hash.copy_from_slice(&digest[..8]);
    hash
}

/// Hex identifier of a configuration directory, used to name its IPC channel.
///
/// ### Arguments
/// - `config_dir`: The resolved configuration directory
///
/// ### Returns
/// - `String`: 16 lowercase hex characters, stable for a given directory
#[must_use]
pub fn instance_id(config_dir: &Path) -> String {
    hex::encode(instance_hash(config_dir))
}

/// Path of the IPC socket for a configuration directory, on Linux and macOS.
///
/// ### Arguments
/// - `config_dir`: The resolved configuration directory
/// - `runtime_dir`: `$XDG_RUNTIME_DIR`, a private per-user folder, when set
/// - `fallback_dir`: Folder used when there is no runtime folder
///
/// ### Returns
/// - `PathBuf`: `fulgur-<hash>.sock` in the runtime folder, else in the fallback
#[cfg(any(unix, test))]
#[must_use]
pub fn socket_path(
    config_dir: &Path,
    runtime_dir: Option<&Path>,
    fallback_dir: &Path,
) -> std::path::PathBuf {
    let file_name = format!("fulgur-{}.sock", instance_id(config_dir));
    runtime_dir.unwrap_or(fallback_dir).join(file_name)
}

#[cfg(test)]
mod tests {
    use super::{instance_id, socket_path};
    use std::path::{Path, PathBuf};

    /// Configuration directories of an installed and a portable Fulgur.
    ///
    /// ### Returns
    /// - `(PathBuf, PathBuf)`: The installed and the portable directory
    fn installed_and_portable() -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir();
        (
            base.join("profile").join(".fulgur"),
            base.join("stick").join("Fulgur").join("data"),
        )
    }

    #[test]
    fn test_distinct_config_dirs_get_distinct_ids() {
        let (installed, portable) = installed_and_portable();

        assert_ne!(instance_id(&installed), instance_id(&portable));
    }

    #[test]
    fn test_instance_id_is_stable_hex() {
        let (installed, _) = installed_and_portable();
        let id = instance_id(&installed);

        assert_eq!(id, instance_id(&installed));
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_distinct_config_dirs_get_distinct_sockets() {
        let (installed, portable) = installed_and_portable();
        let runtime = Path::new("run");

        assert_ne!(
            socket_path(&installed, Some(runtime), runtime),
            socket_path(&portable, Some(runtime), runtime)
        );
    }

    #[test]
    fn test_socket_prefers_the_runtime_dir_over_the_fallback() {
        let (installed, _) = installed_and_portable();
        let runtime = Path::new("run");
        let fallback = Path::new("tmp");

        assert!(socket_path(&installed, Some(runtime), fallback).starts_with(runtime));
        assert!(socket_path(&installed, None, fallback).starts_with(fallback));
    }
}
