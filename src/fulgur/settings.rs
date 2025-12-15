use std::{fs, path::PathBuf};

use gpui::*;
use gpui_component::{
    ActiveTheme, Sizable, Size, StyledExt, WindowExt,
    button::{Button, ButtonVariants},
    group_box::GroupBoxVariant,
    h_flex,
    notification::NotificationType,
    scroll::ScrollbarShow,
    setting::{
        NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage,
        Settings as SettingsComponent,
    },
    v_flex,
};
use serde::{Deserialize, Serialize};

use crate::fulgur::{
    Fulgur, crypto_helper,
    icons::CustomIcon,
    menus::build_menus,
    themes::{self, BundledThemes, themes_directory_path},
};

const DEVICE_KEY_PLACEHOLDER: &str = "<Device Key>";

#[derive(Clone, Serialize, Deserialize)]
pub struct SynchronizationSettings {
    pub is_synchronization_activated: bool,
    pub server_url: Option<String>,
    pub email: Option<String>,
    /// Encrypted key stored in settings.json (base64-encoded)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_key: Option<String>,
    /// Plaintext key cached in memory (not serialized)
    #[serde(skip)]
    pub key: Option<String>,
}

impl SynchronizationSettings {
    pub fn new() -> Self {
        Self {
            is_synchronization_activated: false,
            server_url: None,
            email: None,
            encrypted_key: None,
            key: None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct MarkdownSettings {
    pub show_markdown_preview: bool,
    pub show_markdown_toolbar: bool,
}

impl MarkdownSettings {
    pub fn new() -> Self {
        Self {
            show_markdown_preview: true,
            show_markdown_toolbar: false,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct EditorSettings {
    pub show_line_numbers: bool,
    pub show_indent_guides: bool,
    pub soft_wrap: bool,
    pub font_size: f32,
    pub tab_size: usize,
    pub markdown_settings: MarkdownSettings,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub confirm_exit: bool,
    pub theme: SharedString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scrollbar_show: Option<ScrollbarShow>,
    pub synchronization_settings: SynchronizationSettings,
}

impl EditorSettings {
    pub fn new() -> Self {
        Self {
            show_line_numbers: true,
            show_indent_guides: true,
            soft_wrap: false,
            font_size: 14.0,
            tab_size: 4,
            markdown_settings: MarkdownSettings::new(),
        }
    }
}

impl AppSettings {
    pub fn new() -> Self {
        Self {
            confirm_exit: true,
            theme: "Default Light".into(),
            scrollbar_show: None,
            synchronization_settings: SynchronizationSettings::new(),
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
    pub fn save(&mut self) -> anyhow::Result<()> {
        // Encrypt the key before saving
        if let Some(ref plaintext_key) = self.app_settings.synchronization_settings.key {
            if !plaintext_key.is_empty() {
                match crypto_helper::encrypt(plaintext_key) {
                    Ok(encrypted) => {
                        self.app_settings.synchronization_settings.encrypted_key = Some(encrypted);
                    }
                    Err(e) => {
                        log::error!("Failed to encrypt key: {}", e);
                        return Err(e);
                    }
                }
            } else {
                self.app_settings.synchronization_settings.encrypted_key = None;
            }
        } else {
            self.app_settings.synchronization_settings.encrypted_key = None;
        }

        let path = Self::settings_file_path()?;
        let json = serde_json::to_string_pretty(&self)?;
        fs::write(path, json)?;
        Ok(())
    }

    // Load the settings from the state file
    // @return: The settings
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::settings_file_path()?;
        let json = fs::read_to_string(&path)?;

        // Load settings
        let mut settings: Settings = serde_json::from_str(&json)?;

        // Decrypt the key if it exists
        if let Some(ref encrypted_key) =
            settings.app_settings.synchronization_settings.encrypted_key
        {
            match crypto_helper::decrypt(encrypted_key) {
                Ok(decrypted) => {
                    settings.app_settings.synchronization_settings.key = Some(decrypted);
                    log::info!("Successfully decrypted sync key");
                }
                Err(e) => {
                    log::error!("Failed to decrypt sync key: {}", e);
                    // If decryption fails, clear both encrypted and plaintext keys
                    settings.app_settings.synchronization_settings.encrypted_key = None;
                    settings.app_settings.synchronization_settings.key = None;
                }
            }
        }

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
                                    .markdown_settings
                                    .show_markdown_preview
                            }
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, _cx| {
                                    this.settings
                                        .editor_settings
                                        .markdown_settings
                                        .show_markdown_preview = val;
                                    this.settings_changed = true;
                                    if let Err(e) = this.settings.save() {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(
                        default_editor_settings
                            .markdown_settings
                            .show_markdown_preview,
                    ),
                )
                .description("Show preview when opening Markdown files."),
                SettingItem::new(
                    "Show Toolbar by default",
                    SettingField::switch(
                        {
                            let entity = entity.clone();
                            move |cx: &App| {
                                entity
                                    .read(cx)
                                    .settings
                                    .editor_settings
                                    .markdown_settings
                                    .show_markdown_toolbar
                            }
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, _cx| {
                                    this.settings
                                        .editor_settings
                                        .markdown_settings
                                        .show_markdown_toolbar = val;
                                    this.settings_changed = true;
                                    if let Err(e) = this.settings.save() {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(
                        default_editor_settings
                            .markdown_settings
                            .show_markdown_toolbar,
                    ),
                )
                .description("Show toolbar by default when opening Markdown files."),
            ]),
        ])
    }

    // Create the Application settings page
    // @param entity: The Fulgur entity
    // @return: The Application settings page
    fn create_application_page(entity: Entity<Self>) -> SettingPage {
        let default_app_settings = AppSettings::new();

        SettingPage::new("Application")
            .default_open(true)
            .groups(vec![
                SettingGroup::new().title("General").items(vec![
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
                ]),
                SettingGroup::new().title("Synchronization").items(vec![
                    SettingItem::new(
                        "Activate Synchronization",
                        SettingField::switch(
                            {
                                let entity = entity.clone();
                                move |cx: &App| {
                                    entity
                                        .read(cx)
                                        .settings
                                        .app_settings
                                        .synchronization_settings
                                        .is_synchronization_activated
                                }
                            },
                            {
                                let entity = entity.clone();
                                move |val: bool, cx: &mut App| {
                                    entity.update(cx, |this, _cx| {
                                        this.settings
                                            .app_settings
                                            .synchronization_settings
                                            .is_synchronization_activated = val;
                                        if let Err(e) = this.settings.save() {
                                            log::error!("Failed to save settings: {}", e);
                                        }
                                    });
                                }
                            },
                        )
                        .default_value(default_app_settings.confirm_exit),
                    )
                    .description("Activate synchronization with the server."),
                    SettingItem::new(
                        "Server URL",
                        SettingField::input(
                            {
                                let entity = entity.clone();
                                move |cx: &App| {
                                    entity
                                        .read(cx)
                                        .settings
                                        .app_settings
                                        .synchronization_settings
                                        .server_url
                                        .as_ref()
                                        .map(|s| SharedString::from(s.clone()))
                                        .unwrap_or_default()
                                }
                            },
                            {
                                let entity = entity.clone();
                                move |val: SharedString, cx: &mut App| {
                                    entity.update(cx, |this, _cx| {
                                        let url = if val.is_empty() {
                                            None
                                        } else {
                                            Some(val.to_string())
                                        };
                                        this.settings
                                            .app_settings
                                            .synchronization_settings
                                            .server_url = url;
                                        if let Err(e) = this.settings.save() {
                                            log::error!("Failed to save settings: {}", e);
                                        }
                                    });
                                }
                            },
                        )
                        .default_value(
                            default_app_settings
                                .synchronization_settings
                                .server_url
                                .clone()
                                .map(|s| SharedString::from(s))
                                .unwrap_or_default(),
                        ),
                    )
                    .description("URL of the synchronization server."),
                    SettingItem::new(
                        "Email",
                        SettingField::input(
                            {
                                let entity = entity.clone();
                                move |cx: &App| {
                                    entity
                                        .read(cx)
                                        .settings
                                        .app_settings
                                        .synchronization_settings
                                        .email
                                        .as_ref()
                                        .map(|s| SharedString::from(s.clone()))
                                        .unwrap_or_default()
                                }
                            },
                            {
                                let entity = entity.clone();
                                move |val: SharedString, cx: &mut App| {
                                    entity.update(cx, |this, _cx| {
                                        let email = if val.is_empty() {
                                            None
                                        } else {
                                            Some(val.to_string())
                                        };
                                        this.settings.app_settings.synchronization_settings.email =
                                            email;
                                        if let Err(e) = this.settings.save() {
                                            log::error!("Failed to save settings: {}", e);
                                        }
                                    });
                                }
                            },
                        )
                        .default_value(
                            default_app_settings
                                .synchronization_settings
                                .email
                                .clone()
                                .map(|s| SharedString::from(s))
                                .unwrap_or_default(),
                        ),
                    )
                    .description("Email for synchronization."),
                    SettingItem::new(
                        "Device Key",
                        SettingField::input(
                            move |_cx: &App| SharedString::from(DEVICE_KEY_PLACEHOLDER),
                            {
                                let entity = entity.clone();
                                move |val: SharedString, cx: &mut App| {
                                    entity.update(cx, |this, _cx| {
                                        let key = if val.is_empty() {
                                            None
                                        } else {
                                            if val.to_string() == DEVICE_KEY_PLACEHOLDER {
                                                None
                                            } else {
                                                Some(
                                                    crypto_helper::encrypt(&val.to_string())
                                                        .unwrap(),
                                                )
                                            }
                                        };
                                        this.settings.app_settings.synchronization_settings.key =
                                            key;
                                        if let Err(e) = this.settings.save() {
                                            log::error!("Failed to save settings: {}", e);
                                        }
                                    });
                                }
                            },
                        ),
                    )
                    .description("Device Key for synchronization (stored encrypted)."),
                    // SettingItem::new(
                    //     "Device Key",
                    //     SettingField::render({
                    //         let entity = entity.clone();
                    //         move |options, window, cx| {
                    //             // Get the cached key from memory (no keyring access!)
                    //             let cached_key = entity
                    //                 .read(cx)
                    //                 .settings
                    //                 .app_settings
                    //                 .synchronization_settings
                    //                 .key
                    //                 .clone()
                    //                 .unwrap_or_default();

                    //             let state_key = SharedString::from("sync-key-input");
                    //             let state = window.use_keyed_state(state_key, cx, |window, cx| {
                    //                 let input = cx.new(|cx| {
                    //                     InputState::new(window, cx)
                    //                         .masked(true)
                    //                         .default_value(&cached_key)
                    //                         .placeholder("Enter synchronization key...")
                    //                 });

                    //                 let _subscription = cx.subscribe_in(&input, window, {
                    //                     let entity = entity.clone();
                    //                     move |_, input, event: &InputEvent, _window, cx| {
                    //                         if let InputEvent::Change = event {
                    //                             let value = input.read(cx).value();
                    //                             // Update the key in memory and save to settings.json (encrypted)
                    //                             entity.update(cx, |this, _cx| {
                    //                                 if value.is_empty() {
                    //                                     this.settings
                    //                                         .app_settings
                    //                                         .synchronization_settings
                    //                                         .key = None;
                    //                                 } else {
                    //                                     this.settings
                    //                                         .app_settings
                    //                                         .synchronization_settings
                    //                                         .key = Some(value.to_string());
                    //                                 }
                    //                                 // Save settings (will encrypt the key)
                    //                                 if let Err(e) = this.settings.save() {
                    //                                     log::error!(
                    //                                         "Failed to save settings: {}",
                    //                                         e
                    //                                     );
                    //                                 }
                    //                             });
                    //                         }
                    //                     }
                    //                 });

                    //                 (input, _subscription)
                    //             });

                    //             let (input, _sub) = state.read(cx);

                    //             Input::new(&input)
                    //                 .with_size(options.size)
                    //                 .map(|this| {
                    //                     if options.layout.is_horizontal() {
                    //                         this.w_64()
                    //                     } else {
                    //                         this.w_full()
                    //                     }
                    //                 })
                    //                 .cleanable(true)
                    //                 .into_any_element()
                    //         }
                    //     }),
                    // )
                    // .description("Device key for synchronization (stored encrypted in settings)."),
                    SettingItem::render({
                        let entity = entity.clone();
                        move |_options, _window, _cx| {
                            h_flex()
                                .w_full()
                                .justify_end()
                                .mt_2()
                                .child(
                                    Button::new("test-connection-button")
                                        .label("Test Connection")
                                        .primary()
                                        .small()
                                        .cursor_pointer()
                                        .on_click({
                                            let entity = entity.clone();

                                            move |_, window, cx| {
                                                let result = entity
                                                    .read(cx)
                                                    .test_synchronization_connection();
                                                let notification = match result {
                                                    SynchronizationTestResult::Success => (
                                                        NotificationType::Success,
                                                        SharedString::from(
                                                            "Connection test successful!",
                                                        ),
                                                    ),
                                                    SynchronizationTestResult::Failure(msg) => (
                                                        NotificationType::Error,
                                                        SharedString::from(format!(
                                                            "Connection test failed: {}",
                                                            msg
                                                        )),
                                                    ),
                                                };
                                                window.push_notification(notification, cx);
                                            }
                                        }),
                                )
                                .into_any_element()
                        }
                    }),
                ]),
            ])
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
                    .text_color(cx.theme().foreground)
                    .text_size(px(12.0))
                    .border_r_1()
                    .border_color(cx.theme().border)
                    //.child(div().text_2xl().font_semibold().px_3().child("Settings"))
                    .child(
                        SettingsComponent::new("fulgur-settings")
                            .with_size(Size::Medium)
                            .with_group_variant(GroupBoxVariant::Outline)
                            .pages(self.create_settings_pages(window, cx)),
                    ),
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
        let menus = build_menus(
            cx,
            &self.settings.recent_files.get_files(),
            self.update_link.clone(),
        );
        cx.set_menus(menus);
    }

    // Test the synchronization connection
    // @return: The result of the connection test
    pub fn test_synchronization_connection(&self) -> SynchronizationTestResult {
        let server_url = self
            .settings
            .app_settings
            .synchronization_settings
            .server_url
            .clone();
        let email = self
            .settings
            .app_settings
            .synchronization_settings
            .email
            .clone();
        let key = self
            .settings
            .app_settings
            .synchronization_settings
            .key
            .clone();

        if server_url.is_none() {
            return SynchronizationTestResult::Failure("Server URL is missing".to_string());
        }
        if email.is_none() {
            return SynchronizationTestResult::Failure("Email is missing".to_string());
        }
        if key.is_none() {
            return SynchronizationTestResult::Failure("Key is missing".to_string());
        }
        let decrypted_key = crypto_helper::decrypt(&key.unwrap()).unwrap();
        let ping_url = format!("{}/api/ping", server_url.unwrap());
        log::debug!("Ping URL: {:?}", ping_url);
        let response = ureq::get(&ping_url)
            .header("Authorization", &format!("Bearer {}", decrypted_key))
            .header("X-User-Email", &email.unwrap())
            .call();
        if response.is_ok() {
            return SynchronizationTestResult::Success;
        } else {
            log::error!("Connection test failed: {}", response.unwrap_err());
            return SynchronizationTestResult::Failure("Connection test failed".to_string());
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum SynchronizationTestResult {
    Success,
    Failure(String),
}
