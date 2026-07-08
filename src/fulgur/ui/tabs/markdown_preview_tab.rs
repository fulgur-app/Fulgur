use crate::fulgur::ui::tabs::tab::TabId;
use gpui::{Entity, SharedString};
use gpui_component::input::InputState;
use gpui_component::text::TextViewState;

/// A read-only tab that renders a live Markdown preview for a linked editor tab.
#[derive(Clone)]
pub struct MarkdownPreviewTab {
    pub id: TabId,
    pub title: SharedString,
    pub source_tab_id: TabId,
    pub content: Entity<InputState>,
    /// Persistent text view state retained across renders so that the scroll
    /// position survives switching to another tab and back within a session.
    pub view_state: Entity<TextViewState>,
}
