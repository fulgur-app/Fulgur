use gpui::*;
use gpui_component::{
    ActiveTheme, IndexPath, StyledExt,
    dropdown::{Dropdown, DropdownEvent, DropdownState},
    h_flex,
    switch::Switch,
    v_flex,
};

use crate::lightspeed::Lightspeed;

#[derive(Clone)]
pub struct EditorSettings {
    pub show_line_numbers: bool,
    pub show_indent_guides: bool,
    pub soft_wrap: bool,
    pub font_size: f32,
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
            font_size: 14.0,
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

// Create the font size dropdown state
// @param settings: The current settings
// @param window: The window
// @param cx: The app context
// @return: The dropdown state entity
pub fn create_font_size_dropdown(
    settings: &Settings,
    window: &mut Window,
    cx: &mut App,
) -> Entity<DropdownState<Vec<SharedString>>> {
    let font_sizes: Vec<SharedString> = vec![
        "8".into(),
        "10".into(),
        "12".into(),
        "14".into(),
        "16".into(),
        "18".into(),
        "20".into(),
    ];

    // Find the index of the current font size
    let current_font_size = settings.editor_settings.font_size.to_string();
    let selected_index = font_sizes
        .iter()
        .position(|s| s.as_ref() == current_font_size);

    cx.new(|cx| {
        DropdownState::new(
            font_sizes,
            selected_index.map(|i| IndexPath::default().row(i)),
            window,
            cx,
        )
    })
}

// Subscribe to font size dropdown changes
// @param dropdown: The dropdown state entity
// @param cx: The context
// @return: The subscription
pub fn subscribe_to_font_size_changes(
    dropdown: &Entity<DropdownState<Vec<SharedString>>>,
    cx: &mut Context<Lightspeed>,
) -> Subscription {
    cx.subscribe(
        dropdown,
        |this, _dropdown, event: &DropdownEvent<Vec<SharedString>>, cx| {
            if let DropdownEvent::Confirm(Some(selected)) = event {
                if let Ok(size) = selected.parse::<f32>() {
                    this.settings.editor_settings.font_size = size;
                    cx.notify();
                }
            }
        },
    )
}

// Make a switch
// @param id: The ID of the switch
// @param checked: Whether the switch is checked
// @param cx: The context
// @param on_click_function: The function to call when the switch is clicked
// @return: The switch
fn make_switch(
    id: &'static str,
    checked: bool,
    cx: &mut Context<Lightspeed>,
    on_click_function: fn(&mut Lightspeed, &bool, &mut Context<Lightspeed>),
) -> Switch {
    Switch::new(id)
        .checked(checked)
        .on_click(cx.listener(move |this, checked: &bool, _, cx| {
            on_click_function(this, checked, cx);
        }))
}

// Make a settings section
// @param title: The title of the settings section
// @return: The settings section
fn make_settings_section(title: &'static str) -> Div {
    v_flex()
        .py_6()
        .gap_1()
        .child(div().text_xl().px_2().child(title))
}

// Make a setting description
// @param title: The title of the setting
// @param description: The description of the setting
// @param cx: The context
// @return: The setting description
fn make_setting_description(
    title: &'static str,
    description: &'static str,
    cx: &mut Context<Lightspeed>,
) -> Div {
    h_flex()
        .w_full()
        .px_3()
        .py_2()
        .bg(cx.theme().muted)
        .rounded(px(2.0))
        .child(
            v_flex()
                .text_size(px(14.0))
                .flex_1()
                .child(div().font_semibold().child(title))
                .child(description),
        )
}

// Make a toggle option
// @param id: The ID of the toggle option
// @param title: The title of the toggle option
// @param description: The description of the toggle option
// @param checked: Whether the toggle option is checked
// @param cx: The context
// @param on_click_function: The function to call when the toggle option is clicked
// @return: The toggle option
fn make_toggle_option(
    id: &'static str,
    title: &'static str,
    description: &'static str,
    checked: bool,
    cx: &mut Context<Lightspeed>,
    on_click_function: fn(&mut Lightspeed, &bool, &mut Context<Lightspeed>),
) -> Div {
    make_setting_description(title, description, cx).child(make_switch(
        id,
        checked,
        cx,
        on_click_function,
    ))
}

// Make a dropdown option
// @param title: The title of the dropdown option
// @param description: The description of the dropdown option
// @param state: The state of the dropdown option
// @param cx: The context
// @return: The dropdown option
fn make_dropdown_option(
    title: &'static str,
    description: &'static str,
    state: &Entity<DropdownState<Vec<SharedString>>>,
    cx: &mut Context<Lightspeed>,
) -> Div {
    h_flex()
        .w_full()
        .px_3()
        .py_2()
        .bg(cx.theme().muted)
        .rounded(px(2.0))
        .child(
            v_flex()
                .text_size(px(14.0))
                .flex_1()
                .child(div().font_semibold().child(title))
                .child(description),
        )
        .child(div().child(Dropdown::new(state).w(px(120.)).bg(cx.theme().background)))
}

impl Lightspeed {
    // Make a switch
    // @param id: The ID of the switch
    // @param checked: Whether the switch is checked
    // @param cx: The context
    // @param on_click_function: The function to call when the switch is clicked
    // @return: The switch
    // fn make_switch(
    //     &self,
    //     id: &'static str,
    //     checked: bool,
    //     cx: &mut Context<Self>,
    //     on_click_function: fn(&mut Lightspeed, &bool, &mut Context<Self>),
    // ) -> Switch {
    //     Switch::new(id).checked(checked).on_click(cx.listener(
    //         move |this, checked: &bool, _, cx| {
    //             on_click_function(this, checked, cx);
    //         },
    //     ))
    // }

    // fn make_setting_description(
    //     &self,
    //     title: &'static str,
    //     description: &'static str,
    //     cx: &mut Context<Self>,
    // ) -> Div {
    //     h_flex()
    //         .w_full()
    //         .px_3()
    //         .py_2()
    //         .bg(cx.theme().muted)
    //         .rounded(px(2.0))
    //         .child(
    //             v_flex()
    //                 .text_size(px(14.0))
    //                 .flex_1()
    //                 .child(div().font_semibold().child(title))
    //                 .child(description),
    //         )
    // }

    // fn make_toggle_option(
    //     &self,
    //     id: &'static str,
    //     title: &'static str,
    //     description: &'static str,
    //     checked: bool,
    //     cx: &mut Context<Self>,
    //     on_click_function: fn(&mut Lightspeed, &bool, &mut Context<Self>),
    // ) -> Div {
    //     make_setting_description(title, description, cx).child(make_switch(
    //         id,
    //         checked,
    //         cx,
    //         on_click_function,
    //     ))
    // }

    // fn make_dropdown_option(
    //     &self,
    //     title: &'static str,
    //     description: &'static str,
    //     state: &Entity<DropdownState<Vec<SharedString>>>,
    //     cx: &mut Context<Self>,
    // ) -> Div {
    //     h_flex()
    //         .w_full()
    //         .px_3()
    //         .py_2()
    //         .bg(cx.theme().muted)
    //         .rounded(px(2.0))
    //         .child(
    //             v_flex()
    //                 .text_size(px(14.0))
    //                 .flex_1()
    //                 .child(div().font_semibold().child(title))
    //                 .child(description),
    //         )
    //         .child(div().child(Dropdown::new(state).w(px(120.)).bg(cx.theme().background)))
    // }

    // Make the editor settings section
    // @param cx: The context
    // @return: The editor settings section
    fn make_editor_settings_section(&self, _window: &mut Window, cx: &mut Context<Self>) -> Div {
        make_settings_section("Editor")
            .child(make_dropdown_option(
                "Font size",
                "The size of the font in the editor",
                &self.font_size_dropdown,
                cx,
            ))
            .child(make_toggle_option(
                "show_line_numbers",
                "Show line numbers",
                "Show the line numbers in the editor",
                self.settings.editor_settings.show_line_numbers,
                cx,
                |this, checked, cx| {
                    this.settings.editor_settings.show_line_numbers = *checked;
                    cx.notify();
                },
            ))
            .child(make_toggle_option(
                "show_indent_guides",
                "Show indent guides",
                "Show ithe vertical lines that indicate the indentation of the text",
                self.settings.editor_settings.show_indent_guides,
                cx,
                |this, checked, cx| {
                    this.settings.editor_settings.show_indent_guides = *checked;
                    cx.notify();
                },
            ))
            .child(make_toggle_option(
                "soft_wrap",
                "Soft wrap",
                "Wraps the text to the next line when the line is too long",
                self.settings.editor_settings.soft_wrap,
                cx,
                |this, checked, cx| {
                    this.settings.editor_settings.soft_wrap = *checked;
                    cx.notify();
                },
            ))
    }

    // Make the application settings section
    // @param cx: The context
    // @return: The application settings section
    fn make_application_settings_section(&self, cx: &mut Context<Self>) -> Div {
        make_settings_section("Application").child(make_toggle_option(
            "confirm_exit",
            "Confirm exit",
            "Confirm before exiting the application",
            self.settings.app_settings.confirm_exit,
            cx,
            |this, checked, cx| {
                this.settings.app_settings.confirm_exit = *checked;
                cx.notify();
            },
        ))
    }

    // Render the settings
    // @param window: The window
    // @param cx: The context
    // @return: The settings UI
    pub fn render_settings(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .min_w_128()
            .max_w_1_2()
            .mx_auto()
            .h_full()
            .p_6()
            .text_color(cx.theme().foreground)
            .text_size(px(12.0))
            .child(div().text_2xl().font_semibold().px_2().child("Settings"))
            .child(self.make_editor_settings_section(window, cx))
            .child(self.make_application_settings_section(cx))
    }
}
