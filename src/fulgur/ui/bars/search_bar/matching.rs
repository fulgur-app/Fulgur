use std::borrow::Cow;

use super::SearchMatch;

/// A character whose lowercase form has a different byte length than the original
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LengthChange {
    lowered_start: usize,
    lowered_end: usize,
    original_start: usize,
    original_end: usize,
}

/// Rebuild the lowercased search text together with the characters that changed byte length.
///
/// ### Arguments
/// - `text`: The original source text
/// - `lowercase_text_scratch`: Reusable buffer filled with the lowercased text
/// - `length_changes_scratch`: Reusable buffer filled with the length-changing characters,
///   sorted by offset and left empty when no offset moves
pub(super) fn rebuild_lowercase_text(
    text: &str,
    lowercase_text_scratch: &mut String,
    length_changes_scratch: &mut Vec<LengthChange>,
) {
    lowercase_text_scratch.clear();
    length_changes_scratch.clear();
    if text.is_ascii() {
        lowercase_text_scratch.push_str(text);
        lowercase_text_scratch.make_ascii_lowercase();
        return;
    }
    for (original_start, ch) in text.char_indices() {
        let lowered_start = lowercase_text_scratch.len();
        lowercase_text_scratch.extend(ch.to_lowercase());
        let lowered_end = lowercase_text_scratch.len();
        if lowered_end - lowered_start != ch.len_utf8() {
            length_changes_scratch.push(LengthChange {
                lowered_start,
                lowered_end,
                original_start,
                original_end: original_start + ch.len_utf8(),
            });
        }
    }
}

/// Map a char-boundary offset in the lowercased text back to the original text.
///
/// ### Arguments
/// - `lowered_offset`: A char-boundary byte offset in the lowercased text
/// - `length_changes`: The length-changing characters, sorted by offset
///
/// ### Returns
/// - `usize`: The matching byte offset in the original text
pub(super) fn lowered_to_original(lowered_offset: usize, length_changes: &[LengthChange]) -> usize {
    let changes_started =
        length_changes.partition_point(|change| change.lowered_start <= lowered_offset);
    let Some(change) = changes_started
        .checked_sub(1)
        .map(|index| &length_changes[index])
    else {
        return lowered_offset;
    };
    if lowered_offset < change.lowered_end {
        change.original_start
    } else {
        change.original_end + (lowered_offset - change.lowered_end)
    }
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
#[cfg(all(test, feature = "gpui-test-support"))]
pub(super) fn find_matches(
    text: &str,
    query: &str,
    match_case: bool,
    match_whole_word: bool,
) -> Vec<SearchMatch> {
    let mut lowercase_text_scratch = String::new();
    let mut length_changes_scratch = Vec::new();
    find_matches_with_scratch(
        text,
        query,
        match_case,
        match_whole_word,
        &mut lowercase_text_scratch,
        &mut length_changes_scratch,
    )
}

/// Find all matches in the text while reusing caller-provided scratch buffers.
///
/// ### Arguments
/// - `text`: The text to search in
/// - `query`: The search query
/// - `match_case`: Whether to match case
/// - `match_whole_word`: Whether to match whole words only
/// - `lowercase_text_scratch`: Reusable lowercase-text buffer
/// - `length_changes_scratch`: Reusable list of the characters whose lowercase byte length differs
///
/// ### Returns
/// - `Vec<SearchMatch>`: A vector of search matches with offsets into the original `text`
pub(super) fn find_matches_with_scratch(
    text: &str,
    query: &str,
    match_case: bool,
    match_whole_word: bool,
    lowercase_text_scratch: &mut String,
    length_changes_scratch: &mut Vec<LengthChange>,
) -> Vec<SearchMatch> {
    let mut matches = Vec::new();
    if query.is_empty() {
        return matches;
    }

    let search_text = if match_case {
        text
    } else {
        rebuild_lowercase_text(text, lowercase_text_scratch, length_changes_scratch);
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
                lowered_to_original(search_start, length_changes_scratch),
                lowered_to_original(search_end, length_changes_scratch),
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
        matches.push(SearchMatch {
            start: match_start,
            end: match_end,
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

/// Find the first match starting at or after an offset, wrapping to the first match
///
/// ### Arguments
/// - `search_matches`: The match positions, sorted by offset
/// - `offset`: The byte offset to search from
///
/// ### Returns
/// - `Some(usize)`: The index of the first match starting at or after `offset`, or
///   `0` when every match starts before it
/// - `None`: There are no matches
pub(super) fn first_match_from(search_matches: &[SearchMatch], offset: usize) -> Option<usize> {
    if search_matches.is_empty() {
        return None;
    }
    let first_after_offset = search_matches
        .iter()
        .position(|search_match| search_match.start >= offset);
    Some(first_after_offset.unwrap_or(0))
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

/// Map a byte offset through the edits `apply_replacements` makes, so it stays on the same text
///
/// Overlapping matches are skipped the way `apply_replacements` skips them. An offset
/// strictly inside a replaced match moves to the start of its replacement.
///
/// ### Arguments
/// - `search_matches`: The match positions being replaced, sorted by offset
/// - `offset`: The byte offset in the original text
/// - `replace_len`: The byte length of the replacement text
///
/// ### Returns
/// - `usize`: The matching byte offset in the replaced text
pub(super) fn offset_after_replacements(
    search_matches: &[SearchMatch],
    offset: usize,
    replace_len: usize,
) -> usize {
    let mut original_pos = 0;
    let mut replaced_pos = 0;
    for search_match in search_matches {
        if search_match.start < original_pos {
            continue;
        }
        if search_match.end > offset {
            return replaced_pos + offset.min(search_match.start) - original_pos;
        }
        replaced_pos += search_match.start - original_pos + replace_len;
        original_pos = search_match.end;
    }
    replaced_pos + offset - original_pos
}
