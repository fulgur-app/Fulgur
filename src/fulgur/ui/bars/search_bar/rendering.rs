use super::actions::MatchStep;
use super::field::render_field;
use super::{SearchBar, search_bar_button_factory, search_bar_toggle_button_factory};
use crate::fulgur::ui::{
    components_utils::{LINE_HEIGHT, SEARCH_BAR_HEIGHT},
    icons::CustomIcon,
};
use gpui_kit::component::{
    ActiveTheme,
    input::{Enter, Escape},
};
use gpui_kit::{
    Context, Div, InteractiveElement, IntoElement, ParentElement, Render, Styled, Window, div,
    prelude::FluentBuilder,
};

impl Render for SearchBar {
    /// Render the search bar
    ///
    /// ### Arguments
    /// - `_window`: The window to render the search bar in
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered search bar
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            .on_action(cx.listener(|this, _: &Escape, _window, cx| this.close(cx)))
            .child(self.render_search_input_section(cx))
            .child(self.render_search_navigation_section(cx))
            .when(self.editor_is_editable(cx), |this| {
                this.child(self.render_replace_section(cx))
            })
            .child(Self::render_search_close_button(cx))
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
                render_field("search-input", &self.search_input, cx).capture_action(cx.listener(
                    |this, enter: &Enter, _window, cx| {
                        cx.stop_propagation();
                        let step = if enter.shift {
                            MatchStep::Previous
                        } else {
                            MatchStep::Next
                        };
                        this.step_match(step, cx);
                    },
                )),
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
                        .on_click(cx.listener(|this, _, _window, cx| {
                            this.match_case = !this.match_case;
                            this.perform_search(cx);
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
                        .on_click(cx.listener(|this, _, _window, cx| {
                            this.match_whole_word = !this.match_whole_word;
                            this.perform_search(cx);
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
                .on_click(cx.listener(|this, _, _window, cx| {
                    this.step_match(MatchStep::Previous, cx);
                })),
            )
            .child(
                search_bar_button_factory(
                    "search-next-button",
                    "Next",
                    CustomIcon::ChevronDown,
                    cx.theme().tab_bar,
                )
                .on_click(cx.listener(|this, _, _window, cx| {
                    this.step_match(MatchStep::Next, cx);
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
                render_field("replace-input", &self.replace_input, cx)
                    .capture_action(|_: &Enter, _window, cx| cx.stop_propagation()),
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
                    this.close(cx);
                })),
            )
    }
}
