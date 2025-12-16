use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use std::collections::hash_map::DefaultHasher;
use std::fmt::Write;
use std::hash::{Hash, Hasher};

const APP_SALT: &[u8] = b"Fulgur-Sync-Key-v1";

/// Get a machine-specific encryption key
/// This is derived from the machine's unique ID + app salt
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
/// @param plaintext: The string to encrypt
/// @return: Base64-encoded encrypted string (nonce + ciphertext)
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
/// @param encrypted: Base64-encoded encrypted string (nonce + ciphertext)
/// @return: Decrypted plaintext string
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
/// @param key_b64: Base64-encoded encryption key
/// @return: 32-byte encryption key
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

/// Encrypt content for sharing between devices
/// Uses AES-256-GCM with the user's shared encryption key from the server
/// @param content: The plaintext content to encrypt
/// @param encryption_key_b64: The base64-encoded encryption key from the server
/// @return: Base64-encoded encrypted content (nonce + ciphertext)
pub fn encrypt_content(content: &str, encryption_key_b64: &str) -> anyhow::Result<String> {
    let key_bytes = decode_encryption_key(encryption_key_b64)?;
    let cipher = Aes256Gcm::new_from_slice(&key_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;

    // Generate a random nonce for this encryption
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    // Encrypt the content
    let ciphertext = cipher
        .encrypt(&nonce, content.as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    // Combine nonce + ciphertext and encode as base64
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);

    Ok(BASE64.encode(combined))
}

/// Decrypt content received from another device
/// @param encrypted: Base64-encoded encrypted content (nonce + ciphertext)
/// @param encryption_key_b64: The base64-encoded encryption key from the server
/// @return: Decrypted plaintext content
pub fn decrypt_content(encrypted: &str, encryption_key_b64: &str) -> anyhow::Result<String> {
    let key_bytes = decode_encryption_key(encryption_key_b64)?;
    let cipher = Aes256Gcm::new_from_slice(&key_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;

    // Decode from base64
    let combined = BASE64
        .decode(encrypted)
        .map_err(|e| anyhow::anyhow!("Failed to decode base64: {}", e))?;

    // Extract nonce (first 12 bytes) and ciphertext (remaining bytes)
    if combined.len() < 12 {
        return Err(anyhow::anyhow!(
            "Invalid encrypted data: too short (expected at least 12 bytes for nonce)"
        ));
    }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    // Decrypt
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

    String::from_utf8(plaintext).map_err(|e| anyhow::anyhow!("Invalid UTF-8: {}", e))
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
    fn test_encrypt_decrypt_content() {
        // Generate a test encryption key (base64-encoded 32 bytes)
        let key_bytes = [42u8; 32]; // Simple test key
        let encryption_key = BASE64.encode(&key_bytes);

        let original_content = "This is a test file content with some data!";

        // Encrypt the content
        let encrypted =
            encrypt_content(original_content, &encryption_key).expect("Encryption should succeed");

        // Verify encrypted is different from original and longer (includes nonce)
        assert_ne!(encrypted, original_content);
        assert!(encrypted.len() > original_content.len());

        // Decrypt the content
        let decrypted =
            decrypt_content(&encrypted, &encryption_key).expect("Decryption should succeed");

        // Verify decrypted matches original
        assert_eq!(decrypted, original_content);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertext() {
        let key_bytes = [42u8; 32];
        let encryption_key = BASE64.encode(&key_bytes);
        let content = "Same content";

        // Encrypt the same content twice
        let encrypted1 = encrypt_content(content, &encryption_key).unwrap();
        let encrypted2 = encrypt_content(content, &encryption_key).unwrap();

        // Should produce different ciphertext due to random nonce
        assert_ne!(encrypted1, encrypted2);

        // But both should decrypt to the same content
        assert_eq!(
            decrypt_content(&encrypted1, &encryption_key).unwrap(),
            content
        );
        assert_eq!(
            decrypt_content(&encrypted2, &encryption_key).unwrap(),
            content
        );
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let key_bytes1 = [42u8; 32];
        let key_bytes2 = [99u8; 32];
        let encryption_key1 = BASE64.encode(&key_bytes1);
        let encryption_key2 = BASE64.encode(&key_bytes2);

        let content = "Secret data";
        let encrypted = encrypt_content(content, &encryption_key1).unwrap();

        // Trying to decrypt with wrong key should fail
        let result = decrypt_content(&encrypted, &encryption_key2);
        assert!(result.is_err());
    }
}
