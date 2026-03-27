use crate::fulgur::{
    Fulgur,
    ui::components_utils::{MARKDOWN_BAR_BUTTON_SIZE, MARKDOWN_BAR_HEIGHT, button_factory},
    ui::icons::CustomIcon,
};
use gpui::*;
use gpui_component::{ActiveTheme, button::Button, h_flex};

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

impl Fulgur {
    /// Handle bold action
    ///
    /// ### Arguments
    /// - `prefix`: The prefix to insert or surround with
    /// - `suffix`: The suffix to insert or surround with
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn insert_or_surround(
        &mut self,
        prefix: &str,
        suffix: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.active_tab_index
            && let Some(crate::fulgur::tab::Tab::Editor(editor_tab)) = self.tabs.get_mut(index)
        {
            editor_tab.content.update(cx, |input_state, cx| {
                let selection = input_state.selected_text_range(true, window, cx);
                if let Some(selection) = selection {
                    let selected_text = input_state
                        .text()
                        .slice(selection.range.start..selection.range.end)
                        .to_string();
                    let bold_text = format!("{}{}{}", prefix, selected_text, suffix);
                    input_state.replace(bold_text, window, cx);
                } else {
                    let bold_text = format!("{}{}{}", prefix, " ", suffix);
                    input_state.insert(bold_text, window, cx);
                }
                cx.notify();
            });
        }
    }

    /// Render the markdown bar
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Option<Div>`: The rendered markdown bar element
    pub fn render_markdown_bar(&self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.is_markdown() {
            return None;
        }
        let current_tab = self.get_active_editor_tab()?;
        if !current_tab.show_markdown_toolbar {
            return None;
        }
        Some(
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
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.insert_or_surround("**", "**", window, cx);
                                },
                            )),
                        )
                        .child(
                            markdown_bar_button_factory(
                                "markdown-italic-button",
                                "Italic",
                                CustomIcon::Italic,
                                cx.theme().border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.insert_or_surround("*", "*", window, cx);
                                },
                            )),
                        )
                        .child(
                            markdown_bar_button_factory(
                                "markdown-strikethrough-button",
                                "Strikethrough",
                                CustomIcon::Strikethrough,
                                cx.theme().border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.insert_or_surround("~~", "~~", window, cx);
                                },
                            )),
                        )
                        .child(
                            markdown_bar_button_factory(
                                "markdown-link-button",
                                "Link",
                                CustomIcon::Link,
                                cx.theme().border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.insert_or_surround("[", "](https://)", window, cx);
                                },
                            )),
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
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.insert_or_surround("# ", "", window, cx);
                                },
                            )),
                        )
                        .child(
                            markdown_bar_button_factory(
                                "markdown-heading-2-button",
                                "Heading 2",
                                CustomIcon::Heading2,
                                cx.theme().border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.insert_or_surround("## ", "", window, cx);
                                },
                            )),
                        )
                        .child(
                            markdown_bar_button_factory(
                                "markdown-heading-3-button",
                                "Heading 3",
                                CustomIcon::Heading3,
                                cx.theme().border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.insert_or_surround("### ", "", window, cx);
                                },
                            )),
                        )
                        .child(
                            markdown_bar_button_factory(
                                "markdown-heading-4-button",
                                "Heading 4",
                                CustomIcon::Heading4,
                                cx.theme().border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.insert_or_surround("#### ", "", window, cx);
                                },
                            )),
                        )
                        .child(
                            markdown_bar_button_factory(
                                "markdown-heading-5-button",
                                "Heading 5",
                                CustomIcon::Heading5,
                                cx.theme().border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.insert_or_surround("##### ", "", window, cx);
                                },
                            )),
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
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.insert_or_surround("- ", "", window, cx);
                                },
                            )),
                        )
                        .child(
                            markdown_bar_button_factory(
                                "markdown-numbered-list-button",
                                "Numbered List",
                                CustomIcon::ListNumbered,
                                cx.theme().border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.insert_or_surround("1. ", "", window, cx);
                                },
                            )),
                        )
                        .child(
                            markdown_bar_button_factory(
                                "markdown-task-list-button",
                                "Task List",
                                CustomIcon::TaskList,
                                cx.theme().border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.insert_or_surround("* [ ] ", "", window, cx);
                                },
                            )),
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
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.insert_or_surround("> ", "", window, cx);
                                },
                            )),
                        )
                        .child(
                            markdown_bar_button_factory(
                                "markdown-separator-button",
                                "Separator",
                                CustomIcon::Separator,
                                cx.theme().border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.insert_or_surround("---", "", window, cx);
                                },
                            )),
                        )
                        .child(
                            markdown_bar_button_factory(
                                "markdown-code-button",
                                "Code",
                                CustomIcon::Code,
                                cx.theme().border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.insert_or_surround("`", "`", window, cx);
                                },
                            )),
                        )
                        .child(
                            markdown_bar_button_factory(
                                "markdown-code-block-button",
                                "Code Block",
                                CustomIcon::FileCode,
                                cx.theme().border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.insert_or_surround("```", "```", window, cx);
                                },
                            )),
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
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.insert_or_surround("![", "](https://)", window, cx);
                                },
                            )),
                        )
                        .child(
                            markdown_bar_button_factory(
                                "markdown-table-button",
                                "Table",
                                CustomIcon::Table,
                                cx.theme().border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.insert_or_surround(
                                        "|",
                                        "|||\n|---|---|---|\n||||\n||||\n",
                                        window,
                                        cx,
                                    );
                                },
                            )),
                        ),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "gpui-test-support")]
    use super::Fulgur;
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::{
        settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
    };
    #[cfg(feature = "gpui-test-support")]
    use core::prelude::v1::test;
    #[cfg(feature = "gpui-test-support")]
    use gpui::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext};
    #[cfg(feature = "gpui-test-support")]
    use gpui_component::{Root, input::Position, input::SelectAll};
    #[cfg(feature = "gpui-test-support")]
    use parking_lot::Mutex;
    #[cfg(feature = "gpui-test-support")]
    use std::{cell::RefCell, path::PathBuf, sync::Arc};

    #[cfg(feature = "gpui-test-support")]
    fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let mut settings = Settings::new();
            settings.editor_settings.watch_files = false;
            let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
            cx.set_global(SharedAppState::new(settings, pending_files));
            cx.set_global(WindowManager::new());
        });

        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        let window = cx
            .update(|cx| {
                cx.open_window(Default::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                    *fulgur_slot.borrow_mut() = Some(fulgur.clone());
                    cx.new(|cx| Root::new(fulgur, window, cx))
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

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_insert_or_surround_wraps_selected_text(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                {
                    let editor = this
                        .get_active_editor_tab_mut()
                        .expect("expected active editor tab");
                    editor.content.update(cx, |content, cx| {
                        content.set_value("hello", window, cx);
                    });
                }

                this.focus_active_tab(window, cx);
                let focus = this
                    .get_active_editor_tab()
                    .expect("expected active editor tab")
                    .content
                    .read(cx)
                    .focus_handle(cx);
                focus.dispatch_action(&SelectAll, window, cx);
                let selected = this
                    .get_active_editor_tab()
                    .expect("expected active editor tab")
                    .content
                    .read(cx)
                    .selected_value()
                    .to_string();
                assert_eq!(selected, "hello");

                this.insert_or_surround("**", "**", window, cx);

                let text = this
                    .get_active_editor_tab()
                    .expect("expected active editor tab")
                    .content
                    .read(cx)
                    .text()
                    .to_string();
                assert_eq!(text, "**hello**");
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_insert_or_surround_inserts_at_cursor_when_no_selection(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                {
                    let editor = this
                        .get_active_editor_tab_mut()
                        .expect("expected active editor tab");
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
                }

                this.insert_or_surround("[", "](https://)", window, cx);

                let text = this
                    .get_active_editor_tab()
                    .expect("expected active editor tab")
                    .content
                    .read(cx)
                    .text()
                    .to_string();
                assert_eq!(text, "hello[](https://)");
            });
        });
    }
}
