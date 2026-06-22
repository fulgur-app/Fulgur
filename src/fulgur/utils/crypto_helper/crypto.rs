use age::{
    secrecy::ExposeSecret,
    x25519::{Identity, Recipient},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use zeroize::Zeroizing;

use super::keychain::{load_private_key_from_keychain, save_private_key_to_keychain};
use crate::fulgur::settings::{ServerProfile, Settings};

/// Generate a matching pair of private/public keys
///
/// ### Returns
/// - `(Identity, Recipient)`: The generated (private, public) key pair
#[must_use]
pub fn generate_key_pair() -> (Identity, Recipient) {
    let private_key = age::x25519::Identity::generate();
    let public_key = private_key.to_public();
    log::info!("New public and private keys generated!");
    (private_key, public_key)
}

/// Serializes the private key to a zeroizing string. The returned `Zeroizing<String>`
/// ensures the key material is overwritten with zeros when dropped.
///
/// ### Arguments
/// - `private_key`: the private key to serialize
///
/// ### Returns
/// - `Zeroizing<String>`: The serialized private key, zeroed on drop
#[must_use]
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
/// ### Errors
/// - Returns an error if reading from the keychain fails, if the stored private
///   key cannot be parsed, or if saving a newly generated key to the keychain fails.
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
/// ### Errors
/// - Returns an error if keychain access or key generation fails for any active
///   profile, or if persisting the updated settings fails.
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
#[must_use]
pub fn is_valid_public_key(key: &str) -> bool {
    key.parse::<Recipient>().is_ok()
}

/// Encrypt bytes (e.g., compressed data) for file sharing using age encryption
///
/// ### Arguments
/// - `content_bytes`: The bytes to encrypt
/// - `recipient_public_key`: The recipient's age x25519 public key (format: "age1...")
///
/// ### Errors
/// - Returns an error if the recipient public key cannot be parsed, the
///   encryptor cannot be created, or writing the encrypted payload fails.
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
/// ### Errors
/// - Returns an error if the input is not valid base64, the private key cannot
///   be parsed, the decryptor cannot be created, or reading the decrypted
///   payload fails.
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
    use super::super::keychain::{
        load_private_key_from_keychain, private_key_user, save_or_remove_to_keychain,
        save_private_key_to_keychain,
    };
    use super::{
        decrypt_bytes, encrypt_bytes, ensure_profile_keypair, generate_key_pair,
        is_valid_public_key, serialize,
    };
    use crate::fulgur::settings::ServerProfile;

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
