use gpui::*;
use gpui_component::input::InputState;

/// A read-only tab that renders a live Markdown preview for a linked editor tab.
#[derive(Clone)]
pub struct MarkdownPreviewTab {
    pub id: usize,
    pub title: SharedString,
    pub source_tab_id: usize,
    pub content: Entity<InputState>,
}
