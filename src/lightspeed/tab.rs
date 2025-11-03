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

    /// Check if the tab can be closed
    pub fn is_closable(&self) -> bool {
        match self {
            Tab::Editor(_) => true,
            Tab::Settings(_) => true, // Settings can be closed
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

    /// Check if this is an editor tab
    pub fn is_editor(&self) -> bool {
        matches!(self, Tab::Editor(_))
    }

    /// Check if this is a settings tab
    pub fn is_settings(&self) -> bool {
        matches!(self, Tab::Settings(_))
    }
}
