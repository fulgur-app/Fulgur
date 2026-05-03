use gpui::SharedString;
use regex::Regex;
use std::sync::LazyLock;

/// Regex for matching line numbers and line:column positions
static LINE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+|\d+:\d+)$").unwrap());

/// A line and optional character position for cursor navigation
#[derive(Copy, Clone)]
pub struct Jump {
    pub line: u32,
    pub character: Option<u32>,
}

/// Extract the line number and character from a destination string
///
/// ### Arguments
/// - `destination`: The destination string
///
/// ### Returns
/// - `Ok(Jump)`: The jump struct
/// - `Err(anyhow::Error)`: If the destination string is not a valid jump
pub fn extract_line_number(destination: &SharedString) -> anyhow::Result<Jump> {
    let mut jump = Jump {
        line: 0,
        character: None,
    };
    let re = LINE_REGEX.clone();
    re.is_match(destination.as_str())
        .then(|| {
            if destination.contains(":") {
                let parts = destination.split(":").collect::<Vec<&str>>();
                if parts.len() == 2 {
                    let line = string_to_u32(parts[0]);
                    jump.line = if line > 0 { line - 1 } else { 0 };
                    jump.character = Some(string_to_u32(parts[1]));
                }
            } else {
                let line = string_to_u32(destination.as_str());
                jump.line = if line > 0 { line - 1 } else { 0 };
            }
        })
        .ok_or(anyhow::anyhow!("Invalid destination"))?;
    Ok(jump)
}

/// Convert a string to a u32
///
/// ### Arguments
/// - `string`: The string to convert
///
/// ### Returns
/// - `u32`: The u32 value of the string, or 0 if the string is not a valid u32
fn string_to_u32(string: &str) -> u32 {
    match string.parse::<u32>() {
        Ok(line) => line,
        Err(e) => match e.kind() {
            std::num::IntErrorKind::PosOverflow => u32::MAX,
            _ => 0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_line_number, string_to_u32};
    use gpui::SharedString;

    // ========== extract_line_number() tests ==========

    #[test]
    fn test_extract_line_number_simple() {
        let destination = SharedString::from("23");
        let result = extract_line_number(&destination).unwrap();
        assert_eq!(result.line, 22);
        assert_eq!(result.character, None);
    }

    #[test]
    fn test_extract_line_number_with_character() {
        let destination = SharedString::from("23:17");
        let result = extract_line_number(&destination).unwrap();
        assert_eq!(result.line, 22);
        assert_eq!(result.character, Some(17));
    }

    #[test]
    fn test_extract_line_number_invalid() {
        let destination = SharedString::from("azerty");
        let result = extract_line_number(&destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_line_1() {
        // Line 1 should convert to index 0
        let destination = SharedString::from("1");
        let result = extract_line_number(&destination).unwrap();
        assert_eq!(result.line, 0);
        assert_eq!(result.character, None);
    }

    #[test]
    fn test_extract_line_number_line_0() {
        // Line 0 should lead to the first line
        let destination = SharedString::from("0");
        let result = extract_line_number(&destination).unwrap();
        assert_eq!(result.line, 0);
        assert_eq!(result.character, None);
    }

    #[test]
    fn test_extract_line_number_negative() {
        // Negative numbers should fail regex validation
        let destination = SharedString::from("-5");
        let result = extract_line_number(&destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_very_large() {
        // Very large valid number
        let destination = SharedString::from("999999999");
        let result = extract_line_number(&destination).unwrap();
        assert_eq!(result.line, 999999998);
        assert_eq!(result.character, None);
    }

    #[test]
    fn test_extract_line_number_overflow() {
        // Number larger than u32::MAX should cause parse to fail, returning 0
        let destination = SharedString::from("99999999999999999999");
        let result = extract_line_number(&destination).unwrap();
        assert_eq!(result.line, u32::MAX - 1);
        assert_eq!(result.character, None);
    }

    #[test]
    fn test_extract_line_number_non_numeric() {
        // Non-numeric input should fail regex validation
        let destination = SharedString::from("abc");
        let result = extract_line_number(&destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_partial_numeric() {
        // Partially numeric input should fail regex validation
        let destination = SharedString::from("123abc");
        let result = extract_line_number(&destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_malformed_colon() {
        // Malformed input with double colon should fail regex validation
        let destination = SharedString::from("file.txt::");
        let result = extract_line_number(&destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_missing_number_after_colon() {
        // "line:" without number should fail regex validation
        let destination = SharedString::from("23:");
        let result = extract_line_number(&destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_missing_number_before_colon() {
        // ":23" without line number should fail regex validation
        let destination = SharedString::from(":23");
        let result = extract_line_number(&destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_empty_string() {
        // Empty string should fail regex validation
        let destination = SharedString::from("");
        let result = extract_line_number(&destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_with_whitespace() {
        // Whitespace should fail regex validation
        let destination = SharedString::from(" 23 ");
        let result = extract_line_number(&destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_zero_character() {
        // Line with character 0
        let destination = SharedString::from("10:0");
        let result = extract_line_number(&destination).unwrap();
        assert_eq!(result.line, 9);
        assert_eq!(result.character, Some(0));
    }

    #[test]
    fn test_extract_line_number_three_parts() {
        // Three parts separated by colons should fail regex validation
        let destination = SharedString::from("10:5:2");
        let result = extract_line_number(&destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_with_character_overflow() {
        // Very large character position
        let destination = SharedString::from("10:99999999999999999999");
        let result = extract_line_number(&destination).unwrap();
        assert_eq!(result.line, 9);
        // Overflow causes parse to fail, string_to_u32 returns 0
        assert_eq!(result.character, Some(u32::MAX));
    }

    // ========== string_to_u32() tests ==========

    #[test]
    fn test_string_to_u32_valid() {
        assert_eq!(string_to_u32("123"), 123);
        assert_eq!(string_to_u32("0"), 0);
        assert_eq!(string_to_u32("1"), 1);
    }

    #[test]
    fn test_string_to_u32_invalid_non_numeric() {
        // Invalid strings return 0
        assert_eq!(string_to_u32("abc"), 0);
        assert_eq!(string_to_u32("xyz123"), 0);
        assert_eq!(string_to_u32("123abc"), 0);
    }

    #[test]
    fn test_string_to_u32_negative() {
        // Negative numbers return 0
        assert_eq!(string_to_u32("-5"), 0);
        assert_eq!(string_to_u32("-123"), 0);
    }

    #[test]
    fn test_string_to_u32_overflow() {
        // Numbers larger than u32::MAX return 0
        assert_eq!(string_to_u32("99999999999999999999"), u32::MAX);
        assert_eq!(string_to_u32("4294967296"), u32::MAX);
    }

    #[test]
    fn test_string_to_u32_max_value() {
        // u32::MAX should parse correctly
        assert_eq!(string_to_u32("4294967295"), u32::MAX);
    }

    #[test]
    fn test_string_to_u32_empty_string() {
        // Empty string returns 0
        assert_eq!(string_to_u32(""), 0);
    }

    #[test]
    fn test_string_to_u32_whitespace() {
        // Whitespace returns 0 (parse fails)
        assert_eq!(string_to_u32(" "), 0);
        assert_eq!(string_to_u32("  123  "), 0);
        assert_eq!(string_to_u32("\t123\n"), 0);
    }

    #[test]
    fn test_string_to_u32_special_characters() {
        // Special characters return 0
        assert_eq!(string_to_u32("!@#$"), 0);
        assert_eq!(string_to_u32("12.34"), 0); // Decimal point
        assert_eq!(string_to_u32("12,345"), 0); // Comma
    }
}
