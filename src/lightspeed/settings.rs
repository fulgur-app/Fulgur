use gpui::*;
use gpui_component::{ActiveTheme, StyledExt, checkbox::Checkbox, v_flex};

use crate::lightspeed::Lightspeed;

#[derive(Clone)]
pub struct EditorSettings {
    pub show_line_numbers: bool,
    pub show_indent_guides: bool,
    pub soft_wrap: bool,
}

#[derive(Clone)]
pub struct AppSettings {
    pub theme: SharedString,
    pub auto_save: bool,
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
        Self {
            theme: SharedString::from("Light"),
            auto_save: false,
            confirm_exit: true,
        }
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
    pub fn new(id: usize, _window: &mut Window, _cx: &mut App) -> Self {
        Self {
            id,
            title: SharedString::from("Settings"),
        }
    }
}

impl Lightspeed {
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
            .gap_6()
            .child(
                // Header
                div()
                    .text_2xl()
                    .font_semibold()
                    .text_color(cx.theme().foreground)
                    .child("Settings"),
            )
            .child(
                v_flex()
                    .gap_6()
                    .child(
                        Checkbox::new("show_line_numbers")
                            .label("Show line numbers")
                            .checked(self.settings.editor_settings.show_line_numbers)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.settings.editor_settings.show_line_numbers = *checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new("show_indent_guides")
                            .label("Show indent guides")
                            .checked(self.settings.editor_settings.show_indent_guides)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.settings.editor_settings.show_indent_guides = *checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new("soft_wrap")
                            .label("Soft wrap")
                            .checked(self.settings.editor_settings.soft_wrap)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.settings.editor_settings.soft_wrap = *checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new("auto_save")
                            .label("Auto save")
                            .checked(self.settings.app_settings.auto_save)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.settings.app_settings.auto_save = *checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new("confirm_exit")
                            .label("Confirm exit")
                            .checked(self.settings.app_settings.confirm_exit)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.settings.app_settings.confirm_exit = *checked;
                                cx.notify();
                            })),
                    ),
            )
    }
}
