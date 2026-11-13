use super::{EditorTab, Jump, TabLocation, hex_color_provider};
use crate::fulgur::languages::supported_languages::{
    SupportedLanguage, language_from_content, language_registry_name,
};
use crate::fulgur::settings::EditorSettings;
use crate::fulgur::ui::components_utils::UNTITLED;
use gpui_kit::component::input::{DocumentColorProvider, EditorState, InputEvent, Position};
use gpui_kit::{App, Context, Window};
use std::rc::Rc;
use std::time::SystemTime;

/// Replace the whole text of an editor buffer without recording undo history.
///
/// `set_value` alone emits no `InputEvent::Change`, which would leave content
/// subscribers (modified tracking, Markdown previews) on the old text, so every
/// programmatic full replacement of an editor buffer that must not be undoable
/// (loading or reloading a file) goes through here. Replacements the user should
/// be able to undo go through [`replace_editor_text_undoable`].
///
/// ### Arguments
/// - `state`: The editor buffer to replace
/// - `text`: The new buffer text
/// - `window`: The window context
/// - `cx`: The editor buffer context
pub fn replace_editor_text(
    state: &mut EditorState,
    text: &str,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) {
    state.set_value(text, window, cx);
    cx.emit(InputEvent::Change);
}

/// Bring an editor buffer to `text` as a single undoable edit, keeping the earlier
/// undo history.
///
/// Only the span between the common prefix and the common suffix is replaced, so
/// the undo entry holds the changed span instead of two copies of the whole buffer.
/// The edit emits one `InputEvent::Change`; an unchanged buffer emits none.
///
/// ### Arguments
/// - `state`: The editor buffer to update
/// - `text`: The new buffer text
/// - `window`: The window context
/// - `cx`: The editor buffer context
///
/// ### Returns
/// - `true`: The buffer changed and an `InputEvent::Change` was emitted
/// - `false`: The buffer already held `text`, nothing was recorded or emitted
pub fn replace_editor_text_undoable(
    state: &mut EditorState,
    text: &str,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) -> bool {
    let current = state.text().to_string();
    let Some((range, replacement)) = changed_span(&current, text) else {
        return false;
    };
    state.set_selected_range(range, cx);
    state.replace(replacement.to_string(), window, cx);
    true
}

/// Locate the smallest byte span of `old` that must be replaced to obtain `new`.
///
/// ### Arguments
/// - `old`: The current text
/// - `new`: The target text
///
/// ### Returns
/// - `Some((Range<usize>, &str))`: The span of `old` to replace, on character
///   boundaries, and the slice of `new` to put there
/// - `None`: Both texts are identical
fn changed_span<'a>(old: &str, new: &'a str) -> Option<(std::ops::Range<usize>, &'a str)> {
    if old == new {
        return None;
    }
    let (old_bytes, new_bytes) = (old.as_bytes(), new.as_bytes());
    let mut prefix = old_bytes
        .iter()
        .zip(new_bytes)
        .take_while(|(a, b)| a == b)
        .count();
    // Identical bytes up to `prefix`, so a boundary in `old` is one in `new` too.
    while !old.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let max_suffix = old.len().min(new.len()) - prefix;
    let mut suffix = old_bytes
        .iter()
        .rev()
        .zip(new_bytes.iter().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - suffix) {
        suffix -= 1;
    }
    Some((prefix..old.len() - suffix, &new[prefix..new.len() - suffix]))
}

impl EditorTab {
    /// Record the size and modification time of the file version the buffer now matches.
    ///
    /// ### Arguments
    /// - `content_len`: File size in bytes
    /// - `last_modified`: The file's modification time for that version (see `file_last_modified`)
    pub fn record_file_metadata(&mut self, content_len: usize, last_modified: Option<SystemTime>) {
        self.file_size_bytes = Some(content_len as u64);
        self.file_last_modified = last_modified;
    }

    /// Update the editor's display settings. Tab size cannot be changed after `EditorState` creation.
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    /// - `settings`: The settings for the input state
    pub fn update_settings(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
    ) {
        let large_file = self.large_file;
        self.content.update(cx, |input_state, cx| {
            input_state.set_line_number(settings.show_line_numbers, window, cx);
            input_state.set_indent_guides(settings.show_indent_guides && !large_file, window, cx);
            input_state.set_soft_wrap(settings.soft_wrap && !large_file, window, cx);
            input_state.set_show_whitespaces(settings.show_whitespaces && !large_file, window, cx);
            input_state.set_auto_close(settings.auto_close_pairs, window, cx);
            input_state.set_smart_indent(settings.smart_indent, window, cx);
        });
        self.set_highlight_colors(cx, settings.highlight_colors);
    }

    /// Turn document color swatches on or off on the live `EditorState`.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `enabled`: Whether color swatches should be shown
    ///
    /// [`NoColorProvider`]: super::hex_color_provider::NoColorProvider
    pub(crate) fn set_highlight_colors(&mut self, cx: &mut App, enabled: bool) {
        let enabled = enabled && !self.large_file && !self.log_view;
        if enabled == self.highlight_colors {
            return;
        }
        self.highlight_colors = enabled;
        self.content.update(cx, |state, cx| {
            state.lsp_mut().document_color_provider = Some(if enabled {
                Rc::new(hex_color_provider::ColorHighlightProvider) as Rc<dyn DocumentColorProvider>
            } else {
                Rc::new(hex_color_provider::NoColorProvider)
            });
            state.refresh(cx);
        });
    }

    /// Replace the subscription that keeps `modified` in sync with the content entity.
    ///
    /// ### Arguments
    /// - `subscription`: The new subscription to the content entity
    pub(crate) fn set_content_subscription(&mut self, subscription: gpui_kit::Subscription) {
        self.content_subscription = Some(subscription);
    }

    /// Check if the tab's content has been modified
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `True` if the tab's content has been modified, `False` otherwise
    pub fn check_modified(&mut self, cx: &mut App) -> bool {
        let is_modified = self.content_differs_from_original(cx);
        self.modified = is_modified;
        self.modified
    }

    /// Whether the saved baseline is tracked by byte length and the `modified`
    /// flag alone, without fingerprinting the buffer.
    ///
    /// ### Returns
    /// - `bool`: `true` when the baseline hash is not meaningful
    fn baseline_skips_fingerprint(&self) -> bool {
        self.large_file || self.log_view
    }

    /// Mark the tab as saved
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub fn mark_as_saved(&mut self, cx: &mut App) {
        let current_text = self.content.read(cx).text();
        let (hash, len) = if self.baseline_skips_fingerprint() {
            (0, current_text.len())
        } else {
            super::content_fingerprint_from_rope(current_text)
        };
        self.original_content_hash = hash;
        self.original_content_len = len;
        self.saved_baseline_known = true;
        self.modified = false;
    }

    /// Advance the saved baseline to a snapshot that completed writing.
    ///
    /// ### Arguments
    /// - `saved_hash`: Fingerprint of the content dispatched to the writer
    /// - `saved_len`: Byte length of the content dispatched to the writer
    /// - `cx`: The application context
    pub fn mark_snapshot_as_saved(&mut self, saved_hash: u64, saved_len: usize, cx: &mut App) {
        let current_text = self.content.read(cx).text();
        let (current_hash, current_len) = super::content_fingerprint_from_rope(current_text);
        self.original_content_hash = if self.large_file { 0 } else { saved_hash };
        self.original_content_len = saved_len;
        self.saved_baseline_known = true;
        self.modified = current_len != saved_len || current_hash != saved_hash;
    }

    /// Establish a known original-content fingerprint from a string source.
    ///
    /// ### Arguments
    /// - `content`: Content to set as the new saved baseline
    pub fn set_original_content_from_str(&mut self, content: &str) {
        let (hash, len) = if self.large_file {
            (0, content.len())
        } else {
            super::content_fingerprint_from_str(content)
        };
        self.original_content_hash = hash;
        self.original_content_len = len;
        self.saved_baseline_known = true;
    }

    /// Mark the saved-content baseline as unknown after recovering unsaved text
    /// or losing the file the baseline described.
    pub fn mark_saved_baseline_unknown(&mut self) {
        self.saved_baseline_known = false;
        self.modified = true;
    }

    /// Whether this tab's content is too large to embed in the persisted state file.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `bool`: `true` when the content must not be persisted
    pub fn content_too_large_to_persist(&self, cx: &App) -> bool {
        self.large_file || super::is_large_file(self.content.read(cx).text().len())
    }

    /// Whether a session snapshot would embed this tab's buffer if its size allowed it.
    ///
    /// ### Arguments
    /// - `persist_unsaved`: The value of the `persist_unsaved_buffers` setting
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `bool`: `true` when the buffer is the only copy of content worth restoring
    fn snapshot_wants_buffer(&self, persist_unsaved: bool, cx: &App) -> bool {
        if !persist_unsaved {
            return false;
        }
        match &self.location {
            TabLocation::Local(path) => self.has_unsaved_changes_for_snapshot(cx) || !path.exists(),
            TabLocation::Remote(_) => self.has_unsaved_changes_for_snapshot(cx),
            TabLocation::Untitled | TabLocation::Shared(_) => true,
        }
    }

    /// Whether a session snapshot embeds this tab's buffer.
    ///
    /// ### Arguments
    /// - `persist_unsaved`: The value of the `persist_unsaved_buffers` setting
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `bool`: `true` when the snapshot must carry the buffer text
    pub fn snapshot_keeps_buffer(&self, persist_unsaved: bool, cx: &App) -> bool {
        self.snapshot_wants_buffer(persist_unsaved, cx) && !self.content_too_large_to_persist(cx)
    }

    /// Whether closing would silently drop this tab's buffer because it is too
    /// large to persist, so the close flows must offer to save it first.
    ///
    /// ### Arguments
    /// - `persist_unsaved`: The value of the `persist_unsaved_buffers` setting
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `bool`: `true` when the snapshot would keep the buffer but for its size
    pub fn needs_large_file_close_warning(&self, persist_unsaved: bool, cx: &App) -> bool {
        self.snapshot_wants_buffer(persist_unsaved, cx) && self.content_too_large_to_persist(cx)
    }

    /// Check whether current buffer content differs from the saved baseline.
    /// An unknown baseline always reports the content as different.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `True` when content differs from baseline, `False` otherwise
    pub fn content_differs_from_original(&self, cx: &App) -> bool {
        if !self.saved_baseline_known {
            return true;
        }
        let current_text = self.content.read(cx).text();
        if current_text.len() != self.original_content_len {
            return true;
        }
        if self.baseline_skips_fingerprint() {
            return self.modified;
        }
        let (current_hash, _) = super::content_fingerprint_from_rope(current_text);
        current_hash != self.original_content_hash
    }

    /// Whether a session snapshot must carry this tab's buffer.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `bool`: `true` when the buffer holds changes that are not saved
    pub fn has_unsaved_changes_for_snapshot(&self, cx: &App) -> bool {
        self.modified
            || !self.saved_baseline_known
            || self.content.read(cx).text().len() != self.original_content_len
    }

    /// Get suggested filename for "Save as..." dialog
    ///
    /// ### Returns
    /// - `Some(String)`: The suggested filename
    /// - `None`: If the title is UNTITLED
    pub fn get_suggested_filename(&self) -> Option<String> {
        let title = self.title.trim();
        if title.is_empty() || title.starts_with(UNTITLED) {
            None
        } else {
            Some(crate::fulgur::utils::sanitize::sanitize_filename(title))
        }
    }

    /// Get the file name used when sharing this tab with another device
    ///
    /// ### Returns
    /// - `String`: The on-disk file name, the tab title, or `UNTITLED` when the
    ///   title is still a default one
    pub fn share_file_name(&self) -> String {
        self.file_path()
            .and_then(|path| path.file_name())
            .and_then(std::ffi::OsStr::to_str)
            .map(ToString::to_string)
            .or_else(|| self.get_suggested_filename())
            .unwrap_or_else(|| UNTITLED.to_string())
    }

    /// Whether this tab's title can be renamed by the user.
    ///
    /// ### Returns
    /// - `bool`: `true` when the tab has no associated file
    pub fn is_renameable(&self) -> bool {
        !self.location.has_backing_file()
    }

    /// Update the language/syntax highlighting based on the file extension
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub fn update_language(&mut self, cx: &mut App) {
        if let Some(path) = self.location.local_path() {
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let current_content = self.content.read(cx).text().to_string();
            let language = language_from_content(file_name, &current_content);
            self.force_language(cx, language);
        }
    }

    /// Force the language/syntax highlighting based on the file extension.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `language`: The language to force
    pub fn force_language(&mut self, cx: &mut App, language: SupportedLanguage) {
        if self.language == language {
            return;
        }
        self.language = language;
        if self.large_file {
            return;
        }
        let registry_name = language_registry_name(&language).to_string();
        self.content.update(cx, |state, cx| {
            state.set_highlighter(registry_name, cx);
            state.refresh(cx);
        });
    }

    /// Jump to a specific line
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    /// - `jump`: The jump to perform
    pub fn jump_to_line(&mut self, window: &mut Window, cx: &mut App, jump: Jump) {
        self.content.update(cx, |input_state, cx| {
            input_state.set_cursor_position(
                Position {
                    line: jump.line,
                    character: jump.character.unwrap_or(0),
                },
                window,
                cx,
            );
            input_state.focus(window, cx);
            cx.notify();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::changed_span;

    #[test]
    fn test_changed_span_is_none_for_identical_texts() {
        assert_eq!(changed_span("a,b\n1,2", "a,b\n1,2"), None);
    }

    #[test]
    fn test_changed_span_keeps_only_the_differing_middle() {
        assert_eq!(
            changed_span("a,b\n1,2\n3,4", "a,b\n1,9\n3,4"),
            Some((6..7, "9"))
        );
    }

    #[test]
    fn test_changed_span_handles_pure_insertion_and_deletion() {
        assert_eq!(changed_span("aaa", "aaaa"), Some((3..3, "a")));
        assert_eq!(changed_span("abc", "ac"), Some((1..2, "")));
        assert_eq!(changed_span("", "x"), Some((0..0, "x")));
        assert_eq!(changed_span("x", ""), Some((0..1, "")));
    }

    #[test]
    fn test_changed_span_widens_to_character_boundaries() {
        // `é` and `è` share their lead byte, `é` and `ũ` their continuation byte.
        assert_eq!(changed_span("aé", "aè"), Some((1..3, "è")));
        assert_eq!(changed_span("é", "ũ"), Some((0..2, "ũ")));
    }
}
