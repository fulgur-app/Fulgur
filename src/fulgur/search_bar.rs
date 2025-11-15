use crate::fulgur::{
    Fulgur,
    components_utils::{
        self, CORNERS_SIZE, LINE_HEIGHT, SEARCH_BAR_HEIGHT, TEXT_SIZE, button_factory,
    },
    icons::CustomIcon,
};
use gpui::*;
use gpui_component::{ActiveTheme, StyledExt, button::Button, input::Input};

// Create a search bar button
// @param id: The ID of the button
// @param tooltip: The tooltip of the button
// @param icon: The icon of the button
// @param border_color: The color of the border
// @return: A search bar button
pub fn search_bar_button_factory(
    id: &'static str,
    tooltip: &'static str,
    icon: CustomIcon,
    _background_color: Hsla,
    border_color: Hsla,
) -> Button {
    button_factory(id, tooltip, icon, border_color)
}

// Create a search bar toggle button
// @param id: The ID of the button
// @param tooltip: The tooltip of the button
// @param icon: The icon of the button
// @param border_color: The color of the border
// @param bg_color: The background color when active
// @param checked: Whether the toggle is checked
// @return: A search bar toggle button
pub fn search_bar_toggle_button_factory(
    id: &'static str,
    tooltip: &'static str,
    icon: CustomIcon,
    border_color: Hsla,
    background_color: Hsla,
    accent_color: Hsla,
    checked: bool,
) -> Button {
    let mut button = components_utils::button_factory(id, tooltip, icon, border_color);
    if checked {
        button = button.bg(accent_color);
    } else {
        button = button.bg(background_color);
    }
    button
}

impl Fulgur {
    // Render the search bar
    // @param window: The window context
    // @param cx: The application context
    // @return: The rendered search bar element (wrapped in Option)
    pub(super) fn render_search_bar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        if !self.show_search {
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
                .child(self.render_search_navigation_section(window, cx))
                .child(self.render_replace_section(window, cx))
                .child(self.render_search_close_button(window, cx)),
        )
    }

    // Render the search input section (left part of search bar)
    // @param cx: The application context
    // @return: The rendered search input section element
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
                Input::new(&self.search_input)
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
                            self.match_case,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.match_case = !this.match_case;
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
                            self.match_whole_word,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.match_whole_word = !this.match_whole_word;
                            this.perform_search(window, cx);
                        })),
                    ),
            )
    }

    // Render the search navigation section (match count and prev/next buttons)
    // @param _window: The window context
    // @param cx: The application context
    // @return: The rendered search navigation section element
    fn render_search_navigation_section(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
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
                    .child(if self.search_matches.is_empty() {
                        "No matches".to_string()
                    } else if let Some(current) = self.current_match_index {
                        format!("{} of {}", current + 1, self.search_matches.len())
                    } else {
                        format!("{} matches", self.search_matches.len())
                    }),
            )
            .child(
                search_bar_button_factory(
                    "search-previous-button",
                    "Previous",
                    CustomIcon::ChevronUp,
                    cx.theme().tab_bar,
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
                    cx.theme().border,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.search_next(window, cx);
                })),
            )
    }

    // Render the replace section (replace input and buttons)
    // @param _window: The window context
    // @param cx: The application context
    // @return: The rendered replace section element
    fn render_replace_section(&self, _window: &mut Window, cx: &mut Context<Self>) -> Div {
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
                Input::new(&self.replace_input)
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
                            cx.theme().tab_bar,
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
                            cx.theme().tab_bar,
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.replace_all(window, cx);
                        })),
                    ),
            )
    }

    // Render the close button for the search bar
    // @param _window: The window context
    // @param cx: The application context
    // @return: The rendered close button element
    fn render_search_close_button(&self, _window: &mut Window, cx: &mut Context<Self>) -> Div {
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
                    cx.theme().tab_bar,
                    cx.theme().border,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.close_search(window, cx);
                })),
            )
    }
}
