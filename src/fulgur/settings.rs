use std::{fs, path::PathBuf};

use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    ActiveTheme, StyledExt,
    button::Button,
    h_flex,
    scroll::ScrollbarShow,
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
    v_flex,
};
use serde::{Deserialize, Serialize};

use crate::fulgur::{
    Fulgur,
    components_utils::create_select_state,
    icons::CustomIcon,
    menus::build_menus,
    themes::{self, BundledThemes, themes_directory_path},
};

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
pub struct RecentFiles {
    files: Vec<PathBuf>,
    max_files: usize,
}

impl RecentFiles {
    // Create a new recent files instance
    // @param max_files: The maximum number of files to store
    // @return: The new recent files instance
    pub fn new(max_files: usize) -> Self {
        Self {
            files: Vec::new(),
            max_files,
        }
    }

    // Add a file to the recent files
    // @param file: The file to add
    // @return: The result of the operation
    pub fn add_file(&mut self, file: PathBuf) {
        self.files.push(file);
        if self.files.len() > self.max_files {
            self.files.remove(0);
        }
    }

    // Remove a file from the recent files
    // @param file: The file to remove
    // @return: The result of the operation
    pub fn remove_file(&mut self, file: PathBuf) {
        self.files.retain(|f| f != &file);
    }

    // Get the recent files
    // @return: The recent files
    pub fn get_files(&self) -> &Vec<PathBuf> {
        &self.files
    }

    // Clear the recent files
    // @return: The result of the operation
    pub fn clear(&mut self) {
        self.files.clear();
    }
}

#[derive(Clone, Deserialize)]
pub struct ThemeInfo {
    pub name: String,
    pub mode: String,
}

#[derive(Clone, Deserialize)]
pub struct ThemeFile {
    pub name: String,
    pub author: String,
    pub themes: Vec<ThemeInfo>,
    #[serde(skip)]
    pub path: PathBuf,
}
impl ThemeFile {
    // Load a theme file from a path
    // @param path: The path to the theme file
    // @return: The theme file
    pub fn load(path: PathBuf) -> anyhow::Result<Self> {
        let json = fs::read_to_string(&path)?;
        let mut theme_file: ThemeFile = serde_json::from_str(&json)?;
        theme_file.path = path;
        Ok(theme_file)
    }
}

#[derive(Clone)]
pub struct Themes {
    pub default_themes: Vec<ThemeFile>,
    pub user_themes: Vec<ThemeFile>,
}

impl Themes {
    // Load the theme settings from the themes folder
    // @param path: The path to the themes folder
    // @return: The theme settings
    pub fn load() -> anyhow::Result<Self> {
        let themes_dir = themes_directory_path()?;
        let themes_files = fs::read_dir(&themes_dir)?;
        let default_themes: Vec<ThemeFile> = BundledThemes::iter()
            .map(|file| ThemeFile::load(themes_dir.join(file.as_ref())))
            .collect::<Result<Vec<ThemeFile>, anyhow::Error>>()?;
        let default_themes_names = BundledThemes::iter()
            .map(|file| file.as_ref().to_string())
            .collect::<Vec<String>>();
        let user_themes: Vec<ThemeFile> = themes_files
            .filter_map(|entry| {
                entry.ok().and_then(|entry| {
                    let filename = entry.file_name().to_string_lossy().to_string();
                    if !default_themes_names.contains(&filename) {
                        Some(entry)
                    } else {
                        None
                    }
                })
            })
            .filter_map(|entry| ThemeFile::load(entry.path()).ok())
            .collect();
        Ok(Themes {
            default_themes,
            user_themes,
        })
    }

    // Remove a theme from the user themes
    // @param theme_name: The name of the theme to remove
    // @return: The result of the operation
    pub fn remove_theme(&mut self, theme_name: String) {
        self.user_themes.retain(|theme| theme.name != theme_name);
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub editor_settings: EditorSettings,
    pub app_settings: AppSettings,
    pub recent_files: RecentFiles,
}

impl Settings {
    // Create a new settings instance
    // @return: The new settings instance
    pub fn new() -> Self {
        Self {
            editor_settings: EditorSettings::new(),
            app_settings: AppSettings::new(),
            recent_files: RecentFiles::new(10),
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

    // Get the recent files
    // @return: The recent files
    pub fn get_recent_files(&mut self) -> Vec<PathBuf> {
        let mut files = self.recent_files.get_files().clone();
        files.reverse();
        files
    }

    // Add a file to the recent files
    // @param file: The file to add
    pub fn add_file(&mut self, file: PathBuf) -> anyhow::Result<()> {
        if self.recent_files.get_files().contains(&file) {
            self.recent_files.remove_file(file.clone());
        }
        self.recent_files.add_file(file);
        self.save()
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
) -> Entity<SelectState<Vec<SharedString>>> {
    let font_sizes: Vec<SharedString> = vec![
        "8".into(),
        "10".into(),
        "12".into(),
        "14".into(),
        "16".into(),
        "18".into(),
        "20".into(),
    ];
    let current_font_size = settings.editor_settings.font_size.to_string();
    create_select_state(window, current_font_size, font_sizes, cx)
}

// Subscribe to font size dropdown changes
// @param select: The select state entity
// @param cx: The context
// @return: The subscription
pub fn subscribe_to_font_size_changes(
    select: &Entity<SelectState<Vec<SharedString>>>,
    cx: &mut Context<Fulgur>,
) -> Subscription {
    cx.subscribe(
        select,
        |this: &mut Fulgur,
         _select: Entity<SelectState<Vec<SharedString>>>,
         event: &SelectEvent<Vec<SharedString>>,
         cx: &mut Context<Fulgur>| {
            if let SelectEvent::Confirm(Some(selected)) = event {
                if let Ok(size) = selected.parse::<f32>() {
                    this.settings.editor_settings.font_size = size;
                    if let Err(e) = this.settings.save() {
                        log::error!("Failed to save settings: {}", e);
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
) -> Entity<SelectState<Vec<SharedString>>> {
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
    create_select_state(window, current_tab_size, tab_sizes, cx)
}

// Subscribe to font size dropdown changes
// @param select: The select state entity
// @param cx: The context
// @return: The subscription
pub fn subscribe_to_tab_size_changes(
    select: &Entity<SelectState<Vec<SharedString>>>,
    cx: &mut Context<Fulgur>,
) -> Subscription {
    cx.subscribe(
        &select,
        |this: &mut Fulgur,
         _select: Entity<SelectState<Vec<SharedString>>>,
         event: &SelectEvent<Vec<SharedString>>,
         cx: &mut Context<Fulgur>| {
            if let SelectEvent::Confirm(Some(selected)) = event {
                if let Ok(size) = selected.parse::<f32>() {
                    this.settings.editor_settings.tab_size = size as usize;
                    if let Err(e) = this.settings.save() {
                        log::error!("Failed to save settings: {}", e);
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
    id: impl Into<SharedString>,
    checked: bool,
    cx: &mut Context<Fulgur>,
    on_click_function: fn(&mut Fulgur, &bool, &mut Context<Fulgur>),
) -> Div {
    let id_str: SharedString = id.into();
    let id_static: &'static str = Box::leak(id_str.to_string().into_boxed_str());
    div().child(
        Switch::new(id_static)
            .checked(checked)
            .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                on_click_function(this, checked, cx);
                this.settings_changed = true;
                if let Err(e) = this.settings.save() {
                    log::error!("Failed to save settings: {}", e);
                }
                cx.notify();
            })),
    )
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
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
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
                .child(div().font_semibold().child(title.into()))
                .child(
                    div()
                        .max_w_full()
                        .line_height(relative(1.4))
                        .overflow_x_hidden()
                        .child(description.into()),
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
    id: impl Into<SharedString>,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    checked: bool,
    cx: &mut Context<Fulgur>,
    on_click_function: fn(&mut Fulgur, &bool, &mut Context<Fulgur>),
) -> Div {
    let id_str = id.into();
    make_setting_description(title, description, cx).child(make_switch(
        id_str,
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
    state: &Entity<SelectState<Vec<SharedString>>>,
    cx: &mut Context<Fulgur>,
) -> Div {
    make_setting_description(title, description, cx)
        .child(div().child(Select::new(state).w(px(120.)).bg(cx.theme().background)))
}

// Make a theme section
// @param title: The title of the theme section
// @return: The theme section
fn make_theme_section(title: &'static str) -> Div {
    v_flex().gap_3().child(div().text_xl().px_3().child(title))
}

// Make a theme option
// @param theme: The theme
// @param cx: The context
// @param is_default: Whether the theme is a default theme
// @return: The theme option
fn make_theme_option(theme: &ThemeFile, cx: &mut Context<Fulgur>, is_default: bool) -> Div {
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
                .child(
                    div()
                        .h_flex()
                        .w_full()
                        .when(is_default, |div| {
                            div.font_semibold().child(format!(
                                "{} by {} (Default theme)",
                                theme.name.clone(),
                                theme.author.clone()
                            ))
                        })
                        .when(!is_default, |div| {
                            div.font_semibold().child(format!(
                                "{} by {}",
                                theme.name.clone(),
                                theme.author.clone()
                            ))
                        }),
                )
                .child(
                    div()
                        .max_w_full()
                        .line_height(relative(1.4))
                        .overflow_x_hidden()
                        .child(
                            theme
                                .themes
                                .iter()
                                .map(|theme| {
                                    format!("{} ({})", theme.name.clone(), theme.mode.clone())
                                })
                                .collect::<Vec<String>>()
                                .join(", "),
                        ),
                ),
        )
        .when(!is_default, |div| {
            let theme_name = theme.name.clone();
            let theme_path = theme.path.clone();
            let button_id: SharedString = format!("Delete_{}", theme_name.clone()).into();
            div.child(
                Button::new(button_id)
                    .icon(CustomIcon::Close)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _window, cx| {
                        if let Err(e) = fs::remove_file(&theme_path) {
                            log::error!(
                                "Failed to delete theme file {}: {}",
                                theme_path.display(),
                                e
                            );
                        } else {
                            log::info!("Deleted theme file: {:?}", theme_path);
                        }
                        themes::reload_themes_and_update(&this.settings, cx.entity(), cx);
                        cx.notify();
                    })),
            )
        })
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

    // Make the themes section
    // @param cx: The context
    // @return: The themes section
    fn make_themes_section(&self, cx: &mut Context<Self>) -> Div {
        if self.themes.is_none() {
            return div();
        }
        let themes = self.themes.as_ref().unwrap();
        make_theme_section("Themes")
            .children(
                themes
                    .user_themes
                    .iter()
                    .map(|theme| make_theme_option(theme, cx, false))
                    .collect::<Vec<Div>>(),
            )
            .children(
                themes
                    .default_themes
                    .iter()
                    .map(|theme| make_theme_option(theme, cx, true))
                    .collect::<Vec<Div>>(),
            )
    }

    // Render the settings
    // @param window: The window
    // @param cx: The context
    // @return: The settings UI
    pub fn render_settings(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // let scroll_handle = window
        //     .use_keyed_state("settings-scroll-handle", cx, |_, _| ScrollHandle::default())
        //     .read(cx)
        //     .clone();

        div().id("settings-scroll-container").size_full().child(
            v_flex()
                .w(px(680.0))
                .mx_auto()
                .py_6()
                .text_color(cx.theme().foreground)
                .text_size(px(12.0))
                .gap_6()
                .child(div().text_2xl().font_semibold().px_3().child("Settings"))
                .child(self.make_editor_settings_section(window, cx))
                .child(self.make_application_settings_section(cx))
                .child(self.make_themes_section(cx).mb_24()),
        )
    }

    // Clear the recent files
    // @param window: The window
    // @param cx: The context
    // @return: The result of the operation
    pub fn clear_recent_files(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.settings.recent_files.clear();
        if let Err(e) = self.settings.save() {
            log::error!("Failed to save settings: {}", e);
        }
        let menus = build_menus(cx, &self.settings.recent_files.get_files());
        cx.set_menus(menus);
    }
}
