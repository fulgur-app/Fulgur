mod persistence;
mod propagation;
mod types;
mod writer;

pub use persistence::StartupSettings;

pub use types::{
    AppSettings, DEFAULT_PROFILE_NAME, EditorSettings, MAX_PROFILES, MarkdownPreviewMode,
    MarkdownSettings, ProfileId, RecentFiles, ServerProfile, Settings, SynchronizationSettings,
    TabColorStyle, ThemeFile, ThemeInfo, Themes, TitleBarStyle, new_profile_id,
};
pub use writer::SettingsWriter;

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests;
