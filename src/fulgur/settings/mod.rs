mod persistence;
mod propagation;
mod types;

pub use types::{
    AppSettings, DEFAULT_LEGACY_PROFILE_NAME, EditorSettings, MAX_PROFILES, MarkdownPreviewMode,
    MarkdownSettings, ProfileId, RecentFiles, ServerProfile, Settings, SynchronizationSettings,
    TabColorStyle, ThemeFile, ThemeInfo, Themes, new_profile_id,
};

#[cfg(test)]
mod tests;
