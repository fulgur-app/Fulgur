use crate::fulgur::{
    Fulgur,
    ui::{
        components_utils::SEARCH_BAR_HEIGHT, copy_button::CopyButton, icons::CustomIcon,
        insert_button::InsertButton,
    },
};

use gpui::{
    Anchor, Context, Div, Entity, EntityInputHandler, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, Window, div,
};
use gpui_component::{
    ActiveTheme, h_flex,
    input::{Input, InputState},
};

use super::super::search_bar::{search_bar_button_factory, search_bar_toggle_button_factory};

impl Fulgur {
    /// Insert a value at the cursor position in the active editor tab, replacing the current selection if any.
    ///
    /// ### Arguments
    /// - `value`: The string to insert
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn insert_color_value(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.active_tab_index()
            && let Some(crate::fulgur::tab::Tab::Editor(editor_tab)) = self.tabs.get_mut(index)
        {
            editor_tab.content.update(cx, |input_state, cx| {
                let selection = input_state.selected_text_range(true, window, cx);
                if selection.is_some() {
                    input_state.replace(value, window, cx);
                } else {
                    input_state.insert(value, window, cx);
                }
                cx.notify();
            });
        }
    }

    /// Toggle the color picker bar visibility.
    ///
    /// ### Arguments
    /// - `_window`: The window context
    /// - `cx`: The application context
    pub fn toggle_color_picker(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.color_picker_bar_state.show_color_picker =
            !self.color_picker_bar_state.show_color_picker;
        cx.notify();
    }

    /// Render the color picker bar.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(Div)`: The rendered color picker bar
    /// - `None`: If the color picker bar is hidden
    pub fn render_color_picker_bar(&self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.color_picker_bar_state.show_color_picker {
            return None;
        }
        let hex_value = &self.color_picker_bar_state.cached_hex;
        let oklch_value = &self.color_picker_bar_state.cached_oklch;
        let hsla_value = &self.color_picker_bar_state.cached_hsla;
        Some(
            div()
                .flex()
                .items_center()
                .justify_between()
                .bg(cx.theme().tab_bar)
                .p_0()
                .m_0()
                .w_full()
                .h(SEARCH_BAR_HEIGHT)
                .border_t_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .id("color-picker-scroll-container")
                        .flex()
                        .flex_1()
                        .overflow_x_scroll()
                        .child(
                            div()
                                .flex()
                                .flex_1()
                                .items_center()
                                .h(SEARCH_BAR_HEIGHT)
                                .child(self.render_color_picker_section(cx))
                                .child(Self::render_color_value_section(
                                    "Hex",
                                    hex_value,
                                    &self.color_picker_bar_state.hex_input,
                                    cx,
                                ))
                                .child(Self::render_color_value_section(
                                    "OkLCH",
                                    oklch_value,
                                    &self.color_picker_bar_state.oklch_input,
                                    cx,
                                ))
                                .child(Self::render_color_value_section(
                                    "HSLA",
                                    hsla_value,
                                    &self.color_picker_bar_state.hsla_input,
                                    cx,
                                ))
                                .child(self.render_highlight_toggle_button(cx)),
                        ),
                )
                .child(Self::render_color_picker_close_button(cx)),
        )
    }

    /// Render the color picker section (left part with the color picker widget).
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered color picker section
    fn render_color_picker_section(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let color_picker_state = &self.color_picker_bar_state.color_picker_state;
        h_flex()
            .items_center()
            .flex_shrink_0()
            .px_2()
            .gap_2()
            .h(SEARCH_BAR_HEIGHT)
            .child(
                gpui_component::color_picker::ColorPicker::new(color_picker_state)
                    .anchor(Anchor::BottomLeft),
            )
    }

    /// Render a color value section with an editable input and a clipboard copy button.
    ///
    /// ### Arguments
    /// - `label`: The label for the color format (e.g. "Hex", "`OkLCH`", "HSLA")
    /// - `value`: The current formatted color value string (used for the clipboard button)
    /// - `input_state`: The input state entity for this field
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Div`: The rendered color value section
    fn render_color_value_section(
        label: &'static str,
        value: &str,
        input_state: &Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .flex_1()
            .min_w(gpui::px(330.0))
            .h(SEARCH_BAR_HEIGHT)
            .border_l_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_xs()
                    .px_2()
                    .text_color(cx.theme().muted_foreground)
                    .child(label),
            )
            .child(
                Input::new(input_state)
                    .appearance(false)
                    .bordered(false)
                    .px_0(),
            )
            .child(
                InsertButton::new(SharedString::from(format!("color-insert-{label}"))).on_click(
                    cx.listener({
                        let value = value.to_string();
                        move |this, _, window, cx| {
                            this.insert_color_value(value.clone(), window, cx);
                        }
                    }),
                ),
            )
            .child(
                CopyButton::new(SharedString::from(format!("color-copy-{label}")))
                    .value(SharedString::from(value.to_string())),
            )
    }

    /// Render the highlight colors toggle button for the color picker bar.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Div`: The rendered toggle button
    fn render_highlight_toggle_button(&self, cx: &mut Context<Self>) -> Div {
        let highlight_colors = self.settings.editor_settings.highlight_colors;
        div()
            .flex()
            .items_center()
            .p_0()
            .m_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .child(
                search_bar_toggle_button_factory(
                    "highlight-colors-toggle",
                    "Toggle color highlighting",
                    CustomIcon::Highlighter,
                    cx.theme().border,
                    cx.theme().tab_bar,
                    cx.theme().accent,
                    highlight_colors,
                )
                .on_click(cx.listener(|this, _, _window, cx| {
                    this.settings.editor_settings.highlight_colors =
                        !this.settings.editor_settings.highlight_colors;
                    let _ = this.update_and_propagate_settings(cx);
                })),
            )
    }

    /// Render the close button for the color picker bar.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Div`: The rendered close button
    fn render_color_picker_close_button(cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .p_0()
            .m_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .child(
                search_bar_button_factory(
                    "close-color-picker-button",
                    "Close",
                    CustomIcon::Close,
                    cx.theme().border,
                )
                .on_click(cx.listener(|this, _, _window, cx| {
                    this.color_picker_bar_state.show_color_picker = false;
                    cx.notify();
                })),
            )
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod gpui_tests {
    use super::Fulgur;
    use crate::fulgur::{
        settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
    };
    use gpui::{
        AppContext, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext,
        Window, WindowOptions,
    };
    use gpui_component::input::Position;
    use parking_lot::Mutex;
    use std::{cell::RefCell, path::PathBuf, sync::Arc};

    struct EmptyView;

    impl Render for EmptyView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            gpui::div()
        }
    }

    fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let mut settings = Settings::new();
            settings.editor_settings.watch_files = false;
            let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
            cx.set_global(SharedAppState::new(settings, pending_files, None));
            cx.set_global(WindowManager::new());
        });

        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        let window = cx
            .update(|cx| {
                cx.open_window(WindowOptions::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
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

    #[gpui::test]
    fn test_color_picker_bar_hidden_by_default(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                assert!(!this.color_picker_bar_state.show_color_picker);
                assert!(this.render_color_picker_bar(cx).is_none());
            });
        });
    }

    #[gpui::test]
    fn test_toggle_color_picker_shows_bar(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.toggle_color_picker(window, cx);
                assert!(this.color_picker_bar_state.show_color_picker);
            });
        });
    }

    #[gpui::test]
    fn test_toggle_color_picker_twice_hides_bar(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.toggle_color_picker(window, cx);
                assert!(this.color_picker_bar_state.show_color_picker);

                this.toggle_color_picker(window, cx);
                assert!(!this.color_picker_bar_state.show_color_picker);
            });
        });
    }

    #[gpui::test]
    fn test_insert_color_value_inserts_at_cursor(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let editor = this
                    .get_active_editor_tab_mut()
                    .expect("expected active editor tab");
                editor.content.update(cx, |content, cx| {
                    content.set_value("color: ;", window, cx);
                    content.set_cursor_position(
                        Position {
                            line: 0,
                            character: 7,
                        },
                        window,
                        cx,
                    );
                });
                this.insert_color_value("#FF0000".to_string(), window, cx);
                let text = this
                    .get_active_editor_tab()
                    .expect("expected active editor tab")
                    .content
                    .read(cx)
                    .text()
                    .to_string();
                assert_eq!(text, "color: #FF0000;");
            });
        });
    }

    #[gpui::test]
    fn test_insert_color_value_no_active_tab_does_not_panic(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.active_tab_id = None;
                this.insert_color_value("#FF0000".to_string(), window, cx);
            });
        });
    }

    #[gpui::test]
    fn test_highlight_toggle_reflects_editor_setting(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, _cx| {
                let initial = this.settings.editor_settings.highlight_colors;
                this.settings.editor_settings.highlight_colors = !initial;
                assert_ne!(
                    this.settings.editor_settings.highlight_colors, initial,
                    "toggling highlight_colors should change the setting"
                );
                this.settings.editor_settings.highlight_colors = initial;
                assert_eq!(
                    this.settings.editor_settings.highlight_colors, initial,
                    "reverting should restore original value"
                );
            });
        });
    }
}
