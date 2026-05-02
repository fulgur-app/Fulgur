use flate2::{
    Compression,
    read::{GzDecoder, GzEncoder},
};
use std::io::Read;

/// Maximum allowed compression ratio (decompressed / compressed).
const MAX_COMPRESSION_RATIO: usize = 512;

/// Minimum decompression buffer size, regardless of compressed input size.
const MIN_DECOMPRESSION_BUFFER_BYTES: usize = 64 * 1024;

/// Initial buffer allocation ratio used to pre-size the decompression `Vec`.
const INITIAL_BUFFER_RATIO: usize = 16;

/// Compress content using gzip compression
///
/// ### Arguments
/// - `content`: The content to compress
///
/// ### Returns
/// - `Ok(Vec<u8>)`: The compressed content as bytes
/// - `Err(anyhow::Error)`: If the content could not be compressed
pub(super) fn compress_content(content: &str) -> anyhow::Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(content.as_bytes(), Compression::default());
    let mut compressed = Vec::new();
    encoder.read_to_end(&mut compressed)?;
    let original_size_kb = content.len() as f64 / 1024.0;
    let compressed_size_kb = compressed.len() as f64 / 1024.0;
    let compression_ratio = (1.0 - (compressed.len() as f64 / content.len() as f64)) * 100.0;
    log::debug!(
        "Compression: {original_size_kb:.2} KB -> {compressed_size_kb:.2} KB ({compression_ratio:.1}% reduction)"
    );
    Ok(compressed)
}

/// Decompress content that was compressed with gzip
///
/// ### Arguments
/// - `compressed`: The compressed content as bytes
/// - `max_size`: The server-advertised maximum file size in bytes. `u64::MAX`
///   means the user configured the server for no limit.
///
/// ### Returns
/// - `Ok(String)`: The decompressed content as string
/// - `Err(anyhow::Error)`: If the compressed payload is oversized for the
///   advertised limit, the decompressed payload exceeds the ratio-bounded cap
///   or the absolute limit, or the output is not valid UTF-8
pub fn decompress_content(compressed: &[u8], max_size: u64) -> anyhow::Result<String> {
    let max_size_usize = if max_size == u64::MAX {
        usize::MAX
    } else {
        usize::try_from(max_size).unwrap_or(usize::MAX)
    };
    if max_size != u64::MAX && compressed.len() > max_size_usize {
        return Err(anyhow::anyhow!(
            "Compressed payload ({} bytes) exceeds server max file size ({} bytes)",
            compressed.len(),
            max_size_usize
        ));
    }
    let effective_floor = MIN_DECOMPRESSION_BUFFER_BYTES.min(max_size_usize);
    let decompressed_cap = compressed
        .len()
        .saturating_mul(MAX_COMPRESSION_RATIO)
        .clamp(effective_floor, max_size_usize);
    let initial_capacity = compressed
        .len()
        .saturating_mul(INITIAL_BUFFER_RATIO)
        .clamp(effective_floor, decompressed_cap);
    let decoder = GzDecoder::new(compressed);
    let mut limited_reader = decoder.take((decompressed_cap as u64).saturating_add(1));
    let mut decompressed_bytes = Vec::with_capacity(initial_capacity);
    limited_reader.read_to_end(&mut decompressed_bytes)?;
    if decompressed_bytes.len() > decompressed_cap {
        return Err(anyhow::anyhow!(
            "Decompressed payload exceeds {decompressed_cap} bytes cap (ratio-bounded gzip bomb defense)"
        ));
    }
    let decompressed = String::from_utf8(decompressed_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to decode decompressed content as UTF-8: {e}"))?;
    Ok(decompressed)
}

#[cfg(test)]
mod tests {
    use super::{compress_content, decompress_content};
    use crate::fulgur::sync::share::MAX_SYNC_SHARE_PAYLOAD_BYTES;

    /// Default test cap used by roundtrip tests: no server-advertised limit.
    const TEST_NO_LIMIT: u64 = u64::MAX;

    #[test]
    fn test_roundtrip_empty_string() {
        let original = "";
        let compressed = compress_content(original).unwrap();
        let decompressed = decompress_content(&compressed, TEST_NO_LIMIT).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_roundtrip_ascii_text() {
        let original = "The quick brown fox jumps over the lazy dog.";
        let compressed = compress_content(original).unwrap();
        let decompressed = decompress_content(&compressed, TEST_NO_LIMIT).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_roundtrip_unicode_text() {
        let original = "Héllo Wörld! 你好世界 🌍 Привет";
        let compressed = compress_content(original).unwrap();
        let decompressed = decompress_content(&compressed, TEST_NO_LIMIT).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_roundtrip_multiline_text() {
        let original = "Line 1\nLine 2\r\nLine 3\n\tIndented\n\nEmpty line above";
        let compressed = compress_content(original).unwrap();
        let decompressed = decompress_content(&compressed, TEST_NO_LIMIT).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_roundtrip_special_characters() {
        let original = "!@#$%^&*()_+-=[]{}|;':\",./<>?`~";
        let compressed = compress_content(original).unwrap();
        let decompressed = decompress_content(&compressed, TEST_NO_LIMIT).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_roundtrip_large_repetitive_content() {
        let original = "AAAA".repeat(5000);
        let compressed = compress_content(&original).unwrap();
        let decompressed = decompress_content(&compressed, TEST_NO_LIMIT).unwrap();
        assert_eq!(decompressed, original);
        // Should compress very well
        assert!(compressed.len() < original.len() / 10);
    }

    #[test]
    fn test_roundtrip_json_like_content() {
        let original = r#"{"name": "test", "value": 123, "nested": {"key": "value"}}"#;
        let compressed = compress_content(original).unwrap();
        let decompressed = decompress_content(&compressed, TEST_NO_LIMIT).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_roundtrip_code_like_content() {
        let original = r#"
fn main() {
    let x = 42;
    println!("Hello, world! {}", x);
}
"#;
        let compressed = compress_content(original).unwrap();
        let decompressed = decompress_content(&compressed, TEST_NO_LIMIT).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_roundtrip_mixed_unicode() {
        let original = "English, 中文, 日本語, 한국어, العربية, עברית, Ελληνικά, Русский";
        let compressed = compress_content(original).unwrap();
        let decompressed = decompress_content(&compressed, TEST_NO_LIMIT).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_roundtrip_emoji_heavy() {
        let original = "🔥🎉🚀💻⚡️🌟✨🎯🌈🦄🐉🌸🍕🎮🎨🎭🎪🎬🎤🎧🎼";
        let compressed = compress_content(original).unwrap();
        let decompressed = decompress_content(&compressed, TEST_NO_LIMIT).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_roundtrip_very_large_content() {
        let original = "Lorem ipsum dolor sit amet. ".repeat(10000);
        assert!(original.len() > 250000); // Over 250KB
        let compressed = compress_content(&original).unwrap();
        let decompressed = decompress_content(&compressed, TEST_NO_LIMIT).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_decompress_rejects_compressed_payload_larger_than_server_max() {
        // A server advertising a 1 MB limit must reject a compressed blob that
        // already exceeds that on its own (decompressed can only be larger).
        let oversized_payload = vec![0_u8; MAX_SYNC_SHARE_PAYLOAD_BYTES + 1];
        let result = decompress_content(&oversized_payload, MAX_SYNC_SHARE_PAYLOAD_BYTES as u64);
        assert!(result.is_err());
    }

    #[test]
    fn test_decompress_rejects_decompressed_payload_larger_than_server_max() {
        // Server says 1 MB max. The original is 2 MB+1 of identical characters,
        // which compresses small but decompresses well past the server limit.
        let original = "A".repeat(2 * MAX_SYNC_SHARE_PAYLOAD_BYTES + 1);
        let compressed = compress_content(&original).unwrap();
        let result = decompress_content(&compressed, MAX_SYNC_SHARE_PAYLOAD_BYTES as u64);
        assert!(result.is_err());
    }

    #[test]
    fn test_decompress_rejects_high_ratio_gzip_bomb_even_when_unlimited() {
        // Even with `u64::MAX` (user's "no limit" config), a gzip bomb whose
        // decompressed size exceeds `compressed.len() * MAX_COMPRESSION_RATIO`
        // must be rejected by the ratio cap. It's the only defense when the
        // server absolute cap is disabled.
        let original = "A".repeat(1_000_000);
        let compressed = compress_content(&original).unwrap();
        assert!(
            compressed.len() < 10_000,
            "precondition: highly repetitive content should compress to a tiny payload, got {}",
            compressed.len()
        );
        let err =
            decompress_content(&compressed, TEST_NO_LIMIT).expect_err("bomb must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("ratio-bounded"),
            "bomb rejection should cite the ratio cap, got: {msg}"
        );
    }

    #[test]
    fn test_decompress_with_unlimited_max_size_accepts_large_content() {
        // When the user configures their server for no limit, a genuinely
        // large legitimate payload that stays within the ratio cap must be
        // accepted rather than capped by any hard-coded ceiling.
        let original = "Lorem ipsum dolor sit amet. ".repeat(100_000); // ~2.8 MB
        let compressed = compress_content(&original).unwrap();
        let decompressed = decompress_content(&compressed, TEST_NO_LIMIT).unwrap();
        assert_eq!(decompressed.len(), original.len());
    }

    #[test]
    fn test_decompress_allows_small_payload_up_to_min_buffer() {
        // A short legitimate payload must still decompress even though the
        // ratio cap (compressed.len() * 200) is smaller than what the minimum
        // decompression buffer allows.
        let original = "Hello, world!".repeat(100); // ~1.3 KB
        let compressed = compress_content(&original).unwrap();
        let decompressed = decompress_content(&compressed, TEST_NO_LIMIT).unwrap();
        assert_eq!(decompressed, original);
    }
}
