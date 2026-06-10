use crate::fulgur::ui::components_utils::UTF_8;
use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};

/// Number of leading bytes inspected by the binary-content heuristic.
const BINARY_SNIFF_LEN: usize = 8_000;

/// Outcome of decoding raw file bytes into editor text.
pub struct DecodedContents {
    pub encoding: String,
    pub content: String,
    pub lossy: bool,
}

/// Outcome of re-encoding editor text for writing back to disk.
pub enum EncodedContents {
    Encoded(Vec<u8>),
    Lossy,
}

/// Detect encoding from file bytes
///
/// ### Arguments
/// - `bytes`: The bytes to detect encoding from
///
/// ### Returns
/// - `DecodedContents`: The detected encoding, decoded text, and whether the decode was lossy
pub fn detect_encoding_and_decode(bytes: &[u8]) -> DecodedContents {
    if let Ok(text) = std::str::from_utf8(bytes) {
        log::debug!("File encoding detected as UTF-8");
        return DecodedContents {
            encoding: UTF_8.to_string(),
            content: text.to_string(),
            lossy: false,
        };
    }
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Allow);
    detector.feed(bytes, true);
    let encoding = detector.guess(None, Utf8Detection::Allow);
    let (decoded, _, had_errors) = encoding.decode(bytes);
    if had_errors {
        log::warn!("File encoding detection failed, using UTF-8 lossy conversion");
        return DecodedContents {
            encoding: UTF_8.to_string(),
            content: String::from_utf8_lossy(bytes).to_string(),
            lossy: true,
        };
    }
    let encoding_name = encoding.name().to_string();
    log::debug!("File encoding detected as: {encoding_name}");
    DecodedContents {
        encoding: encoding_name,
        content: decoded.to_string(),
        lossy: false,
    }
}

/// Re-encode editor text into bytes using the tab's stored encoding label.
///
/// ### Arguments
/// - `contents`: The editor text to encode
/// - `label`: The target encoding label (the tab's `encoding` field)
///
/// ### Returns
/// - `EncodedContents::Encoded`: The encoded bytes ready to write
/// - `EncodedContents::Lossy`: The text cannot be represented losslessly in the
///   target encoding
pub fn encode_for_save(contents: &str, label: &str) -> EncodedContents {
    if label.eq_ignore_ascii_case(UTF_8) {
        return EncodedContents::Encoded(contents.as_bytes().to_vec());
    }
    let Some(encoding) = encoding_rs::Encoding::for_label(label.as_bytes()) else {
        log::warn!("Unknown encoding label '{label}', saving as UTF-8");
        return EncodedContents::Encoded(contents.as_bytes().to_vec());
    };
    if encoding == encoding_rs::UTF_16LE
        || encoding == encoding_rs::UTF_16BE
        || encoding == encoding_rs::REPLACEMENT
    {
        log::warn!("Cannot encode to '{label}', saving as UTF-8");
        return EncodedContents::Encoded(contents.as_bytes().to_vec());
    }
    let (encoded, _, had_unmappable) = encoding.encode(contents);
    if had_unmappable {
        EncodedContents::Lossy
    } else {
        EncodedContents::Encoded(encoded.into_owned())
    }
}

/// Heuristically determine whether bytes represent a binary (non-text) file.
///
/// ### Arguments
/// - `bytes`: The file bytes to inspect
///
/// ### Returns
/// - `bool`: `true` if the prefix contains a NUL byte
pub fn looks_binary(bytes: &[u8]) -> bool {
    let prefix = &bytes[..bytes.len().min(BINARY_SNIFF_LEN)];
    prefix.contains(&0)
}

#[cfg(test)]
mod tests {
    use super::{EncodedContents, detect_encoding_and_decode, encode_for_save, looks_binary};
    use crate::fulgur::ui::components_utils::UTF_8;

    #[test]
    fn test_detect_encoding_returns_utf8_for_valid_utf8_text() {
        let text = "Hello, world! Fulgur rocks.";
        let decoded = detect_encoding_and_decode(text.as_bytes());
        assert_eq!(decoded.encoding, UTF_8);
        assert_eq!(decoded.content, text);
        assert!(!decoded.lossy);
    }

    #[test]
    fn test_detect_encoding_returns_utf8_for_ascii_content() {
        let text = "fn main() { println!(\"hi\"); }";
        let decoded = detect_encoding_and_decode(text.as_bytes());
        assert_eq!(decoded.encoding, UTF_8);
        assert_eq!(decoded.content, text);
        assert!(!decoded.lossy);
    }

    #[test]
    fn test_detect_encoding_detects_non_utf8_encoding() {
        // 0xE9 is 'é' in Latin-1 but not a valid UTF-8 byte sequence on its own
        let bytes: &[u8] = &[0x63, 0x61, 0x66, 0xE9]; // "café" in Latin-1
        let decoded = detect_encoding_and_decode(bytes);
        assert_ne!(decoded.encoding, UTF_8);
        assert!(!decoded.content.is_empty());
    }

    #[test]
    fn test_encode_for_save_roundtrips_latin1() {
        // "café" decoded from Latin-1, re-encoded must restore the original bytes.
        let original: &[u8] = &[0x63, 0x61, 0x66, 0xE9];
        let decoded = detect_encoding_and_decode(original);
        let EncodedContents::Encoded(bytes) = encode_for_save(&decoded.content, &decoded.encoding)
        else {
            panic!("expected lossless re-encode for Latin-1 content");
        };
        assert_eq!(bytes, original);
    }

    #[test]
    fn test_encode_for_save_utf8_passthrough() {
        let EncodedContents::Encoded(bytes) = encode_for_save("héllo", UTF_8) else {
            panic!("UTF-8 must always encode losslessly");
        };
        assert_eq!(bytes, "héllo".as_bytes());
    }

    #[test]
    fn test_encode_for_save_reports_lossy_for_unrepresentable_chars() {
        // The euro sign maps to 0x80 in windows-1252, so it encodes cleanly.
        assert!(matches!(
            encode_for_save("\u{20AC}", "windows-1252"),
            EncodedContents::Encoded(_)
        ));
        // CJK characters have no windows-1252 representation, so encoding is lossy.
        assert!(matches!(
            encode_for_save("你好", "windows-1252"),
            EncodedContents::Lossy
        ));
    }

    #[test]
    fn test_looks_binary_detects_nul_byte() {
        assert!(looks_binary(&[0x66, 0x6F, 0x00, 0x6F]));
        assert!(!looks_binary(b"plain text content"));
    }
}
