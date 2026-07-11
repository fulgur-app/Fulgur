use crate::fulgur::{
    Fulgur,
    settings::EditorSettings,
    ui::tabs::{
        editor_tab::EditorTab, markdown_preview_tab::MarkdownPreviewTab, settings_tab::SettingsTab,
    },
};
use gpui::{App, AppContext, Context, Entity, SharedString, Window};
use gpui_component::input::InputEvent;

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

    /// Wrap this tab in its own entity, attaching the content subscription
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Entity<Tab>`: The tab wrapped in a new entity
    pub fn into_entity(self, cx: &mut App) -> Entity<Tab> {
        cx.new(|cx| {
            let mut tab = self;
            tab.attach_content_subscription(cx);
            tab
        })
    }

    /// Subscribe to the editor content so the tab keeps its own modified state
    ///
    /// ### Arguments
    /// - `cx`: The tab entity context
    pub fn attach_content_subscription(&mut self, cx: &mut Context<Tab>) {
        let Tab::Editor(editor_tab) = self else {
            return;
        };
        let content = editor_tab.content.clone();
        editor_tab.content_subscription = Some(cx.subscribe(
            &content,
            |this: &mut Tab, _, event: &InputEvent, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                if let Tab::Editor(editor_tab) = this {
                    let old_modified = editor_tab.modified;
                    editor_tab.check_modified(cx);
                    if editor_tab.modified != old_modified {
                        cx.notify();
                    }
                }
            },
        ));
    }

    /// Update the editor's display settings, re-attaching the content subscription on rebuild
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The tab entity context
    /// - `settings`: The editor settings to apply
    pub fn update_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Tab>,
        settings: &EditorSettings,
    ) {
        let Tab::Editor(editor_tab) = self else {
            return;
        };
        let content_before = editor_tab.content.entity_id();
        editor_tab.update_settings(window, cx, settings);
        self.reattach_if_content_swapped(content_before, cx);
    }

    /// Update the language from the file extension, re-attaching the content subscription
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The tab entity context
    /// - `settings`: The editor settings for the new input state
    pub fn update_language(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Tab>,
        settings: &EditorSettings,
    ) {
        let Tab::Editor(editor_tab) = self else {
            return;
        };
        let content_before = editor_tab.content.entity_id();
        editor_tab.update_language(window, cx, settings);
        self.reattach_if_content_swapped(content_before, cx);
    }

    /// Force the language, re-attaching the content subscription
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The tab entity context
    /// - `language`: The language to force
    /// - `settings`: The editor settings for the new input state
    pub fn force_language(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Tab>,
        language: crate::fulgur::languages::supported_languages::SupportedLanguage,
        settings: &EditorSettings,
    ) {
        let Tab::Editor(editor_tab) = self else {
            return;
        };
        let content_before = editor_tab.content.entity_id();
        editor_tab.force_language(window, cx, language, settings);
        self.reattach_if_content_swapped(content_before, cx);
    }

    /// Re-attach the content subscription when the content entity was replaced
    ///
    /// ### Arguments
    /// - `content_before`: The content entity id before the possibly swapping call
    /// - `cx`: The tab entity context
    fn reattach_if_content_swapped(
        &mut self,
        content_before: gpui::EntityId,
        cx: &mut Context<Tab>,
    ) {
        if let Tab::Editor(editor_tab) = self
            && editor_tab.content.entity_id() != content_before
        {
            self.attach_content_subscription(cx);
            cx.notify();
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
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(usize)`: The current position of the tab in the tab strip
    /// - `None`: If no tab with this identifier exists
    #[must_use]
    pub fn tab_index_of(&self, tab_id: TabId, cx: &App) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.read(cx).id() == tab_id)
    }

    /// Resolve the current position of the active tab
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(usize)`: The current position of the active tab
    /// - `None`: If there is no active tab or it no longer exists
    #[must_use]
    pub fn active_tab_index(&self, cx: &App) -> Option<usize> {
        self.active_tab_id.and_then(|id| self.tab_index_of(id, cx))
    }

    /// Get the entity of a tab from its stable identifier
    ///
    /// ### Arguments
    /// - `tab_id`: The identifier of the tab to locate
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(Entity<Tab>)`: A handle to the tab entity
    /// - `None`: If no tab with this identifier exists
    #[must_use]
    pub fn tab_entity_of(&self, tab_id: TabId, cx: &App) -> Option<Entity<Tab>> {
        self.tabs
            .iter()
            .find(|tab| tab.read(cx).id() == tab_id)
            .cloned()
    }

    /// Get the entity of the active tab
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(Entity<Tab>)`: A handle to the active tab entity
    /// - `None`: If there is no active tab
    #[must_use]
    pub fn active_tab_entity(&self, cx: &App) -> Option<Entity<Tab>> {
        self.active_tab_id.and_then(|id| self.tab_entity_of(id, cx))
    }

    /// Get the active tab
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(&Tab)`: The active tab, borrowed from the application context
    /// - `None`: If there is no active tab
    #[must_use]
    pub fn active_tab<'a>(&self, cx: &'a App) -> Option<&'a Tab> {
        let id = self.active_tab_id?;
        self.tabs
            .iter()
            .map(|tab| tab.read(cx))
            .find(|tab| tab.id() == id)
    }

    /// Get the active editor tab
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(&EditorTab)`: The active editor tab, borrowed from the application context
    /// - `None`: If there is no active editor tab
    #[must_use]
    pub fn get_active_editor_tab<'a>(&self, cx: &'a App) -> Option<&'a EditorTab> {
        self.active_tab(cx).and_then(Tab::as_editor)
    }

    /// Update an editor tab by its stable identifier
    ///
    /// ### Arguments
    /// - `tab_id`: The identifier of the tab to update
    /// - `cx`: The application context
    /// - `f`: Closure applied to the editor tab inside its entity update
    ///
    /// ### Returns
    /// - `Some(R)`: The closure result when the tab exists and is an editor tab
    /// - `None`: If no editor tab with this identifier exists
    pub fn update_editor_tab<R>(
        &self,
        tab_id: TabId,
        cx: &mut App,
        f: impl FnOnce(&mut EditorTab, &mut Context<Tab>) -> R,
    ) -> Option<R> {
        let tab = self.tab_entity_of(tab_id, cx)?;
        tab.update(cx, |tab, cx| {
            tab.as_editor_mut().map(|editor| f(editor, cx))
        })
    }

    /// Update the active editor tab
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `f`: Closure applied to the editor tab inside its entity update
    ///
    /// ### Returns
    /// - `Some(R)`: The closure result when the active tab is an editor tab
    /// - `None`: If there is no active editor tab
    pub fn update_active_editor_tab<R>(
        &self,
        cx: &mut App,
        f: impl FnOnce(&mut EditorTab, &mut Context<Tab>) -> R,
    ) -> Option<R> {
        let tab_id = self.active_tab_id?;
        self.update_editor_tab(tab_id, cx, f)
    }
}
