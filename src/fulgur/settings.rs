use std::{fs, io::Write, path::PathBuf};

use fs2::FileExt;
use gpui::SharedString;
use gpui_component::scroll::ScrollbarShow;
use serde::{Deserialize, Serialize};

use crate::fulgur::themes::{BundledThemes, themes_directory_path};

#[derive(Clone, Serialize, Deserialize)]
pub struct SynchronizationSettings {
    pub is_synchronization_activated: bool,
    pub server_url: Option<String>,
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(default = "default_is_deduplication")]
    pub is_deduplication: bool,
    //#[serde(skip_serializing_if = "Option::is_none")]
    //pub encrypted_key: Option<String>,
    //#[serde(skip)]
    //pub key: Option<String>,
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

/// Default value for is_deduplication setting
///
/// ### Returns
/// - `true`: enable deduplication by default
fn default_is_deduplication() -> bool {
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
    /// - `Ok(PathBuf)`: The path to the settings file
    /// - `Err(anyhow::Error)`: If there was an error getting the path
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

    /// Save the settings to a specific path
    ///
    /// ### Description
    /// Core implementation for saving settings. Can be used with custom paths
    /// for testing or alternative storage locations. Uses file locking to prevent
    /// corruption when multiple windows write simultaneously.
    ///
    /// ### Arguments
    /// - `path`: The path to save the settings to
    ///
    /// ### Returns
    /// - `Ok(())`: The result of the operation
    /// - `Err(anyhow::Error)`: If there was an error saving the settings
    pub fn save_to_path(&mut self, path: &PathBuf) -> anyhow::Result<()> {
        // Serialize settings to JSON first (fast, no I/O)
        let json = serde_json::to_string_pretty(&self)?;

        let file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(path)
            .map_err(|e| anyhow::anyhow!("Failed to open settings file: {}", e))?;
        file.lock_exclusive()
            .map_err(|e| anyhow::anyhow!("Failed to acquire lock on settings file: {}", e))?;
        file.set_len(0)
            .map_err(|e| anyhow::anyhow!("Failed to truncate settings file: {}", e))?;
        let mut writer = std::io::BufWriter::new(&file);
        writer
            .write_all(json.as_bytes())
            .map_err(|e| anyhow::anyhow!("Failed to write settings: {}", e))?;
        writer
            .flush()
            .map_err(|e| anyhow::anyhow!("Failed to flush settings: {}", e))?;
        Ok(())
    }

    /// Load the settings from a specific path
    ///
    /// ### Description
    /// Core implementation for loading settings. Can be used with custom paths
    /// for testing or alternative storage locations. Uses shared file locking to
    /// allow concurrent reads while preventing reads during writes.
    ///
    /// ### Arguments
    /// - `path`: The path to load the settings from
    ///
    /// ### Returns
    /// - `Ok(Settings)`: The loaded settings
    /// - `Err(anyhow::Error)`: If there was an error loading the settings
    pub fn load_from_path(path: &PathBuf) -> anyhow::Result<Self> {
        let file = fs::OpenOptions::new()
            .read(true)
            .open(path)
            .map_err(|e| anyhow::anyhow!("Failed to open settings file for reading: {}", e))?;
        file.lock_shared().map_err(|e| {
            anyhow::anyhow!("Failed to acquire shared lock on settings file: {}", e)
        })?;
        let mut reader = std::io::BufReader::new(&file);
        let mut json = String::new();
        std::io::Read::read_to_string(&mut reader, &mut json)
            .map_err(|e| anyhow::anyhow!("Failed to read settings: {}", e))?;
        let settings: Settings = serde_json::from_str(&json)
            .map_err(|e| anyhow::anyhow!("Failed to parse settings: {}", e))?;
        Ok(settings)
    }

    /// Save the settings to the default state file location
    ///
    /// ### Returns
    /// - `Ok(())`: The result of the operation
    /// - `Err(anyhow::Error)`: If there was an error saving the settings
    pub fn save(&mut self) -> anyhow::Result<()> {
        let path = Self::settings_file_path()?;
        self.save_to_path(&path)
    }

    /// Load the settings from the default state file location
    ///
    /// ### Returns
    /// - `Ok(Settings)`: The loaded settings
    /// - `Err(anyhow::Error)`: If there was an error loading the settings
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::settings_file_path()?;
        Self::load_from_path(&path)
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
    /// - `Ok(())`: The result of the operation
    /// - `Err(anyhow::Error)`: If there was an error adding the file
    pub fn add_file(&mut self, file: PathBuf) -> anyhow::Result<()> {
        if self.recent_files.get_files().contains(&file) {
            self.recent_files.remove_file(file.clone());
        }
        self.recent_files.add_file(file);
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn recent_files_new_creates_empty_list_with_correct_max() {
        let recent_files = RecentFiles::new(5);
        assert_eq!(recent_files.get_files().len(), 0);
        assert_eq!(recent_files.max_files, 5);
    }

    #[test]
    fn recent_files_new_with_zero_max() {
        let recent_files = RecentFiles::new(0);
        assert_eq!(recent_files.get_files().len(), 0);
        assert_eq!(recent_files.max_files, 0);
    }

    #[test]
    fn recent_files_new_with_large_max() {
        let recent_files = RecentFiles::new(1000);
        assert_eq!(recent_files.get_files().len(), 0);
        assert_eq!(recent_files.max_files, 1000);
    }

    #[test]
    fn recent_files_add_file_below_max() {
        let mut recent_files = RecentFiles::new(5);
        let file1 = PathBuf::from("/path/to/file1.txt");
        let file2 = PathBuf::from("/path/to/file2.txt");

        recent_files.add_file(file1.clone());
        assert_eq!(recent_files.get_files().len(), 1);
        assert_eq!(recent_files.get_files()[0], file1);

        recent_files.add_file(file2.clone());
        assert_eq!(recent_files.get_files().len(), 2);
        assert_eq!(recent_files.get_files()[1], file2);
    }

    #[test]
    fn recent_files_add_file_at_max_evicts_oldest() {
        let mut recent_files = RecentFiles::new(3);
        let file1 = PathBuf::from("/path/to/file1.txt");
        let file2 = PathBuf::from("/path/to/file2.txt");
        let file3 = PathBuf::from("/path/to/file3.txt");
        let file4 = PathBuf::from("/path/to/file4.txt");

        recent_files.add_file(file1.clone());
        recent_files.add_file(file2.clone());
        recent_files.add_file(file3.clone());
        assert_eq!(recent_files.get_files().len(), 3);

        // Adding 4th file should evict file1
        recent_files.add_file(file4.clone());
        assert_eq!(recent_files.get_files().len(), 3);
        assert!(!recent_files.get_files().contains(&file1));
        assert_eq!(recent_files.get_files()[0], file2);
        assert_eq!(recent_files.get_files()[1], file3);
        assert_eq!(recent_files.get_files()[2], file4);
    }

    #[test]
    fn recent_files_lru_eviction_behavior() {
        let mut recent_files = RecentFiles::new(3);
        let file1 = PathBuf::from("/path/to/file1.txt");
        let file2 = PathBuf::from("/path/to/file2.txt");
        let file3 = PathBuf::from("/path/to/file3.txt");
        let file4 = PathBuf::from("/path/to/file4.txt");
        let file5 = PathBuf::from("/path/to/file5.txt");

        recent_files.add_file(file1.clone());
        recent_files.add_file(file2.clone());
        recent_files.add_file(file3.clone());
        recent_files.add_file(file4.clone());
        recent_files.add_file(file5.clone());

        // Should keep most recently added files (file3, file4, file5)
        assert_eq!(recent_files.get_files().len(), 3);
        assert_eq!(recent_files.get_files()[0], file3);
        assert_eq!(recent_files.get_files()[1], file4);
        assert_eq!(recent_files.get_files()[2], file5);
        assert!(!recent_files.get_files().contains(&file1));
        assert!(!recent_files.get_files().contains(&file2));
    }

    #[test]
    fn recent_files_remove_existing_file() {
        let mut recent_files = RecentFiles::new(5);
        let file1 = PathBuf::from("/path/to/file1.txt");
        let file2 = PathBuf::from("/path/to/file2.txt");
        let file3 = PathBuf::from("/path/to/file3.txt");

        recent_files.add_file(file1.clone());
        recent_files.add_file(file2.clone());
        recent_files.add_file(file3.clone());
        assert_eq!(recent_files.get_files().len(), 3);

        recent_files.remove_file(file2.clone());
        assert_eq!(recent_files.get_files().len(), 2);
        assert!(!recent_files.get_files().contains(&file2));
        assert_eq!(recent_files.get_files()[0], file1);
        assert_eq!(recent_files.get_files()[1], file3);
    }

    #[test]
    fn recent_files_remove_non_existing_file() {
        let mut recent_files = RecentFiles::new(5);
        let file1 = PathBuf::from("/path/to/file1.txt");
        let file2 = PathBuf::from("/path/to/file2.txt");
        let non_existing = PathBuf::from("/path/to/nonexisting.txt");

        recent_files.add_file(file1.clone());
        recent_files.add_file(file2.clone());
        assert_eq!(recent_files.get_files().len(), 2);

        // Should not change anything
        recent_files.remove_file(non_existing);
        assert_eq!(recent_files.get_files().len(), 2);
        assert_eq!(recent_files.get_files()[0], file1);
        assert_eq!(recent_files.get_files()[1], file2);
    }

    #[test]
    fn recent_files_remove_from_empty_list() {
        let mut recent_files = RecentFiles::new(5);
        let file1 = PathBuf::from("/path/to/file1.txt");

        // Should not panic
        recent_files.remove_file(file1);
        assert_eq!(recent_files.get_files().len(), 0);
    }

    #[test]
    fn recent_files_clear_removes_all_files() {
        let mut recent_files = RecentFiles::new(5);
        let file1 = PathBuf::from("/path/to/file1.txt");
        let file2 = PathBuf::from("/path/to/file2.txt");
        let file3 = PathBuf::from("/path/to/file3.txt");

        recent_files.add_file(file1);
        recent_files.add_file(file2);
        recent_files.add_file(file3);
        assert_eq!(recent_files.get_files().len(), 3);

        recent_files.clear();
        assert_eq!(recent_files.get_files().len(), 0);
    }

    #[test]
    fn recent_files_clear_empty_list() {
        let mut recent_files = RecentFiles::new(5);
        assert_eq!(recent_files.get_files().len(), 0);

        // Should not panic
        recent_files.clear();
        assert_eq!(recent_files.get_files().len(), 0);
    }

    #[test]
    fn settings_load_without_is_deduplication_field() {
        let json = r#"{
            "editor_settings": {
                "show_line_numbers": true,
                "show_indent_guides": true,
                "soft_wrap": false,
                "font_size": 14.0,
                "tab_size": 4,
                "markdown_settings": {
                    "show_markdown_preview": true,
                    "show_markdown_toolbar": false
                },
                "watch_files": true
            },
            "app_settings": {
                "confirm_exit": true,
                "theme": "Catppuccin Frappe",
                "synchronization_settings": {
                    "is_synchronization_activated": true,
                    "server_url": "http://localhost:3000",
                    "email": "test@example.com",
                    "public_key": "age1abc123"
                }
            },
            "recent_files": {
                "files": [],
                "max_files": 10
            }
        }"#;
        let settings: Settings = serde_json::from_str(json).unwrap();
        // is_deduplication should default to true when missing from JSON
        assert!(
            settings
                .app_settings
                .synchronization_settings
                .is_deduplication
        );
        // Other settings should be preserved
        assert_eq!(settings.app_settings.theme, "Catppuccin Frappe");
        assert!(
            settings
                .app_settings
                .synchronization_settings
                .is_synchronization_activated
        );
        assert_eq!(
            settings.app_settings.synchronization_settings.server_url,
            Some("http://localhost:3000".to_string())
        );
        assert_eq!(
            settings.app_settings.synchronization_settings.email,
            Some("test@example.com".to_string())
        );
    }
}
