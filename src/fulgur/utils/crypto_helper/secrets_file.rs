//! Sync secrets of a portable Fulgur, stored in a plaintext file on the drive.
//!
//! A portable Fulgur cannot use the OS keychain, which belongs to the host
//! computer, so the keychain entries (`private_key:<id>`, `device_api_key:<id>`)
//! live in `data/secrets.json` as a JSON map of entry name to secret. The file is
//! not encrypted: it is protected by the user keeping the drive, and a lost drive
//! is handled by revoking the device from another device.
//!
//! Only an absent file or an absent entry means "no secret". An unreadable or
//! corrupt file is an error, otherwise `ensure_profile_keypair` would generate a
//! new key pair and break existing shares.

use crate::fulgur::utils::{atomic_write::atomic_write_private_file, paths};
use anyhow::anyhow;
use parking_lot::Mutex;
use std::{collections::BTreeMap, fs, io, path::Path};
use zeroize::{Zeroize, Zeroizing};

/// Name of the secrets file inside the portable data folder.
pub const SECRETS_FILE_NAME: &str = "secrets.json";

/// Serializes every read-modify-write of the secrets file, which sync workers
/// reach from their own threads.
static SECRETS_FILE_LOCK: Mutex<()> = Mutex::new(());

/// The decoded secrets, wiped from memory when dropped.
#[derive(Default)]
struct SecretsMap(BTreeMap<String, String>);

impl Drop for SecretsMap {
    fn drop(&mut self) {
        for secret in self.0.values_mut() {
            secret.zeroize();
        }
    }
}

/// Loads a secret from the secrets file of the portable data folder.
///
/// ### Arguments
/// - `user`: The keychain entry name
///
/// ### Errors
/// - Returns an error if the data folder cannot be resolved, or if the file
///   exists but cannot be read or parsed.
///
/// ### Returns
/// - `Ok(Some(String))`: The stored non-empty secret
/// - `Ok(None)`: The file or the entry does not exist
/// - `Err(anyhow::Error)`: The file is unreadable or corrupt
pub(super) fn load_from_secrets_file(user: &str) -> anyhow::Result<Option<String>> {
    let _guard = SECRETS_FILE_LOCK.lock();
    load_entry(&paths::config_file(SECRETS_FILE_NAME)?, user)
}

/// Saves or removes a secret in the secrets file of the portable data folder.
///
/// ### Arguments
/// - `user`: The keychain entry name
/// - `value`: The secret to save. `None` or empty removes the entry.
///
/// ### Errors
/// - Returns an error if the data folder cannot be resolved, if the existing file
///   is unreadable or corrupt (it is then left untouched), or if the write fails.
///
/// ### Returns
/// - `Ok(())`: The secret was saved or removed
/// - `Err(anyhow::Error)`: The file could not be updated
pub(super) fn save_or_remove_to_secrets_file(
    user: &str,
    value: Option<&str>,
) -> anyhow::Result<()> {
    let _guard = SECRETS_FILE_LOCK.lock();
    save_entry(&paths::config_file(SECRETS_FILE_NAME)?, user, value)
}

/// Read and decode a secrets file.
///
/// ### Arguments
/// - `path`: The secrets file
///
/// ### Errors
/// - Returns an error if the file exists but cannot be read or is not a JSON map
///   of strings. Secret values never appear in the error.
///
/// ### Returns
/// - `Ok(Some(SecretsMap))`: The decoded secrets
/// - `Ok(None)`: The file does not exist
/// - `Err(anyhow::Error)`: The file is unreadable or corrupt
fn read_map(path: &Path) -> anyhow::Result<Option<SecretsMap>> {
    let contents = match fs::read(path) {
        Ok(bytes) => Zeroizing::new(bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(anyhow!(
                "Failed to read the secrets file '{}': {e}",
                path.display()
            ));
        }
    };
    let map = serde_json::from_slice(&contents)
        .map_err(|e| anyhow!("The secrets file '{}' is corrupt: {e}", path.display()))?;
    Ok(Some(SecretsMap(map)))
}

/// Read one secret from a secrets file.
///
/// ### Arguments
/// - `path`: The secrets file
/// - `user`: The keychain entry name
///
/// ### Errors
/// - Returns an error if the file exists but is unreadable or corrupt.
///
/// ### Returns
/// - `Ok(Some(String))`: The stored non-empty secret
/// - `Ok(None)`: The file or the entry does not exist, or the entry is empty
/// - `Err(anyhow::Error)`: The file is unreadable or corrupt
fn load_entry(path: &Path, user: &str) -> anyhow::Result<Option<String>> {
    Ok(read_map(path)?.and_then(|map| map.0.get(user).filter(|v| !v.is_empty()).cloned()))
}

/// Save or remove one secret, rewriting the whole file atomically.
///
/// ### Arguments
/// - `path`: The secrets file
/// - `user`: The keychain entry name
/// - `value`: The secret to save. `None` or empty removes the entry.
///
/// ### Errors
/// - Returns an error if the existing file is unreadable or corrupt, in which case
///   it is not overwritten, or if serialization or the atomic write fails.
///
/// ### Returns
/// - `Ok(())`: The file holds the requested state
/// - `Err(anyhow::Error)`: The file could not be updated
fn save_entry(path: &Path, user: &str, value: Option<&str>) -> anyhow::Result<()> {
    let mut map = read_map(path)?.unwrap_or_default();
    let previous = match value.filter(|value| !value.is_empty()) {
        Some(value) => map.0.insert(user.to_string(), value.to_string()),
        None => match map.0.remove(user) {
            Some(removed) => Some(removed),
            None => return Ok(()),
        },
    };
    if let Some(mut previous) = previous {
        previous.zeroize();
    }
    let json = Zeroizing::new(
        serde_json::to_vec_pretty(&map.0)
            .map_err(|e| anyhow!("Failed to serialize the secrets file: {e}"))?,
    );
    atomic_write_private_file(path, &json)
}

#[cfg(test)]
mod tests {
    use super::{load_entry, save_entry};
    use std::fs;

    #[test]
    fn test_secrets_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");

        save_entry(&path, "private_key:a", Some("AGE-SECRET-KEY-1")).unwrap();
        save_entry(&path, "device_api_key:a", Some("api-key")).unwrap();

        assert_eq!(
            load_entry(&path, "private_key:a").unwrap().as_deref(),
            Some("AGE-SECRET-KEY-1")
        );
        assert_eq!(
            load_entry(&path, "device_api_key:a").unwrap().as_deref(),
            Some("api-key")
        );
    }

    #[test]
    fn test_absent_file_or_entry_is_no_secret() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");

        assert_eq!(load_entry(&path, "private_key:a").unwrap(), None);
        save_entry(&path, "private_key:a", Some("key")).unwrap();
        assert_eq!(load_entry(&path, "private_key:b").unwrap(), None);
    }

    #[test]
    fn test_corrupt_file_is_an_error_and_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");
        fs::write(&path, "{\"private_key:a\": \"trunc").unwrap();

        assert!(load_entry(&path, "private_key:a").is_err());
        assert!(save_entry(&path, "private_key:b", Some("key")).is_err());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\"private_key:a\": \"trunc"
        );
    }

    #[test]
    fn test_none_or_empty_value_removes_the_entry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");
        save_entry(&path, "private_key:a", Some("key")).unwrap();
        save_entry(&path, "device_api_key:a", Some("api-key")).unwrap();

        save_entry(&path, "private_key:a", None).unwrap();
        save_entry(&path, "device_api_key:a", Some("")).unwrap();

        assert_eq!(load_entry(&path, "private_key:a").unwrap(), None);
        assert_eq!(load_entry(&path, "device_api_key:a").unwrap(), None);
    }

    #[test]
    fn test_removing_from_an_absent_file_does_not_create_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");

        save_entry(&path, "private_key:a", None).unwrap();

        assert!(!path.exists());
    }

    #[test]
    fn test_write_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");

        save_entry(&path, "private_key:a", Some("key")).unwrap();
        save_entry(&path, "private_key:a", Some("rotated")).unwrap();

        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["secrets.json"]);
    }

    #[cfg(unix)]
    #[test]
    fn test_secrets_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");

        save_entry(&path, "private_key:a", Some("key")).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
