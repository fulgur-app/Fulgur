//! Keeps a Markdown preview's rendered text in step with its source editor tab.

use crate::fulgur::editor_tab::replace_editor_range;
use crate::fulgur::tab::Tab;
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::utils::markdown_images::{
    RewriteOffsets, RewrittenMarkdown, rewrite_markdown_image_paths,
};
use gpui_kit::base::text::TaskToggleEvent;
use gpui_kit::component::input::{EditorState, InputEvent, Rope};
use gpui_kit::component::text::TextViewState;
use gpui_kit::{App, AppContext, Context, Entity, Subscription, Task, WeakEntity, Window};
use std::borrow::Cow;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// Largest source, in bytes, rewritten synchronously on the UI thread. Larger
/// sources are rewritten on the background executor so typing stays fluid.
const MAX_SYNC_REWRITE_BYTES: usize = 64 * 1024;

/// The preview text of a Markdown editor tab, pushed on every content change.
///
/// Rendering only reads `view_state`: the image-path rewrite runs when the
/// source buffer emits `InputEvent::Change` and when the source tab moves to
/// another directory, never per frame.
pub struct MarkdownPreviewSource {
    source_tab_id: TabId,
    source_tab: WeakEntity<Tab>,
    base_dir: Option<PathBuf>,
    view_state: Entity<TextViewState>,
    /// Maps offsets of the text last pushed to `view_state` back to the buffer.
    rewrite_offsets: RewriteOffsets,
    refresh_task: Option<Task<()>>,
    _subscriptions: [Subscription; 2],
}

impl MarkdownPreviewSource {
    /// Create a preview source bound to an editor tab.
    ///
    /// ### Arguments
    /// - `source_tab`: The editor tab whose content is previewed
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(Entity<MarkdownPreviewSource>)`: The preview source, already
    ///   holding the current preview text
    /// - `None`: If `source_tab` is not an editor tab
    pub fn for_editor_tab(source_tab: &Entity<Tab>, cx: &mut App) -> Option<Entity<Self>> {
        let editor_tab = source_tab.read(cx).as_editor()?;
        let source_tab_id = editor_tab.id;
        let content = editor_tab.content.clone();
        let base_dir = preview_base_dir(source_tab, cx);
        Some(cx.new(|cx| {
            let subscriptions = [
                cx.subscribe(&content, |this: &mut Self, content, event, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.refresh(&content, cx);
                    }
                }),
                cx.observe(source_tab, |this: &mut Self, source_tab, cx| {
                    this.follow_source_location(&source_tab, cx);
                }),
            ];
            let mut this = Self {
                source_tab_id,
                source_tab: source_tab.downgrade(),
                base_dir,
                view_state: cx.new(|cx| TextViewState::markdown("", cx)),
                rewrite_offsets: RewriteOffsets::default(),
                refresh_task: None,
                _subscriptions: subscriptions,
            };
            this.refresh(&content, cx);
            this
        }))
    }

    /// Return the identifier of the editor tab this preview mirrors.
    ///
    /// ### Returns
    /// - `TabId`: The source editor tab identifier
    pub fn source_tab_id(&self) -> TabId {
        self.source_tab_id
    }

    /// Return the directory used to resolve relative images and links.
    ///
    /// ### Returns
    /// - `Some(&Path)`: The directory of the source file
    /// - `None`: If the source buffer has no local file
    pub fn base_dir(&self) -> Option<&Path> {
        self.base_dir.as_deref()
    }

    /// Return the text view state rendered by the preview.
    ///
    /// ### Returns
    /// - `Entity<TextViewState>`: The persistent preview state, which also
    ///   keeps the scroll position across renders
    pub fn view_state(&self) -> Entity<TextViewState> {
        self.view_state.clone()
    }

    /// Tell whether the preview's task checkboxes may edit the source buffer.
    ///
    /// ### Returns
    /// - `true`: The source tab is open and its buffer accepts user edits
    /// - `false`: The source tab is gone, read-only, a log view, or still
    ///   waiting for its remote content
    pub fn accepts_task_toggles(&self, cx: &App) -> bool {
        self.source_tab.upgrade().is_some_and(|source_tab| {
            source_tab
                .read(cx)
                .as_editor()
                .is_some_and(|editor_tab| !editor_tab.blocks_user_edits())
        })
    }

    /// Check or uncheck the task whose preview checkbox was clicked.
    ///
    /// ### Arguments
    /// - `event`: The toggle reported by the preview's `TextView`
    /// - `window`: The window context
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `true`: The task marker was rewritten in the source buffer
    /// - `false`: Nothing was edited (see [`Self::set_task_checked`])
    pub fn toggle_task(&self, event: &TaskToggleEvent, window: &mut Window, cx: &mut App) -> bool {
        self.set_task_checked(event.marker_range(), event.checked(), window, cx)
    }

    /// Rewrite a task's `[ ]` / `[x]` marker in the source buffer.
    ///
    /// ### Arguments
    /// - `preview_marker`: The marker's byte range in the preview text
    /// - `checked`: The checked state the task should now have
    /// - `window`: The window context
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `true`: The marker was rewritten
    /// - `false`: Nothing was edited: the source is locked or gone, or the
    ///   buffer does not hold the opposite marker at that position
    fn set_task_checked(
        &self,
        preview_marker: Range<usize>,
        checked: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if !self.accepts_task_toggles(cx) {
            log::debug!("Ignoring a preview task toggle: the source buffer is locked");
            return false;
        }
        let Some(content) = self
            .source_tab
            .upgrade()
            .and_then(|source_tab| Some(source_tab.read(cx).as_editor()?.content.clone()))
        else {
            return false;
        };
        let Some(marker_range) = self.rewrite_offsets.to_source(preview_marker) else {
            log::warn!("Ignoring a preview task toggle that does not map back to the source");
            return false;
        };
        content.update(cx, |state, cx| {
            if !state.is_editable() {
                return false;
            }
            let text = state.text();
            // ropey's `try_slice` panics instead of failing on an end past the
            // buffer, so a preview that lags a shortened buffer is caught here.
            if marker_range.end > text.len() {
                return false;
            }
            let Some(current) = text
                .try_slice(marker_range.clone())
                .ok()
                .map(|marker| marker.to_string())
            else {
                return false;
            };
            let Some(replacement) = toggled_task_marker(&current, checked) else {
                log::warn!(
                    "Ignoring a preview task toggle: the buffer holds {current:?} at its marker"
                );
                return false;
            };
            let selection = state.selected_range();
            let reversed = state.is_selection_reversed();
            replace_editor_range(state, marker_range, replacement, window, cx);
            // Same-length replacement, so the old selection offsets still hold.
            state.set_selected_range_with_direction(selection, reversed, cx);
            true
        })
    }

    /// Re-resolve relative image paths when the source tab changes directory,
    /// for instance after Save As.
    ///
    /// ### Arguments
    /// - `source_tab`: The editor tab this preview mirrors
    /// - `cx`: The preview source context
    fn follow_source_location(&mut self, source_tab: &Entity<Tab>, cx: &mut Context<Self>) {
        let base_dir = preview_base_dir(source_tab, cx);
        if base_dir == self.base_dir {
            return;
        }
        self.base_dir = base_dir;
        if let Some(editor_tab) = source_tab.read(cx).as_editor() {
            let content = editor_tab.content.clone();
            self.refresh(&content, cx);
        }
    }

    /// Push the current source text, with local images rewritten, to the view.
    ///
    /// ### Arguments
    /// - `content`: The source editor buffer
    /// - `cx`: The preview source context
    fn refresh(&mut self, content: &Entity<EditorState>, cx: &mut Context<Self>) {
        let rope = content.read(cx).text().clone();
        let base_dir = self.base_dir.clone();
        let view_state = self.view_state.clone();
        if rope.len() <= MAX_SYNC_REWRITE_BYTES {
            self.refresh_task = None;
            let (text, offsets) = preview_text(&rope, base_dir.as_deref());
            self.rewrite_offsets = offsets;
            view_state.update(cx, |state, cx| state.set_text(&text, cx));
            return;
        }
        // Replacing the task drops, and so cancels, a rewrite still in flight.
        self.refresh_task = Some(cx.spawn(async move |this, cx| {
            let (text, offsets) = cx
                .background_executor()
                .spawn(async move { preview_text(&rope, base_dir.as_deref()) })
                .await;
            this.update(cx, |this, cx| {
                this.rewrite_offsets = offsets;
                view_state.update(cx, |state, cx| state.set_text(&text, cx));
            })
            .ok();
        }));
    }
}

/// Resolve the directory a preview resolves relative paths against.
///
/// ### Arguments
/// - `source_tab`: The editor tab being previewed
/// - `cx`: The application context
///
/// ### Returns
/// - `Some(PathBuf)`: The parent directory of the tab's local file
/// - `None`: If the tab is not a local file
fn preview_base_dir(source_tab: &Entity<Tab>, cx: &App) -> Option<PathBuf> {
    source_tab
        .read(cx)
        .as_editor()?
        .file_path()?
        .parent()
        .map(Path::to_path_buf)
}

/// Build the preview text for a Markdown buffer.
///
/// ### Arguments
/// - `rope`: The source buffer snapshot
/// - `base_dir`: The directory used to resolve relative image paths
///
/// ### Returns
/// - `(String, RewriteOffsets)`: The Markdown with local image references
///   rewritten, and the offsets mapping it back to the buffer
fn preview_text(rope: &Rope, base_dir: Option<&Path>) -> (String, RewriteOffsets) {
    let source = rope.to_string();
    let RewrittenMarkdown { text, offsets } = rewrite_markdown_image_paths(&source, base_dir);
    let rewritten = match text {
        Cow::Owned(rewritten) => Some(rewritten),
        Cow::Borrowed(_) => None,
    };
    (rewritten.unwrap_or(source), offsets)
}

/// Pick the marker that gives a task item the requested checked state.
///
/// ### Arguments
/// - `current`: The bytes the buffer holds where the item's marker should be
/// - `checked`: The checked state the item should now have
///
/// ### Returns
/// - `Some(&str)`: `"[x]"` or `"[ ]"`, to replace `current` with
/// - `None`: `current` is not a task marker, or already has that state, so
///   the preview the click came from no longer matches the buffer
fn toggled_task_marker(current: &str, checked: bool) -> Option<&'static str> {
    let was_checked = match current {
        "[ ]" => false,
        "[x]" | "[X]" => true,
        _ => return None,
    };
    (was_checked != checked).then_some(if checked { "[x]" } else { "[ ]" })
}

#[cfg(test)]
mod marker_tests {
    use super::toggled_task_marker;

    #[test]
    fn toggled_task_marker_flips_only_the_opposite_state() {
        assert_eq!(toggled_task_marker("[ ]", true), Some("[x]"));
        assert_eq!(toggled_task_marker("[x]", false), Some("[ ]"));
        assert_eq!(toggled_task_marker("[X]", false), Some("[ ]"));
        assert_eq!(toggled_task_marker("[x]", true), None);
        assert_eq!(toggled_task_marker("[ ]", false), None);
        assert_eq!(toggled_task_marker("- [", true), None);
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::{MAX_SYNC_REWRITE_BYTES, MarkdownPreviewSource};
    use crate::fulgur::Fulgur;
    use crate::fulgur::editor_tab::{TabLocation, replace_editor_text};
    use crate::fulgur::tab::Tab;
    use crate::test_support::setup_fulgur;
    use gpui_kit::component::text::SelectionFormat;
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};
    use std::ops::Range;
    use std::path::PathBuf;

    /// Bind a preview source to the active editor tab of a fresh window.
    fn preview_of_active_tab(
        cx: &mut TestAppContext,
    ) -> (
        Entity<Tab>,
        Entity<MarkdownPreviewSource>,
        Entity<Fulgur>,
        VisualTestContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let (tab, preview) = visual_cx.update(|_window, cx| {
            let tab = fulgur.read(cx).tabs.last().expect("expected a tab").clone();
            let preview =
                MarkdownPreviewSource::for_editor_tab(&tab, cx).expect("expected an editor tab");
            (tab, preview)
        });
        (tab, preview, fulgur, visual_cx)
    }

    /// Replace the whole text of the tab's buffer the way reloads do.
    fn replace_buffer(tab: &Entity<Tab>, text: &str, visual_cx: &mut VisualTestContext) {
        visual_cx.update(|window, cx| {
            let content = tab
                .read(cx)
                .as_editor()
                .expect("editor tab")
                .content
                .clone();
            content.update(cx, |state, cx| replace_editor_text(state, text, window, cx));
        });
        visual_cx.run_until_parked();
    }

    /// Read back the Markdown source currently held by the preview.
    fn preview_source(
        preview: &Entity<MarkdownPreviewSource>,
        visual_cx: &mut VisualTestContext,
    ) -> String {
        visual_cx.run_until_parked();
        visual_cx.update(|_window, cx| {
            let view_state = preview.read(cx).view_state();
            view_state.update(cx, |state, cx| {
                state.set_selection_format(SelectionFormat::Source, cx);
                state.select_all(cx);
                state.selected_text()
            })
        })
    }

    /// Read back the whole text of the tab's buffer.
    fn buffer_text(tab: &Entity<Tab>, visual_cx: &mut VisualTestContext) -> String {
        visual_cx.update(|_window, cx| {
            tab.read(cx)
                .as_editor()
                .expect("editor tab")
                .content
                .read(cx)
                .text()
                .to_string()
        })
    }

    /// Ask the preview to set a task's checked state, as a checkbox click does.
    fn set_task_checked(
        preview: &Entity<MarkdownPreviewSource>,
        preview_marker: Range<usize>,
        checked: bool,
        visual_cx: &mut VisualTestContext,
    ) -> bool {
        let toggled = visual_cx.update(|window, cx| {
            preview.update(cx, |preview, cx| {
                preview.set_task_checked(preview_marker, checked, window, cx)
            })
        });
        visual_cx.run_until_parked();
        toggled
    }

    #[gpui_kit::test]
    fn task_toggles_rewrite_the_marker_in_the_source(cx: &mut TestAppContext) {
        let (tab, preview, _fulgur, mut visual_cx) = preview_of_active_tab(cx);
        replace_buffer(&tab, "- [ ] Open\n- [x] Done\n", &mut visual_cx);

        assert!(set_task_checked(&preview, 2..5, true, &mut visual_cx));
        assert!(set_task_checked(&preview, 13..16, false, &mut visual_cx));

        let expected = "- [x] Open\n- [ ] Done\n";
        assert_eq!(buffer_text(&tab, &mut visual_cx), expected);
        assert_eq!(
            preview_source(&preview, &mut visual_cx),
            expected,
            "the edit emits a change, so the preview follows it"
        );
    }

    #[gpui_kit::test]
    fn task_toggles_keep_the_editor_selection(cx: &mut TestAppContext) {
        let (tab, preview, _fulgur, mut visual_cx) = preview_of_active_tab(cx);
        replace_buffer(&tab, "- [ ] Open\nmore text\n", &mut visual_cx);
        let content = visual_cx.update(|_window, cx| {
            tab.read(cx)
                .as_editor()
                .expect("editor tab")
                .content
                .clone()
        });
        visual_cx.update(|_window, cx| {
            content.update(cx, |state, cx| state.set_selected_range(11..15, cx));
        });

        assert!(set_task_checked(&preview, 2..5, true, &mut visual_cx));

        assert_eq!(
            visual_cx.update(|_window, cx| content.read(cx).selected_range()),
            11..15
        );
    }

    #[gpui_kit::test]
    fn task_toggles_map_back_through_rewritten_images(cx: &mut TestAppContext) {
        let (tab, preview, _fulgur, mut visual_cx) = preview_of_active_tab(cx);
        let dir = std::env::temp_dir().join("fulgur_preview_task_docs");
        visual_cx.update(|_window, cx| {
            tab.update(cx, |tab, cx| {
                if let Some(editor_tab) = tab.as_editor_mut() {
                    editor_tab.location = TabLocation::Local(dir.join("notes.md"));
                }
                cx.notify();
            });
        });
        replace_buffer(&tab, "![logo](logo.png)\n\n- [ ] Task\n", &mut visual_cx);
        let preview_text = preview_source(&preview, &mut visual_cx);
        assert!(
            preview_text.contains("file:"),
            "the image path is rewritten"
        );
        let marker = preview_text.find("[ ]").expect("expected a task marker");

        assert!(set_task_checked(
            &preview,
            marker..marker + 3,
            true,
            &mut visual_cx
        ));

        assert_eq!(
            buffer_text(&tab, &mut visual_cx),
            "![logo](logo.png)\n\n- [x] Task\n"
        );
    }

    #[gpui_kit::test]
    fn task_toggles_leave_a_read_only_source_untouched(cx: &mut TestAppContext) {
        let (tab, preview, _fulgur, mut visual_cx) = preview_of_active_tab(cx);
        replace_buffer(&tab, "- [ ] Open\n", &mut visual_cx);
        visual_cx.update(|_window, cx| {
            tab.update(cx, |tab, cx| {
                tab.as_editor_mut().expect("editor tab").make_read_only(cx);
            });
        });

        assert!(!visual_cx.update(|_window, cx| preview.read(cx).accepts_task_toggles(cx)));
        assert!(!set_task_checked(&preview, 2..5, true, &mut visual_cx));
        assert_eq!(buffer_text(&tab, &mut visual_cx), "- [ ] Open\n");
    }

    #[gpui_kit::test]
    fn task_toggles_ignore_a_stale_marker(cx: &mut TestAppContext) {
        let (tab, preview, _fulgur, mut visual_cx) = preview_of_active_tab(cx);
        replace_buffer(&tab, "- [x] Done\n", &mut visual_cx);

        assert!(
            !set_task_checked(&preview, 2..5, true, &mut visual_cx),
            "the task is already checked"
        );
        assert!(
            !set_task_checked(&preview, 0..3, false, &mut visual_cx),
            "the range does not hold a marker"
        );
        assert!(
            !set_task_checked(&preview, 40..43, false, &mut visual_cx),
            "the range is past the end of the buffer"
        );
        assert_eq!(buffer_text(&tab, &mut visual_cx), "- [x] Done\n");
    }

    #[gpui_kit::test]
    fn typing_in_the_source_updates_the_preview(cx: &mut TestAppContext) {
        let (tab, preview, _fulgur, mut visual_cx) = preview_of_active_tab(cx);
        visual_cx.update(|window, cx| {
            let content = tab
                .read(cx)
                .as_editor()
                .expect("editor tab")
                .content
                .clone();
            content.update(cx, |state, cx| state.insert("# Typed", window, cx));
        });
        assert_eq!(preview_source(&preview, &mut visual_cx), "# Typed");
    }

    #[gpui_kit::test]
    fn programmatic_replacement_updates_the_preview(cx: &mut TestAppContext) {
        let (tab, preview, _fulgur, mut visual_cx) = preview_of_active_tab(cx);
        replace_buffer(&tab, "# Reloaded", &mut visual_cx);
        assert_eq!(preview_source(&preview, &mut visual_cx), "# Reloaded");
    }

    #[gpui_kit::test]
    fn moving_the_source_file_re_resolves_relative_images(cx: &mut TestAppContext) {
        let (tab, preview, _fulgur, mut visual_cx) = preview_of_active_tab(cx);
        replace_buffer(&tab, "![logo](logo.png)", &mut visual_cx);
        assert_eq!(
            preview_source(&preview, &mut visual_cx),
            "![logo](logo.png)"
        );

        let dir = std::env::temp_dir().join("fulgur_preview_source_docs");
        visual_cx.update(|_window, cx| {
            tab.update(cx, |tab, cx| {
                if let Some(editor_tab) = tab.as_editor_mut() {
                    editor_tab.location = TabLocation::Local(dir.join("notes.md"));
                }
                cx.notify();
            });
        });

        let expected_url = gpui_kit::http_client::Url::from_file_path(dir.join("logo.png"))
            .expect("expected an absolute path")
            .to_string();
        assert_eq!(
            preview_source(&preview, &mut visual_cx),
            format!("![logo]({expected_url})")
        );
        assert_eq!(
            visual_cx.update(|_window, cx| preview.read(cx).base_dir().map(PathBuf::from)),
            Some(dir)
        );
    }

    #[gpui_kit::test]
    fn large_sources_are_rewritten_in_the_background(cx: &mut TestAppContext) {
        let (tab, preview, _fulgur, mut visual_cx) = preview_of_active_tab(cx);
        let large = format!(
            "{}\n\n# End",
            "Some filler text.\n".repeat(MAX_SYNC_REWRITE_BYTES / 16)
        );
        assert!(large.len() > MAX_SYNC_REWRITE_BYTES);
        replace_buffer(&tab, &large, &mut visual_cx);
        assert_eq!(preview_source(&preview, &mut visual_cx), large);
    }
}
