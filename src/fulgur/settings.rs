use std::{fs, path::PathBuf};

use gpui::*;
use gpui_component::{
    ActiveTheme, Sizable, Size, StyledExt,
    button::Button,
    group_box::GroupBoxVariant,
    h_flex,
    scroll::ScrollbarShow,
    select::{SelectEvent, SelectState},
    setting::{
        NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage,
        Settings as SettingsComponent,
    },
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
    pub default_show_markdown_preview: bool,
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
            default_show_markdown_preview: true,
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
    #[allow(dead_code)]
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

impl Fulgur {
    // Create the Editor settings page
    // @param entity: The Fulgur entity
    // @return: The Editor settings page
    fn create_editor_page(entity: Entity<Self>) -> SettingPage {
        let default_editor_settings = EditorSettings::new();
        SettingPage::new("Editor").default_open(true).groups(vec![
            SettingGroup::new().title("Font").items(vec![
                SettingItem::new(
                    "Font Size",
                    SettingField::number_input(
                        NumberFieldOptions {
                            min: 8.0,
                            max: 24.0,
                            step: 2.0,
                            ..Default::default()
                        },
                        {
                            let entity = entity.clone();
                            move |cx: &App| {
                                entity.read(cx).settings.editor_settings.font_size as f64
                            }
                        },
                        {
                            let entity = entity.clone();
                            move |val: f64, cx: &mut App| {
                                entity.update(cx, |this, _cx| {
                                    this.settings.editor_settings.font_size = val as f32;
                                    this.settings_changed = true;
                                    if let Err(e) = this.settings.save() {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_editor_settings.font_size as f64),
                )
                .description("Adjust the font size for the editor (8-24)."),
            ]),
            SettingGroup::new().title("Indentation").items(vec![
                SettingItem::new(
                    "Tab Size",
                    SettingField::number_input(
                        NumberFieldOptions {
                            min: 2.0,
                            max: 12.0,
                            step: 2.0,
                            ..Default::default()
                        },
                        {
                            let entity = entity.clone();
                            move |cx: &App| entity.read(cx).settings.editor_settings.tab_size as f64
                        },
                        {
                            let entity = entity.clone();
                            move |val: f64, cx: &mut App| {
                                entity.update(cx, |this, _cx| {
                                    this.settings.editor_settings.tab_size = val as usize;
                                    this.settings_changed = true;
                                    if let Err(e) = this.settings.save() {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_editor_settings.tab_size as f64),
                )
                .description("Number of spaces for indentation. Requires restart."),
                SettingItem::new(
                    "Show Indent Guides",
                    SettingField::switch(
                        {
                            let entity = entity.clone();
                            move |cx: &App| {
                                entity.read(cx).settings.editor_settings.show_indent_guides
                            }
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, _cx| {
                                    this.settings.editor_settings.show_indent_guides = val;
                                    this.settings_changed = true;
                                    if let Err(e) = this.settings.save() {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_editor_settings.show_indent_guides),
                )
                .description("Show vertical lines indicating indentation levels."),
            ]),
            SettingGroup::new().title("Display").items(vec![
                SettingItem::new(
                    "Show Line Numbers",
                    SettingField::switch(
                        {
                            let entity = entity.clone();
                            move |cx: &App| {
                                entity.read(cx).settings.editor_settings.show_line_numbers
                            }
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, _cx| {
                                    this.settings.editor_settings.show_line_numbers = val;
                                    this.settings_changed = true;
                                    if let Err(e) = this.settings.save() {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_editor_settings.show_line_numbers),
                )
                .description("Display line numbers in the editor gutter."),
                SettingItem::new(
                    "Soft Wrap",
                    SettingField::switch(
                        {
                            let entity = entity.clone();
                            move |cx: &App| entity.read(cx).settings.editor_settings.soft_wrap
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, _cx| {
                                    this.settings.editor_settings.soft_wrap = val;
                                    this.settings_changed = true;
                                    if let Err(e) = this.settings.save() {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_editor_settings.soft_wrap),
                )
                .description("Wrap long lines to the next line instead of scrolling."),
            ]),
            SettingGroup::new().title("Markdown").items(vec![
                SettingItem::new(
                    "Default Show Preview",
                    SettingField::switch(
                        {
                            let entity = entity.clone();
                            move |cx: &App| {
                                entity
                                    .read(cx)
                                    .settings
                                    .editor_settings
                                    .default_show_markdown_preview
                            }
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, _cx| {
                                    this.settings.editor_settings.default_show_markdown_preview =
                                        val;
                                    this.settings_changed = true;
                                    if let Err(e) = this.settings.save() {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_editor_settings.default_show_markdown_preview),
                )
                .description("Show markdown preview by default when opening .md files."),
            ]),
        ])
    }

    // Create the Application settings page
    // @param entity: The Fulgur entity
    // @return: The Application settings page
    fn create_application_page(entity: Entity<Self>) -> SettingPage {
        let default_app_settings = AppSettings::new();

        SettingPage::new("Application").groups(vec![SettingGroup::new().title("General").items(
            vec![
                    SettingItem::new(
                        "Confirm Exit",
                        SettingField::switch(
                            {
                                let entity = entity.clone();
                                move |cx: &App| entity.read(cx).settings.app_settings.confirm_exit
                            },
                            {
                                let entity = entity.clone();
                                move |val: bool, cx: &mut App| {
                                    entity.update(cx, |this, _cx| {
                                        this.settings.app_settings.confirm_exit = val;
                                        if let Err(e) = this.settings.save() {
                                            log::error!("Failed to save settings: {}", e);
                                        }
                                    });
                                }
                            },
                        )
                        .default_value(default_app_settings.confirm_exit),
                    )
                    .description("Show confirmation dialog before exiting the application."),
                ],
        )])
    }

    // Create the Themes settings page
    // @param entity: The Fulgur entity
    // @param themes: The themes to display
    // @return: The Themes settings page
    fn create_themes_page(entity: Entity<Self>, themes: &Themes) -> SettingPage {
        let mut user_theme_items = Vec::new();
        let mut default_theme_items = Vec::new();
        for theme in &themes.user_themes {
            let theme_name = theme.name.clone();
            let theme_author = theme.author.clone();
            let theme_path = theme.path.clone();
            let themes_info = theme
                .themes
                .iter()
                .map(|t| format!("{} ({})", t.name, t.mode))
                .collect::<Vec<String>>()
                .join(", ");
            let button_id = format!("delete-theme-{}", theme_name);
            let button_id_static: &'static str = Box::leak(button_id.into_boxed_str());
            user_theme_items.push(SettingItem::render({
                let entity = entity.clone();
                move |_options, _window, cx| {
                    let theme_path = theme_path.clone();
                    let entity_clone = entity.clone();
                    h_flex()
                        .w_full()
                        .justify_between()
                        .gap_3()
                        .child(
                            v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .font_semibold()
                                        .child(format!("{} by {}", theme_name, theme_author)),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(themes_info.clone()),
                                ),
                        )
                        .child(
                            Button::new(button_id_static)
                                .icon(CustomIcon::Close)
                                .small()
                                .cursor_pointer()
                                .on_click(move |_, _window, cx| {
                                    if let Err(e) = fs::remove_file(&theme_path) {
                                        log::error!(
                                            "Failed to delete theme file {}: {}",
                                            theme_path.display(),
                                            e
                                        );
                                    } else {
                                        log::info!("Deleted theme file: {:?}", theme_path);
                                    }
                                    let entity_for_update = entity_clone.clone();
                                    entity_clone.update(cx, |this, cx| {
                                        themes::reload_themes_and_update(
                                            &this.settings,
                                            entity_for_update,
                                            cx,
                                        );
                                    });
                                }),
                        )
                        .into_any_element()
                }
            }));
        }
        for theme in &themes.default_themes {
            let theme_name = theme.name.clone();
            let theme_author = theme.author.clone();
            let themes_info = theme
                .themes
                .iter()
                .map(|t| format!("{} ({})", t.name, t.mode))
                .collect::<Vec<String>>()
                .join(", ");
            default_theme_items.push(SettingItem::render(move |_options, _window, cx| {
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .font_semibold()
                            .child(format!("{} by {} (Default)", theme_name, theme_author)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(themes_info.clone()),
                    )
                    .into_any_element()
            }));
        }
        let mut groups = Vec::new();
        if !user_theme_items.is_empty() {
            groups.push(
                SettingGroup::new()
                    .title("User Themes")
                    .items(user_theme_items),
            );
        }
        if !default_theme_items.is_empty() {
            groups.push(
                SettingGroup::new()
                    .title("Default Themes")
                    .items(default_theme_items),
            );
        }
        SettingPage::new("Themes").groups(groups)
    }

    // Create settings pages using the Settings component
    // @param window: The window
    // @param cx: The context
    // @return: The settings pages
    fn create_settings_pages(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<SettingPage> {
        let entity = cx.entity();
        let mut pages = vec![
            Self::create_editor_page(entity.clone()),
            Self::create_application_page(entity.clone()),
        ];
        if let Some(ref themes) = self.themes {
            pages.push(Self::create_themes_page(entity, themes));
        }
        pages
    }

    // Render the settings
    // @param window: The window
    // @param cx: The context
    // @return: The settings UI
    pub fn render_settings(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("settings-scroll-container")
            .size_full()
            .overflow_x_scroll()
            .child(
                v_flex()
                    .w(px(980.0))
                    .h_full()
                    .mx_auto()
                    .py_6()
                    .text_color(cx.theme().foreground)
                    .text_size(px(12.0))
                    .gap_6()
                    .child(div().text_2xl().font_semibold().px_3().child("Settings"))
                    .child(
                        SettingsComponent::new("fulgur-settings")
                            .with_size(Size::Medium)
                            .with_group_variant(GroupBoxVariant::Outline)
                            .pages(self.create_settings_pages(window, cx)),
                    )
                    .mb_24(),
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
