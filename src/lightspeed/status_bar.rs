use crate::lightspeed::Lightspeed;
use gpui::*;
use gpui_component::{ActiveTheme, h_flex, input::Position};

/// Create a status bar item
/// @param content: The content of the status bar item
/// @param border_color: The color of the border
/// @return: A status bar item
pub fn status_bar_item_factory(content: String, border_color: Hsla) -> Div {
    div()
        .text_xs()
        .px_2()
        .py_1()
        .border_color(border_color)
        .child(content)
}

/// Create a status bar right item
/// @param content: The content of the status bar right item
/// @param border_color: The color of the border
/// @return: A status bar right item
pub fn status_bar_right_item_factory(content: String, border_color: Hsla) -> impl IntoElement {
    status_bar_item_factory(content, border_color).border_l_1()
}

/// Create a status bar left item
/// @param content: The content of the status bar left item
/// @param border_color: The color of the border
/// @return: A status bar left item
pub fn status_bar_left_item_factory(content: String, border_color: Hsla) -> impl IntoElement {
    status_bar_item_factory(content, border_color).border_r_1()
}

impl Lightspeed {
    /// Render the status bar
    /// @param window: The window context
    /// @param cx: The application context
    /// @return: The rendered status bar element
    pub(super) fn render_status_bar(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let cursor_pos = match self.active_tab_index {
            Some(index) => self.tabs[index].content.read(cx).cursor_position(),
            None => Position::default(),
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
                    .child(status_bar_left_item_factory(
                        format!("Ln {}, Col {}", 132, 22),
                        cx.theme().border,
                    )),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .child(status_bar_right_item_factory(
                        format!("Ln {}, Col {}", 123, 48),
                        cx.theme().border,
                    ))
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
