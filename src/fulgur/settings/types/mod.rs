mod app;
mod editor;
mod recent_files;
mod sync;
mod themes;

pub use app::{AppSettings, TabColorStyle, TitleBarStyle};
pub use editor::{EditorSettings, MarkdownPreviewMode, MarkdownSettings};
pub use recent_files::RecentFiles;
pub use sync::{
    DEFAULT_PROFILE_NAME, MAX_PROFILES, ProfileId, ServerProfile, SynchronizationSettings,
    new_profile_id,
};
pub use themes::{ThemeFile, ThemeInfo, Themes};

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
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
