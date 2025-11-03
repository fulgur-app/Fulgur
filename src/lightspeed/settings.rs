use gpui::*;
use gpui_component::{ActiveTheme, StyledExt, checkbox::Checkbox, h_flex, v_flex};

#[derive(Clone)]
pub struct SettingsTab {
    pub id: usize,
    pub title: SharedString,
    pub show_line_numbers: bool,
    pub show_indent_guides: bool,
    pub soft_wrap: bool,
    pub auto_save: bool,
    pub confirm_exit: bool,
}

impl SettingsTab {
    pub fn new(id: usize, _window: &mut Window, _cx: &mut App) -> Self {
        Self {
            id,
            title: SharedString::from("Settings"),
            show_line_numbers: true,
            show_indent_guides: true,
            soft_wrap: false,
            auto_save: false,
            confirm_exit: true,
        }
    }

    pub fn render(&self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Get current theme name
        let current_theme = cx.theme().theme_name().clone();

        v_flex()
            .w_full()
            .h_full()
            .p_6()
            .gap_6()
            .bg(cx.theme().background)
            .child(
                // Header
                div()
                    .text_2xl()
                    .font_semibold()
                    .text_color(cx.theme().foreground)
                    .child("Settings"),
            )
    }
}
