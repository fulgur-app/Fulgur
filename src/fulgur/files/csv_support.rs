//! CSV parsing and serialization helpers backing the table view.
//!
/// The delimiter used when none can be detected.
pub const DEFAULT_DELIMITER: u8 = b',';

/// Candidate delimiters considered by [`detect_delimiter`], in priority order.
const CANDIDATE_DELIMITERS: [u8; 3] = *b",;\t";

/// The UTF-8 byte order mark, as written by Excel's "CSV UTF-8" export.
const UTF8_BOM: &str = "\u{feff}";

/// A rectangular view of a CSV file: a header row plus data rows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CsvData {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// The byte-level conventions of a CSV document, recorded at parse time so a
/// table edit writes the file back the way it was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsvDialect {
    pub delimiter: u8,
    pub has_bom: bool,
    pub crlf: bool,
    pub trailing_newline: bool,
}

impl Default for CsvDialect {
    fn default() -> Self {
        Self::with_delimiter(DEFAULT_DELIMITER)
    }
}

impl CsvDialect {
    /// Build the dialect of a freshly written CSV document: no BOM, `\n` line
    /// endings, and a final newline.
    ///
    /// ### Arguments
    /// - `delimiter`: The field delimiter byte
    ///
    /// ### Returns
    /// - `CsvDialect`: The default dialect using `delimiter`
    #[must_use]
    pub const fn with_delimiter(delimiter: u8) -> Self {
        Self {
            delimiter,
            has_bom: false,
            crlf: false,
            trailing_newline: true,
        }
    }

    /// Detect the BOM, dominant line terminator, and final newline of a document.
    ///
    /// ### Arguments
    /// - `text`: The full CSV document text
    /// - `delimiter`: The field delimiter byte the document is parsed with
    ///
    /// ### Returns
    /// - `CsvDialect`: The dialect observed in `text`; CRLF wins when at least
    ///   half of the line feeds are preceded by a carriage return
    #[must_use]
    pub fn detect(text: &str, delimiter: u8) -> Self {
        let line_feeds = text.matches('\n').count();
        let crlf_line_feeds = text.matches("\r\n").count();
        Self {
            delimiter,
            has_bom: text.starts_with(UTF8_BOM),
            crlf: line_feeds > 0 && crlf_line_feeds * 2 >= line_feeds,
            trailing_newline: text.is_empty() || text.ends_with('\n'),
        }
    }

    /// The record terminator written between records.
    ///
    /// ### Returns
    /// - `&'static str`: `"\r\n"` for CRLF documents, `"\n"` otherwise
    #[must_use]
    pub const fn line_terminator(&self) -> &'static str {
        if self.crlf { "\r\n" } else { "\n" }
    }
}

/// The outcome of parsing CSV text: the table model, the dialect needed to
/// write it back unchanged, and the number of records that were dropped
/// because they could not be parsed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CsvParseOutcome {
    pub data: CsvData,
    pub dialect: CsvDialect,
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

/// Parse CSV text into a [`CsvData`] model.
///
/// ### Arguments
/// - `text`: The CSV document text
/// - `delimiter`: The field delimiter byte
///
/// ### Returns
/// - `CsvParseOutcome`: The parsed headers and rows, the document's dialect,
///   and the count of records that failed to parse. Empty input yields empty
///   headers and no rows.
pub fn parse_csv(text: &str, delimiter: u8) -> CsvParseOutcome {
    let dialect = CsvDialect::detect(text, delimiter);
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(text.strip_prefix(UTF8_BOM).unwrap_or(text).as_bytes());

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
            dialect,
            dropped_records,
        };
    }

    let width = records.iter().map(Vec::len).max().unwrap_or(0);
    let mut headers = records.remove(0);
    headers.resize(width, String::new());
    CsvParseOutcome {
        data: CsvData {
            headers,
            rows: records,
        },
        dialect,
        dropped_records,
    }
}

/// Serialize a [`CsvData`] model back to CSV text.
///
/// ### Arguments
/// - `headers`: The column headers, written as the first record
/// - `rows`: The data rows
/// - `dialect`: The delimiter, BOM, line terminator, and final newline to write
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
    dialect: &CsvDialect,
) -> Result<String, String> {
    let terminator = if dialect.crlf {
        csv::Terminator::CRLF
    } else {
        csv::Terminator::Any(b'\n')
    };
    let mut output = Vec::new();
    if dialect.has_bom {
        output.extend_from_slice(UTF8_BOM.as_bytes());
    }
    let mut writer = csv::WriterBuilder::new()
        .delimiter(dialect.delimiter)
        .terminator(terminator)
        .flexible(true)
        .from_writer(output);

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
    let mut text = String::from_utf8(bytes)
        .map_err(|error| format!("CSV output is not valid UTF-8: {error}"))?;
    if !dialect.trailing_newline && text.ends_with(dialect.line_terminator()) {
        text.truncate(text.len() - dialect.line_terminator().len());
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::{CsvDialect, CsvParseOutcome, detect_delimiter, parse_csv, serialize_csv};

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
    fn test_parse_keeps_short_rows_unpadded() {
        let outcome = parse_csv("a,b,c\n1,2\n3\n", b',');
        assert_eq!(outcome.data.headers, vec!["a", "b", "c"]);
        assert_eq!(outcome.data.rows, vec![vec!["1", "2"], vec!["3"]]);
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
            serialize_csv(&outcome.data.headers, &outcome.data.rows, &outcome.dialect),
            Ok(text.to_string())
        );
    }

    #[test]
    fn test_roundtrip_quoted_embedded_comma() {
        let text = "name,note\n\"Doe, John\",hello\n";
        let outcome = parse_csv(text, b',');
        assert_eq!(outcome.data.rows, vec![vec!["Doe, John", "hello"]]);
        assert_eq!(
            serialize_csv(&outcome.data.headers, &outcome.data.rows, &outcome.dialect),
            Ok(text.to_string())
        );
    }

    #[test]
    fn test_roundtrip_embedded_newline() {
        let text = "name,note\nAlice,\"line1\nline2\"\n";
        let outcome = parse_csv(text, b',');
        assert_eq!(outcome.data.rows, vec![vec!["Alice", "line1\nline2"]]);
        assert_eq!(
            serialize_csv(&outcome.data.headers, &outcome.data.rows, &outcome.dialect),
            Ok(text.to_string())
        );
    }

    #[test]
    fn test_roundtrip_semicolon() {
        let text = "a;b\n1;2\n";
        let outcome = parse_csv(text, b';');
        assert_eq!(
            serialize_csv(&outcome.data.headers, &outcome.data.rows, &outcome.dialect),
            Ok(text.to_string())
        );
    }

    #[test]
    fn test_roundtrip_tab() {
        let text = "a\tb\n1\t2\n";
        let outcome = parse_csv(text, b'\t');
        assert_eq!(
            serialize_csv(&outcome.data.headers, &outcome.data.rows, &outcome.dialect),
            Ok(text.to_string())
        );
    }

    /// Parse `text`, then serialize the unchanged model with the detected dialect.
    fn roundtrip(text: &str) -> Result<String, String> {
        let outcome = parse_csv(text, detect_delimiter(text));
        serialize_csv(&outcome.data.headers, &outcome.data.rows, &outcome.dialect)
    }

    #[test]
    fn test_detect_dialect_of_excel_utf8_export() {
        assert_eq!(
            CsvDialect::detect("\u{feff}a,b\r\n1,2\r\n", b','),
            CsvDialect {
                delimiter: b',',
                has_bom: true,
                crlf: true,
                trailing_newline: true,
            }
        );
    }

    #[test]
    fn test_parse_strips_bom_from_first_header() {
        let outcome = parse_csv("\u{feff}name,age\nAlice,30\n", b',');
        assert_eq!(outcome.data.headers, vec!["name", "age"]);
    }

    #[test]
    fn test_roundtrip_bom_and_crlf() {
        let text = "\u{feff}name,age\r\nAlice,30\r\nBob,25\r\n";
        assert_eq!(roundtrip(text), Ok(text.to_string()));
    }

    #[test]
    fn test_roundtrip_crlf_keeps_embedded_newline() {
        let text = "name,note\r\nAlice,\"line1\nline2\"\r\n";
        assert_eq!(roundtrip(text), Ok(text.to_string()));
    }

    #[test]
    fn test_roundtrip_short_rows_are_not_padded() {
        let text = "a,b\n1\n2,\n";
        assert_eq!(roundtrip(text), Ok(text.to_string()));
    }

    #[test]
    fn test_roundtrip_without_final_newline() {
        let text = "a,b\n1,2";
        assert_eq!(roundtrip(text), Ok(text.to_string()));
    }

    #[test]
    fn test_roundtrip_crlf_without_final_newline() {
        let text = "a;b\r\n1;2";
        assert_eq!(roundtrip(text), Ok(text.to_string()));
    }

    #[test]
    fn test_serialize_default_dialect_ends_with_line_feed() {
        let headers = vec!["a".to_string()];
        let rows = vec![vec!["1".to_string()]];
        assert_eq!(
            serialize_csv(&headers, &rows, &CsvDialect::default()),
            Ok("a\n1\n".to_string())
        );
    }
}
