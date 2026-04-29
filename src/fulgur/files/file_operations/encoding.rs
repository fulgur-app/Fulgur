use crate::fulgur::ui::components_utils::UTF_8;
use chardetng::EncodingDetector;

/// Detect encoding from file bytes
///
/// ### Arguments
/// - `bytes`: The bytes to detect encoding from
///
/// ### Returns
/// - `(String, String)`: The detected encoding and decoded string
pub fn detect_encoding_and_decode(bytes: &[u8]) -> (String, String) {
    if let Ok(text) = std::str::from_utf8(bytes) {
        log::debug!("File encoding detected as UTF-8");
        return (UTF_8.to_string(), text.to_string());
    }
    let mut detector = EncodingDetector::new();
    detector.feed(bytes, true);
    let encoding = detector.guess(None, true);
    let (decoded, _, had_errors) = encoding.decode(bytes);
    let encoding_name = if had_errors {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                log::debug!("File encoding detected as UTF-8 (after error recovery)");
                return (UTF_8.to_string(), text.to_string());
            }
            Err(_) => {
                let text = String::from_utf8_lossy(bytes).to_string();
                log::warn!("File encoding detection failed, using UTF-8 lossy conversion");
                return (UTF_8.to_string(), text);
            }
        }
    } else {
        encoding.name().to_string()
    };
    log::debug!("File encoding detected as: {}", encoding_name);
    (encoding_name, decoded.to_string())
}

#[cfg(test)]
mod tests {
    use super::detect_encoding_and_decode;
    use crate::fulgur::ui::components_utils::UTF_8;

    #[test]
    fn test_detect_encoding_returns_utf8_for_valid_utf8_text() {
        let text = "Hello, world! Fulgur rocks.";
        let (encoding, decoded) = detect_encoding_and_decode(text.as_bytes());
        assert_eq!(encoding, UTF_8);
        assert_eq!(decoded, text);
    }

    #[test]
    fn test_detect_encoding_returns_utf8_for_ascii_content() {
        let text = "fn main() { println!(\"hi\"); }";
        let (encoding, decoded) = detect_encoding_and_decode(text.as_bytes());
        assert_eq!(encoding, UTF_8);
        assert_eq!(decoded, text);
    }

    #[test]
    fn test_detect_encoding_detects_non_utf8_encoding() {
        // 0xE9 is 'é' in Latin-1 but not a valid UTF-8 byte sequence on its own
        let bytes: &[u8] = &[0x63, 0x61, 0x66, 0xE9]; // "café" in Latin-1
        let (encoding, decoded) = detect_encoding_and_decode(bytes);
        assert_ne!(encoding, UTF_8);
        assert!(!decoded.is_empty());
    }
}
