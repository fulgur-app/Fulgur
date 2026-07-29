use std::borrow::Cow;

use super::SearchMatch;

/// Refresh the newline-offset scratch buffer for fast line/column lookup.
///
/// ### Arguments
/// - `text`: Source text being searched
/// - `newline_offsets_scratch`: Reusable scratch vector populated with `\n` byte offsets
fn refresh_newline_offsets(text: &str, newline_offsets_scratch: &mut Vec<usize>) {
    newline_offsets_scratch.clear();
    newline_offsets_scratch.extend(
        text.bytes()
            .enumerate()
            .filter_map(|(i, b)| if b == b'\n' { Some(i) } else { None }),
    );
}

/// Rebuild the lowercased search text together with a byte-offset map back to the original text.
///
/// ### Arguments
/// - `text`: The original source text
/// - `lowercase_text_scratch`: Reusable buffer filled with the lowercased text
/// - `lowercase_offsets_scratch`: Reusable buffer filled with the lowercased-to-original offset map
fn rebuild_lowercase_text(
    text: &str,
    lowercase_text_scratch: &mut String,
    lowercase_offsets_scratch: &mut Vec<usize>,
) {
    lowercase_text_scratch.clear();
    lowercase_offsets_scratch.clear();
    for (orig_offset, ch) in text.char_indices() {
        let before = lowercase_text_scratch.len();
        for lowered in ch.to_lowercase() {
            lowercase_text_scratch.push(lowered);
        }
        let added = lowercase_text_scratch.len() - before;
        for _ in 0..added {
            lowercase_offsets_scratch.push(orig_offset);
        }
    }
    lowercase_offsets_scratch.push(text.len());
}

/// Find all matches in the text
///
/// ### Arguments
/// - `text`: The text to search in
/// - `query`: The search query
/// - `match_case`: Whether to match case
/// - `match_whole_word`: Whether to match whole words only
///
/// ### Returns
/// - `Vec<SearchMatch>`: A vector of search matches
#[cfg(test)]
pub(super) fn find_matches(
    text: &str,
    query: &str,
    match_case: bool,
    match_whole_word: bool,
) -> Vec<SearchMatch> {
    let mut newline_offsets_scratch = Vec::new();
    let mut lowercase_text_scratch = String::new();
    let mut lowercase_offsets_scratch = Vec::new();
    find_matches_with_scratch(
        text,
        query,
        match_case,
        match_whole_word,
        &mut newline_offsets_scratch,
        &mut lowercase_text_scratch,
        &mut lowercase_offsets_scratch,
    )
}

/// Find all matches in the text while reusing caller-provided scratch buffers.
///
/// ### Arguments
/// - `text`: The text to search in
/// - `query`: The search query
/// - `match_case`: Whether to match case
/// - `match_whole_word`: Whether to match whole words only
/// - `newline_offsets_scratch`: Reusable newline-offset buffer
/// - `lowercase_text_scratch`: Reusable lowercase-text buffer
/// - `lowercase_offsets_scratch`: Reusable lowercased-to-original byte-offset map
///
/// ### Returns
/// - `Vec<SearchMatch>`: A vector of search matches with offsets into the original `text`
pub(super) fn find_matches_with_scratch(
    text: &str,
    query: &str,
    match_case: bool,
    match_whole_word: bool,
    newline_offsets_scratch: &mut Vec<usize>,
    lowercase_text_scratch: &mut String,
    lowercase_offsets_scratch: &mut Vec<usize>,
) -> Vec<SearchMatch> {
    let mut matches = Vec::new();
    if query.is_empty() {
        return matches;
    }

    refresh_newline_offsets(text, newline_offsets_scratch);

    let search_text = if match_case {
        text
    } else {
        rebuild_lowercase_text(text, lowercase_text_scratch, lowercase_offsets_scratch);
        lowercase_text_scratch.as_str()
    };
    let search_query: Cow<str> = if match_case {
        Cow::Borrowed(query)
    } else {
        Cow::Owned(query.to_lowercase())
    };

    let mut start_pos = 0;
    while let Some(pos) = search_text[start_pos..].find(search_query.as_ref()) {
        let search_start = start_pos + pos;
        let search_end = search_start + search_query.len();
        // Map both endpoints from search-text space back to original-text space.
        // With case-sensitive search the two spaces are identical.
        let (match_start, match_end) = if match_case {
            (search_start, search_end)
        } else {
            (
                lowercase_offsets_scratch[search_start],
                lowercase_offsets_scratch[search_end],
            )
        };
        if match_whole_word {
            let is_word_start = match_start == 0
                || !text[..match_start]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_');
            let is_word_end = match_end >= text.len()
                || !text[match_end..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_');

            if !is_word_start || !is_word_end {
                start_pos = advance_past_char(search_text, search_start);
                continue;
            }
        }
        let (line, col) = get_line_col_fast(text, match_start, newline_offsets_scratch);
        matches.push(SearchMatch {
            start: match_start,
            end: match_end,
            line,
            col,
        });
        start_pos = advance_past_char(search_text, search_start);
    }
    matches
}

/// Advance a scan cursor past the character starting at `pos`, staying on a char boundary.
///
/// ### Arguments
/// - `text`: The text being scanned
/// - `pos`: A char-boundary byte offset within `text`
///
/// ### Returns
/// - `usize`: The byte offset of the next character boundary after `pos`
fn advance_past_char(text: &str, pos: usize) -> usize {
    let char_len = text[pos..].chars().next().map_or(1, char::len_utf8);
    pos + char_len
}

/// Get line and column from byte position using precomputed newline offsets
///
/// ### Arguments
/// - `text`: The text
/// - `byte_pos`: The byte position
/// - `newline_offsets`: Precomputed byte offsets of all newline characters
///
/// ### Returns
/// - `(usize, usize)`: A tuple of (line, column)
pub(super) fn get_line_col_fast(
    text: &str,
    byte_pos: usize,
    newline_offsets: &[usize],
) -> (usize, usize) {
    let pos = byte_pos.min(text.len());
    let line = newline_offsets.partition_point(|&nl| nl < pos);
    let line_start = if line == 0 {
        0
    } else {
        newline_offsets[line - 1] + 1
    };
    let col = text[line_start..pos].chars().count();
    (line, col)
}

/// Replace text at all match positions with the replacement string
///
/// ### Arguments
/// - `search_matches`: The precomputed search match positions
/// - `text`: The original text
/// - `replace`: The replacement text
///
/// ### Returns
/// - `String`: The text with all matches replaced
pub(super) fn apply_replacements(
    search_matches: &[SearchMatch],
    text: &str,
    replace: &str,
) -> String {
    let mut result = String::new();
    let mut last_pos = 0;
    for m in search_matches {
        if m.start < last_pos
            || m.end > text.len()
            || m.start > m.end
            || !text.is_char_boundary(m.start)
            || !text.is_char_boundary(m.end)
        {
            continue;
        }
        result.push_str(&text[last_pos..m.start]);
        result.push_str(replace);
        last_pos = m.end;
    }
    result.push_str(&text[last_pos..]);
    result
}
