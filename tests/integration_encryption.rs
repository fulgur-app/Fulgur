//! Integration tests for Encryption with Real Key Pairs
//!
//! These tests verify the complete encryption flow:
//! 1. Key pair generation (x25519)
//! 2. Keychain storage and retrieval (platform-specific)
//! 3. Encryption/decryption roundtrip using age
//!
//! **IMPORTANT**: These tests use the real system keychain with a test-specific
//! service name to avoid interfering with production data. All test entries are
//! cleaned up after each test.
//!
//! ## Platform Support
//! - macOS: Keychain Access
//! - Windows: Windows Credential Manager
//! - Linux: Secret Service API (libsecret/gnome-keyring)

use fulgur::fulgur::utils::crypto_helper::{
    decrypt_bytes, encrypt_bytes, generate_key_pair, serialize,
};
use keyring::Entry;

// Test-specific service name to isolate from production
const TEST_SERVICE_NAME: &str = "FulgurTest";

/// Clean up test keychain entries
///
/// ### Arguments
/// - `entry_name`: The name of the keychain entry to clean up
fn cleanup_keychain_entry(entry_name: &str) {
    if let Ok(entry) = Entry::new(TEST_SERVICE_NAME, entry_name) {
        let _ = entry.delete_credential();
    }
}

/// Helper to save a value to the test keychain
///
/// ### Arguments
/// - `entry_name`: The name of the keychain entry
/// - `value`: The value to save
///
/// ### Returns
/// - `Ok(())`: If the value was saved successfully
/// - `Err(anyhow::Error)`: If the save failed
fn save_to_test_keychain(entry_name: &str, value: &str) -> anyhow::Result<()> {
    let entry = Entry::new(TEST_SERVICE_NAME, entry_name)?;
    entry.set_password(value)?;
    Ok(())
}

/// Helper to load a value from the test keychain
///
/// ### Arguments
/// - `entry_name`: The name of the keychain entry
///
/// ### Returns
/// - `Ok(Some(String))`: The value if it exists
/// - `Ok(None)`: If the entry doesn't exist
/// - `Err(anyhow::Error)`: If the load failed
fn load_from_test_keychain(entry_name: &str) -> anyhow::Result<Option<String>> {
    let entry = Entry::new(TEST_SERVICE_NAME, entry_name)?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::anyhow!("Failed to load from keychain: {}", e)),
    }
}

#[test]
fn test_generate_key_pair_produces_valid_keys() {
    let (private_key, public_key) = generate_key_pair();
    let private_str = serialize(private_key);
    let public_str = public_key.to_string();
    assert!(
        private_str.starts_with("AGE-SECRET-KEY-"),
        "Private key should have correct format"
    );
    assert!(
        public_str.starts_with("age1"),
        "Public key should have correct format"
    );
}

#[test]
fn test_generate_key_pair_produces_unique_keys() {
    let (private1, public1) = generate_key_pair();
    let (private2, public2) = generate_key_pair();
    assert_ne!(
        serialize(private1),
        serialize(private2),
        "Each generation should produce unique private keys"
    );
    assert_ne!(
        public1.to_string(),
        public2.to_string(),
        "Each generation should produce unique public keys"
    );
}

#[test]
fn test_keychain_save_and_load_private_key() {
    let entry_name = "test_keychain_save_and_load";
    cleanup_keychain_entry(entry_name);
    let (private_key, _) = generate_key_pair();
    let private_str = serialize(private_key);
    save_to_test_keychain(entry_name, &private_str).expect("Failed to save to keychain");
    let loaded = load_from_test_keychain(entry_name)
        .expect("Failed to load from keychain")
        .expect("Value should exist");
    assert_eq!(loaded, private_str, "Loaded key should match saved key");
    cleanup_keychain_entry(entry_name);
}

#[test]
fn test_keychain_load_nonexistent_entry() {
    cleanup_keychain_entry("nonexistent_entry");
    let result = load_from_test_keychain("nonexistent_entry").expect("Should not error");
    assert!(result.is_none(), "Nonexistent entry should return None");
}

#[test]
fn test_keychain_overwrite_existing_entry() {
    let entry_name = "test_keychain_overwrite";
    cleanup_keychain_entry(entry_name);
    let value1 = "first_value";
    let value2 = "second_value";
    save_to_test_keychain(entry_name, value1).expect("Failed to save first value");
    save_to_test_keychain(entry_name, value2).expect("Failed to save second value");
    let loaded = load_from_test_keychain(entry_name)
        .expect("Failed to load")
        .expect("Value should exist");
    assert_eq!(loaded, value2, "Should load the most recent value");
    cleanup_keychain_entry(entry_name);
}

#[test]
fn test_keychain_save_and_load_api_key() {
    let entry_name = "test_keychain_api_key";
    cleanup_keychain_entry(entry_name);
    let api_key = "test_device_api_key_12345";
    save_to_test_keychain(entry_name, api_key).expect("Failed to save API key");
    let loaded = load_from_test_keychain(entry_name)
        .expect("Failed to load API key")
        .expect("API key should exist");
    assert_eq!(loaded, api_key, "Loaded API key should match");
    cleanup_keychain_entry(entry_name);
}

#[test]
fn test_encrypt_decrypt_roundtrip_simple_text() {
    let (private_key, public_key) = generate_key_pair();
    let public_str = public_key.to_string();
    let private_str = serialize(private_key);
    let original = b"Hello, World!";
    let encrypted = encrypt_bytes(original, &public_str).expect("Encryption should succeed");
    let decrypted = decrypt_bytes(&encrypted, &private_str).expect("Decryption should succeed");
    assert_eq!(
        decrypted, original,
        "Decrypted content should match original"
    );
}

#[test]
fn test_encrypt_decrypt_roundtrip_large_data() {
    let (private_key, public_key) = generate_key_pair();
    let public_str = public_key.to_string();
    let private_str = serialize(private_key);
    let original: Vec<u8> = (0..1_000_000).map(|i| (i % 256) as u8).collect();
    let encrypted = encrypt_bytes(&original, &public_str).expect("Encryption should succeed");
    let decrypted = decrypt_bytes(&encrypted, &private_str).expect("Decryption should succeed");
    assert_eq!(decrypted, original, "Large data should roundtrip correctly");
}

#[test]
fn test_encrypt_decrypt_roundtrip_unicode_content() {
    let (private_key, public_key) = generate_key_pair();
    let public_str = public_key.to_string();
    let private_str = serialize(private_key);
    let original = "Hello 世界 🦀 Здравствуй مرحبا";
    let original_bytes = original.as_bytes();
    let encrypted = encrypt_bytes(original_bytes, &public_str).expect("Encryption should succeed");
    let decrypted = decrypt_bytes(&encrypted, &private_str).expect("Decryption should succeed");
    assert_eq!(
        String::from_utf8(decrypted).unwrap(),
        original,
        "Unicode content should roundtrip correctly"
    );
}

#[test]
fn test_encrypt_decrypt_roundtrip_binary_data() {
    let (private_key, public_key) = generate_key_pair();
    let public_str = public_key.to_string();
    let private_str = serialize(private_key);
    let original: Vec<u8> = vec![0x00, 0xFF, 0x7F, 0x80, 0xAA, 0x55, 0xDE, 0xAD, 0xBE, 0xEF];
    let encrypted = encrypt_bytes(&original, &public_str).expect("Encryption should succeed");
    let decrypted = decrypt_bytes(&encrypted, &private_str).expect("Decryption should succeed");
    assert_eq!(
        decrypted, original,
        "Binary data should roundtrip correctly"
    );
}

#[test]
fn test_encrypt_decrypt_roundtrip_empty_data() {
    let (private_key, public_key) = generate_key_pair();
    let public_str = public_key.to_string();
    let private_str = serialize(private_key);
    let original: Vec<u8> = vec![];
    let encrypted = encrypt_bytes(&original, &public_str).expect("Encryption should succeed");
    let decrypted = decrypt_bytes(&encrypted, &private_str).expect("Decryption should succeed");
    assert_eq!(decrypted, original, "Empty data should roundtrip correctly");
}

#[test]
fn test_encrypt_produces_different_ciphertext_each_time() {
    let (private_key, public_key) = generate_key_pair();
    let public_str = public_key.to_string();
    let private_str = serialize(private_key);
    let original = b"Same content every time";
    let encrypted1 = encrypt_bytes(original, &public_str).expect("Encryption 1 should succeed");
    let encrypted2 = encrypt_bytes(original, &public_str).expect("Encryption 2 should succeed");
    assert_ne!(
        encrypted1, encrypted2,
        "Encrypting same content twice should produce different ciphertexts"
    );
    let decrypted1 = decrypt_bytes(&encrypted1, &private_str).expect("Decryption 1 should succeed");
    let decrypted2 = decrypt_bytes(&encrypted2, &private_str).expect("Decryption 2 should succeed");
    assert_eq!(decrypted1, original);
    assert_eq!(decrypted2, original);
}

#[test]
fn test_decrypt_with_wrong_private_key_fails() {
    let (_private_key1, public_key1) = generate_key_pair();
    let (private_key2, _public_key2) = generate_key_pair();
    let public_str1 = public_key1.to_string();
    let private_str2 = serialize(private_key2);
    let original = b"Secret message";
    let encrypted = encrypt_bytes(original, &public_str1).expect("Encryption should succeed");
    let result = decrypt_bytes(&encrypted, &private_str2);
    assert!(
        result.is_err(),
        "Decryption with wrong private key should fail"
    );
}

#[test]
fn test_decrypt_with_corrupted_ciphertext_fails() {
    let (private_key, public_key) = generate_key_pair();
    let public_str = public_key.to_string();
    let private_str = serialize(private_key);
    let original = b"Test data";
    let mut encrypted = encrypt_bytes(original, &public_str).expect("Encryption should succeed");
    if encrypted.len() > 10 {
        encrypted.replace_range(5..6, "X");
    }
    let result = decrypt_bytes(&encrypted, &private_str);
    assert!(
        result.is_err(),
        "Decryption of corrupted ciphertext should fail"
    );
}

// ============================================================================
// Full Integration: Keychain + Encryption
// ============================================================================

#[test]
fn test_full_integration_keychain_and_encryption() {
    let entry_name = "test_full_integration";
    cleanup_keychain_entry(entry_name);
    let (private_key, public_key) = generate_key_pair();
    let public_str = public_key.to_string();
    let private_str = serialize(private_key);
    save_to_test_keychain(entry_name, &private_str)
        .expect("Failed to save private key to keychain");
    let original = b"Sensitive data to be encrypted";
    let encrypted = encrypt_bytes(original, &public_str).expect("Encryption should succeed");
    let loaded_private_str = load_from_test_keychain(entry_name)
        .expect("Failed to load from keychain")
        .expect("Private key should exist");
    let decrypted =
        decrypt_bytes(&encrypted, &loaded_private_str).expect("Decryption should succeed");
    assert_eq!(
        decrypted, original,
        "Full roundtrip through keychain should work"
    );
    cleanup_keychain_entry(entry_name);
}

#[test]
fn test_multi_device_simulation() {
    cleanup_keychain_entry("device1_private");
    cleanup_keychain_entry("device2_private");
    cleanup_keychain_entry("device3_private");
    let (device1_private, device1_public) = generate_key_pair();
    let (device2_private, device2_public) = generate_key_pair();
    let (device3_private, device3_public) = generate_key_pair();
    save_to_test_keychain("device1_private", &serialize(device1_private))
        .expect("Save device1 key");
    save_to_test_keychain("device2_private", &serialize(device2_private))
        .expect("Save device2 key");
    save_to_test_keychain("device3_private", &serialize(device3_private))
        .expect("Save device3 key");
    let original = b"Shared file content across devices";
    let encrypted_for_device1 =
        encrypt_bytes(original, &device1_public.to_string()).expect("Encrypt for device1");
    let encrypted_for_device2 =
        encrypt_bytes(original, &device2_public.to_string()).expect("Encrypt for device2");
    let encrypted_for_device3 =
        encrypt_bytes(original, &device3_public.to_string()).expect("Encrypt for device3");
    let device1_private_str = load_from_test_keychain("device1_private").unwrap().unwrap();
    let device2_private_str = load_from_test_keychain("device2_private").unwrap().unwrap();
    let device3_private_str = load_from_test_keychain("device3_private").unwrap().unwrap();
    let decrypted1 =
        decrypt_bytes(&encrypted_for_device1, &device1_private_str).expect("Device1 decrypt");
    let decrypted2 =
        decrypt_bytes(&encrypted_for_device2, &device2_private_str).expect("Device2 decrypt");
    let decrypted3 =
        decrypt_bytes(&encrypted_for_device3, &device3_private_str).expect("Device3 decrypt");
    assert_eq!(decrypted1, original);
    assert_eq!(decrypted2, original);
    assert_eq!(decrypted3, original);
    let wrong_decrypt = decrypt_bytes(&encrypted_for_device2, &device1_private_str);
    assert!(
        wrong_decrypt.is_err(),
        "Device1 should not be able to decrypt Device2's content"
    );
    cleanup_keychain_entry("device1_private");
    cleanup_keychain_entry("device2_private");
    cleanup_keychain_entry("device3_private");
}

#[test]
fn test_keychain_persistence_across_multiple_operations() {
    let entry_name = "test_keychain_persistence";
    cleanup_keychain_entry(entry_name);
    let (private_key, public_key) = generate_key_pair();
    let private_str = serialize(private_key);
    let public_str = public_key.to_string();
    save_to_test_keychain(entry_name, &private_str).expect("Failed to save to keychain");
    for i in 0..10 {
        let original = format!("Test message number {}", i);
        let encrypted = encrypt_bytes(original.as_bytes(), &public_str)
            .expect(&format!("Encryption {} should succeed", i));
        let loaded_private = load_from_test_keychain(entry_name)
            .expect("Load should succeed")
            .expect("Key should exist");
        let decrypted = decrypt_bytes(&encrypted, &loaded_private)
            .expect(&format!("Decryption {} should succeed", i));
        assert_eq!(
            String::from_utf8(decrypted).unwrap(),
            original,
            "Iteration {} should succeed",
            i
        );
    }
    cleanup_keychain_entry(entry_name);
}
