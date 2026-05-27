use crate::fulgur::settings::{ServerProfile, Settings};
use age::{
    secrecy::ExposeSecret,
    x25519::{Identity, Recipient},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use keyring::Entry;
use std::{
    collections::HashMap,
    ffi::OsStr,
    sync::{Mutex, OnceLock},
};
use zeroize::Zeroizing;

// Prefixes used to namespace per-profile entries inside the keychain.
const PRIVATE_KEY_PREFIX: &str = "private_key";
const DEVICE_API_KEY_PREFIX: &str = "device_api_key";

const SERVICE_NAME: &str = "Fulgur";

/// Build the keychain user string for a profile's private key entry.
///
/// ### Arguments
/// - `profile_id`: The profile's stable id.
///
/// ### Returns
/// - `String`: Namespaced user string, e.g. `private_key:<id>`.
fn private_key_user(profile_id: &str) -> String {
    format!("{PRIVATE_KEY_PREFIX}:{profile_id}")
}

/// Build the keychain user string for a profile's device API key entry.
///
/// ### Arguments
/// - `profile_id`: The profile's stable id.
///
/// ### Returns
/// - `String`: Namespaced user string, e.g. `device_api_key:<id>`.
fn device_api_key_user(profile_id: &str) -> String {
    format!("{DEVICE_API_KEY_PREFIX}:{profile_id}")
}

/// Checks whether an environment variable contains a truthy value.
///
/// Accepted truthy values are: `1`, `true`, `yes`, `on` (case-insensitive).
///
/// ### Arguments
/// - `name`: The environment variable name to evaluate.
///
/// ### Returns
/// - `true`: If the variable exists and is set to a recognized truthy value.
/// - `false`: Otherwise.
fn env_var_is_truthy(name: &str) -> bool {
    matches!(
        std::env::var(name)
            .ok()
            .map(|v| v.to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

/// Determines whether keychain operations should use the in-memory backend.
///
/// This avoids interactive keychain prompts during `cargo test` and in CI.
/// Set `FULGUR_USE_REAL_KEYCHAIN=1` to force real keychain access.
///
/// Precedence:
/// 1. `FULGUR_USE_REAL_KEYCHAIN=1` always forces the real keychain backend.
/// 2. `FULGUR_DISABLE_KEYCHAIN=1` forces the in-memory backend.
/// 3. `CI=1` forces the in-memory backend.
/// 4. Test binary heuristics (`target/*/deps/*-<hash>`) use the in-memory backend.
///
/// ### Returns
/// - `true`: Use in-memory keychain storage.
/// - `false`: Use the platform keychain backend.
fn should_use_in_memory_keychain() -> bool {
    if env_var_is_truthy("FULGUR_USE_REAL_KEYCHAIN") {
        return false;
    }
    if env_var_is_truthy("FULGUR_DISABLE_KEYCHAIN") {
        return true;
    }
    if env_var_is_truthy("CI") {
        return true;
    }
    // `cargo test` binaries are typically emitted under `target/*/deps/`.
    if let Ok(exe) = std::env::current_exe() {
        let in_deps_dir = exe.parent().is_some_and(|parent| parent.ends_with("deps"));
        let has_hashed_test_name = exe
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.contains('-'));
        if in_deps_dir && has_hashed_test_name {
            return true;
        }
    }
    false
}

/// Returns the process-local in-memory keychain store.
///
/// ### Returns
/// - `&'static Mutex<HashMap<String, String>>`: Shared in-memory credential store.
fn in_memory_keychain() -> &'static Mutex<HashMap<String, String>> {
    static STORE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Builds the in-memory storage key for a keychain user entry.
///
/// ### Arguments
/// - `user`: The keychain entry name (for example, `private_key`).
///
/// ### Returns
/// - `String`: A namespaced key using the service name and user.
fn in_memory_key(user: &str) -> String {
    format!("{SERVICE_NAME}:{user}")
}

/// Saves or removes a value in the in-memory keychain backend.
///
/// If `value` is `None` or empty, the entry is removed.
///
/// ### Arguments
/// - `user`: The keychain entry name.
/// - `value`: The value to save.
///
/// ### Returns
/// - `Ok(())`: The in-memory operation succeeded.
/// - `Err(anyhow::Error)`: The in-memory store lock failed.
fn save_or_remove_to_in_memory_keychain(user: &str, value: Option<&str>) -> anyhow::Result<()> {
    let mut keychain = in_memory_keychain()
        .lock()
        .map_err(|_| anyhow::anyhow!("Failed to lock in-memory keychain"))?;
    let key = in_memory_key(user);
    if let Some(value) = value
        && !value.is_empty()
    {
        keychain.insert(key, value.to_string());
    } else {
        keychain.remove(&key);
    }
    Ok(())
}

/// Loads a value from the in-memory keychain backend.
///
/// ### Arguments
/// - `user`: The keychain entry name.
///
/// ### Returns
/// - `Ok(Some(String))`: The value exists.
/// - `Ok(None)`: The value does not exist.
/// - `Err(anyhow::Error)`: The in-memory store lock failed.
fn load_from_in_memory_keychain(user: &str) -> anyhow::Result<Option<String>> {
    let keychain = in_memory_keychain()
        .lock()
        .map_err(|_| anyhow::anyhow!("Failed to lock in-memory keychain"))?;
    Ok(keychain.get(&in_memory_key(user)).cloned())
}

/// Generate a matching pair of private/public keys
///
/// ### Returns
/// - `(Identity, Recipient)`: The generated (private, public) key pair
pub fn generate_key_pair() -> (Identity, Recipient) {
    let private_key = age::x25519::Identity::generate();
    let public_key = private_key.to_public();
    log::info!("New public and private keys generated!");
    (private_key, public_key)
}

/// Saves a profile's private key in the keychain. Accepts `Zeroizing<String>`
/// to ensure the key material is zeroed on drop.
///
/// ### Arguments
/// - `profile_id`: The profile id used to namespace the keychain entry.
/// - `private_key`: The key to save, wrapped in `Zeroizing` for secure memory
///   handling. `None` deletes the entry.
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
fn save_or_remove_to_keychain(user: &str, value: Option<&str>) -> anyhow::Result<()> {
    if should_use_in_memory_keychain() {
        return save_or_remove_to_in_memory_keychain(user, value);
    }
    let entry = Entry::new(SERVICE_NAME, user)?;
    if let Some(value) = value
        && !value.is_empty()
    {
        entry.set_password(value)?;
        return Ok(());
    }
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
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
/// ### Returns
/// - `Ok(Some(String))`: The device API key when present.
/// - `Ok(None)`: The keychain has no entry for this profile.
/// - `Err(anyhow::Error)`: If the keychain access failed.
pub fn load_device_api_key_from_keychain(profile_id: &str) -> anyhow::Result<Option<String>> {
    load_from_keychain(&device_api_key_user(profile_id))
}

/// Migrate legacy single-server keychain entries into per-profile entries
/// for the given profile id.
///
/// ### Arguments
/// - `profile_id`: The id of the profile that should receive the migrated
///   credentials (typically the migrated "Fulgurant" profile).
///
/// ### Returns
/// - `Ok(())`: Migration completed (or had nothing to migrate).
/// - `Err(anyhow::Error)`: If a keychain operation failed; the legacy entries
///   are left in place so a future startup can retry.
pub fn migrate_legacy_keychain_to_profile(profile_id: &str) -> anyhow::Result<()> {
    if let Some(legacy_private) = load_from_keychain(PRIVATE_KEY_PREFIX)? {
        let target_user = private_key_user(profile_id);
        if load_from_keychain(&target_user)?.is_none() {
            log::info!("Migrating legacy private key to profile '{profile_id}'");
            save_or_remove_to_keychain(&target_user, Some(&legacy_private))?;
        }
        save_or_remove_to_keychain(PRIVATE_KEY_PREFIX, None)?;
    }
    if let Some(legacy_api) = load_from_keychain(DEVICE_API_KEY_PREFIX)? {
        let target_user = device_api_key_user(profile_id);
        if load_from_keychain(&target_user)?.is_none() {
            log::info!("Migrating legacy device API key to profile '{profile_id}'");
            save_or_remove_to_keychain(&target_user, Some(&legacy_api))?;
        }
        save_or_remove_to_keychain(DEVICE_API_KEY_PREFIX, None)?;
    }
    Ok(())
}

/// Detect whether legacy single-server keychain entries (`Fulgur:private_key` and/or `Fulgur:device_api_key`) are still present.
///
/// ### Returns
/// - `Ok(true)`: At least one legacy entry exists.
/// - `Ok(false)`: No legacy entries are present.
/// - `Err(anyhow::Error)`: A keychain access failed.
fn legacy_keychain_entries_present() -> anyhow::Result<bool> {
    Ok(load_from_keychain(PRIVATE_KEY_PREFIX)?.is_some()
        || load_from_keychain(DEVICE_API_KEY_PREFIX)?.is_some())
}

/// Migrate legacy single-server keychain entries into the first configured profile, regardless of whether sync is currently activated.
///
/// ### Arguments
/// - `settings`: Application settings used to locate the target profile.
///
/// ### Returns
/// - `Ok(())`: Migration completed or there was nothing to migrate.
/// - `Err(anyhow::Error)`: A keychain operation failed.
pub fn migrate_legacy_keychain_entries_if_present(settings: &Settings) -> anyhow::Result<()> {
    if !legacy_keychain_entries_present()? {
        return Ok(());
    }
    let Some(target_profile_id) = settings
        .app_settings
        .synchronization_settings
        .profiles
        .first()
        .map(|profile| profile.id.clone())
    else {
        log::warn!(
            "Legacy keychain entries detected but no profiles are configured; leaving entries in place"
        );
        return Ok(());
    };
    log::info!(
        "Migrating legacy keychain entries to profile '{target_profile_id}' (first configured profile)"
    );
    migrate_legacy_keychain_to_profile(&target_profile_id)
}

/// Loads a value from the keychain
///
/// ### Arguments
/// - `user`: the user name (the entry to look for in the keychain)
///
/// ### Returns
/// - `Ok(Option<String>)`: The value if it exists, otherwise `None`
/// - `Err(anyhow::Error)`: If the value could not be loaded
fn load_from_keychain(user: &str) -> anyhow::Result<Option<String>> {
    if should_use_in_memory_keychain() {
        return load_from_in_memory_keychain(user);
    }
    let entry = Entry::new(SERVICE_NAME, user)?;
    match entry.get_password() {
        Ok(value) if value.is_empty() => {
            // Legacy behavior stored empty strings instead of deleting credentials.
            // TODO: remove this in further version.
            log::warn!(
                "Keychain entry '{user}' is empty; treating as missing and removing stale credential"
            );
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(anyhow::anyhow!(
                    "Failed to clean up empty '{user}' keychain entry: {e}"
                )),
            }
        }
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::anyhow!(
            "Failed to load '{user}' from keychain: {e}"
        )),
    }
}

/// Serializes the private key to a zeroizing string. The returned `Zeroizing<String>`
/// ensures the key material is overwritten with zeros when dropped.
///
/// ### Arguments
/// - `private_key`: the private key to serialize
///
/// ### Returns
/// - `Zeroizing<String>`: The serialized private key, zeroed on drop
pub fn serialize(private_key: &Identity) -> Zeroizing<String> {
    let secret = private_key.to_string();
    Zeroizing::new(secret.expose_secret().to_string())
}

/// Ensure a single profile has a valid X25519 keypair in the keychain and a paired `public_key` set on the profile struct.
///
/// The function does not save settings:  the caller is responsible for
/// persistence so the new public key is broadcast atomically with any
/// other change.
///
/// ### Arguments
/// - `profile`: The profile that may need a keypair.
///
/// ### Returns
/// - `Ok(true)`: The profile's `public_key` was updated (either generated or
///   recovered); the caller must persist settings.
/// - `Ok(false)`: The keypair already exists in full; no change was made.
/// - `Err(anyhow::Error)`: A keychain operation or key parsing/generation
///   failed.
pub fn ensure_profile_keypair(profile: &mut ServerProfile) -> anyhow::Result<bool> {
    let private_key = load_private_key_from_keychain(&profile.id)?;
    if let Some(private_key_str) = private_key {
        if profile.public_key.is_some() {
            log::debug!("Profile '{}' already has a keypair", profile.name);
            return Ok(false);
        }
        log::info!(
            "Profile '{}' has a stored private key but no public key; recovering public key",
            profile.name
        );
        let identity: Identity = private_key_str
            .parse()
            .map_err(|e| anyhow::anyhow!("Failed to parse stored private key: {e}"))?;
        profile.public_key = Some(identity.to_public().to_string());
        return Ok(true);
    }
    log::debug!(
        "Profile '{}' has no keypair, generating a new one",
        profile.name
    );
    let (new_private_key, new_public_key) = generate_key_pair();
    save_private_key_to_keychain(&profile.id, Some(&serialize(&new_private_key)))?;
    profile.public_key = Some(new_public_key.to_string());
    Ok(true)
}

/// Ensure each active profile has a valid X25519 keypair available in the
/// keychain and a paired public key in settings.
///
/// ### Arguments
/// - `settings`: The application settings; profiles whose keys had to be
///   generated will be mutated to carry the new public key.
///
/// ### Returns
/// - `Ok(())`: All active profiles have valid keys.
/// - `Err(anyhow::Error)`: If keychain access or key generation failed for
///   any profile (the operation stops at the first failure).
pub fn check_private_public_keys(settings: &mut Settings) -> anyhow::Result<()> {
    if !settings
        .app_settings
        .synchronization_settings
        .is_synchronization_activated
    {
        return Ok(());
    }
    let mut settings_changed = false;
    let profile_ids: Vec<String> = settings
        .app_settings
        .synchronization_settings
        .profiles
        .iter()
        .filter(|p| p.is_active)
        .map(|p| p.id.clone())
        .collect();
    for profile_id in profile_ids {
        let Some(profile) = settings
            .app_settings
            .synchronization_settings
            .profiles
            .iter_mut()
            .find(|p| p.id == profile_id)
        else {
            continue;
        };
        if ensure_profile_keypair(profile)? {
            settings_changed = true;
        }
    }
    if settings_changed {
        log::debug!("Saving updated settings after key generation");
        settings.save()?;
    }
    Ok(())
}

/// Checks whether a string is a valid age x25519 public key.
///
/// ### Arguments
/// - `key`: The string to validate (expected format: `age1...`)
///
/// ### Returns
/// - `bool`: `true` if the string parses as a valid `age::x25519::Recipient`, `false` otherwise
pub fn is_valid_public_key(key: &str) -> bool {
    key.parse::<Recipient>().is_ok()
}

/// Encrypt bytes (e.g., compressed data) for file sharing using age encryption
///
/// ### Arguments
/// - `content_bytes`: The bytes to encrypt
/// - `recipient_public_key`: The recipient's age x25519 public key (format: "age1...")
///
/// ### Returns
/// - `Ok(String)`: The base64-encoded encrypted content
/// - `Err(anyhow::Error)`: If the encryption failed
pub fn encrypt_bytes(content_bytes: &[u8], recipient_public_key: &str) -> anyhow::Result<String> {
    let recipient: Recipient = recipient_public_key
        .parse()
        .map_err(|e| anyhow::anyhow!("Failed to parse recipient public key: {e}"))?;
    let recipients: Vec<Box<dyn age::Recipient>> = vec![Box::new(recipient)];
    let encryptor =
        age::Encryptor::with_recipients(recipients.iter().map(std::convert::AsRef::as_ref))
            .map_err(|e| anyhow::anyhow!("Failed to create encryptor: {e}"))?;
    let mut encrypted = vec![];
    let mut writer = encryptor
        .wrap_output(&mut encrypted)
        .map_err(|e| anyhow::anyhow!("Failed to create encryption writer: {e}"))?;
    std::io::Write::write_all(&mut writer, content_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to write encrypted data: {e}"))?;
    writer
        .finish()
        .map_err(|e| anyhow::anyhow!("Failed to finish encryption: {e}"))?;
    // Encode to base64 for transmission
    Ok(BASE64.encode(encrypted))
}

/// Decrypt bytes (e.g., compressed data) received from another device using age decryption
///
/// ### Arguments
/// - `encrypted_b64`: Base64-encoded encrypted content
/// - `private_key_str`: The recipient's age x25519 private key
///
/// ### Returns
/// - `Ok(Vec<u8>)`: The decrypted bytes
/// - `Err(anyhow::Error)`: If the decryption failed
pub fn decrypt_bytes(encrypted_b64: &str, private_key_str: &str) -> anyhow::Result<Vec<u8>> {
    let encrypted = BASE64
        .decode(encrypted_b64)
        .map_err(|e| anyhow::anyhow!("Failed to decode base64: {e}"))?;
    let identity: Identity = private_key_str
        .parse()
        .map_err(|e| anyhow::anyhow!("Failed to parse private key: {e}"))?;
    let decryptor = age::Decryptor::new(&encrypted[..])
        .map_err(|e| anyhow::anyhow!("Failed to create decryptor: {e}"))?;
    let mut decrypted = vec![];
    let mut reader = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(|e| anyhow::anyhow!("Failed to decrypt: {e}"))?;
    std::io::Read::read_to_end(&mut reader, &mut decrypted)
        .map_err(|e| anyhow::anyhow!("Failed to read decrypted data: {e}"))?;
    Ok(decrypted)
}

#[cfg(test)]
mod tests {
    use super::{
        DEVICE_API_KEY_PREFIX, PRIVATE_KEY_PREFIX, decrypt_bytes, device_api_key_user,
        encrypt_bytes, ensure_profile_keypair, generate_key_pair, is_valid_public_key,
        load_device_api_key_from_keychain, load_from_keychain, load_private_key_from_keychain,
        migrate_legacy_keychain_entries_if_present, private_key_user, save_or_remove_to_keychain,
        save_private_key_to_keychain, serialize,
    };
    use crate::fulgur::settings::{ServerProfile, Settings};

    #[test]
    fn test_encrypt_decrypt_bytes() {
        // Generate a key pair for testing
        let (private_key, public_key) = generate_key_pair();
        let public_key_str = public_key.to_string();
        let private_key_str = serialize(&private_key);

        let original_bytes = b"This is a test file content with some data!";
        let encrypted =
            encrypt_bytes(original_bytes, &public_key_str).expect("Encryption should succeed");
        assert_ne!(encrypted, String::from_utf8_lossy(original_bytes));
        let decrypted =
            decrypt_bytes(&encrypted, &private_key_str).expect("Decryption should succeed");
        assert_eq!(decrypted, original_bytes);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertext() {
        // Generate a key pair for testing
        let (private_key, public_key) = generate_key_pair();
        let public_key_str = public_key.to_string();
        let private_key_str = serialize(&private_key);

        let content_bytes = b"Same content";
        let encrypted1 = encrypt_bytes(content_bytes, &public_key_str).unwrap();
        let encrypted2 = encrypt_bytes(content_bytes, &public_key_str).unwrap();
        // Age encryption uses random nonces, so ciphertexts should differ
        assert_ne!(encrypted1, encrypted2);
        assert_eq!(
            decrypt_bytes(&encrypted1, &private_key_str).unwrap(),
            content_bytes
        );
        assert_eq!(
            decrypt_bytes(&encrypted2, &private_key_str).unwrap(),
            content_bytes
        );
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        // Generate two different key pairs
        let (_private_key1, public_key1) = generate_key_pair();
        let (private_key2, _public_key2) = generate_key_pair();
        let public_key1_str = public_key1.to_string();
        let private_key2_str = serialize(&private_key2);

        let content_bytes = b"Secret data";
        // Encrypt with public_key1
        let encrypted = encrypt_bytes(content_bytes, &public_key1_str).unwrap();
        // Try to decrypt with private_key2 (should fail)
        let result = decrypt_bytes(&encrypted, &private_key2_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_valid_public_key_accepts_generated_key() {
        let (_, public_key) = generate_key_pair();
        assert!(is_valid_public_key(&public_key.to_string()));
    }

    #[test]
    fn test_is_valid_public_key_rejects_garbage() {
        assert!(!is_valid_public_key("not-a-valid-age-key"));
        assert!(!is_valid_public_key(""));
        assert!(!is_valid_public_key("age1invalidchars!!!"));
    }

    #[test]
    fn test_is_valid_public_key_rejects_private_key() {
        let (private_key, _) = generate_key_pair();
        let private_str = serialize(&private_key);
        assert!(!is_valid_public_key(&private_str));
    }

    /// Serialize migration-related tests so concurrent runs cannot stomp on the
    /// shared legacy keychain entry names.
    fn migration_test_lock() -> &'static std::sync::Mutex<()> {
        use std::sync::OnceLock;
        static LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    fn clear_legacy_keychain_entries() {
        let _ = save_or_remove_to_keychain(PRIVATE_KEY_PREFIX, None);
        let _ = save_or_remove_to_keychain(DEVICE_API_KEY_PREFIX, None);
    }

    #[test]
    fn test_migrate_legacy_keychain_entries_copies_to_first_profile_and_deletes_legacy() {
        let _guard = migration_test_lock().lock().unwrap();
        clear_legacy_keychain_entries();

        save_or_remove_to_keychain(PRIVATE_KEY_PREFIX, Some("legacy-private")).unwrap();
        save_or_remove_to_keychain(DEVICE_API_KEY_PREFIX, Some("legacy-device-key")).unwrap();

        let mut settings = Settings::new();
        let profile = ServerProfile::new("Fulgurant");
        let target_id = profile.id.clone();
        settings
            .app_settings
            .synchronization_settings
            .profiles
            .push(profile);

        migrate_legacy_keychain_entries_if_present(&settings)
            .expect("migration should succeed when legacy entries exist");

        // Legacy entries are gone.
        assert!(
            load_from_keychain(PRIVATE_KEY_PREFIX).unwrap().is_none(),
            "legacy private key entry must be deleted"
        );
        assert!(
            load_from_keychain(DEVICE_API_KEY_PREFIX).unwrap().is_none(),
            "legacy device API key entry must be deleted"
        );
        // Per-profile entries carry the migrated values.
        let migrated_private = load_from_keychain(&private_key_user(&target_id))
            .unwrap()
            .expect("private key must be migrated under the new profile");
        assert_eq!(migrated_private, "legacy-private");
        let migrated_api = load_from_keychain(&device_api_key_user(&target_id))
            .unwrap()
            .expect("device API key must be migrated under the new profile");
        assert_eq!(migrated_api, "legacy-device-key");

        // Cleanup.
        let _ = save_or_remove_to_keychain(&private_key_user(&target_id), None);
        let _ = save_or_remove_to_keychain(&device_api_key_user(&target_id), None);
    }

    #[test]
    fn test_migrate_legacy_keychain_entries_is_noop_when_no_legacy_entries() {
        let _guard = migration_test_lock().lock().unwrap();
        clear_legacy_keychain_entries();

        let mut settings = Settings::new();
        let profile = ServerProfile::new("Fulgurant");
        let target_id = profile.id.clone();
        settings
            .app_settings
            .synchronization_settings
            .profiles
            .push(profile);

        // Pre-state: no entries anywhere for this profile.
        assert!(
            load_private_key_from_keychain(&target_id)
                .unwrap()
                .is_none()
        );
        assert!(
            load_device_api_key_from_keychain(&target_id)
                .unwrap()
                .is_none()
        );

        migrate_legacy_keychain_entries_if_present(&settings)
            .expect("migration without legacy entries must be a no-op");

        // No new entries should have been created.
        assert!(
            load_private_key_from_keychain(&target_id)
                .unwrap()
                .is_none(),
            "no per-profile private key should be created when legacy is absent"
        );
        assert!(
            load_device_api_key_from_keychain(&target_id)
                .unwrap()
                .is_none(),
            "no per-profile device API key should be created when legacy is absent"
        );
    }

    #[test]
    fn test_migrate_legacy_keychain_entries_warns_and_skips_when_no_profiles() {
        let _guard = migration_test_lock().lock().unwrap();
        clear_legacy_keychain_entries();

        save_or_remove_to_keychain(PRIVATE_KEY_PREFIX, Some("orphan-private")).unwrap();
        save_or_remove_to_keychain(DEVICE_API_KEY_PREFIX, Some("orphan-device-key")).unwrap();

        let settings = Settings::new(); // no profiles configured
        migrate_legacy_keychain_entries_if_present(&settings)
            .expect("migration must succeed even when there are no profiles");

        // Legacy entries must be left in place so a later activation can recover them.
        assert_eq!(
            load_from_keychain(PRIVATE_KEY_PREFIX).unwrap().as_deref(),
            Some("orphan-private"),
            "legacy private key must be preserved when no profile exists to receive it"
        );
        assert_eq!(
            load_from_keychain(DEVICE_API_KEY_PREFIX)
                .unwrap()
                .as_deref(),
            Some("orphan-device-key"),
            "legacy device API key must be preserved when no profile exists to receive it"
        );

        // Cleanup so other tests start fresh.
        clear_legacy_keychain_entries();
    }

    #[test]
    fn test_ensure_profile_keypair_generates_when_keychain_is_empty() {
        let mut profile = ServerProfile::new("Fresh");
        assert!(profile.public_key.is_none());
        let generated = ensure_profile_keypair(&mut profile)
            .expect("keypair generation should succeed when keychain is empty");
        assert!(generated, "must report that a keypair was created");
        let public_key = profile
            .public_key
            .as_deref()
            .expect("public_key should be set after generation");
        assert!(
            is_valid_public_key(public_key),
            "generated public_key should parse as a valid age recipient"
        );
        assert!(
            load_private_key_from_keychain(&profile.id)
                .unwrap()
                .is_some(),
            "private key must be persisted in the keychain after generation"
        );
        // Cleanup.
        let _ = save_or_remove_to_keychain(&private_key_user(&profile.id), None);
    }

    #[test]
    fn test_ensure_profile_keypair_is_noop_when_both_halves_present() {
        let mut profile = ServerProfile::new("Already-Has-Keys");
        // Seed both halves.
        let (private_key, public_key) = generate_key_pair();
        save_private_key_to_keychain(&profile.id, Some(&serialize(&private_key))).unwrap();
        let original_public = public_key.to_string();
        profile.public_key = Some(original_public.clone());

        let generated = ensure_profile_keypair(&mut profile)
            .expect("noop path should succeed when both halves are present");
        assert!(
            !generated,
            "no generation should happen when keypair is already set"
        );
        assert_eq!(
            profile.public_key.as_deref(),
            Some(original_public.as_str()),
            "public_key must remain unchanged"
        );
        // Cleanup.
        let _ = save_or_remove_to_keychain(&private_key_user(&profile.id), None);
    }

    #[test]
    fn test_ensure_profile_keypair_recovers_public_key_from_existing_private_key() {
        // Reproduces the Begin-Synchronization-then-Save flow: a prior call
        // wrote the private key to the keychain but the in-memory profile
        // does not carry a public_key. ensure_profile_keypair must recover
        // (not regenerate) so the originally registered public key is
        // preserved.
        let mut profile = ServerProfile::new("Recovery");
        let (private_key, public_key) = generate_key_pair();
        let expected_public = public_key.to_string();
        save_private_key_to_keychain(&profile.id, Some(&serialize(&private_key))).unwrap();
        assert!(profile.public_key.is_none());

        let updated = ensure_profile_keypair(&mut profile)
            .expect("recovery path should succeed when private key exists");
        assert!(updated, "must report that the profile was updated");
        assert_eq!(
            profile.public_key.as_deref(),
            Some(expected_public.as_str()),
            "public_key must be derived from the existing private key, not regenerated"
        );

        // Verify the keychain still holds the same private key (it was not
        // overwritten by a fresh generation).
        let stored = load_private_key_from_keychain(&profile.id)
            .unwrap()
            .expect("private key should still be in keychain");
        let stored_identity: super::Identity = stored.parse().unwrap();
        assert_eq!(
            stored_identity.to_public().to_string(),
            expected_public,
            "stored private key must still match the recovered public key"
        );
        // Cleanup.
        let _ = save_or_remove_to_keychain(&private_key_user(&profile.id), None);
    }
}
