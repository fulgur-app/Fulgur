use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use age::{
    secrecy::ExposeSecret,
    x25519::{Identity, Recipient},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use keyring::Entry;
use std::collections::hash_map::DefaultHasher;
use std::fmt::Write;
use std::hash::{Hash, Hasher};

use crate::fulgur::settings::Settings;

// Names of the entries in the keychain
const PRIVATE_KEY_NAME: &'static str = "private_key";
const DEVICE_API_KEY: &'static str = "device_api_key";
const SERVICE_NAME: &'static str = "Fulgur";

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

/// Saves the private key in the keychain
///
/// ### Arguments
/// - `private_key`: the key to save
///
/// ### Returns
/// - `Ok()`: The private key has been succesfully saved in the keychain
/// - `Err(anyhow::Error)`: If the private key could not be saved
pub fn save_private_key_to_keychain(private_key: Option<String>) -> anyhow::Result<()> {
    save_or_remove_to_keychain(PRIVATE_KEY_NAME, private_key)
}

/// Saves the device API key in the keychain
///
/// ### Arguments
/// - `device_api_key`: the key to save
///
/// ### Returns
/// - `Ok()`: The device API key has been succesfully saved in the keychain
/// - `Err(anyhow::Error)`: If the device API key could not be saved
pub fn save_device_api_key_to_keychain(device_api_key: Option<String>) -> anyhow::Result<()> {
    save_or_remove_to_keychain(DEVICE_API_KEY, device_api_key)
}

/// Saves or removes a value from the keychain. If the value is `None`, the entry is removed from the keychain.
///
/// ### Arguments
/// - `user`: the user name (the entry to look for in the keychain)
/// - `value`: the value to save
///
/// ### Returns
/// - `Ok()`: The value has been succesfully saved in the keychain
/// - `Err(anyhow::Error)`: If the value could not be saved
fn save_or_remove_to_keychain(user: &str, value: Option<String>) -> anyhow::Result<()> {
    let entry = Entry::new(SERVICE_NAME, user)?;
    if let Some(value) = value {
        entry.set_password(value.as_str())?;
    } else {
        entry.set_password("")?;
    }
    Ok(())
}

/// Loads the private key from the keychain
///
/// ### Returns
/// - `Ok(Option<String>)`: The private key if it exists, otherwise `None`
/// - `Err(anyhow::Error)`: If the private key could not be loaded
pub fn load_private_key_from_keychain() -> anyhow::Result<Option<String>> {
    load_from_keychain(PRIVATE_KEY_NAME)
}

/// Loads the device API key from the keychain
///
/// ### Returns
/// - `Ok(Option<String>)`: The device API key if it exists, otherwise `None`
/// - `Err(anyhow::Error)`: If the device API key could not be loaded
pub fn load_device_api_key_from_keychain() -> anyhow::Result<Option<String>> {
    load_from_keychain(DEVICE_API_KEY)
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
    let entry = Entry::new(SERVICE_NAME, user)?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::anyhow!(
            "Failed to load '{}' from keychain: {}",
            user,
            e
        )),
    }
}

/// Serializes the private key to a string
///
/// ### Arguments
/// - `private_key`: the private key to serialize
///
/// ### Returns
/// - `String`: The serialized private key
pub fn serialize(private_key: Identity) -> String {
    let secret = private_key.to_string();
    secret.expose_secret().to_string()
}

/// Checks if the private/public keys exist in the keychain. If not, generates a new pair and saves them to the keychain.
///
/// ### Arguments
/// - `settings`: the settings to check
///
/// ### Returns
/// - `Ok()`: The private/public keys have been succesfully checked/generated and saved to the keychain
/// - `Err(anyhow::Error)`: If the private/public keys could not be checked/generated or saved
pub fn check_private_public_keys(settings: &mut Settings) -> anyhow::Result<()> {
    if settings
        .app_settings
        .synchronization_settings
        .is_synchronization_activated
    {
        let private_key = load_private_key_from_keychain()?;
        if private_key.is_none()
            || settings
                .app_settings
                .synchronization_settings
                .public_key
                .is_none()
        {
            log::debug!("No private key, need to generate keys.");
            let (new_private_key, new_public_key) = generate_key_pair();
            save_private_key_to_keychain(Some(serialize(new_private_key)))?;
            settings.app_settings.synchronization_settings.public_key =
                Some(new_public_key.to_string());
            log::debug!("Saving keys");
            settings.save()?;
        } else {
            log::debug!("Private  key exists, no generation needed.")
        }
    }
    Ok(())
}

// NOTE: The following machine-specific encryption functions (encrypt/decrypt) are currently unused
// because device API keys are now stored directly in the system keychain, which provides its own
// encryption. These functions are kept for potential future use with other sensitive data that
// needs to be stored in plain files (e.g., settings.json).
#[allow(dead_code)]
const APP_SALT: &[u8] = b"Fulgur-Sync-Key-v1";

/// Get a machine-specific encryption key, derived from the machine's unique ID + app salt
///
/// ### Returns
/// - `Ok([u8; 32])`: The machine-specific encryption key
/// - `Err(anyhow::Error)`: If the machine ID could not be retrieved
#[allow(dead_code)]
fn get_machine_key() -> anyhow::Result<[u8; 32]> {
    let machine_id =
        machine_uid::get().map_err(|e| anyhow::anyhow!("Failed to get machine ID: {}", e))?;
    let mut combined = String::new();
    write!(
        &mut combined,
        "{}{}",
        machine_id,
        String::from_utf8_lossy(APP_SALT)
    )
    .map_err(|e| anyhow::anyhow!("Failed to combine keys: {}", e))?;
    let mut hasher = DefaultHasher::new();
    combined.hash(&mut hasher);
    let hash1 = hasher.finish();
    let mut hasher = DefaultHasher::new();
    format!("{}{}", hash1, combined).hash(&mut hasher);
    let hash2 = hasher.finish();
    let mut hasher = DefaultHasher::new();
    format!("{}{}", hash2, machine_id).hash(&mut hasher);
    let hash3 = hasher.finish();
    let mut hasher = DefaultHasher::new();
    format!("{}{}", hash3, hash1).hash(&mut hasher);
    let hash4 = hasher.finish();
    let mut key = [0u8; 32];
    key[0..8].copy_from_slice(&hash1.to_le_bytes());
    key[8..16].copy_from_slice(&hash2.to_le_bytes());
    key[16..24].copy_from_slice(&hash3.to_le_bytes());
    key[24..32].copy_from_slice(&hash4.to_le_bytes());
    Ok(key)
}

/// Encrypt a string using machine-specific key
///
/// Note: Currently unused for device API keys (stored in keychain instead).
/// Kept for potential future use with settings files or other plain text storage.
///
/// ### Arguments
/// - `plaintext`: The string to encrypt
///
/// ### Returns
/// - `Ok(String)`: The base64-encoded encrypted string (nonce + ciphertext)
/// - `Err(anyhow::Error)`: If the encryption faile
#[allow(dead_code)]
pub fn encrypt(plaintext: &str) -> anyhow::Result<String> {
    let key = get_machine_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(BASE64.encode(combined))
}

/// Decrypt a base64-encoded encrypted string
///
/// Note: Currently unused for device API keys (stored in keychain instead).
/// Kept for potential future use with settings files or other plain text storage.
///
/// ### Arguments
/// - `encrypted`: Base64-encoded encrypted string (nonce + ciphertext)
///
/// ### Returns
/// - `Ok(String)`: The decrypted plaintext string
/// - `Err(anyhow::Error)`: If the decryption failed
#[allow(dead_code)]
pub fn decrypt(encrypted: &str) -> anyhow::Result<String> {
    let key = get_machine_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;
    let combined = BASE64
        .decode(encrypted)
        .map_err(|e| anyhow::anyhow!("Failed to decode base64: {}", e))?;
    if combined.len() < 12 {
        return Err(anyhow::anyhow!(
            "Invalid encrypted data: too short (expected at least 12 bytes for nonce)"
        ));
    }
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;
    String::from_utf8(plaintext).map_err(|e| anyhow::anyhow!("Invalid UTF-8: {}", e))
}

/// Convert base64 encryption key to bytes
///
/// ### Arguments
/// - `key_b64`: Base64-encoded encryption key
///
/// ### Returns
/// - `Ok([u8; 32])`: The 32-byte encryption key
/// - `Err(anyhow::Error)`: If the encryption key could not be decoded
pub fn decode_encryption_key(key_b64: &str) -> anyhow::Result<[u8; 32]> {
    let key_bytes = BASE64.decode(key_b64)?;
    if key_bytes.len() != 32 {
        return Err(anyhow::anyhow!(
            "Invalid encryption key length: expected 32 bytes, got {}",
            key_bytes.len()
        ));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&key_bytes);
    Ok(key)
}

/// Encrypt bytes (e.g., compressed data) for file sharing
///
/// ### Arguments
/// - `content_bytes`: The bytes to encrypt
/// - `encryption_key_b64`: The base64-encoded encryption key from the server
///
/// ### Returns
/// - `Ok(String)`: The base64-encoded encrypted content (nonce + ciphertext)
/// - `Err(anyhow::Error)`: If the encryption failed
pub fn encrypt_bytes(content_bytes: &[u8], encryption_key_b64: &str) -> anyhow::Result<String> {
    let key_bytes = decode_encryption_key(encryption_key_b64)?;
    let cipher = Aes256Gcm::new_from_slice(&key_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, content_bytes)
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(BASE64.encode(combined))
}

/// Decrypt bytes (e.g., compressed data) received from another device
///
/// ### Arguments
/// - `encrypted`: Base64-encoded encrypted content (nonce + ciphertext)
/// - `encryption_key_b64`: The base64-encoded encryption key from the server
///
/// ### Returns
/// - `Ok(Vec<u8>)`: The decrypted bytes
/// - `Err(anyhow::Error)`: If the decryption failed
pub fn decrypt_bytes(encrypted: &str, encryption_key_b64: &str) -> anyhow::Result<Vec<u8>> {
    let key_bytes = decode_encryption_key(encryption_key_b64)?;
    let cipher = Aes256Gcm::new_from_slice(&key_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;
    let combined = BASE64
        .decode(encrypted)
        .map_err(|e| anyhow::anyhow!("Failed to decode base64: {}", e))?;
    if combined.len() < 12 {
        return Err(anyhow::anyhow!(
            "Invalid encrypted data: too short (expected at least 12 bytes for nonce)"
        ));
    }
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let original = "test_api_key_123";
        let encrypted = encrypt(original).unwrap();
        let decrypted = decrypt(&encrypted).unwrap();
        assert_eq!(original, decrypted);
    }

    #[test]
    fn test_empty_string() {
        let original = "";
        let encrypted = encrypt(original).unwrap();
        let decrypted = decrypt(&encrypted).unwrap();
        assert_eq!(original, decrypted);
    }

    #[test]
    fn test_encrypt_decrypt_bytes() {
        let key_bytes = [42u8; 32]; // Simple test key
        let encryption_key = BASE64.encode(&key_bytes);
        let original_bytes = b"This is a test file content with some data!";
        let encrypted =
            encrypt_bytes(original_bytes, &encryption_key).expect("Encryption should succeed");
        assert_ne!(encrypted, String::from_utf8_lossy(original_bytes));
        let decrypted =
            decrypt_bytes(&encrypted, &encryption_key).expect("Decryption should succeed");
        assert_eq!(decrypted, original_bytes);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertext() {
        let key_bytes = [42u8; 32];
        let encryption_key = BASE64.encode(&key_bytes);
        let content_bytes = b"Same content";
        let encrypted1 = encrypt_bytes(content_bytes, &encryption_key).unwrap();
        let encrypted2 = encrypt_bytes(content_bytes, &encryption_key).unwrap();
        assert_ne!(encrypted1, encrypted2);
        assert_eq!(
            decrypt_bytes(&encrypted1, &encryption_key).unwrap(),
            content_bytes
        );
        assert_eq!(
            decrypt_bytes(&encrypted2, &encryption_key).unwrap(),
            content_bytes
        );
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let key_bytes1 = [42u8; 32];
        let key_bytes2 = [99u8; 32];
        let encryption_key1 = BASE64.encode(&key_bytes1);
        let encryption_key2 = BASE64.encode(&key_bytes2);
        let content_bytes = b"Secret data";
        let encrypted = encrypt_bytes(content_bytes, &encryption_key1).unwrap();
        let result = decrypt_bytes(&encrypted, &encryption_key2);
        assert!(result.is_err());
    }
}
