//! Keeps a Markdown preview's rendered text in step with its source editor tab.

use crate::fulgur::tab::Tab;
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::utils::markdown_images::rewrite_markdown_image_paths;
use gpui_kit::component::input::{EditorState, InputEvent, Rope};
use gpui_kit::component::text::TextViewState;
use gpui_kit::{App, AppContext, Context, Entity, Subscription, Task};
use std::borrow::Cow;
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
    base_dir: Option<PathBuf>,
    view_state: Entity<TextViewState>,
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
                base_dir,
                view_state: cx.new(|cx| TextViewState::markdown("", cx)),
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
            let text = preview_text(&rope, base_dir.as_deref());
            view_state.update(cx, |state, cx| state.set_text(&text, cx));
            return;
        }
        // Replacing the task drops, and so cancels, a rewrite still in flight.
        self.refresh_task = Some(cx.spawn(async move |_, cx| {
            let text = cx
                .background_executor()
                .spawn(async move { preview_text(&rope, base_dir.as_deref()) })
                .await;
            view_state.update(cx, |state, cx| state.set_text(&text, cx));
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
/// - `String`: The Markdown with local image references rewritten
fn preview_text(rope: &Rope, base_dir: Option<&Path>) -> String {
    let source = rope.to_string();
    let rewritten = match rewrite_markdown_image_paths(&source, base_dir) {
        Cow::Owned(rewritten) => Some(rewritten),
        Cow::Borrowed(_) => None,
    };
    rewritten.unwrap_or(source)
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
