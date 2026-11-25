//! One-row text field shared by the search and replace inputs.

use super::SearchBar;
use crate::fulgur::ui::components_utils::{
    CORNERS_SIZE, LINE_HEIGHT, SEARCH_BAR_HEIGHT, TEXT_SIZE,
};
use gpui_kit::component::{
    ActiveTheme, Sizable, StyledExt,
    input::{Textarea, TextareaState},
};
use gpui_kit::{
    App, AppContext, ClipboardItem, Context, Empty, Entity, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Role, Styled, TestSupportExt, Window, div,
};
use std::rc::Rc;

/// Create the state of a one-row search bar field
///
/// ### Arguments
/// - `placeholder`: The text shown while the field is empty
/// - `window`: The window context
/// - `cx`: The search bar context
///
/// ### Returns
/// - `Entity<TextareaState>`: The state of the field
pub(super) fn new_field(
    placeholder: &'static str,
    window: &mut Window,
    cx: &mut Context<SearchBar>,
) -> Entity<TextareaState> {
    cx.new(|cx| {
        let mut state = TextareaState::new(window, cx)
            .placeholder(placeholder)
            .auto_grow(1, 1);
        state.set_soft_wrap(false, window, cx);
        // A scrollbar would cover the only row of text.
        state.set_scrollbar_renderer(Some(Rc::new(|_, _, _| Empty.into_any_element())));
        state
    })
}

/// Render a one-row search bar field
///
/// ### Arguments
/// - `id`: The element id of the field
/// - `state`: The state of the field
/// - `cx`: The application context
///
/// ### Returns
/// - `impl InteractiveElement + IntoElement`: The field, filling the height of the search bar
pub(super) fn render_field(
    id: &'static str,
    state: &Entity<TextareaState>,
    cx: &App,
) -> impl InteractiveElement + IntoElement + use<> {
    let pasted_into = state.clone();
    let clicked = state.clone();
    div()
        .id(id)
        .test_support()
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .h(SEARCH_BAR_HEIGHT)
        .cursor_text()
        // The textarea is only as tall as its row, so the space around it takes clicks too.
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            window.prevent_default();
            clicked.update(cx, |state, cx| state.focus(window, cx));
        })
        .child(
            Textarea::new(state)
                // The small size is the one whose horizontal padding matches the bar.
                .small()
                .role(Role::TextInput)
                .appearance(false)
                .bordered(false)
                .on_paste(move |item, window, cx| paste_on_one_line(&pasted_into, item, window, cx))
                .text_size(TEXT_SIZE)
                .line_height(LINE_HEIGHT)
                .corner_radii(CORNERS_SIZE)
                .text_color(cx.theme().muted_foreground)
                .bg(cx.theme().background),
        )
}

/// Paste clipboard text spanning several lines without its line breaks
///
/// ### Description
/// The field shows a single row, so a pasted line break would hide the text around it.
///
/// ### Arguments
/// - `state`: The field receiving the text
/// - `item`: The clipboard content
/// - `window`: The window context
/// - `cx`: The application context
///
/// ### Returns
/// - `true`: The text was pasted here without its line breaks
/// - `false`: The text has no line break, so the field must paste it itself
fn paste_on_one_line(
    state: &Entity<TextareaState>,
    item: &ClipboardItem,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let Some(text) = item.text().filter(|text| text.contains(['\n', '\r'])) else {
        return false;
    };
    state.update(cx, |state, cx| {
        state.replace(text.replace(['\n', '\r'], ""), window, cx);
    });
    true
}
