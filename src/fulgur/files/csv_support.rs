//! CSV parsing and serialization helpers backing the table view.
//!
/// The delimiter used when none can be detected.
pub const DEFAULT_DELIMITER: u8 = b',';

/// Candidate delimiters considered by [`detect_delimiter`], in priority order.
const CANDIDATE_DELIMITERS: [u8; 3] = *b",;\t";

/// A rectangular view of a CSV file: a header row plus data rows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CsvData {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// The outcome of parsing CSV text: the rectangular model plus the number of
/// records that were dropped because they could not be parsed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CsvParseOutcome {
    pub data: CsvData,
    pub dropped_records: usize,
}

/// Detect the most likely delimiter of a CSV document.
///
/// ### Arguments
/// - `text`: The full CSV document text
///
/// ### Returns
/// - `u8`: The detected delimiter byte, or [`DEFAULT_DELIMITER`] when no
///   candidate appears on the first non-empty line
#[must_use]
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
/// - `CsvParseOutcome`: The parsed headers and rows, padded to a common width,
///   together with the count of records that failed to parse. Empty input
///   yields empty headers and no rows.
pub fn parse_csv(text: &str, delimiter: u8) -> CsvParseOutcome {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(text.as_bytes());

    let mut records: Vec<Vec<String>> = Vec::new();
    let mut dropped_records = 0usize;
    for result in reader.records() {
        match result {
            Ok(record) => records.push(record.iter().map(str::to_string).collect()),
            Err(error) => {
                dropped_records += 1;
                log::warn!("Failed to parse CSV record: {error}");
            }
        }
    }

    if records.is_empty() {
        return CsvParseOutcome {
            data: CsvData::default(),
            dropped_records,
        };
    }

    let width = records.iter().map(Vec::len).max().unwrap_or(0);
    for record in &mut records {
        record.resize(width, String::new());
    }

    let headers = records.remove(0);
    CsvParseOutcome {
        data: CsvData {
            headers,
            rows: records,
        },
        dropped_records,
    }
}

/// Serialize a [`CsvData`] model back to CSV text.
///
/// ### Arguments
/// - `headers`: The column headers, written as the first record
/// - `rows`: The data rows
/// - `delimiter`: The field delimiter byte
///
/// ### Errors
/// Returns an `Err(String)` describing the failure if a record cannot be
/// written, the writer cannot be finalized, or the produced bytes are not valid
/// UTF-8. Callers must leave the source buffer untouched on error rather than
/// writing back a partial or empty result.
///
/// ### Returns
/// - `Ok(String)`: The serialized CSV text
/// - `Err(String)`: A description of the serialization failure
pub fn serialize_csv(
    headers: &[String],
    rows: &[Vec<String>],
    delimiter: u8,
) -> Result<String, String> {
    let mut writer = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_writer(Vec::new());

    writer
        .write_record(headers)
        .map_err(|error| format!("failed to write CSV header record: {error}"))?;
    for row in rows {
        writer
            .write_record(row)
            .map_err(|error| format!("failed to write CSV data record: {error}"))?;
    }

    let bytes = writer
        .into_inner()
        .map_err(|error| format!("failed to finalize CSV writer: {error}"))?;
    String::from_utf8(bytes).map_err(|error| format!("CSV output is not valid UTF-8: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{CsvParseOutcome, detect_delimiter, parse_csv, serialize_csv};

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
        let outcome = parse_csv("name,age\nAlice,30\nBob,25\n", b',');
        assert_eq!(outcome.dropped_records, 0);
        assert_eq!(outcome.data.headers, vec!["name", "age"]);
        assert_eq!(
            outcome.data.rows,
            vec![vec!["Alice", "30"], vec!["Bob", "25"]]
        );
    }

    #[test]
    fn test_parse_empty() {
        assert_eq!(parse_csv("", b','), CsvParseOutcome::default());
    }

    #[test]
    fn test_parse_pads_ragged_rows() {
        let outcome = parse_csv("a,b,c\n1,2\n3\n", b',');
        assert_eq!(outcome.data.headers, vec!["a", "b", "c"]);
        assert_eq!(
            outcome.data.rows,
            vec![vec!["1", "2", ""], vec!["3", "", ""]]
        );
    }

    #[test]
    fn test_parse_extends_width_for_long_rows() {
        let outcome = parse_csv("a,b\n1,2,3\n", b',');
        assert_eq!(outcome.data.headers, vec!["a", "b", ""]);
        assert_eq!(outcome.data.rows, vec![vec!["1", "2", "3"]]);
    }

    #[test]
    fn test_parse_does_not_count_ragged_rows_as_dropped() {
        // Ragged rows are recovered by padding, not dropped, so they must not
        // trip the lossy-parse guard that forces a fallback to text mode.
        let outcome = parse_csv("a,b,c\n1,2\n3\n", b',');
        assert_eq!(outcome.dropped_records, 0);
        assert_eq!(outcome.data.rows.len(), 2);
    }

    #[test]
    fn test_roundtrip_simple() {
        let text = "name,age\nAlice,30\nBob,25\n";
        let outcome = parse_csv(text, b',');
        assert_eq!(
            serialize_csv(&outcome.data.headers, &outcome.data.rows, b','),
            Ok(text.to_string())
        );
    }

    #[test]
    fn test_roundtrip_quoted_embedded_comma() {
        let text = "name,note\n\"Doe, John\",hello\n";
        let outcome = parse_csv(text, b',');
        assert_eq!(outcome.data.rows, vec![vec!["Doe, John", "hello"]]);
        assert_eq!(
            serialize_csv(&outcome.data.headers, &outcome.data.rows, b','),
            Ok(text.to_string())
        );
    }

    #[test]
    fn test_roundtrip_embedded_newline() {
        let text = "name,note\nAlice,\"line1\nline2\"\n";
        let outcome = parse_csv(text, b',');
        assert_eq!(outcome.data.rows, vec![vec!["Alice", "line1\nline2"]]);
        assert_eq!(
            serialize_csv(&outcome.data.headers, &outcome.data.rows, b','),
            Ok(text.to_string())
        );
    }

    #[test]
    fn test_roundtrip_semicolon() {
        let text = "a;b\n1;2\n";
        let outcome = parse_csv(text, b';');
        assert_eq!(
            serialize_csv(&outcome.data.headers, &outcome.data.rows, b';'),
            Ok(text.to_string())
        );
    }

    #[test]
    fn test_roundtrip_tab() {
        let text = "a\tb\n1\t2\n";
        let outcome = parse_csv(text, b'\t');
        assert_eq!(
            serialize_csv(&outcome.data.headers, &outcome.data.rows, b'\t'),
            Ok(text.to_string())
        );
    }
}
