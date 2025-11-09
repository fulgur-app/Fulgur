use std::{fs, path::PathBuf};

use gpui::*;
use gpui_component::{
    ActiveTheme, IndexPath, StyledExt,
    dropdown::{Dropdown, DropdownEvent, DropdownState},
    h_flex,
    scroll::ScrollbarShow,
    switch::Switch,
    v_flex,
};
use serde::{Deserialize, Serialize};

use crate::fulgur::Fulgur;

#[derive(Clone, Serialize, Deserialize)]
pub struct EditorSettings {
    pub show_line_numbers: bool,
    pub show_indent_guides: bool,
    pub soft_wrap: bool,
    pub font_size: f32,
    pub tab_size: usize,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub confirm_exit: bool,
    pub theme: SharedString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scrollbar_show: Option<ScrollbarShow>,
}

impl EditorSettings {
    pub fn new() -> Self {
        Self {
            show_line_numbers: true,
            show_indent_guides: true,
            soft_wrap: false,
            font_size: 14.0,
            tab_size: 4,
        }
    }
}

impl AppSettings {
    pub fn new() -> Self {
        Self {
            confirm_exit: true,
            theme: "Default Light".into(),
            scrollbar_show: None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub editor_settings: EditorSettings,
    pub app_settings: AppSettings,
}

impl Settings {
    // Create a new settings instance
    // @return: The new settings instance
    pub fn new() -> Self {
        Self {
            editor_settings: EditorSettings::new(),
            app_settings: AppSettings::new(),
        }
    }

    // Get the path to the settings file
    // @return: The path to the settings file
    fn settings_file_path() -> anyhow::Result<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            let app_data = std::env::var("APPDATA")?;
            let mut path = PathBuf::from(app_data);
            path.push("Fulgur");
            fs::create_dir_all(&path)?;
            path.push("settings.json");
            Ok(path)
        }

        #[cfg(not(target_os = "windows"))]
        {
            let home = std::env::var("HOME")?;
            let mut path = PathBuf::from(home);
            path.push(".fulgur");
            fs::create_dir_all(&path)?;
            path.push("settings.json");
            Ok(path)
        }
    }

    // Save the settings to the state file
    // @return: The result of the operation
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::settings_file_path()?;
        let json = serde_json::to_string_pretty(&self)?;
        fs::write(path, json)?;
        Ok(())
    }

    // Load the settings from the state file
    // @return: The settings
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::settings_file_path()?;
        let json = fs::read_to_string(path)?;
        let settings: Settings = serde_json::from_str(&json)?;
        Ok(settings)
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

// Create the a dropdown state
// @param settings: The current settings
// @param window: The window
// @param cx: The app context
// @return: The dropdown state entity
pub fn create_dropdown(
    _settings: &Settings,
    window: &mut Window,
    current_value: String,
    options: Vec<SharedString>,
    cx: &mut App,
) -> Entity<DropdownState<Vec<SharedString>>> {
    let selected_index = options.iter().position(|s| s.as_ref() == current_value);
    cx.new(|cx| {
        DropdownState::new(
            options,
            selected_index.map(|i| IndexPath::default().row(i)),
            window,
            cx,
        )
    })
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
    create_dropdown(settings, window, current_font_size, font_sizes, cx)
}

// Subscribe to font size dropdown changes
// @param dropdown: The dropdown state entity
// @param cx: The context
// @return: The subscription
pub fn subscribe_to_font_size_changes(
    dropdown: &Entity<DropdownState<Vec<SharedString>>>,
    cx: &mut Context<Fulgur>,
) -> Subscription {
    cx.subscribe(
        dropdown,
        |this, _dropdown, event: &DropdownEvent<Vec<SharedString>>, cx| {
            if let DropdownEvent::Confirm(Some(selected)) = event {
                if let Ok(size) = selected.parse::<f32>() {
                    this.settings.editor_settings.font_size = size;
                    if let Err(e) = this.settings.save() {
                        eprintln!("Failed to save settings: {}", e);
                    }
                    cx.notify();
                }
            }
        },
    )
}

// Create the tab size dropdown state
// @param settings: The current settings
// @param window: The window
// @param cx: The app context
// @return: The dropdown state entity
pub fn create_tab_size_dropdown(
    settings: &Settings,
    window: &mut Window,
    cx: &mut App,
) -> Entity<DropdownState<Vec<SharedString>>> {
    let tab_sizes: Vec<SharedString> = vec![
        "2".into(),
        "4".into(),
        "6".into(),
        "8".into(),
        "10".into(),
        "12".into(),
        "42".into(),
    ];
    let current_tab_size = settings.editor_settings.tab_size.to_string();
    create_dropdown(settings, window, current_tab_size, tab_sizes, cx)
}

// Subscribe to font size dropdown changes
// @param dropdown: The dropdown state entity
// @param cx: The context
// @return: The subscription
pub fn subscribe_to_tab_size_changes(
    dropdown: &Entity<DropdownState<Vec<SharedString>>>,
    cx: &mut Context<Fulgur>,
) -> Subscription {
    cx.subscribe(
        dropdown,
        |this, _dropdown, event: &DropdownEvent<Vec<SharedString>>, cx| {
            if let DropdownEvent::Confirm(Some(selected)) = event {
                if let Ok(size) = selected.parse::<usize>() {
                    this.settings.editor_settings.tab_size = size;
                    if let Err(e) = this.settings.save() {
                        eprintln!("Failed to save settings: {}", e);
                    }
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
    cx: &mut Context<Fulgur>,
    on_click_function: fn(&mut Fulgur, &bool, &mut Context<Fulgur>),
) -> Div {
    div().child(Switch::new(id).checked(checked).on_click(cx.listener(
        move |this, checked: &bool, _, cx| {
            on_click_function(this, checked, cx);
            this.settings_changed = true;
            if let Err(e) = this.settings.save() {
                eprintln!("Failed to save settings: {}", e);
            }
            cx.notify();
        },
    )))
}

// Make a settings section
// @param title: The title of the settings section
// @return: The settings section
fn make_settings_section(title: &'static str) -> Div {
    v_flex().gap_3().child(div().text_xl().px_3().child(title))
}

// Make a setting description
// @param title: The title of the setting
// @param description: The description of the setting
// @param cx: The context
// @return: The setting description
fn make_setting_description(
    title: &'static str,
    description: &'static str,
    cx: &mut Context<Fulgur>,
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
                .min_w_0()
                .overflow_hidden()
                .child(div().font_semibold().child(title))
                .child(
                    div()
                        .max_w_full()
                        .line_height(relative(1.4))
                        .overflow_x_hidden()
                        .child(description),
                ),
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
    cx: &mut Context<Fulgur>,
    on_click_function: fn(&mut Fulgur, &bool, &mut Context<Fulgur>),
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
    cx: &mut Context<Fulgur>,
) -> Div {
    make_setting_description(title, description, cx)
        .child(div().child(Dropdown::new(state).w(px(120.)).bg(cx.theme().background)))
}

impl Fulgur {
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
            .child(make_dropdown_option(
                "Spaces for indentation",
                "The number of spaces for indentation. Fulgur must be restarted to apply the changes.",
                &self.tab_size_dropdown,
                cx,
            ))
            .child(make_toggle_option(
                "show_line_numbers",
                "Show line numbers",
                "Show the line numbers in the editor",
                self.settings.editor_settings.show_line_numbers,
                cx,
                |this, checked, _cx| {
                    this.settings.editor_settings.show_line_numbers = *checked;
                },
            ))
            .child(make_toggle_option(
                "show_indent_guides",
                "Show indent guides",
                "Show the vertical lines that indicate the indentation of the text",
                self.settings.editor_settings.show_indent_guides,
                cx,
                |this, checked, _cx| {
                    this.settings.editor_settings.show_indent_guides = *checked;
                },
            ))
            .child(make_toggle_option(
                "soft_wrap",
                "Soft wrap",
                "Wraps the text to the next line when the line is too long",    
                self.settings.editor_settings.soft_wrap,
                cx,
                |this, checked, _cx| {
                    this.settings.editor_settings.soft_wrap = *checked;
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
            |this, checked, _cx| {
                this.settings.app_settings.confirm_exit = *checked;
            },
        ))
    }

    // Render the settings
    // @param window: The window
    // @param cx: The context
    // @return: The settings UI
    pub fn render_settings(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().h_full().w_full().scrollable(Axis::Vertical).child(
            v_flex()
                .w(px(680.0))
                .mx_auto()
                .py_6()
                .text_color(cx.theme().foreground)
                .text_size(px(12.0))
                .gap_6()
                .child(div().text_2xl().font_semibold().px_3().child("Settings"))
                .child(self.make_editor_settings_section(window, cx))
                .child(self.make_application_settings_section(cx)),
        )
    }
}
