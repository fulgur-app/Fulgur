use gpui_kit::component::input::{InputEvent, RopeExt};
use gpui_kit::{App, Context, Focusable, Window};
use std::time::Duration;

use super::matching::{find_matches_with_scratch, first_match_from};
use super::scratch::{free_search_scratch, with_search_scratch};
use super::{SearchBar, SearchBarEvent};

/// Largest buffer, in bytes, rescanned synchronously after each edit or query change
pub(super) const MAX_IMMEDIATE_RESCAN_BYTES: usize = 256 * 1024;

/// Quiet period after the last edit or query change before a larger buffer is rescanned
pub(super) const RESCAN_DEBOUNCE: Duration = Duration::from_millis(150);

/// Direction of one step through the search matches
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MatchStep {
    Next,
    Previous,
}

/// What a debounced rescan does with the editor caret once the matches are known
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CaretAfterRescan {
    Keep,
    MoveToCurrentMatch,
}

impl SearchBar {
    /// Toggle the search bar open or closed
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The search bar context
    pub(super) fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.show_search {
            self.close(cx);
        } else {
            self.show_search = true;
            let search_focus = self.search_input.read(cx).focus_handle(cx);
            window.focus(&search_focus, cx);
            self.watch_editor(cx);
            self.perform_search(cx);
            cx.notify();
        }
    }

    /// Open the search bar with the caret in the replace field
    ///
    /// When the buffer is read-only the replace field is not rendered, so the
    /// caret lands in the search field instead.
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The search bar context
    pub(super) fn open_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.show_search {
            self.show_search = true;
            self.watch_editor(cx);
            self.perform_search(cx);
        }
        let target = if self.editor_is_editable(cx) {
            &self.replace_input
        } else {
            &self.search_input
        };
        let focus = target.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Close the search bar, clear highlighting, free the scan buffers, and notify the owning window
    ///
    /// ### Arguments
    /// - `cx`: The search bar context
    pub(super) fn close(&mut self, cx: &mut Context<Self>) {
        self.show_search = false;
        self.pending_rescan = None;
        self.editor_subscription = None;
        self.clear_match_decorations(cx);
        self.search_matches = Vec::new();
        self.current_match_index = None;
        free_search_scratch();
        cx.emit(SearchBarEvent::Closed);
        cx.notify();
    }

    /// Re-run the search after the query text changed
    ///
    /// A large buffer waits for the typing to settle. Clearing the query never
    /// waits, since an empty query scans nothing.
    ///
    /// ### Arguments
    /// - `cx`: The search bar context
    pub(super) fn on_query_changed(&mut self, cx: &mut Context<Self>) {
        let query_is_empty = self.search_input.read(cx).value().is_empty();
        let large_buffer = self
            .editor()
            .is_some_and(|editor| editor.read(cx).text().len() > MAX_IMMEDIATE_RESCAN_BYTES);
        if !query_is_empty && large_buffer {
            self.schedule_rescan(CaretAfterRescan::MoveToCurrentMatch, cx);
        } else {
            self.perform_search(cx);
        }
    }

    /// Search the editor and move its caret to the current match
    ///
    /// ### Arguments
    /// - `cx`: The search bar context
    pub(super) fn perform_search(&mut self, cx: &mut Context<Self>) {
        self.rescan_matches(cx);
        self.scroll_to_current_match(cx);
    }

    /// Recompute the matches and repaint them without moving the editor caret
    ///
    /// ### Arguments
    /// - `cx`: The search bar context
    pub(super) fn rescan_matches(&mut self, cx: &mut Context<Self>) {
        self.pending_rescan = None;
        self.search_matches.clear();
        self.current_match_index = None;
        if let Some(editor) = self.editor() {
            let query = self.search_input.read(cx).text().to_string();
            if !query.is_empty() {
                let (matches, cursor_pos) = with_search_scratch(|scratch| {
                    let content = editor.read(cx);
                    scratch.text.clear();
                    for chunk in content.text().chunks() {
                        scratch.text.push_str(chunk);
                    }
                    let matches = find_matches_with_scratch(
                        scratch.text.as_str(),
                        &query,
                        self.match_case,
                        self.match_whole_word,
                        &mut scratch.lowercase_text,
                        &mut scratch.length_changes,
                    );
                    (matches, content.cursor())
                });
                self.search_matches = matches;
                self.current_match_index = first_match_from(&self.search_matches, cursor_pos);
            }
            self.apply_match_decorations(&editor, cx);
        }
        cx.notify();
    }

    /// Follow the edits of the bar's editor so its matches never go stale
    ///
    /// ### Description
    /// The bar keeps following its editor while its tab is inactive, so a reload
    /// of a background tab is reflected when the user returns to it.
    ///
    /// ### Arguments
    /// - `cx`: The search bar context
    pub(super) fn watch_editor(&mut self, cx: &mut Context<Self>) {
        self.pending_rescan = None;
        self.editor_subscription = self.editor().map(|editor| {
            cx.subscribe(&editor, |this: &mut Self, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.on_editor_changed(cx);
                }
            })
        });
    }

    /// Rescan the matches after the editor's text changed, keeping its caret
    ///
    /// ### Arguments
    /// - `cx`: The search bar context
    fn on_editor_changed(&mut self, cx: &mut Context<Self>) {
        if !self.show_search {
            return;
        }
        let large_buffer = self
            .editor()
            .is_some_and(|editor| editor.read(cx).text().len() > MAX_IMMEDIATE_RESCAN_BYTES);
        if large_buffer {
            self.schedule_rescan(CaretAfterRescan::Keep, cx);
        } else {
            self.rescan_matches(cx);
        }
    }

    /// Rescan the editor once the debounce period passes without a newer request
    ///
    /// ### Arguments
    /// - `caret`: What to do with the editor caret after the rescan
    /// - `cx`: The search bar context
    fn schedule_rescan(&mut self, caret: CaretAfterRescan, cx: &mut Context<Self>) {
        // Replacing the task drops, and so cancels, a rescan still waiting.
        self.pending_rescan = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(RESCAN_DEBOUNCE).await;
            this.update(cx, |this, cx| match caret {
                CaretAfterRescan::Keep => this.rescan_matches(cx),
                CaretAfterRescan::MoveToCurrentMatch => this.perform_search(cx),
            })
            .ok();
        }));
    }

    /// Navigate to the next search match
    ///
    /// ### Arguments
    /// - `cx`: The search bar context
    pub(super) fn search_next(&mut self, cx: &mut Context<Self>) {
        if self.search_matches.is_empty() {
            return;
        }
        if let Some(current) = self.current_match_index {
            self.current_match_index = Some((current + 1) % self.search_matches.len());
        } else {
            self.current_match_index = Some(0);
        }
        self.show_current_match(cx);
    }

    /// Navigate to the previous search match
    ///
    /// ### Arguments
    /// - `cx`: The search bar context
    pub(super) fn search_previous(&mut self, cx: &mut Context<Self>) {
        if self.search_matches.is_empty() {
            return;
        }
        if let Some(current) = self.current_match_index {
            self.current_match_index = Some(if current == 0 {
                self.search_matches.len() - 1
            } else {
                current - 1
            });
        } else {
            self.current_match_index = Some(0);
        }
        self.show_current_match(cx);
    }

    /// Move the caret and the accent onto the current match
    ///
    /// ### Arguments
    /// - `cx`: The search bar context
    fn show_current_match(&mut self, cx: &mut Context<Self>) {
        self.scroll_to_current_match(cx);
        if let Some(editor) = self.editor() {
            self.apply_current_match_decoration(&editor, cx);
        }
        cx.notify();
    }

    /// Step to the next or previous match
    ///
    /// ### Arguments
    /// - `step`: Which way to move through the matches
    /// - `cx`: The search bar context
    pub(super) fn step_match(&mut self, step: MatchStep, cx: &mut Context<Self>) {
        if self.pending_rescan.is_some() {
            self.rescan_matches(cx);
        }
        match step {
            MatchStep::Next => self.search_next(cx),
            MatchStep::Previous => self.search_previous(cx),
        }
    }

    /// Move the editor cursor to the current search match, scrolling it into view
    ///
    /// A folded region hiding the match is unfolded first, otherwise the caret
    /// would stop at the fold boundary and the match would stay invisible.
    ///
    /// Keyboard focus is left where it is: `EditorState::set_cursor_position`
    /// would focus the editor, so a search run from the bar would send the
    /// user's next keystrokes into the document.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub(super) fn scroll_to_current_match(&self, cx: &mut App) {
        if let Some(match_index) = self.current_match_index
            && let Some(search_match) = self.search_matches.get(match_index)
            && let Some(editor) = self.editor()
        {
            editor.update(cx, |content, cx| {
                let match_position = content.text().offset_to_position(search_match.start);
                content.unfold_at(match_position, cx);
                content.set_selected_range(search_match.start..search_match.start, cx);
            });
        }
    }
}
