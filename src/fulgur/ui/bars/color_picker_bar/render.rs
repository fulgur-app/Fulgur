use super::state::{ColorPickerBar, ColorPickerBarEvent};
use crate::fulgur::ui::{
    components_utils::SEARCH_BAR_HEIGHT, copy_button::CopyButton, icons::CustomIcon,
    insert_button::InsertButton,
};

use gpui_kit::component::{
    ActiveTheme, h_flex,
    input::{Input, InputState},
};
use gpui_kit::{
    Anchor, Context, Div, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, Window, div,
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
    /// - `impl IntoElement`: The rendered color picker bar
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                            .child(Self::render_color_value_section("Hex", &self.hex_input, cx))
                            .child(Self::render_color_value_section(
                                "OkLCH",
                                &self.oklch_input,
                                cx,
                            ))
                            .child(Self::render_color_value_section(
                                "HSLA",
                                &self.hsla_input,
                                cx,
                            ))
                            .child(self.render_highlight_toggle_button(cx)),
                    ),
            )
            .child(Self::render_close_button(cx))
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
                gpui_kit::component::color_picker::ColorPicker::new(&self.color_picker_state)
                    .anchor(Anchor::BottomLeft),
            )
    }

    /// Render a color value section with an editable input and a clipboard copy button.
    ///
    /// ### Arguments
    /// - `label`: The label for the color format (e.g. "Hex", "`OkLCH`", "HSLA")
    /// - `input_state`: The input state entity for this field
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Div`: The rendered color value section
    fn render_color_value_section(
        label: &'static str,
        input_state: &Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Div {
        let value = input_state.read(cx).value();
        div()
            .flex()
            .items_center()
            .flex_1()
            .min_w(gpui_kit::px(330.0))
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
                        let input_state = input_state.clone();
                        move |this, _, window, cx| {
                            let value = input_state.read(cx).value();
                            this.insert_color_value(value, window, cx);
                        }
                    }),
                ),
            )
            .child(CopyButton::new(SharedString::from(format!("color-copy-{label}"))).value(value))
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

    use gpui_kit::component::input::{InputEvent, Position};
    use gpui_kit::{Entity, Modifiers, TestAppContext, VisualTestContext};

    use crate::fulgur::Fulgur;
    use crate::test_support::{setup_fulgur, setup_fulgur_with_root};

    /// Show the color picker bar in a rendered window, then type `#FF0000` into the Hex
    /// input and press Enter, the way a user commits a typed color
    ///
    /// ### Arguments
    /// - `cx`: The test application context
    ///
    /// ### Returns
    /// - `(Entity<Fulgur>, VisualTestContext)`: The Fulgur entity and a visual test context
    ///   whose window has been drawn with the committed color
    fn show_bar_with_typed_red_hex(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| this.toggle_color_picker(window, cx));
            let hex_input = fulgur.read(cx).color_picker_bar.read(cx).hex_input.clone();
            hex_input.update(cx, |state, cx| {
                state.set_value("#FF0000", window, cx);
                cx.emit(InputEvent::PressEnter {
                    secondary: false,
                    shift: false,
                });
            });
        });
        visual_cx.run_until_parked();
        visual_cx.update(|window, cx| window.draw(cx).clear(cx));
        (fulgur, visual_cx)
    }

    #[gpui_kit::test]
    fn test_insert_button_uses_typed_hex_value(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = show_bar_with_typed_red_hex(cx);
        let insert = visual_cx
            .debug_bounds("color-insert-Hex")
            .expect("hex insert button should be rendered");
        visual_cx.simulate_click(insert.center(), Modifiers::none());
        let text = visual_cx.update(|_window, cx| {
            fulgur
                .read(cx)
                .get_active_editor_tab(cx)
                .expect("expected active editor tab")
                .content
                .read(cx)
                .text()
                .to_string()
        });
        assert_eq!(text, "#FF0000");
    }

    #[gpui_kit::test]
    fn test_copy_button_uses_synced_sibling_value(cx: &mut TestAppContext) {
        let (_fulgur, mut visual_cx) = show_bar_with_typed_red_hex(cx);
        let copy = visual_cx
            .debug_bounds("color-copy-HSLA")
            .expect("hsla copy button should be rendered");
        visual_cx.simulate_click(copy.center(), Modifiers::none());
        let copied = visual_cx.read_from_clipboard().and_then(|item| item.text());
        assert_eq!(copied.as_deref(), Some("hsla(0, 100%, 50%, 1.00)"));
    }

    #[gpui_kit::test]
    fn test_color_picker_bar_hidden_by_default(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            assert!(!fulgur.read(cx).color_picker_bar.read(cx).is_visible());
        });
    }

    #[gpui_kit::test]
    fn test_toggle_color_picker_shows_bar(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.toggle_color_picker(window, cx);
            });
            assert!(fulgur.read(cx).color_picker_bar.read(cx).is_visible());
        });
    }

    #[gpui_kit::test]
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

    #[gpui_kit::test]
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

    #[gpui_kit::test]
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
                bar.insert_color_value("#FF0000".into(), window, cx);
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

    #[gpui_kit::test]
    fn test_insert_color_value_no_active_tab_does_not_panic(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            let bar = fulgur.update(cx, |this, _cx| {
                this.active_tab_id = None;
                this.color_picker_bar.clone()
            });
            bar.update(cx, |bar, cx| {
                bar.insert_color_value("#FF0000".into(), window, cx);
            });
        });
    }

    #[gpui_kit::test]
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
