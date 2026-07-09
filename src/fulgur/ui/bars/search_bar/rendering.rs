use super::{SearchBar, search_bar_button_factory, search_bar_toggle_button_factory};
use crate::fulgur::ui::{
    components_utils::{CORNERS_SIZE, LINE_HEIGHT, SEARCH_BAR_HEIGHT, TEXT_SIZE},
    icons::CustomIcon,
};
use gpui::{Context, Div, IntoElement, ParentElement, Render, Styled, Window, div};
use gpui_component::{ActiveTheme, StyledExt, input::Input};

impl Render for SearchBar {
    /// Render the search bar
    ///
    /// ### Arguments
    /// - `_window`: The window to render the search bar in
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered search bar, or an empty element when hidden
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.show_search {
            return div().into_any_element();
        }
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
            .child(Self::render_search_close_button(cx))
            .into_any_element()
    }
}

impl SearchBar {
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
                            self.match_case,
                        )
                        .line_height(LINE_HEIGHT)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.match_case = !this.match_case;
                            let content = this.active_editor_content(cx);
                            this.perform_search(content, window, cx);
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
                        .line_height(LINE_HEIGHT)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.match_whole_word = !this.match_whole_word;
                            let content = this.active_editor_content(cx);
                            this.perform_search(content, window, cx);
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
                    cx.theme().border,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    let content = this.active_editor_content(cx);
                    this.search_previous(content, window, cx);
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
                    let content = this.active_editor_content(cx);
                    this.search_next(content, window, cx);
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
                            cx.theme().border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            let content = this.active_editor_content(cx);
                            this.replace_current(content, window, cx);
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
                            let content = this.active_editor_content(cx);
                            this.replace_all(content, window, cx);
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
    fn render_search_close_button(cx: &mut Context<Self>) -> Div {
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
                .on_click(cx.listener(|this, _, _window, cx| {
                    let content = this.active_editor_content(cx);
                    this.close(content, cx);
                })),
            )
    }
}
