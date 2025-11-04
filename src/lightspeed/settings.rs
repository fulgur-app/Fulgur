use gpui::*;
use gpui_component::{ActiveTheme, StyledExt, v_flex};

use crate::lightspeed::{Lightspeed, components_utils::checkbox_factory};

#[derive(Clone)]
pub struct EditorSettings {
    pub show_line_numbers: bool,
    pub show_indent_guides: bool,
    pub soft_wrap: bool,
}

#[derive(Clone)]
pub struct AppSettings {
    pub confirm_exit: bool,
}

impl EditorSettings {
    pub fn new() -> Self {
        Self {
            show_line_numbers: true,
            show_indent_guides: true,
            soft_wrap: false,
        }
    }
}

impl AppSettings {
    pub fn new() -> Self {
        Self { confirm_exit: true }
    }
}

#[derive(Clone)]
pub struct Settings {
    pub editor_settings: EditorSettings,
    pub app_settings: AppSettings,
}

impl Settings {
    pub fn new() -> Self {
        Self {
            editor_settings: EditorSettings::new(),
            app_settings: AppSettings::new(),
        }
    }
}

#[derive(Clone)]
pub struct SettingsTab {
    pub id: usize,
    pub title: SharedString,
}

impl SettingsTab {
    // Create a new settings tab
    // @param id: The ID of the settings tab
    // @param window: The window
    // @param cx: The context
    // @return: The settings tab
    pub fn new(id: usize, _window: &mut Window, _cx: &mut App) -> Self {
        Self {
            id,
            title: SharedString::from("Settings"),
        }
    }
}

// Make a settings section
// @param title: The title of the settings section
// @return: The settings section
fn make_settings_section(title: &'static str) -> Div {
    v_flex().py_6().gap_3().child(div().text_xl().child(title))
}

impl Lightspeed {
    // Make the editor settings section
    // @param cx: The context
    // @return: The editor settings section
    fn make_editor_settings_section(&self, cx: &mut Context<Self>) -> Div {
        make_settings_section("Editor")
            .child(
                checkbox_factory(
                    "show_line_numbers",
                    "Show line numbers",
                    self.settings.editor_settings.show_line_numbers,
                )
                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                    this.settings.editor_settings.show_line_numbers = *checked;
                    cx.notify();
                })),
            )
            .child(
                checkbox_factory(
                    "show_indent_guides",
                    "Show indent guides",
                    self.settings.editor_settings.show_indent_guides,
                )
                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                    this.settings.editor_settings.show_indent_guides = *checked;
                    cx.notify();
                })),
            )
            .child(
                checkbox_factory(
                    "soft_wrap",
                    "Soft wrap",
                    self.settings.editor_settings.soft_wrap,
                )
                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                    this.settings.editor_settings.soft_wrap = *checked;
                    cx.notify();
                })),
            )
    }

    // Make the application settings section
    // @param cx: The context
    // @return: The application settings section
    fn make_application_settings_section(&self, cx: &mut Context<Self>) -> Div {
        make_settings_section("Application").child(
            checkbox_factory(
                "confirm_exit",
                "Confirm exit",
                self.settings.app_settings.confirm_exit,
            )
            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                this.settings.app_settings.confirm_exit = *checked;
                cx.notify();
            })),
        )
    }

    // Render the settings
    // @param window: The window
    // @param cx: The context
    // @return: The settings UI
    pub fn render_settings(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_flex()
            .min_w_128()
            .max_w_1_2()
            .mx_auto()
            .h_full()
            .p_6()
            .text_color(cx.theme().foreground)
            .text_size(px(12.0))
            .child(div().text_2xl().font_semibold().child("Settings"))
            .child(self.make_editor_settings_section(cx))
            .child(self.make_application_settings_section(cx))
    }
}
