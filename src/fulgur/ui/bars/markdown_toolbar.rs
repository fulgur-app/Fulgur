use crate::fulgur::{
    Fulgur,
    ui::components_utils::{MARKDOWN_BAR_BUTTON_SIZE, MARKDOWN_BAR_HEIGHT, button_factory},
    ui::icons::CustomIcon,
};

use gpui::{
    App, Context, Entity, Hsla, IntoElement, ParentElement, Render, Styled, WeakEntity, Window, div,
};
use gpui_component::{ActiveTheme, button::Button, h_flex, input::EditorState};

/// Create a markdown bar button
///
/// ### Arguments
/// - `id`: The ID of the button
/// - `tooltip`: The tooltip of the button
/// - `icon`: The icon of the button
/// - `border_color`: The color of the border
///
/// ### Returns
/// - `Button`: A markdown bar button
pub fn markdown_bar_button_factory(
    id: &'static str,
    tooltip: &'static str,
    icon: CustomIcon,
    border_color: Hsla,
) -> Button {
    button_factory(id, tooltip, icon, border_color)
        .h(MARKDOWN_BAR_BUTTON_SIZE)
        .w(MARKDOWN_BAR_BUTTON_SIZE)
}

/// The markdown formatting toolbar, rendered as its own entity
pub(crate) struct MarkdownToolbar {
    fulgur: WeakEntity<Fulgur>,
}

impl MarkdownToolbar {
    /// Create a new markdown toolbar view
    ///
    /// ### Arguments
    /// - `fulgur`: Weak handle to the owning window entity the bar reads the active editor from
    ///
    /// ### Returns
    /// - `MarkdownToolbar`: The new markdown toolbar view
    pub(crate) fn new(fulgur: WeakEntity<Fulgur>) -> Self {
        Self { fulgur }
    }

    /// Get the active editor's content entity from the owning window
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(Entity<EditorState>)`: The active editor tab's content
    /// - `None`: If the window is gone or the active tab is not an editor
    fn active_editor_content(&self, cx: &App) -> Option<Entity<EditorState>> {
        let fulgur = self.fulgur.upgrade()?;
        fulgur
            .read(cx)
            .get_active_editor_tab(cx)
            .map(|editor_tab| editor_tab.content.clone())
    }

    /// Surround the active editor's selection with a prefix and suffix, or insert them at the cursor
    ///
    /// ### Arguments
    /// - `prefix`: The prefix to insert or surround with
    /// - `suffix`: The suffix to insert or surround with
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(crate) fn insert_or_surround(
        &mut self,
        prefix: &str,
        suffix: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(content) = self.active_editor_content(cx) {
            content.update(cx, |input_state, cx| {
                let selected_text = input_state.selected_value();
                let surrounded_text = format!("{prefix}{selected_text}{suffix}");
                input_state.replace(surrounded_text, window, cx);
                cx.notify();
            });
        }
    }
}

impl Fulgur {
    /// Whether the markdown toolbar should be mounted for the active tab
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `bool`: True if the active tab is markdown and its toolbar is enabled
    pub(crate) fn markdown_toolbar_visible(&self, cx: &gpui::App) -> bool {
        self.is_markdown(cx)
            && self
                .get_active_editor_tab(cx)
                .is_some_and(|editor_tab| editor_tab.show_markdown_toolbar)
    }
}

impl Render for MarkdownToolbar {
    /// Render the markdown toolbar
    ///
    /// ### Arguments
    /// - `_window`: The window to render the markdown toolbar in
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered markdown toolbar
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .p_0()
            .m_0()
            .h(MARKDOWN_BAR_HEIGHT)
            .bg(cx.theme().tab_bar)
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .border_r_1()
                    .border_color(cx.theme().border)
                    .child(
                        markdown_bar_button_factory(
                            "markdown-bold-button",
                            "Bold",
                            CustomIcon::Bold,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.insert_or_surround("**", "**", window, cx);
                        })),
                    )
                    .child(
                        markdown_bar_button_factory(
                            "markdown-italic-button",
                            "Italic",
                            CustomIcon::Italic,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.insert_or_surround("*", "*", window, cx);
                        })),
                    )
                    .child(
                        markdown_bar_button_factory(
                            "markdown-strikethrough-button",
                            "Strikethrough",
                            CustomIcon::Strikethrough,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.insert_or_surround("~~", "~~", window, cx);
                        })),
                    )
                    .child(
                        markdown_bar_button_factory(
                            "markdown-link-button",
                            "Link",
                            CustomIcon::Link,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.insert_or_surround("[", "](https://)", window, cx);
                        })),
                    ),
            )
            .child(
                h_flex()
                    .border_r_1()
                    .border_color(cx.theme().border)
                    .child(
                        markdown_bar_button_factory(
                            "markdown-heading-1-button",
                            "Heading 1",
                            CustomIcon::Heading1,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.insert_or_surround("# ", "", window, cx);
                        })),
                    )
                    .child(
                        markdown_bar_button_factory(
                            "markdown-heading-2-button",
                            "Heading 2",
                            CustomIcon::Heading2,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.insert_or_surround("## ", "", window, cx);
                        })),
                    )
                    .child(
                        markdown_bar_button_factory(
                            "markdown-heading-3-button",
                            "Heading 3",
                            CustomIcon::Heading3,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.insert_or_surround("### ", "", window, cx);
                        })),
                    )
                    .child(
                        markdown_bar_button_factory(
                            "markdown-heading-4-button",
                            "Heading 4",
                            CustomIcon::Heading4,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.insert_or_surround("#### ", "", window, cx);
                        })),
                    )
                    .child(
                        markdown_bar_button_factory(
                            "markdown-heading-5-button",
                            "Heading 5",
                            CustomIcon::Heading5,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.insert_or_surround("##### ", "", window, cx);
                        })),
                    ),
            )
            .child(
                h_flex()
                    .border_r_1()
                    .border_color(cx.theme().border)
                    .child(
                        markdown_bar_button_factory(
                            "markdown-list-button",
                            "List",
                            CustomIcon::List,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.insert_or_surround("- ", "", window, cx);
                        })),
                    )
                    .child(
                        markdown_bar_button_factory(
                            "markdown-numbered-list-button",
                            "Numbered List",
                            CustomIcon::ListNumbered,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.insert_or_surround("1. ", "", window, cx);
                        })),
                    )
                    .child(
                        markdown_bar_button_factory(
                            "markdown-task-list-button",
                            "Task List",
                            CustomIcon::TaskList,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.insert_or_surround("* [ ] ", "", window, cx);
                        })),
                    ),
            )
            .child(
                h_flex()
                    .border_r_1()
                    .border_color(cx.theme().border)
                    .child(
                        markdown_bar_button_factory(
                            "markdown-quote-button",
                            "Quote",
                            CustomIcon::Quote,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.insert_or_surround("> ", "", window, cx);
                        })),
                    )
                    .child(
                        markdown_bar_button_factory(
                            "markdown-separator-button",
                            "Separator",
                            CustomIcon::Separator,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.insert_or_surround("---", "", window, cx);
                        })),
                    )
                    .child(
                        markdown_bar_button_factory(
                            "markdown-code-button",
                            "Code",
                            CustomIcon::Code,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.insert_or_surround("`", "`", window, cx);
                        })),
                    )
                    .child(
                        markdown_bar_button_factory(
                            "markdown-code-block-button",
                            "Code Block",
                            CustomIcon::FileCode,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.insert_or_surround("```", "```", window, cx);
                        })),
                    ),
            )
            .child(
                h_flex()
                    .border_r_1()
                    .border_color(cx.theme().border)
                    .child(
                        markdown_bar_button_factory(
                            "markdown-upload-button",
                            "Image or file",
                            CustomIcon::Upload,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.insert_or_surround("![", "](https://)", window, cx);
                        })),
                    )
                    .child(
                        markdown_bar_button_factory(
                            "markdown-table-button",
                            "Table",
                            CustomIcon::Table,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.insert_or_surround(
                                "|",
                                "|||\n|---|---|---|\n||||\n||||\n",
                                window,
                                cx,
                            );
                        })),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "gpui-test-support")]
    use super::MarkdownToolbar;
    use crate::fulgur::WindowInit;
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::{
        Fulgur, settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
    };
    #[cfg(feature = "gpui-test-support")]
    use core::prelude::v1::test;
    #[cfg(feature = "gpui-test-support")]
    use gpui::{
        App, AppContext, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext,
        Window, WindowOptions, div,
    };
    #[cfg(feature = "gpui-test-support")]
    use gpui_component::input::{EditorState, Position};
    #[cfg(feature = "gpui-test-support")]
    use parking_lot::Mutex;
    #[cfg(feature = "gpui-test-support")]
    use std::{cell::RefCell, ops::Range, path::PathBuf, sync::Arc};

    /// Window root that avoids `gpui_component::Root`, whose macOS accessibility hook panics on
    /// gpui's `TestWindow`. The toolbar reads the active editor through its `WeakEntity<Fulgur>`
    /// and needs nothing that `Root` provides, so these tests can run on every platform.
    #[cfg(feature = "gpui-test-support")]
    struct EmptyView;

    #[cfg(feature = "gpui-test-support")]
    impl Render for EmptyView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    #[cfg(feature = "gpui-test-support")]
    fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let mut settings = Settings::new();
            settings.editor_settings.watch_files = false;
            let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
            cx.set_global(SharedAppState::new(settings, pending_files, None, None));
            cx.set_global(WindowManager::new());
        });

        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        let window = cx
            .update(|cx| {
                cx.open_window(WindowOptions::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, WindowInit::Empty);
                    *fulgur_slot.borrow_mut() = Some(fulgur);
                    cx.new(|_| EmptyView)
                })
            })
            .expect("failed to open test window");

        let visual_cx = VisualTestContext::from_window(window.into(), cx);
        visual_cx.run_until_parked();
        let fulgur = fulgur_slot
            .into_inner()
            .expect("failed to capture Fulgur entity");
        (fulgur, visual_cx)
    }

    /// Set up a `Fulgur` window and return its markdown toolbar entity.
    #[cfg(feature = "gpui-test-support")]
    fn setup_toolbar(
        cx: &mut TestAppContext,
    ) -> (Entity<Fulgur>, Entity<MarkdownToolbar>, VisualTestContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let toolbar = visual_cx.update(|_window, cx| fulgur.read(cx).markdown_toolbar.clone());
        (fulgur, toolbar, visual_cx)
    }

    /// Load `text` into the active editor, select `selection`, then bold it through the toolbar
    ///
    /// ### Arguments
    /// - `cx`: The test application context
    /// - `text`: The buffer contents to load into the active editor
    /// - `selection`: The range to select, as UTF-8 byte offsets into `text`
    ///
    /// ### Returns
    /// - `(String, String)`: The text the editor reports as selected, and the resulting buffer
    #[cfg(feature = "gpui-test-support")]
    fn bold_byte_selection(
        cx: &mut TestAppContext,
        text: &str,
        selection: Range<usize>,
    ) -> (String, String) {
        let (fulgur, toolbar, mut visual_cx) = setup_toolbar(cx);

        visual_cx.update(|window, cx| {
            fulgur
                .update(cx, |this, cx| {
                    this.update_active_editor_tab(cx, |editor, cx| {
                        editor.content.update(cx, |content, cx| {
                            content.set_value(text.to_string(), window, cx);
                            content.set_selected_range(selection, cx);
                        });
                    })
                })
                .expect("expected active editor tab");

            let selected =
                active_content_text(&fulgur, cx, |content| content.selected_value().to_string());

            toolbar.update(cx, |bar, cx| {
                bar.insert_or_surround("**", "**", window, cx);
            });

            let result = active_content_text(&fulgur, cx, |content| content.text().to_string());
            (selected, result)
        })
    }

    /// Read a string out of the active editor tab's content entity
    ///
    /// ### Arguments
    /// - `fulgur`: The window entity owning the tabs
    /// - `cx`: The application context
    /// - `read`: Extracts the wanted string from the content entity
    ///
    /// ### Returns
    /// - `String`: Whatever `read` extracted
    #[cfg(feature = "gpui-test-support")]
    fn active_content_text(
        fulgur: &Entity<Fulgur>,
        cx: &App,
        read: impl FnOnce(&EditorState) -> String,
    ) -> String {
        read(
            fulgur
                .read(cx)
                .get_active_editor_tab(cx)
                .expect("expected active editor tab")
                .content
                .read(cx),
        )
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_insert_or_surround_wraps_selected_text(cx: &mut TestAppContext) {
        let (selected, text) = bold_byte_selection(cx, "hello", 0..5);

        assert_eq!(selected, "hello");
        assert_eq!(text, "**hello**");
    }

    /// A selection sitting after an accented character must be wrapped as-is. The input handler
    /// reports selections in UTF-16 code units while the rope is indexed in UTF-8 bytes, and
    /// slicing the rope with the UTF-16 range used to wrap an earlier stretch of the document:
    /// here it produced `héllo ** worl**` because bytes `6..11` spell `" worl"`.
    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_insert_or_surround_wraps_selection_after_accented_character(cx: &mut TestAppContext) {
        let (selected, text) = bold_byte_selection(cx, "héllo world", 7..12);

        assert_eq!(selected, "world");
        assert_eq!(text, "héllo **world**");
    }

    /// Same divergence as the accented case, but wider: an emoji outside the basic multilingual
    /// plane is 4 UTF-8 bytes against 2 UTF-16 code units, so the stale range started inside the
    /// emoji rather than merely early. A fix counting characters rather than code units would
    /// still fail here.
    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_insert_or_surround_wraps_selection_after_emoji(cx: &mut TestAppContext) {
        let (selected, text) = bold_byte_selection(cx, "🚀 launch", 5..11);

        assert_eq!(selected, "launch");
        assert_eq!(text, "🚀 **launch**");
    }

    /// The selection itself may hold multi-byte characters without being truncated
    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_insert_or_surround_wraps_multibyte_selection(cx: &mut TestAppContext) {
        let (selected, text) = bold_byte_selection(cx, "dis élève ok", 4..11);

        assert_eq!(selected, "élève");
        assert_eq!(text, "dis **élève** ok");
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_insert_or_surround_inserts_at_cursor_when_no_selection(cx: &mut TestAppContext) {
        let (fulgur, toolbar, mut visual_cx) = setup_toolbar(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.update_active_editor_tab(cx, |editor, cx| {
                    editor.content.update(cx, |content, cx| {
                        content.set_value("hello", window, cx);
                        content.set_cursor_position(
                            Position {
                                line: 0,
                                character: 5,
                            },
                            window,
                            cx,
                        );
                    });
                })
                .expect("expected active editor tab");
            });

            toolbar.update(cx, |bar, cx| {
                bar.insert_or_surround("[", "](https://)", window, cx);
            });

            let text = fulgur
                .read(cx)
                .get_active_editor_tab(cx)
                .expect("expected active editor tab")
                .content
                .read(cx)
                .text()
                .to_string();
            assert_eq!(text, "hello[](https://)");
        });
    }
}
