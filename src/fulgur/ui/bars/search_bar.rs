use crate::fulgur::{
    Fulgur,
    ui::components_utils::{
        CORNERS_SIZE, LINE_HEIGHT, SEARCH_BAR_BUTTON_SIZE, SEARCH_BAR_HEIGHT, TEXT_SIZE,
        button_factory,
    },
    ui::icons::CustomIcon,
};
use gpui::*;
use gpui_component::{ActiveTheme, StyledExt, button::Button, input::Input};

/// Create a search bar button
///
/// ### Arguments
/// - `id`: The ID of the button
/// - `tooltip`: The tooltip of the button
/// - `icon`: The icon of the button
/// - `border_color`: The color of the border
///
/// ### Returns
/// - `Button`: A search bar button
pub fn search_bar_button_factory(
    id: &'static str,
    tooltip: &'static str,
    icon: CustomIcon,
    border_color: Hsla,
) -> Button {
    button_factory(id, tooltip, icon, border_color)
        .h(SEARCH_BAR_BUTTON_SIZE)
        .w(SEARCH_BAR_BUTTON_SIZE)
}

/// Create a search bar toggle button
///
/// ### Arguments
/// - `id`: The ID of the button
/// - `tooltip`: The tooltip of the button
/// - `icon`: The icon of the button
/// - `border_color`: The color of the border
/// - `background_color`: The background color when inactive
/// - `accent_color`: The background color when active
/// - `checked`: Whether the toggle is checked
///
/// ### Returns
/// - `Button`: A search bar toggle button
pub fn search_bar_toggle_button_factory(
    id: &'static str,
    tooltip: &'static str,
    icon: CustomIcon,
    border_color: Hsla,
    background_color: Hsla,
    accent_color: Hsla,
    checked: bool,
) -> Button {
    let mut button = search_bar_button_factory(id, tooltip, icon, border_color);
    if checked {
        button = button.bg(accent_color);
    } else {
        button = button.bg(background_color);
    }
    button
}

impl Fulgur {
    /// Render the search bar
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(Div)`: The rendered search bar element
    /// - `None`: If the search bar is not shown
    pub fn render_search_bar(&self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.search_state.show_search {
            return None;
        }
        Some(
            div()
                .flex()
                .justify_between()
                .items_center()
                .bg(cx.theme().tab_bar)
                .p_0()
                .m_0()
                .w_full()
                .h(SEARCH_BAR_HEIGHT)
                .border_t_1()
                .border_color(cx.theme().border)
                .child(self.render_search_input_section(cx))
                .child(self.render_search_navigation_section(cx))
                .child(self.render_replace_section(cx))
                .child(self.render_search_close_button(cx)),
        )
    }

    /// Render the search input section (left part of search bar)
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Div`: The rendered search input section element
    fn render_search_input_section(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .p_0()
            .m_0()
            .flex_1()
            .h(SEARCH_BAR_HEIGHT)
            .bg(cx.theme().background)
            .text_color(cx.theme().muted_foreground)
            .child(
                Input::new(&self.search_state.search_input)
                    .flex_1()
                    .text_size(TEXT_SIZE)
                    .line_height(LINE_HEIGHT)
                    .m_0()
                    .py_0()
                    .pl_2()
                    .pr_0()
                    .h(SEARCH_BAR_HEIGHT)
                    .border_0()
                    .corner_radii(CORNERS_SIZE)
                    .text_color(cx.theme().muted_foreground)
                    .bg(cx.theme().background),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .p_0()
                    .m_0()
                    .h(SEARCH_BAR_HEIGHT)
                    .line_height(LINE_HEIGHT)
                    .border_l_1()
                    .border_color(cx.theme().border)
                    .text_color(cx.theme().muted_foreground)
                    .bg(cx.theme().tab_bar)
                    .child(
                        search_bar_toggle_button_factory(
                            "match-case-button",
                            "Match case",
                            CustomIcon::CaseSensitive,
                            cx.theme().border,
                            cx.theme().tab_bar,
                            cx.theme().accent,
                            self.search_state.match_case,
                        )
                        .line_height(LINE_HEIGHT)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.search_state.match_case = !this.search_state.match_case;
                            this.perform_search(window, cx);
                        })),
                    )
                    .child(
                        search_bar_toggle_button_factory(
                            "match-whole-word-button",
                            "Match whole word",
                            CustomIcon::WholeWord,
                            cx.theme().border,
                            cx.theme().tab_bar,
                            cx.theme().accent,
                            self.search_state.match_whole_word,
                        )
                        .line_height(LINE_HEIGHT)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.search_state.match_whole_word = !this.search_state.match_whole_word;
                            this.perform_search(window, cx);
                        })),
                    ),
            )
    }

    /// Render the search navigation section (match count and prev/next buttons)
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Div`: The rendered search navigation section element
    fn render_search_navigation_section(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .p_0()
            .m_0()
            .child(
                div()
                    .text_xs()
                    .px_2()
                    .text_color(cx.theme().muted_foreground)
                    .child(if self.search_state.search_matches.is_empty() {
                        "No matches".to_string()
                    } else if let Some(current) = self.search_state.current_match_index {
                        format!("{} of {}", current + 1, self.search_state.search_matches.len())
                    } else {
                        format!("{} matches", self.search_state.search_matches.len())
                    }),
            )
            .child(
                search_bar_button_factory(
                    "search-previous-button",
                    "Previous",
                    CustomIcon::ChevronUp,
                    cx.theme().border,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.search_previous(window, cx);
                })),
            )
            .child(
                search_bar_button_factory(
                    "search-next-button",
                    "Next",
                    CustomIcon::ChevronDown,
                    cx.theme().tab_bar,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.search_next(window, cx);
                })),
            )
    }

    /// Render the replace section (replace input and buttons)
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Div`: The rendered replace section element
    fn render_replace_section(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .p_0()
            .m_0()
            .flex_1()
            .h(SEARCH_BAR_HEIGHT)
            .bg(cx.theme().background)
            .text_color(cx.theme().muted_foreground)
            .border_l_1()
            .border_color(cx.theme().border)
            .child(
                Input::new(&self.search_state.replace_input)
                    .flex_1()
                    .text_size(TEXT_SIZE)
                    .line_height(LINE_HEIGHT)
                    .m_0()
                    .py_0()
                    .px_2()
                    .h(SEARCH_BAR_HEIGHT)
                    .border_0()
                    .corner_radii(CORNERS_SIZE)
                    .text_color(cx.theme().muted_foreground)
                    .bg(cx.theme().background),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .p_0()
                    .m_0()
                    .h(SEARCH_BAR_HEIGHT)
                    .bg(cx.theme().tab_bar)
                    .text_color(cx.theme().muted_foreground)
                    .border_l_1()
                    .border_color(cx.theme().border)
                    .child(
                        search_bar_button_factory(
                            "replace-button",
                            "Replace",
                            CustomIcon::Replace,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.replace_current(window, cx);
                        })),
                    )
                    .child(
                        search_bar_button_factory(
                            "replace-all-button",
                            "Replace all",
                            CustomIcon::ReplaceAll,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.replace_all(window, cx);
                        })),
                    ),
            )
    }

    /// Render the close button for the search bar
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Div`: The rendered close button element
    fn render_search_close_button(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .p_0()
            .m_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .child(
                search_bar_button_factory(
                    "close-search-button",
                    "Close",
                    CustomIcon::Close,
                    cx.theme().border,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_search(window, cx);
                })),
            )
    }
}
