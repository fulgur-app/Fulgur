use crate::lightspeed::{Lightspeed, languages};
use gpui::*;
use gpui_component::{ActiveTheme, h_flex, highlighter::Language, input::Position};

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
pub fn status_bar_left_item_factory(content: String, border_color: Hsla) -> impl IntoElement {
    status_bar_item_factory(content, border_color) //.border_r_1()
}

impl Lightspeed {
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
                        editor_tab.language,
                    )
                } else {
                    (Position::default(), Language::Plain)
                }
            }
            None => (Position::default(), Language::Plain),
        };

        let language = languages::pretty_name(language);

        let encoding = match self.active_tab_index {
            Some(index) => {
                if let Some(editor_tab) = self.tabs[index].as_editor() {
                    editor_tab.encoding.clone()
                } else {
                    "N/A".to_string()
                }
            }
            None => "UTF-8".to_string(),
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
                    .child(status_bar_left_item_factory(language, cx.theme().border)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .child(status_bar_right_item_factory(encoding, cx.theme().border))
                    .child(status_bar_right_item_factory(
                        format!(
                            "Ln {}, Col {}",
                            cursor_pos.line + 1,
                            cursor_pos.character + 1
                        ),
                        cx.theme().border,
                    )),
            )
    }
}
