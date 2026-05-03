use crate::fulgur::themes::{BundledThemes, themes_directory_path};
use gpui::SharedString;
use gpui_component::scroll::ScrollbarShow;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Serialize, Deserialize)]
pub struct SynchronizationSettings {
    pub is_synchronization_activated: bool,
    pub server_url: Option<String>,
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(default = "default_is_deduplication")]
    pub is_deduplication: bool,
}

impl Default for SynchronizationSettings {
    fn default() -> Self {
        Self::new()
    }
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
            public_key: None,
            is_deduplication: true,
        }
    }
}

/// Determines how the Markdown preview is displayed
#[derive(Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum MarkdownPreviewMode {
    #[default]
    DedicatedTab,
    Panel,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct MarkdownSettings {
    #[serde(default)]
    pub preview_mode: MarkdownPreviewMode,
    pub show_markdown_preview: bool,
    pub show_markdown_toolbar: bool,
}

impl Default for MarkdownSettings {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownSettings {
    /// Create a new markdown settings instance
    ///
    /// ### Returns
    /// - `MarkdownSettings`: The new markdown settings instance
    pub fn new() -> Self {
        Self {
            preview_mode: MarkdownPreviewMode::DedicatedTab,
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
    #[serde(default = "default_font_family")]
    pub font_family: String,
    pub font_size: f32,
    pub tab_size: usize,
    pub markdown_settings: MarkdownSettings,
    #[serde(default)]
    pub show_whitespaces: bool,
    #[serde(default = "default_watch_files")]
    pub watch_files: bool,
    #[serde(default = "default_use_spaces")]
    pub use_spaces: bool,
    #[serde(default = "default_highlight_colors")]
    pub highlight_colors: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub confirm_exit: bool,
    #[serde(default = "default_debug_mode")]
    pub debug_mode: bool,
    pub theme: SharedString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scrollbar_show: Option<ScrollbarShow>,
    pub synchronization_settings: SynchronizationSettings,
}

/// Default value for `debug_mode` setting
///
/// ### Returns
/// - `false`: disable debug mode by default
fn default_debug_mode() -> bool {
    false
}

/// Default value for `watch_files` setting
///
/// ### Returns
/// - `true`: enable file watcher by default
fn default_watch_files() -> bool {
    true
}

/// Default value for `use_spaces` setting
///
/// ### Returns
/// - `true`: use spaces instead of hard tabs by default
fn default_use_spaces() -> bool {
    true
}

/// Default value for `font_family` setting
fn default_font_family() -> String {
    "Monaco".to_string()
}

/// Default value for `highlight_colors` setting
///
/// ### Returns
/// - `true`: enable hex color highlighting by default
fn default_highlight_colors() -> bool {
    true
}

/// Default value for `is_deduplication` setting
///
/// ### Returns
/// - `true`: enable deduplication by default
fn default_is_deduplication() -> bool {
    true
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self::new()
    }
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
            font_family: default_font_family(),
            font_size: 14.0,
            tab_size: 4,
            show_whitespaces: false,
            markdown_settings: MarkdownSettings::new(),
            watch_files: default_watch_files(),
            use_spaces: default_use_spaces(),
            highlight_colors: default_highlight_colors(),
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self::new()
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
            debug_mode: false,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RecentFiles {
    pub(super) files: Vec<PathBuf>,
    pub(super) max_files: usize,
}

impl RecentFiles {
    /// Create a new recent files instance
    ///
    /// ### Arguments
    /// - `max_files`: The maximum number of files to store
    ///
    /// ### Returns
    /// - `RecentFiles`: The new recent files instance
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
    pub fn remove_file(&mut self, file: &PathBuf) {
        self.files.retain(|f| f != file);
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
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub editor_settings: EditorSettings,
    pub app_settings: AppSettings,
    pub recent_files: RecentFiles,
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
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
}
