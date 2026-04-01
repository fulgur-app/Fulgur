//! Integration tests for Encryption with Real Key Pairs
//!
//! These tests verify the complete encryption flow:
//! 1. Key pair generation (x25519)
//! 2. Encryption/decryption roundtrip using age
//!
//! Keychain-related tests live in `integration_keychain.rs` (all `#[ignore]`d)
//! to avoid linking `Security.framework` into this binary, which can trigger
//! a macOS keychain prompt even when the tests themselves do not run.

use fulgur::fulgur::utils::crypto_helper::{
    decrypt_bytes, encrypt_bytes, generate_key_pair, serialize,
};

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
