use std::ops::DerefMut;

use crate::fulgur::{
    Fulgur,
    components_utils::{EMPTY, UTF_8},
    editor_tab, languages,
};
use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    ActiveTheme, WindowExt, h_flex,
    highlighter::Language,
    input::{Input, Position},
    select::Select,
};

// Create a status bar item
// @param content: The content of the status bar item
// @param border_color: The color of the border
// @return: A status bar item
pub fn status_bar_item_factory(content: String, border_color: Hsla) -> Div {
    div()
        .text_xs()
        .px_2()
        .py_1()
        .border_color(border_color)
        .child(content)
}

// Create a status bar button
// @param content: The content of the status bar button
// @param border_color: The color of the border
// @param accent_color: The color of the accent
// @return: A status bar button
pub fn status_bar_button_factory(content: String, border_color: Hsla, accent_color: Hsla) -> Div {
    status_bar_item_factory(content, border_color)
        .hover(|this| this.bg(accent_color))
        .cursor_pointer()
}

// Create a status bar right item
// @param content: The content of the status bar right item
// @param border_color: The color of the border
// @return: A status bar right item
pub fn status_bar_right_item_factory(content: String, border_color: Hsla) -> impl IntoElement {
    status_bar_item_factory(content, border_color) //.border_l_1()
}

// Create a status bar left item
// @param content: The content of the status bar left item
// @param border_color: The color of the border
// @return: A status bar left item
// pub fn status_bar_left_item_factory(content: String, border_color: Hsla) -> impl IntoElement {
//     status_bar_item_factory(content, border_color) //.border_r_1()
// }

impl Fulgur {
    // Jump to line
    // @param window: The window context
    // @param cx: The application context
    pub fn jump_to_line(self: &mut Fulgur, window: &mut Window, cx: &mut Context<Self>) {
        let jump_to_line_input = self.jump_to_line_input.clone();
        jump_to_line_input.update(cx, |input_state, cx| {
            input_state.set_value("", window, cx);
            cx.notify();
        });
        let entity = cx.entity().clone();
        self.jump_to_line_dialog_open = true;
        window.open_dialog(cx.deref_mut(), move |modal, window, cx| {
            let focus_handle = jump_to_line_input.read(cx).focus_handle(cx);
            window.focus(&focus_handle);
            let entity_clone = entity.clone();
            let jump_to_line_input_clone = jump_to_line_input.clone();
            modal
                .confirm()
                .keyboard(true)
                .child(Input::new(&jump_to_line_input))
                .overlay_closable(true)
                .close_button(false)
                .on_ok(move |_event: &ClickEvent, _window, cx| {
                    let text = jump_to_line_input_clone.read(cx).value();
                    let text_shared = SharedString::from(text);
                    let jump = editor_tab::extract_line_number(text_shared);
                    let entity_ok = entity_clone.clone();
                    entity_ok.update(cx, |this, cx| {
                        if let Ok(jump) = jump {
                            this.pending_jump = Some(jump);
                            this.jump_to_line_dialog_open = false;
                            cx.notify();
                            return true;
                        } else {
                            this.pending_jump = None;
                            return false;
                        }
                    });
                    false
                })
        });
        return;
    }

    // Set the language via a dialog
    // @param window: The window context
    // @param cx: The application context
    // @param current_language: The current language
    fn set_language(
        self: &mut Fulgur,
        window: &mut Window,
        cx: &mut Context<Self>,
        current_language: SharedString,
    ) {
        let language_dropdown = self.language_dropdown.clone();
        language_dropdown.update(cx, |select_state, cx| {
            select_state.set_selected_value(&current_language, window, cx);
            cx.notify();
        });
        let entity = cx.entity().clone();
        window.open_dialog(cx.deref_mut(), move |modal, window, cx| {
            let focus_handle = language_dropdown.read(cx).focus_handle(cx);
            window.focus(&focus_handle);
            let entity_clone = entity.clone();
            modal
                .confirm()
                .keyboard(true)
                .child(Select::new(&language_dropdown))
                .overlay_closable(true)
                .close_button(false)
                .on_ok({
                    let value = language_dropdown.clone();
                    let entity_ok = entity_clone.clone();
                    move |_event: &ClickEvent, window, cx| {
                        let language_name = value.read(cx).selected_value();
                        if let Some(language_name) = language_name {
                            let language = languages::language_from_pretty_name(&language_name);
                            entity_ok.update(cx, |this, cx| {
                                if let Some(index) = this.active_tab_index {
                                    if let Some(tab) = this.tabs.get_mut(index) {
                                        if let Some(editor_tab) = tab.as_editor_mut() {
                                            editor_tab.force_language(
                                                window,
                                                cx,
                                                language,
                                                &this.settings.editor_settings,
                                            );
                                        }
                                    }
                                }
                            });
                        }
                        true
                    }
                })
        });
    }

    // Render the status bar
    // @param window: The window context
    // @param cx: The application context
    // @return: The rendered status bar element
    pub(super) fn render_status_bar(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (cursor_pos, language) = match self.active_tab_index {
            Some(index) => {
                if let Some(editor_tab) = self.tabs[index].as_editor() {
                    (
                        editor_tab.content.read(cx).cursor_position(),
                        Some(editor_tab.language),
                    )
                } else {
                    (Position::default(), Some(Language::Plain))
                }
            }
            None => (Position::default(), None),
        };
        let language = match language {
            Some(language) => languages::pretty_name(language),
            None => EMPTY.to_string(),
        };
        let encoding = match self.active_tab_index {
            Some(index) => {
                if let Some(editor_tab) = self.tabs[index].as_editor() {
                    editor_tab.encoding.clone()
                } else {
                    EMPTY.to_string()
                }
            }
            None => UTF_8.to_string(),
        };
        let jump_to_line_button_content = format!(
            "Ln {}, Col {}",
            cursor_pos.line + 1,
            cursor_pos.character + 1
        );
        let jump_to_line_button = status_bar_button_factory(
            jump_to_line_button_content,
            cx.theme().border,
            cx.theme().muted,
        );
        let jump_to_line_button = jump_to_line_button.on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                this.jump_to_line(window, cx);
            }),
        );
        let language_shared = SharedString::from(language.clone());
        let language_button =
            status_bar_button_factory(language, cx.theme().border, cx.theme().muted).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    this.set_language(window, cx, language_shared.clone());
                }),
            );
        let preview_button_content = if self.show_markdown_preview {
            "Hide Preview".to_string()
        } else {
            "Show Preview".to_string()
        };
        let preview_button =
            status_bar_button_factory(preview_button_content, cx.theme().border, cx.theme().muted)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                        this.show_markdown_preview = !this.show_markdown_preview;
                        cx.notify();
                    }),
                );
        let current_tab_language = match self.active_tab_index {
            Some(index) => {
                if let Some(editor_tab) = self.tabs[index].as_editor() {
                    editor_tab.language
                } else {
                    Language::Plain
                }
            }
            None => Language::Plain,
        };
        h_flex()
            .justify_between()
            .bg(cx.theme().tab_bar)
            .py_0()
            .my_0()
            .border_t_1()
            .border_color(cx.theme().border)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .flex()
                    .justify_start()
                    .child(language_button)
                    .when(current_tab_language == Language::Markdown, |this| {
                        this.child(preview_button)
                    }),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .child(jump_to_line_button)
                    .child(status_bar_right_item_factory(encoding, cx.theme().border)),
            )
    }
}
