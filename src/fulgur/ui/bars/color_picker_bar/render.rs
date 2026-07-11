use super::state::{ColorPickerBar, ColorPickerBarEvent};
use crate::fulgur::ui::{
    components_utils::SEARCH_BAR_HEIGHT, copy_button::CopyButton, icons::CustomIcon,
    insert_button::InsertButton,
};

use gpui::{
    Anchor, Context, Div, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, Window, div,
};
use gpui_component::{
    ActiveTheme, h_flex,
    input::{Input, InputState},
};

use super::super::search_bar::{search_bar_button_factory, search_bar_toggle_button_factory};

impl Render for ColorPickerBar {
    /// Render the color picker bar
    ///
    /// ### Arguments
    /// - `_window`: The window to render the color picker bar in
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered color picker bar, or an empty element when hidden
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.show_color_picker {
            return div().into_any_element();
        }
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
                                &self.cached_hex,
                                &self.hex_input,
                                cx,
                            ))
                            .child(Self::render_color_value_section(
                                "OkLCH",
                                &self.cached_oklch,
                                &self.oklch_input,
                                cx,
                            ))
                            .child(Self::render_color_value_section(
                                "HSLA",
                                &self.cached_hsla,
                                &self.hsla_input,
                                cx,
                            ))
                            .child(self.render_highlight_toggle_button(cx)),
                    ),
            )
            .child(Self::render_close_button(cx))
            .into_any_element()
    }
}

impl ColorPickerBar {
    /// Render the color picker section (left part with the color picker widget).
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered color picker section
    fn render_color_picker_section(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .items_center()
            .flex_shrink_0()
            .px_2()
            .gap_2()
            .h(SEARCH_BAR_HEIGHT)
            .child(
                gpui_component::color_picker::ColorPicker::new(&self.color_picker_state)
                    .anchor(Anchor::BottomLeft),
            )
    }

    /// Render a color value section with an editable input and a clipboard copy button.
    ///
    /// ### Arguments
    /// - `label`: The label for the color format (e.g. "Hex", "`OkLCH`", "HSLA")
    /// - `value`: The current formatted color value string (used for the insert and clipboard buttons)
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
        let highlight_colors = self
            .fulgur
            .upgrade()
            .is_some_and(|fulgur| fulgur.read(cx).settings.editor_settings.highlight_colors);
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
                .on_click(cx.listener(|_, _, _window, cx| {
                    cx.emit(ColorPickerBarEvent::ToggleHighlightColors);
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
    fn render_close_button(cx: &mut Context<Self>) -> Div {
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
                    this.close(cx);
                })),
            )
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod gpui_tests {
    use super::super::state::ColorPickerBarEvent;
    use crate::fulgur::{
        Fulgur, settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
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
            assert!(!fulgur.read(cx).color_picker_bar.read(cx).is_visible());
        });
    }

    #[gpui::test]
    fn test_toggle_color_picker_shows_bar(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.toggle_color_picker(window, cx);
            });
            assert!(fulgur.read(cx).color_picker_bar.read(cx).is_visible());
        });
    }

    #[gpui::test]
    fn test_toggle_color_picker_twice_hides_bar(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.toggle_color_picker(window, cx);
            });
            assert!(fulgur.read(cx).color_picker_bar.read(cx).is_visible());

            fulgur.update(cx, |this, cx| {
                this.toggle_color_picker(window, cx);
            });
            assert!(!fulgur.read(cx).color_picker_bar.read(cx).is_visible());
        });
    }

    #[gpui::test]
    fn test_close_event_hides_bar(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let bar = visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.toggle_color_picker(window, cx);
                this.color_picker_bar.clone()
            })
        });
        visual_cx.update(|_window, cx| {
            bar.update(cx, super::super::state::ColorPickerBar::close);
        });
        visual_cx.run_until_parked();
        visual_cx.update(|_window, cx| {
            assert!(!bar.read(cx).is_visible());
        });
    }

    #[gpui::test]
    fn test_insert_color_value_inserts_at_cursor(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            let bar = fulgur.update(cx, |this, cx| {
                this.update_active_editor_tab(cx, |editor, cx| {
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
                })
                .expect("expected active editor tab");
                this.color_picker_bar.clone()
            });
            bar.update(cx, |bar, cx| {
                bar.insert_color_value("#FF0000".to_string(), window, cx);
            });
            let text = fulgur
                .read(cx)
                .get_active_editor_tab(cx)
                .expect("expected active editor tab")
                .content
                .read(cx)
                .text()
                .to_string();
            assert_eq!(text, "color: #FF0000;");
        });
    }

    #[gpui::test]
    fn test_insert_color_value_no_active_tab_does_not_panic(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            let bar = fulgur.update(cx, |this, _cx| {
                this.active_tab_id = None;
                this.color_picker_bar.clone()
            });
            bar.update(cx, |bar, cx| {
                bar.insert_color_value("#FF0000".to_string(), window, cx);
            });
        });
    }

    #[gpui::test]
    fn test_toggle_highlight_colors_event_flips_setting(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let (bar, initial) = visual_cx.update(|_window, cx| {
            let this = fulgur.read(cx);
            (
                this.color_picker_bar.clone(),
                this.settings.editor_settings.highlight_colors,
            )
        });
        visual_cx.update(|_window, cx| {
            bar.update(cx, |_, cx| {
                cx.emit(ColorPickerBarEvent::ToggleHighlightColors);
            });
        });
        visual_cx.run_until_parked();
        let after = visual_cx
            .update(|_window, cx| fulgur.read(cx).settings.editor_settings.highlight_colors);
        assert_eq!(after, !initial);
    }
}
