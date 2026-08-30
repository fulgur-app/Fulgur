mod persistence;
mod propagation;
mod types;

pub use types::{
    AppSettings, DEFAULT_PROFILE_NAME, EditorSettings, MAX_PROFILES, MarkdownPreviewMode,
    MarkdownSettings, ProfileId, RecentFiles, ServerProfile, Settings, SynchronizationSettings,
    TabColorStyle, ThemeFile, ThemeInfo, Themes, TitleBarStyle, UNIFIED_TITLE_BAR_SUPPORTED,
    new_profile_id,
};

#[cfg(test)]
mod tests;
