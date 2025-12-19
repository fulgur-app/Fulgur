use gpui::*;

use crate::fulgur::Fulgur;

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
    ///
    /// @return: The ID of the tab
    pub fn id(&self) -> usize {
        match self {
            Tab::Editor(tab) => tab.id,
            Tab::Settings(tab) => tab.id,
        }
    }

    /// Get the tab title
    ///
    /// @return: The title of the tab
    pub fn title(&self) -> SharedString {
        match self {
            Tab::Editor(tab) => tab.title.clone(),
            Tab::Settings(tab) => tab.title.clone(),
        }
    }

    /// Check if the tab has been modified
    ///
    /// @return: True if the tab has been modified
    pub fn is_modified(&self) -> bool {
        match self {
            Tab::Editor(tab) => tab.modified,
            Tab::Settings(_) => false, // Settings tabs are never modified
        }
    }

    /// Get the editor tab if this is an editor tab
    ///
    /// @return: The editor tab if this is an editor tab mutable
    pub fn as_editor(&self) -> Option<&EditorTab> {
        match self {
            Tab::Editor(tab) => Some(tab),
            _ => None,
        }
    }

    /// Get the editor tab mutably if this is an editor tab
    ///
    /// @return: The editor tab mutably if this is an editor tab
    pub fn as_editor_mut(&mut self) -> Option<&mut EditorTab> {
        match self {
            Tab::Editor(tab) => Some(tab),
            _ => None,
        }
    }
}

impl Fulgur {
    /// Get the active editor tab
    ///
    /// @return: The active editor tab
    pub fn get_active_editor_tab(&self) -> Option<&EditorTab> {
        if let Some(index) = self.active_tab_index {
            if let Some(tab) = self.tabs.get(index) {
                if let Some(editor_tab) = tab.as_editor() {
                    return Some(editor_tab);
                }
            }
        }
        None
    }

    /// Get the active editor tab as mutable
    ///
    /// @return: The active editor tab as mutable
    pub fn get_active_editor_tab_mut(&mut self) -> Option<&mut EditorTab> {
        if let Some(index) = self.active_tab_index {
            if let Some(tab) = self.tabs.get_mut(index) {
                if let Some(editor_tab) = tab.as_editor_mut() {
                    return Some(editor_tab);
                }
            }
        }
        None
    }
}
