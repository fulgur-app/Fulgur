//! CSV parsing and serialization helpers backing the table view.
//!
/// The delimiter used when none can be detected.
pub const DEFAULT_DELIMITER: u8 = b',';

/// Candidate delimiters considered by [`detect_delimiter`], in priority order.
const CANDIDATE_DELIMITERS: [u8; 3] = [b',', b';', b'\t'];

/// A rectangular view of a CSV file: a header row plus data rows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CsvData {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Detect the most likely delimiter of a CSV document.
///
/// ### Arguments
/// - `text`: The full CSV document text
///
/// ### Returns
/// - `u8`: The detected delimiter byte, or [`DEFAULT_DELIMITER`] when no
///   candidate appears on the first non-empty line
pub fn detect_delimiter(text: &str) -> u8 {
    let first_line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");

    let mut best = DEFAULT_DELIMITER;
    let mut best_count = 0usize;
    for &candidate in &CANDIDATE_DELIMITERS {
        let count = first_line.bytes().filter(|&byte| byte == candidate).count();
        if count > best_count {
            best_count = count;
            best = candidate;
        }
    }
    best
}

/// Parse CSV text into a rectangular [`CsvData`] model.
///
/// ### Arguments
/// - `text`: The CSV document text
/// - `delimiter`: The field delimiter byte
///
/// ### Returns
/// - `CsvData`: The parsed headers and rows, padded to a common width. Empty
///   input yields empty headers and no rows.
pub fn parse_csv(text: &str, delimiter: u8) -> CsvData {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(text.as_bytes());

    let mut records: Vec<Vec<String>> = Vec::new();
    for result in reader.records() {
        match result {
            Ok(record) => records.push(record.iter().map(str::to_string).collect()),
            Err(error) => log::warn!("Failed to parse CSV record: {error}"),
        }
    }

    if records.is_empty() {
        return CsvData::default();
    }

    let width = records.iter().map(Vec::len).max().unwrap_or(0);
    for record in &mut records {
        record.resize(width, String::new());
    }

    let headers = records.remove(0);
    CsvData {
        headers,
        rows: records,
    }
}

/// Serialize a [`CsvData`] model back to CSV text.
///
/// ### Arguments
/// - `headers`: The column headers, written as the first record
/// - `rows`: The data rows
/// - `delimiter`: The field delimiter byte
///
/// ### Returns
/// - `String`: The serialized CSV text
pub fn serialize_csv(headers: &[String], rows: &[Vec<String>], delimiter: u8) -> String {
    let mut writer = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_writer(Vec::new());

    if let Err(error) = writer.write_record(headers) {
        log::warn!("Failed to write CSV header record: {error}");
    }
    for row in rows {
        if let Err(error) = writer.write_record(row) {
            log::warn!("Failed to write CSV data record: {error}");
        }
    }

    let bytes = writer.into_inner().unwrap_or_default();
    String::from_utf8(bytes).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{CsvData, detect_delimiter, parse_csv, serialize_csv};

    #[test]
    fn test_detect_delimiter_comma() {
        assert_eq!(detect_delimiter("a,b,c\n1,2,3\n"), b',');
    }

    #[test]
    fn test_detect_delimiter_semicolon() {
        assert_eq!(detect_delimiter("a;b;c\n1;2;3\n"), b';');
    }

    #[test]
    fn test_detect_delimiter_tab() {
        assert_eq!(detect_delimiter("a\tb\tc\n1\t2\t3\n"), b'\t');
    }

    #[test]
    fn test_detect_delimiter_defaults_to_comma_when_none() {
        assert_eq!(detect_delimiter("single_column\nvalue\n"), b',');
    }

    #[test]
    fn test_detect_delimiter_skips_blank_leading_lines() {
        assert_eq!(detect_delimiter("\n\na;b;c\n"), b';');
    }

    #[test]
    fn test_parse_simple() {
        let data = parse_csv("name,age\nAlice,30\nBob,25\n", b',');
        assert_eq!(data.headers, vec!["name", "age"]);
        assert_eq!(data.rows, vec![vec!["Alice", "30"], vec!["Bob", "25"]]);
    }

    #[test]
    fn test_parse_empty() {
        assert_eq!(parse_csv("", b','), CsvData::default());
    }

    #[test]
    fn test_parse_pads_ragged_rows() {
        let data = parse_csv("a,b,c\n1,2\n3\n", b',');
        assert_eq!(data.headers, vec!["a", "b", "c"]);
        assert_eq!(data.rows, vec![vec!["1", "2", ""], vec!["3", "", ""]]);
    }

    #[test]
    fn test_parse_extends_width_for_long_rows() {
        let data = parse_csv("a,b\n1,2,3\n", b',');
        assert_eq!(data.headers, vec!["a", "b", ""]);
        assert_eq!(data.rows, vec![vec!["1", "2", "3"]]);
    }

    #[test]
    fn test_roundtrip_simple() {
        let text = "name,age\nAlice,30\nBob,25\n";
        let data = parse_csv(text, b',');
        assert_eq!(serialize_csv(&data.headers, &data.rows, b','), text);
    }

    #[test]
    fn test_roundtrip_quoted_embedded_comma() {
        let text = "name,note\n\"Doe, John\",hello\n";
        let data = parse_csv(text, b',');
        assert_eq!(data.rows, vec![vec!["Doe, John", "hello"]]);
        assert_eq!(serialize_csv(&data.headers, &data.rows, b','), text);
    }

    #[test]
    fn test_roundtrip_embedded_newline() {
        let text = "name,note\nAlice,\"line1\nline2\"\n";
        let data = parse_csv(text, b',');
        assert_eq!(data.rows, vec![vec!["Alice", "line1\nline2"]]);
        assert_eq!(serialize_csv(&data.headers, &data.rows, b','), text);
    }

    #[test]
    fn test_roundtrip_semicolon() {
        let text = "a;b\n1;2\n";
        let data = parse_csv(text, b';');
        assert_eq!(serialize_csv(&data.headers, &data.rows, b';'), text);
    }

    #[test]
    fn test_roundtrip_tab() {
        let text = "a\tb\n1\t2\n";
        let data = parse_csv(text, b'\t');
        assert_eq!(serialize_csv(&data.headers, &data.rows, b'\t'), text);
    }
}
