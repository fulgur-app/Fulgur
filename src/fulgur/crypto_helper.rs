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
}
