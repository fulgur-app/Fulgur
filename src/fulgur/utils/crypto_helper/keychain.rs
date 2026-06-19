use keyring_core::Entry;
use zeroize::Zeroizing;

use super::backend::{
    init_keychain_backend, load_from_in_memory_keychain, save_or_remove_to_in_memory_keychain,
    should_use_in_memory_keychain,
};
use super::{DEVICE_API_KEY_PREFIX, PRIVATE_KEY_PREFIX, SERVICE_NAME};

/// Build the keychain user string for a profile's private key entry.
///
/// ### Arguments
/// - `profile_id`: The profile's stable id.
///
/// ### Returns
/// - `String`: Namespaced user string, e.g. `private_key:<id>`.
pub(super) fn private_key_user(profile_id: &str) -> String {
    format!("{PRIVATE_KEY_PREFIX}:{profile_id}")
}

/// Build the keychain user string for a profile's device API key entry.
///
/// ### Arguments
/// - `profile_id`: The profile's stable id.
///
/// ### Returns
/// - `String`: Namespaced user string, e.g. `device_api_key:<id>`.
pub(super) fn device_api_key_user(profile_id: &str) -> String {
    format!("{DEVICE_API_KEY_PREFIX}:{profile_id}")
}

/// Saves a profile's private key in the keychain. Accepts `Zeroizing<String>`
/// to ensure the key material is zeroed on drop.
///
/// ### Arguments
/// - `profile_id`: The profile id used to namespace the keychain entry.
/// - `private_key`: The key to save, wrapped in `Zeroizing` for secure memory
///   handling. `None` deletes the entry.
///
/// ### Errors
/// - Returns an error if the keychain entry cannot be created, written, odeleted.
///
/// ### Returns
/// - `Ok(())`: The private key was saved (or removed) successfully.
/// - `Err(anyhow::Error)`: If the keychain operation failed.
pub fn save_private_key_to_keychain(
    profile_id: &str,
    private_key: Option<&Zeroizing<String>>,
) -> anyhow::Result<()> {
    save_or_remove_to_keychain(
        &private_key_user(profile_id),
        private_key.map(|k| k.as_str()),
    )
}

/// Saves a profile's device API key in the keychain.
///
/// ### Arguments
/// - `profile_id`: The profile id used to namespace the keychain entry.
/// - `device_api_key`: The key to save. `None` or empty deletes the entry.
///
/// ### Errors
/// - Returns an error if the keychain entry cannot be created, written, or deleted.
///
/// ### Returns
/// - `Ok(())`: The device API key was saved (or removed) successfully.
/// - `Err(anyhow::Error)`: If the keychain operation failed.
pub fn save_device_api_key_to_keychain(
    profile_id: &str,
    device_api_key: Option<&str>,
) -> anyhow::Result<()> {
    save_or_remove_to_keychain(&device_api_key_user(profile_id), device_api_key)
}

/// Saves or removes a value from the keychain. If the value is `None`, the entry is removed from the keychain.
///
/// ### Arguments
/// - `user`: the user name (the entry to look for in the keychain)
/// - `value`: the value to save (borrowed to avoid taking ownership of sensitive data)
///
/// ### Returns
/// - `Ok()`: The value has been succesfully saved in the keychain
/// - `Err(anyhow::Error)`: If the value could not be saved
pub(super) fn save_or_remove_to_keychain(user: &str, value: Option<&str>) -> anyhow::Result<()> {
    if should_use_in_memory_keychain() {
        save_or_remove_to_in_memory_keychain(user, value);
        return Ok(());
    }
    init_keychain_backend()?;
    let entry = Entry::new(SERVICE_NAME, user)?;
    if let Some(value) = value
        && !value.is_empty()
    {
        entry.set_password(value)?;
        return Ok(());
    }
    match entry.delete_credential() {
        Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
        Err(e) => Err(anyhow::anyhow!(
            "Failed to remove '{user}' from keychain: {e}"
        )),
    }
}

/// Loads a profile's private key from the keychain, wrapped in `Zeroizing`
/// to ensure the key material is zeroed when dropped.
///
/// ### Arguments
/// - `profile_id`: The profile id used to namespace the keychain entry.
///
/// ### Errors
/// - Returns an error if the keychain access fails for a reason other than a
///   missing entry.
///
/// ### Returns
/// - `Ok(Some(Zeroizing<String>))`: The private key when present.
/// - `Ok(None)`: The keychain has no entry for this profile.
/// - `Err(anyhow::Error)`: If the keychain access failed.
pub fn load_private_key_from_keychain(
    profile_id: &str,
) -> anyhow::Result<Option<Zeroizing<String>>> {
    load_from_keychain(&private_key_user(profile_id)).map(|opt| opt.map(Zeroizing::new))
}

/// Loads a profile's device API key from the keychain.
///
/// ### Arguments
/// - `profile_id`: The profile id used to namespace the keychain entry.
///
/// ### Errors
/// - Returns an error if the keychain access fails for a reason other than a
///   missing entry.
///
/// ### Returns
/// - `Ok(Some(String))`: The device API key when present.
/// - `Ok(None)`: The keychain has no entry for this profile.
/// - `Err(anyhow::Error)`: If the keychain access failed.
pub fn load_device_api_key_from_keychain(profile_id: &str) -> anyhow::Result<Option<String>> {
    load_from_keychain(&device_api_key_user(profile_id))
}

/// Loads a value from the keychain
///
/// ### Arguments
/// - `user`: the user name (the entry to look for in the keychain)
///
/// ### Returns
/// - `Ok(Option<String>)`: The value if it exists, otherwise `None`
/// - `Err(anyhow::Error)`: If the value could not be loaded
pub(super) fn load_from_keychain(user: &str) -> anyhow::Result<Option<String>> {
    if should_use_in_memory_keychain() {
        return Ok(load_from_in_memory_keychain(user));
    }
    init_keychain_backend()?;
    let entry = Entry::new(SERVICE_NAME, user)?;
    match entry.get_password() {
        Ok(value) if value.is_empty() => {
            // Legacy behavior stored empty strings instead of deleting credentials.
            // TODO: remove this in further version.
            log::warn!(
                "Keychain entry '{user}' is empty; treating as missing and removing stale credential"
            );
            match entry.delete_credential() {
                Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(None),
                Err(e) => Err(anyhow::anyhow!(
                    "Failed to clean up empty '{user}' keychain entry: {e}"
                )),
            }
        }
        Ok(value) => Ok(Some(value)),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::anyhow!(
            "Failed to load '{user}' from keychain: {e}"
        )),
    }
}
