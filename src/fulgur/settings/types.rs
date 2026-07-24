use crate::fulgur::themes::{BundledThemes, themes_directory_path};
use gpui::SharedString;
use gpui_component::scroll::ScrollbarShow;
use serde::{Deserialize, Deserializer, Serialize};
use std::{fs, path::PathBuf};

/// Stable identifier for a server profile. Generated as a UUID v4 string
/// at profile creation and never reused.
pub type ProfileId = String;

/// Maximum number of server profiles a user can configure.
pub const MAX_PROFILES: usize = 16;

/// Default profile name used when creating a new profile through migration
/// of legacy single-server config.
pub const DEFAULT_LEGACY_PROFILE_NAME: &str = "Fulgurant";

/// Generate a new unique profile id.
///
/// ### Returns
/// - `ProfileId`: A freshly generated UUID v4 string.
#[must_use]
pub fn new_profile_id() -> ProfileId {
    uuid::Uuid::new_v4().to_string()
}

/// Default name used when a profile is deserialized without a name field
/// (only happens with hand-edited config files).
///
/// ### Returns
/// - `String`: The default legacy profile name.
fn default_profile_name() -> String {
    DEFAULT_LEGACY_PROFILE_NAME.to_string()
}

/// Configuration for a single Fulgurant sync server.
///
/// ### Fields
/// - `id`: Stable UUID v4 string assigned at creation.
/// - `name`: Human-readable label shown in the settings UI.
/// - `is_active`: Per-profile activation flag (independent of the master switch).
/// - `server_url`: Sync server URL, or `None` if not yet configured.
/// - `email`: Login email for the sync server, or `None`.
/// - `public_key`: X25519 public key advertised to the server. Paired with
///   the per-profile private key stored in the system keychain.
/// - `is_deduplication`: When true, the server deduplicates shares of the
///   same file path.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerProfile {
    pub id: ProfileId,
    #[serde(default = "default_profile_name")]
    pub name: String,
    #[serde(default)]
    pub is_active: bool,
    pub server_url: Option<String>,
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(default = "default_is_deduplication")]
    pub is_deduplication: bool,
}

impl ServerProfile {
    /// Create a new empty profile with a freshly generated id.
    ///
    /// ### Arguments
    /// - `name`: The display name to assign to the new profile.
    ///
    /// ### Returns
    /// - `Self`: A new profile with default values.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: new_profile_id(),
            name: name.into(),
            is_active: false,
            server_url: None,
            email: None,
            public_key: None,
            is_deduplication: default_is_deduplication(),
        }
    }
}

/// Top-level synchronization configuration.
#[derive(Clone, Serialize)]
pub struct SynchronizationSettings {
    pub is_synchronization_activated: bool,
    #[serde(default)]
    pub profiles: Vec<ServerProfile>,
    #[serde(skip)]
    pub migrated_from_legacy: bool,
}

impl PartialEq for SynchronizationSettings {
    fn eq(&self, other: &Self) -> bool {
        self.is_synchronization_activated == other.is_synchronization_activated
            && self.profiles == other.profiles
    }
}

impl Default for SynchronizationSettings {
    fn default() -> Self {
        Self::new()
    }
}

impl SynchronizationSettings {
    /// Create a new empty synchronization settings instance.
    ///
    /// ### Returns
    /// - `SynchronizationSettings`: An empty configuration with sync disabled
    ///   and no profiles.
    #[must_use]
    pub fn new() -> Self {
        Self {
            is_synchronization_activated: false,
            profiles: Vec::new(),
            migrated_from_legacy: false,
        }
    }

    /// Get a reference to the first profile, if any.
    ///
    /// ### Returns
    /// - `Some(&ServerProfile)`: The first profile when at least one is configured.
    /// - `None`: When no profiles are configured.
    #[must_use]
    pub fn primary_profile(&self) -> Option<&ServerProfile> {
        self.profiles.first()
    }

    /// Get a mutable reference to the first profile, if any.
    ///
    /// ### Returns
    /// - `Some(&mut ServerProfile)`: The first profile when at least one is configured.
    /// - `None`: When no profiles are configured.
    pub fn primary_profile_mut(&mut self) -> Option<&mut ServerProfile> {
        self.profiles.first_mut()
    }

    /// Get a mutable reference to the first profile, creating one if needed.
    ///
    /// ### Returns
    /// - `&mut ServerProfile`: The first profile, creating a new "Fulgurant"
    ///   profile if the list was empty.
    pub fn ensure_primary_profile_mut(&mut self) -> &mut ServerProfile {
        if self.profiles.is_empty() {
            self.profiles
                .push(ServerProfile::new(DEFAULT_LEGACY_PROFILE_NAME));
        }
        &mut self.profiles[0]
    }

    /// Look up a profile by id.
    ///
    /// ### Arguments
    /// - `profile_id`: The id to search for.
    ///
    /// ### Returns
    /// - `Some(&ServerProfile)`: The matching profile.
    /// - `None`: When no profile with that id exists.
    #[must_use]
    pub fn find_profile(&self, profile_id: &str) -> Option<&ServerProfile> {
        self.profiles.iter().find(|p| p.id == profile_id)
    }

    /// Look up a profile by id with mutable access.
    ///
    /// ### Arguments
    /// - `profile_id`: The id to search for.
    ///
    /// ### Returns
    /// - `Some(&mut ServerProfile)`: The matching profile.
    /// - `None`: When no profile with that id exists.
    pub fn find_profile_mut(&mut self, profile_id: &str) -> Option<&mut ServerProfile> {
        self.profiles.iter_mut().find(|p| p.id == profile_id)
    }

    /// Check whether a name is already used by another profile.
    ///
    /// ### Arguments
    /// - `candidate`: The candidate display name.
    /// - `exclude_id`: Profile id to skip during the comparison.
    ///
    /// ### Returns
    /// - `true`: At least one other profile carries the same normalized name.
    /// - `false`: The name is unique among the other profiles.
    #[must_use]
    pub fn name_collides(&self, candidate: &str, exclude_id: Option<&str>) -> bool {
        let normalized = candidate.trim().to_lowercase();
        self.profiles.iter().any(|profile| {
            if exclude_id == Some(profile.id.as_str()) {
                return false;
            }
            profile.name.trim().to_lowercase() == normalized
        })
    }
}

/// Custom deserializer that accepts both the legacy single-server JSON shape
/// and the new multi-profile shape.
///
/// ### Description
/// When the `profiles` field is present, the new shape is used as-is. When
/// it is absent, the legacy fields are migrated into a single profile named
/// `"Fulgurant"` if any of them carry data; otherwise an empty `profiles`
/// list is produced.
impl<'de> Deserialize<'de> for SynchronizationSettings {
    //TODO: remove legacy support in 0.10.0
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            #[serde(default)]
            is_synchronization_activated: bool,
            // New shape
            profiles: Option<Vec<ServerProfile>>,
            // Legacy fields (single-server shape)
            #[serde(default)]
            server_url: Option<String>,
            #[serde(default)]
            email: Option<String>,
            #[serde(default)]
            public_key: Option<String>,
            #[serde(default)]
            is_deduplication: Option<bool>,
        }

        let helper = Helper::deserialize(deserializer)?;
        if let Some(profiles) = helper.profiles {
            return Ok(Self {
                is_synchronization_activated: helper.is_synchronization_activated,
                profiles,
                migrated_from_legacy: false,
            });
        }

        let has_legacy_data = helper.server_url.is_some()
            || helper.email.is_some()
            || helper.public_key.is_some()
            || helper.is_deduplication.is_some();
        let profiles = if has_legacy_data {
            vec![ServerProfile {
                id: new_profile_id(),
                name: DEFAULT_LEGACY_PROFILE_NAME.to_string(),
                is_active: helper.is_synchronization_activated,
                server_url: helper.server_url,
                email: helper.email,
                public_key: helper.public_key,
                is_deduplication: helper
                    .is_deduplication
                    .unwrap_or_else(default_is_deduplication),
            }]
        } else {
            Vec::new()
        };
        Ok(Self {
            is_synchronization_activated: helper.is_synchronization_activated,
            profiles,
            migrated_from_legacy: has_legacy_data,
        })
    }
}

/// Determines how the Markdown preview is displayed
#[derive(Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum MarkdownPreviewMode {
    #[default]
    DedicatedTab,
    Panel,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
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
    #[must_use]
    pub fn new() -> Self {
        Self {
            preview_mode: MarkdownPreviewMode::DedicatedTab,
            show_markdown_preview: true,
            show_markdown_toolbar: false,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
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

/// How a tab's color tag is shown in the tab bar.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum TabColorStyle {
    /// Render the tab title in the tag color.
    #[default]
    TextColor,
    /// Render a small colored dot before the tab title.
    Dot,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct AppSettings {
    pub confirm_exit: bool,
    #[serde(default = "default_debug_mode")]
    pub debug_mode: bool,
    pub theme: SharedString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scrollbar_show: Option<ScrollbarShow>,
    pub synchronization_settings: SynchronizationSettings,
    /// How a tab's color tag is displayed in the tab bar.
    #[serde(default)]
    pub tab_color_style: TabColorStyle,
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
    #[must_use]
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
    #[must_use]
    pub fn new() -> Self {
        Self {
            confirm_exit: true,
            theme: "Default Light".into(),
            scrollbar_show: None,
            synchronization_settings: SynchronizationSettings::new(),
            debug_mode: false,
            tab_color_style: TabColorStyle::TextColor,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
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
    #[must_use]
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
    #[must_use]
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
    /// ### Errors
    /// Returns an error if the theme file cannot be read or if it fails to
    /// deserialize from JSON.
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
    /// ### Errors
    /// Returns an error if the themes directory cannot be resolved or read,
    /// or if any bundled or user theme file fails to load.
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
                    if default_themes_names.contains(&filename) {
                        None
                    } else {
                        Some(entry)
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

#[derive(Clone, Serialize, Deserialize, PartialEq)]
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
    #[must_use]
    pub fn new() -> Self {
        Self {
            editor_settings: EditorSettings::new(),
            app_settings: AppSettings::new(),
            recent_files: RecentFiles::new(10),
        }
    }
}
