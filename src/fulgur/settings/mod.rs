mod persistence;
mod propagation;
mod types;

pub use types::{
    AppSettings, EditorSettings, MarkdownPreviewMode, MarkdownSettings, RecentFiles, Settings,
    SynchronizationSettings, ThemeFile, ThemeInfo, Themes,
};

#[cfg(test)]
mod tests;
