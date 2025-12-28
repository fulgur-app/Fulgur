use std::{fs, path::PathBuf};

use gpui::SharedString;
use gpui_component::scroll::ScrollbarShow;
use serde::{Deserialize, Serialize};

use crate::fulgur::{
    crypto_helper,
    themes::{BundledThemes, themes_directory_path},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct SynchronizationSettings {
    pub is_synchronization_activated: bool,
    pub server_url: Option<String>,
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_key: Option<String>,
    #[serde(skip)]
    pub key: Option<String>,
}

impl SynchronizationSettings {
    /// Create a new synchronization settings instance
    ///
    /// ### Returns
    /// - `SynchronizationSettings`: The new synchronization settings instance
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
    /// Create a new markdown settings instance
    ///
    /// ### Returns
    /// - `MarkdownSettings`: The new markdown settings instance
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
    #[serde(default = "default_watch_files")]
    pub watch_files: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub confirm_exit: bool,
    pub theme: SharedString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scrollbar_show: Option<ScrollbarShow>,
    pub synchronization_settings: SynchronizationSettings,
}

/// Default value for watch_files setting
///
/// ### Returns
/// - `true`: enable file watcher by default
fn default_watch_files() -> bool {
    true
}

impl EditorSettings {
    /// Create a new editor settings instance
    ///
    /// ### Returns
    /// - `EditorSettings`: The new editor settings instance
    pub fn new() -> Self {
        Self {
            show_line_numbers: true,
            show_indent_guides: true,
            soft_wrap: false,
            font_size: 14.0,
            tab_size: 4,
            markdown_settings: MarkdownSettings::new(),
            watch_files: default_watch_files(),
        }
    }
}

impl AppSettings {
    /// Create a new app settings instance
    ///
    /// ### Returns
    /// - `AppSettings`: The new app settings instance
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
    /// Create a new recent files instance
    ///
    /// ### Arguments
    /// - `max_files`: The maximum number of files to store
    ///
    /// ### Returns
    /// - `RecentFilles`: the recent files                                                                                                                                                                                                                                                                                                                                                                                                                    lf`: The new recent files instance
    pub fn new(max_files: usize) -> Self {
        Self {
            files: Vec::new(),
            max_files,
        }
    }

    /// Add a file to the recent files
    ///
    /// ### Arguments
    /// - `file`: The file to add
    pub fn add_file(&mut self, file: PathBuf) {
        self.files.push(file);
        if self.files.len() > self.max_files {
            self.files.remove(0);
        }
    }

    /// Remove a file from the recent files
    ///
    /// ### Arguments
    /// - `file`: The file to remove
    pub fn remove_file(&mut self, file: PathBuf) {
        self.files.retain(|f| f != &file);
    }

    /// Get the recent files
    ///
    /// ### Returns
    /// - `&Vec<PathBuf>`: The recent files
    pub fn get_files(&self) -> &Vec<PathBuf> {
        &self.files
    }

    /// Clear the recent files
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
    /// Load a theme file from a path
    ///
    /// ### Arguments
    /// - `path`: The path to the theme file
    ///
    /// ### Returns
    /// - `anyhow::Result<Self>`: The theme file
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
    /// Load the theme settings from the themes folder
    ///
    /// ### Returns
    /// - `anyhow::Result<Self>`: The theme settings
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

    /// Remove a theme from the user themes
    ///
    /// ### Arguments
    /// - `theme_name`: The name of the theme to remove
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
    /// Create a new settings instance
    ///
    /// ### Returns
    /// - `Self`: The new settings instance
    pub fn new() -> Self {
        Self {
            editor_settings: EditorSettings::new(),
            app_settings: AppSettings::new(),
            recent_files: RecentFiles::new(10),
        }
    }

    /// Get the path to the settings file
    ///
    /// ### Returns
    /// - `anyhow::Result<PathBuf>`: The path to the settings file
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

    /// Save the settings to the state file
    ///
    /// ### Returns
    /// - `anyhow::Result<()>`: The result of the operation
    pub fn save(&mut self) -> anyhow::Result<()> {
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

    /// Load the settings from the state file
    ///
    /// ### Returns
    /// - `anyhow::Result<Self>`: The settings
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::settings_file_path()?;
        let json = fs::read_to_string(&path)?;
        let mut settings: Settings = serde_json::from_str(&json)?;
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
                    settings.app_settings.synchronization_settings.encrypted_key = None;
                    settings.app_settings.synchronization_settings.key = None;
                }
            }
        }

        Ok(settings)
    }

    /// Get the recent files
    ///
    /// ### Returns
    /// - `Vec<PathBuf>`: The recent files
    pub fn get_recent_files(&mut self) -> Vec<PathBuf> {
        let mut files = self.recent_files.get_files().clone();
        files.reverse();
        files
    }

    /// Add a file to the recent files
    ///
    /// ### Arguments
    /// - `file`: The file to add
    ///
    /// ### Returns
    /// - `anyhow::Result<()>`: The result of the operation
    pub fn add_file(&mut self, file: PathBuf) -> anyhow::Result<()> {
        if self.recent_files.get_files().contains(&file) {
            self.recent_files.remove_file(file.clone());
        }
        self.recent_files.add_file(file);
        self.save()
    }
}
