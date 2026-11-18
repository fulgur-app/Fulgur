//! Identity of a Fulgur installation for single-instance coordination.
//!
//! Two Fulgur processes forward files to each other only when they share the
//! same configuration directory. An installed Fulgur, a portable Fulgur and a
//! development instance started with `-d` each get their own IPC channel, derived
//! from a hash of their configuration directory. Platform-neutral so its tests
//! run on every OS.

use sha2::{Digest, Sha256};
use std::path::Path;

/// First loopback port of the range used for Fulgur IPC on Windows.
#[cfg(any(target_os = "windows", test))]
const IPC_PORT_BASE: u16 = 29764;

/// Number of ports in the Windows IPC range.
#[cfg(any(target_os = "windows", test))]
const IPC_PORT_RANGE: u16 = 1000;

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

/// Loopback port of the IPC listener for a configuration directory, on Windows.
///
/// ### Arguments
/// - `config_dir`: The resolved configuration directory
///
/// ### Returns
/// - `u16`: A port in `29764..30764`, stable for a given directory
#[cfg(any(target_os = "windows", test))]
#[must_use]
pub fn ipc_port(config_dir: &Path) -> u16 {
    let hash = instance_hash(config_dir);
    IPC_PORT_BASE + u16::from_be_bytes([hash[0], hash[1]]) % IPC_PORT_RANGE
}

/// Path of the IPC socket for a configuration directory, on Linux.
///
/// ### Arguments
/// - `config_dir`: The resolved configuration directory
/// - `runtime_dir`: `$XDG_RUNTIME_DIR`, a private per-user folder, when set
/// - `fallback_dir`: Folder used when there is no runtime folder
///
/// ### Returns
/// - `PathBuf`: `fulgur-<hash>.sock` in the runtime folder, else in the fallback
#[cfg(any(target_os = "linux", test))]
#[must_use]
pub fn socket_path(
    config_dir: &Path,
    runtime_dir: Option<&Path>,
    fallback_dir: &Path,
) -> std::path::PathBuf {
    let file_name = format!("fulgur-{}.sock", hex::encode(instance_hash(config_dir)));
    runtime_dir.unwrap_or(fallback_dir).join(file_name)
}

#[cfg(test)]
mod tests {
    use super::{IPC_PORT_BASE, IPC_PORT_RANGE, ipc_port, socket_path};
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
    fn test_distinct_config_dirs_get_distinct_ports() {
        let (installed, portable) = installed_and_portable();

        assert_ne!(ipc_port(&installed), ipc_port(&portable));
    }

    #[test]
    fn test_ipc_port_is_stable_and_in_range() {
        let (installed, _) = installed_and_portable();
        let port = ipc_port(&installed);

        assert_eq!(port, ipc_port(&installed));
        assert!((IPC_PORT_BASE..IPC_PORT_BASE + IPC_PORT_RANGE).contains(&port));
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
