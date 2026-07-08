use crate::fulgur::{
    Fulgur,
    ui::tabs::{
        editor_tab::EditorTab, markdown_preview_tab::MarkdownPreviewTab, settings_tab::SettingsTab,
    },
};
use gpui::SharedString;

/// Stable identifier of a tab, unique within a window for the process lifetime
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(pub u64);

impl std::fmt::Display for TabId {
    /// Format the tab ID as its raw numeric value
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TabId {
    /// Return the identifier that follows this one
    ///
    /// ### Returns
    /// - `TabId`: The next sequential tab identifier
    #[must_use]
    pub fn next(self) -> TabId {
        TabId(self.0 + 1)
    }
}

/// Enum representing different types of tabs
#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Tab {
    Editor(EditorTab),
    Settings(SettingsTab),
    MarkdownPreview(MarkdownPreviewTab),
}

impl Tab {
    /// Get the tab ID
    ///
    /// ### Returns
    /// - `TabId`: The ID of the tab
    pub fn id(&self) -> TabId {
        match self {
            Tab::Editor(tab) => tab.id,
            Tab::Settings(tab) => tab.id,
            Tab::MarkdownPreview(tab) => tab.id,
        }
    }

    /// Get the tab title
    ///
    /// ### Returns
    /// - `SharedString`: The title of the tab
    pub fn title(&self) -> SharedString {
        match self {
            Tab::Editor(tab) => tab.title.clone(),
            Tab::Settings(tab) => tab.title.clone(),
            Tab::MarkdownPreview(tab) => tab.title.clone(),
        }
    }

    /// Check if the tab has been modified
    ///
    /// ### Returns
    /// - `True`: If the tab has been modified, `False` otherwise
    pub fn is_modified(&self) -> bool {
        match self {
            Tab::Editor(tab) => tab.modified,
            Tab::Settings(_) | Tab::MarkdownPreview(_) => false,
        }
    }

    /// Get the editor tab if this is an editor tab
    ///
    /// ### Returns
    /// - `Some(&EditorTab)`: The editor tab if this is an editor tab
    /// - `None`: If this is not an editor tab
    pub fn as_editor(&self) -> Option<&EditorTab> {
        match self {
            Tab::Editor(tab) => Some(tab),
            _ => None,
        }
    }

    /// Get the Markdown preview tab if this is a preview tab
    ///
    /// ### Returns
    /// - `Some(&MarkdownPreviewTab)`: The preview tab if this is a preview tab
    /// - `None`: If this is not a preview tab
    pub fn as_markdown_preview(&self) -> Option<&MarkdownPreviewTab> {
        match self {
            Tab::MarkdownPreview(tab) => Some(tab),
            _ => None,
        }
    }

    /// Get the editor tab mutably if this is an editor tab
    ///
    /// ### Returns
    /// - `Some(&mut EditorTab)`: The editor tab mutably if this is an editor tab
    /// - `None`: If this is not an editor tab
    pub fn as_editor_mut(&mut self) -> Option<&mut EditorTab> {
        match self {
            Tab::Editor(tab) => Some(tab),
            _ => None,
        }
    }
}

impl Fulgur {
    /// Allocate the next unique tab identifier for this window
    ///
    /// ### Returns
    /// - `TabId`: A tab identifier never handed out before in this window
    pub(crate) fn allocate_tab_id(&mut self) -> TabId {
        let id = self.next_tab_id;
        self.next_tab_id = id.next();
        id
    }

    /// Resolve the current position of a tab from its stable identifier
    ///
    /// ### Arguments
    /// - `tab_id`: The identifier of the tab to locate
    ///
    /// ### Returns
    /// - `Some(usize)`: The current position of the tab in the tab strip
    /// - `None`: If no tab with this identifier exists
    #[must_use]
    pub fn tab_index_of(&self, tab_id: TabId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id() == tab_id)
    }

    /// Resolve the current position of the active tab
    ///
    /// ### Returns
    /// - `Some(usize)`: The current position of the active tab
    /// - `None`: If there is no active tab or it no longer exists
    #[must_use]
    pub fn active_tab_index(&self) -> Option<usize> {
        self.active_tab_id.and_then(|id| self.tab_index_of(id))
    }

    /// Get the active tab
    ///
    /// ### Returns
    /// - `Some(&Tab)`: The active tab
    /// - `None`: If there is no active tab
    #[must_use]
    pub fn active_tab(&self) -> Option<&Tab> {
        self.active_tab_id
            .and_then(|id| self.tabs.iter().find(|tab| tab.id() == id))
    }

    /// Get the active tab mutably
    ///
    /// ### Returns
    /// - `Some(&mut Tab)`: The active tab mutably
    /// - `None`: If there is no active tab
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        let id = self.active_tab_id?;
        self.tabs.iter_mut().find(|tab| tab.id() == id)
    }

    /// Get the active editor tab
    ///
    /// ### Returns
    /// - `Some(&EditorTab)`: The active editor tab
    /// - `None`: If there is no active editor tab
    #[must_use]
    pub fn get_active_editor_tab(&self) -> Option<&EditorTab> {
        self.active_tab().and_then(Tab::as_editor)
    }

    /// Get the active editor tab as mutable
    ///
    /// ### Returns
    /// - `Some(&mut EditorTab)`: The active editor tab as mutable
    /// - `None`: If there is no active editor tab
    pub fn get_active_editor_tab_mut(&mut self) -> Option<&mut EditorTab> {
        self.active_tab_mut().and_then(Tab::as_editor_mut)
    }
}
