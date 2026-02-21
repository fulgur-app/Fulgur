use age::{
    secrecy::ExposeSecret,
    x25519::{Identity, Recipient},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use keyring::Entry;

use crate::fulgur::settings::Settings;

// Names of the entries in the keychain
const PRIVATE_KEY_NAME: &str = "private_key";
const DEVICE_API_KEY: &str = "device_api_key";
const SERVICE_NAME: &str = "Fulgur";

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
    if let Some(value) = value
        && !value.is_empty()
    {
        entry.set_password(value.as_str())?;
        return Ok(());
    }
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(anyhow::anyhow!(
            "Failed to remove '{}' from keychain: {}",
            user,
            e
        )),
    }
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
        Ok(value) if value.is_empty() => {
            // Legacy behavior stored empty strings instead of deleting credentials.
            // TODO: remove this in further version.
            log::warn!(
                "Keychain entry '{}' is empty; treating as missing and removing stale credential",
                user
            );
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(anyhow::anyhow!(
                    "Failed to clean up empty '{}' keychain entry: {}",
                    user,
                    e
                )),
            }
        }
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
        .map_err(|e| anyhow::anyhow!("Failed to parse recipient public key: {}", e))?;
    let recipients: Vec<Box<dyn age::Recipient>> = vec![Box::new(recipient)];
    let encryptor = age::Encryptor::with_recipients(recipients.iter().map(|r| r.as_ref()))
        .map_err(|e| anyhow::anyhow!("Failed to create encryptor: {}", e))?;
    let mut encrypted = vec![];
    let mut writer = encryptor
        .wrap_output(&mut encrypted)
        .map_err(|e| anyhow::anyhow!("Failed to create encryption writer: {}", e))?;
    std::io::Write::write_all(&mut writer, content_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to write encrypted data: {}", e))?;
    writer
        .finish()
        .map_err(|e| anyhow::anyhow!("Failed to finish encryption: {}", e))?;
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
        .map_err(|e| anyhow::anyhow!("Failed to decode base64: {}", e))?;
    let identity: Identity = private_key_str
        .parse()
        .map_err(|e| anyhow::anyhow!("Failed to parse private key: {}", e))?;
    let decryptor = age::Decryptor::new(&encrypted[..])
        .map_err(|e| anyhow::anyhow!("Failed to create decryptor: {}", e))?;
    let mut decrypted = vec![];
    let mut reader = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(|e| anyhow::anyhow!("Failed to decrypt: {}", e))?;
    std::io::Read::read_to_end(&mut reader, &mut decrypted)
        .map_err(|e| anyhow::anyhow!("Failed to read decrypted data: {}", e))?;
    Ok(decrypted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_bytes() {
        // Generate a key pair for testing
        let (private_key, public_key) = generate_key_pair();
        let public_key_str = public_key.to_string();
        let private_key_str = serialize(private_key);

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
        let private_key_str = serialize(private_key);

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
        let private_key2_str = serialize(private_key2);

        let content_bytes = b"Secret data";
        // Encrypt with public_key1
        let encrypted = encrypt_bytes(content_bytes, &public_key1_str).unwrap();
        // Try to decrypt with private_key2 (should fail)
        let result = decrypt_bytes(&encrypted, &private_key2_str);
        assert!(result.is_err());
    }
}
