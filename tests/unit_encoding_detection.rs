//! Unit tests for encoding detection
//!
//! These tests verify the `detect_encoding_and_decode()` function works correctly
//! for various character encodings and edge cases.

use fulgur::fulgur::files::file_operations::detect_encoding_and_decode;

#[test]
fn test_detect_encoding_utf8() {
    let content = "Hello, World! 你好世界";
    let bytes = content.as_bytes();
    let (encoding, decoded) = detect_encoding_and_decode(bytes);
    assert_eq!(encoding, "UTF-8", "Should detect UTF-8 encoding");
    assert_eq!(decoded, content, "Decoded content should match original");
}

#[test]
fn test_detect_encoding_ascii() {
    let content = "Hello, World!";
    let bytes = content.as_bytes();
    let (encoding, decoded) = detect_encoding_and_decode(bytes);
    assert_eq!(encoding, "UTF-8", "ASCII should be detected as UTF-8");
    assert_eq!(decoded, content, "Decoded content should match original");
}

#[test]
fn test_detect_encoding_with_bom() {
    let bom = vec![0xEF, 0xBB, 0xBF];
    let content = "Hello, World!";
    let mut bytes = bom;
    bytes.extend_from_slice(content.as_bytes());
    let (encoding, decoded) = detect_encoding_and_decode(&bytes);
    assert_eq!(encoding, "UTF-8", "Should detect UTF-8 with BOM");
    assert!(
        decoded.contains("Hello, World!"),
        "Decoded content should contain the text"
    );
}

#[test]
fn test_detect_encoding_latin1() {
    let bytes = vec![
        0x48, 0x65, 0x6C, 0x6C, 0x6F, 0x20, // "Hello "
        0xE9, 0xE8, 0xE0, // "éèà" in Latin-1
    ];
    let (encoding, decoded) = detect_encoding_and_decode(&bytes);
    assert!(
        !encoding.is_empty(),
        "Should detect some encoding for Latin-1"
    );
    assert!(!decoded.is_empty(), "Should decode to some content");
}

#[test]
fn test_detect_encoding_empty_file() {
    let bytes: &[u8] = &[];
    let (encoding, decoded) = detect_encoding_and_decode(bytes);
    assert_eq!(encoding, "UTF-8", "Empty file should default to UTF-8");
    assert_eq!(decoded, "", "Empty file should decode to empty string");
}

#[test]
fn test_detect_encoding_binary_like_data() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"Text start ");
    bytes.extend_from_slice(&[0xFF, 0xFE, 0xFD]); // Invalid UTF-8 bytes
    bytes.extend_from_slice(b" text end");
    let (encoding, decoded) = detect_encoding_and_decode(&bytes);
    assert!(
        !encoding.is_empty(),
        "Should detect some encoding for mixed data"
    );
    assert!(
        decoded.contains("Text start") || decoded.contains("text end"),
        "Should decode at least some valid parts"
    );
}

#[test]
fn test_detect_encoding_with_various_newlines() {
    let unix_content = "Line1\nLine2\nLine3";
    let windows_content = "Line1\r\nLine2\r\nLine3";
    let mac_content = "Line1\rLine2\rLine3";
    for (content, name) in [
        (unix_content, "Unix"),
        (windows_content, "Windows"),
        (mac_content, "Mac"),
    ] {
        let bytes = content.as_bytes();
        let (encoding, decoded) = detect_encoding_and_decode(bytes);
        assert_eq!(encoding, "UTF-8", "{} newlines should be UTF-8", name);
        assert_eq!(
            decoded, content,
            "{} newlines should decode correctly",
            name
        );
    }
}

#[test]
fn test_detect_encoding_large_file() {
    let mut content = String::new();
    for i in 0..1000 {
        content.push_str(&format!("Line {} with some content\n", i));
    }
    let bytes = content.as_bytes();
    let (encoding, decoded) = detect_encoding_and_decode(bytes);
    assert_eq!(encoding, "UTF-8", "Large file should be detected as UTF-8");
    assert_eq!(decoded, content, "Large file should decode correctly");
}

#[test]
fn test_detect_encoding_unicode_emoji() {
    let content = "Hello 👋 World 🌍 Rust 🦀";
    let bytes = content.as_bytes();
    let (encoding, decoded) = detect_encoding_and_decode(bytes);
    assert_eq!(encoding, "UTF-8", "Emoji should be detected as UTF-8");
    assert_eq!(decoded, content, "Emoji should decode correctly");
}

#[test]
fn test_detect_encoding_roundtrip() {
    let original_content = "Test content with Unicode: 你好, مرحبا, Здравствуй";
    let bytes = original_content.as_bytes();
    let (encoding1, decoded1) = detect_encoding_and_decode(bytes);
    let bytes2 = decoded1.as_bytes();
    let (encoding2, decoded2) = detect_encoding_and_decode(bytes2);
    assert_eq!(encoding1, encoding2, "Encoding should be stable");
    assert_eq!(decoded1, decoded2, "Content should be stable");
    assert_eq!(decoded2, original_content, "Should match original");
}
