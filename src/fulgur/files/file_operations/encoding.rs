use crate::fulgur::ui::components_utils::UTF_8;
use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};

/// Number of leading bytes inspected by the binary-content heuristic.
const BINARY_SNIFF_LEN: usize = 8_000;

/// Outcome of decoding raw file bytes into editor text.
pub struct DecodedContents {
    pub encoding: String,
    pub content: String,
    pub lossy: bool,
    pub byte_len: usize,
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
/// - `DecodedContents`: The detected encoding, decoded text, the original byte
///   length, and whether the decode was lossy
#[must_use]
pub fn detect_encoding_and_decode(bytes: Vec<u8>) -> DecodedContents {
    let byte_len = bytes.len();
    if let Some((encoding, _)) = utf16_bom(&bytes) {
        let (decoded, had_errors) = encoding.decode_with_bom_removal(&bytes);
        let encoding_name = encoding.name().to_string();
        log::debug!("File encoding detected from BOM as: {encoding_name}");
        return DecodedContents {
            encoding: encoding_name,
            content: decoded.into_owned(),
            lossy: had_errors,
            byte_len,
        };
    }
    let bytes = match String::from_utf8(bytes) {
        Ok(text) => {
            log::debug!("File encoding detected as UTF-8");
            return DecodedContents {
                encoding: UTF_8.to_string(),
                content: text,
                lossy: false,
                byte_len,
            };
        }
        Err(error) => error.into_bytes(),
    };
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Allow);
    detector.feed(&bytes, true);
    let encoding = detector.guess(None, Utf8Detection::Allow);
    let (decoded, _, had_errors) = encoding.decode(&bytes);
    if had_errors {
        log::warn!("File encoding detection failed, using UTF-8 lossy conversion");
        return DecodedContents {
            encoding: UTF_8.to_string(),
            content: String::from_utf8_lossy(&bytes).to_string(),
            lossy: true,
            byte_len,
        };
    }
    let encoding_name = encoding.name().to_string();
    log::debug!("File encoding detected as: {encoding_name}");
    DecodedContents {
        encoding: encoding_name,
        content: decoded.to_string(),
        lossy: false,
        byte_len,
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
#[must_use]
pub fn encode_for_save(contents: &str, label: &str) -> EncodedContents {
    if label.eq_ignore_ascii_case(UTF_8) {
        return EncodedContents::Encoded(contents.as_bytes().to_vec());
    }
    let Some(encoding) = encoding_rs::Encoding::for_label(label.as_bytes()) else {
        log::warn!("Unknown encoding label '{label}', saving as UTF-8");
        return EncodedContents::Encoded(contents.as_bytes().to_vec());
    };
    if encoding == encoding_rs::UTF_16LE || encoding == encoding_rs::UTF_16BE {
        return EncodedContents::Encoded(encode_utf16_with_bom(
            contents,
            encoding == encoding_rs::UTF_16BE,
        ));
    }
    if encoding == encoding_rs::REPLACEMENT {
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

/// Encode text as UTF-16 preceded by a byte order mark.
///
/// ### Arguments
/// - `contents`: The editor text to encode
/// - `big_endian`: Whether to write UTF-16BE instead of UTF-16LE
///
/// ### Returns
/// - `Vec<u8>`: The BOM followed by the encoded code units
fn encode_utf16_with_bom(contents: &str, big_endian: bool) -> Vec<u8> {
    let unit_to_bytes: fn(u16) -> [u8; 2] = if big_endian {
        u16::to_be_bytes
    } else {
        u16::to_le_bytes
    };
    let mut bytes = Vec::with_capacity(2 + contents.len() * 2);
    bytes.extend_from_slice(&unit_to_bytes(0xFEFF));
    for unit in contents.encode_utf16() {
        bytes.extend_from_slice(&unit_to_bytes(unit));
    }
    bytes
}

/// Detect a UTF-16 byte order mark at the start of the bytes.
///
/// ### Arguments
/// - `bytes`: The file bytes to inspect
///
/// ### Returns
/// - `Some((encoding, bom_len))`: The UTF-16 variant named by the BOM and the BOM length
/// - `None`: The bytes do not start with a UTF-16 BOM
fn utf16_bom(bytes: &[u8]) -> Option<(&'static encoding_rs::Encoding, usize)> {
    encoding_rs::Encoding::for_bom(bytes).filter(|(encoding, _)| {
        *encoding == encoding_rs::UTF_16LE || *encoding == encoding_rs::UTF_16BE
    })
}

/// Heuristically determine whether bytes represent a binary (non-text) file.
///
/// ### Arguments
/// - `bytes`: The file bytes to inspect
///
/// ### Returns
/// - `bool`: `true` if the prefix contains a NUL character
#[must_use]
pub fn looks_binary(bytes: &[u8]) -> bool {
    let prefix = &bytes[..bytes.len().min(BINARY_SNIFF_LEN)];
    match utf16_bom(prefix) {
        Some((_, bom_len)) => prefix[bom_len..].as_chunks::<2>().0.contains(&[0, 0]),
        None => prefix.contains(&0),
    }
}

/// Decode file bytes into editor text, refusing content that looks binary.
///
/// ### Arguments
/// - `bytes`: The raw file bytes
///
/// ### Returns
/// - `Some(DecodedContents)`: The decoded text file
/// - `None`: The bytes look binary and must not be loaded into an editor
#[must_use]
pub fn decode_text_file(bytes: Vec<u8>) -> Option<DecodedContents> {
    if looks_binary(&bytes) {
        None
    } else {
        Some(detect_encoding_and_decode(bytes))
    }
}

/// Build the user-facing message shown when a file is refused as binary.
///
/// ### Arguments
/// - `file_name`: The display name of the refused file
///
/// ### Returns
/// - `String`: The notification text
#[must_use]
pub fn binary_file_notice(file_name: &str) -> String {
    format!("Cannot open '{file_name}': appears to be a binary file")
}

#[cfg(test)]
mod tests {
    use super::{
        EncodedContents, decode_text_file, detect_encoding_and_decode, encode_for_save,
        looks_binary,
    };
    use crate::fulgur::ui::components_utils::UTF_8;

    #[test]
    fn test_detect_encoding_returns_utf8_for_valid_utf8_text() {
        let text = "Hello, world! Fulgur rocks.";
        let decoded = detect_encoding_and_decode(text.as_bytes().to_vec());
        assert_eq!(decoded.encoding, UTF_8);
        assert_eq!(decoded.content, text);
        assert!(!decoded.lossy);
        assert_eq!(decoded.byte_len, text.len());
    }

    #[test]
    fn test_detect_encoding_returns_utf8_for_ascii_content() {
        let text = "fn main() { println!(\"hi\"); }";
        let decoded = detect_encoding_and_decode(text.as_bytes().to_vec());
        assert_eq!(decoded.encoding, UTF_8);
        assert_eq!(decoded.content, text);
        assert!(!decoded.lossy);
    }

    #[test]
    fn test_detect_encoding_detects_non_utf8_encoding() {
        // 0xE9 is 'é' in Latin-1 but not a valid UTF-8 byte sequence on its own
        let bytes: Vec<u8> = vec![0x63, 0x61, 0x66, 0xE9]; // "café" in Latin-1
        let decoded = detect_encoding_and_decode(bytes);
        assert_ne!(decoded.encoding, UTF_8);
        assert_ne!(decoded.content, "");
    }

    #[test]
    fn test_encode_for_save_roundtrips_latin1() {
        // "café" decoded from Latin-1, re-encoded must restore the original bytes.
        let original: &[u8] = &[0x63, 0x61, 0x66, 0xE9];
        let decoded = detect_encoding_and_decode(original.to_vec());
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

    #[test]
    fn test_decode_text_file_refuses_binary_and_decodes_text() {
        assert!(decode_text_file(vec![0x66, 0x00, 0x6F]).is_none());
        let decoded = decode_text_file(b"plain text".to_vec()).expect("text must decode");
        assert_eq!(decoded.content, "plain text");
    }

    /// CRLF, Latin-1 and an astral character (U+1D11E, a surrogate pair in UTF-16).
    const UTF16_SAMPLE: &str = "Windows Registry Editor\r\ncaf\u{E9} \u{1D11E}";

    /// Serialize text as UTF-16 with a leading BOM, independently of the code under test.
    ///
    /// ### Arguments
    /// - `text`: The text to serialize
    /// - `big_endian`: Whether to produce UTF-16BE instead of UTF-16LE
    ///
    /// ### Returns
    /// - `Vec<u8>`: The BOM followed by the encoded code units
    fn utf16_file_bytes(text: &str, big_endian: bool) -> Vec<u8> {
        std::iter::once(0xFEFF)
            .chain(text.encode_utf16())
            .flat_map(|unit| {
                if big_endian {
                    unit.to_be_bytes()
                } else {
                    unit.to_le_bytes()
                }
            })
            .collect()
    }

    #[test]
    fn test_decode_text_file_accepts_utf16_with_bom() {
        for (big_endian, label) in [(false, "UTF-16LE"), (true, "UTF-16BE")] {
            let bytes = utf16_file_bytes(UTF16_SAMPLE, big_endian);
            let byte_len = bytes.len();
            let decoded = decode_text_file(bytes).expect("UTF-16 text must not look binary");
            assert_eq!(decoded.encoding, label);
            assert_eq!(decoded.content, UTF16_SAMPLE, "the BOM must be stripped");
            assert!(!decoded.lossy);
            assert_eq!(decoded.byte_len, byte_len);
        }
    }

    #[test]
    fn test_encode_for_save_roundtrips_utf16_with_bom() {
        for big_endian in [false, true] {
            let original = utf16_file_bytes(UTF16_SAMPLE, big_endian);
            let decoded = detect_encoding_and_decode(original.clone());
            let EncodedContents::Encoded(bytes) =
                encode_for_save(&decoded.content, &decoded.encoding)
            else {
                panic!("UTF-16 must always encode losslessly");
            };
            assert_eq!(bytes, original);
        }
    }

    #[test]
    fn test_detect_encoding_flags_malformed_utf16_as_lossy() {
        let mut bytes = utf16_file_bytes("text", false);
        bytes.push(0x41);
        let decoded = detect_encoding_and_decode(bytes);
        assert_eq!(decoded.encoding, "UTF-16LE");
        assert!(
            decoded.lossy,
            "a dangling odd byte must flag the decode as lossy"
        );
    }

    #[test]
    fn test_looks_binary_detects_nul_code_unit_after_utf16_bom() {
        assert!(looks_binary(&[0xFF, 0xFE, 0x68, 0x00, 0x00, 0x00]));
        // A UTF-32LE BOM reads as a UTF-16LE BOM followed by a NUL code unit.
        assert!(looks_binary(&[
            0xFF, 0xFE, 0x00, 0x00, 0x68, 0x00, 0x00, 0x00
        ]));
    }
}
