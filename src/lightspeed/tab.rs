use gpui::*;

use super::editor_tab::EditorTab;
use super::settings::SettingsTab;

/// Enum representing different types of tabs
#[derive(Clone)]
pub enum Tab {
    Editor(EditorTab),
    Settings(SettingsTab),
}

impl Tab {
    /// Get the tab ID
    pub fn id(&self) -> usize {
        match self {
            Tab::Editor(tab) => tab.id,
            Tab::Settings(tab) => tab.id,
        }
    }

    /// Get the tab title
    pub fn title(&self) -> SharedString {
        match self {
            Tab::Editor(tab) => tab.title.clone(),
            Tab::Settings(tab) => tab.title.clone(),
        }
    }

    /// Check if the tab has been modified
    pub fn is_modified(&self) -> bool {
        match self {
            Tab::Editor(tab) => tab.modified,
            Tab::Settings(_) => false, // Settings tabs are never modified
        }
    }

    /// Get the editor tab if this is an editor tab
    pub fn as_editor(&self) -> Option<&EditorTab> {
        match self {
            Tab::Editor(tab) => Some(tab),
            _ => None,
        }
    }

    /// Get the editor tab mutably if this is an editor tab
    pub fn as_editor_mut(&mut self) -> Option<&mut EditorTab> {
        match self {
            Tab::Editor(tab) => Some(tab),
            _ => None,
        }
    }
}
