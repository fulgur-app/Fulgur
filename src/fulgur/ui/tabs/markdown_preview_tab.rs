use crate::fulgur::ui::tabs::markdown_preview_source::MarkdownPreviewSource;
use crate::fulgur::ui::tabs::tab::TabId;
use gpui_kit::{Entity, SharedString};

/// A read-only tab that renders a live Markdown preview for a linked editor tab.
pub struct MarkdownPreviewTab {
    pub id: TabId,
    pub title: SharedString,
    pub source_tab_id: TabId,
    /// Preview text kept in step with the source editor tab. Retained across
    /// renders so that the scroll position survives switching to another tab
    /// and back within a session.
    pub preview: Entity<MarkdownPreviewSource>,
}
