use crate::fulgur::ui::tabs::tab::TabId;
use gpui::{Context, IntoElement, ParentElement, Render, SharedString, Styled, Window, div};
use gpui_component::ActiveTheme;

/// Data carried during a tab drag operation.
///
/// Also implements `Render` so it can serve as the ghost view shown under
/// the cursor while dragging.
#[derive(Clone)]
pub struct DraggedTab {
    /// Stable ID of the dragged tab, resolved to a position at drop time
    pub tab_id: TabId,
    pub title: SharedString,
    pub is_modified: bool,
}

impl Render for DraggedTab {
    /// Renders the floating ghost tab shown under the cursor while dragging
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modified_indicator = if self.is_modified { " •" } else { "" };
        div()
            .px_3()
            .py_1()
            .rounded_md()
            .shadow_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().tab_active)
            .text_color(cx.theme().tab_active_foreground)
            .text_sm()
            .child(format!("{}{}", self.title, modified_indicator))
    }
}

#[cfg(test)]
mod tests {
    use super::{DraggedTab, TabId};

    fn make_tab(tab_id: TabId, title: &'static str, modified: bool) -> DraggedTab {
        DraggedTab {
            tab_id,
            title: title.into(),
            is_modified: modified,
        }
    }

    // ========== DraggedTab struct tests ==========

    #[test]
    fn test_dragged_tab_clone_preserves_all_fields() {
        let original = make_tab(TabId(3), "main.rs", true);
        let cloned = original.clone();
        assert_eq!(cloned.tab_id, TabId(3));
        assert_eq!(cloned.title.as_ref(), "main.rs");
        assert!(cloned.is_modified);
    }

    // ========== ghost tab title formatting tests ==========

    #[test]
    fn test_ghost_title_unmodified_has_no_bullet() {
        let tab = make_tab(TabId(0), "readme.md", false);
        let indicator = if tab.is_modified { " •" } else { "" };
        assert_eq!(format!("{}{}", tab.title, indicator), "readme.md");
    }

    #[test]
    fn test_ghost_title_modified_appends_bullet() {
        let tab = make_tab(TabId(0), "readme.md", true);
        let indicator = if tab.is_modified { " •" } else { "" };
        assert_eq!(format!("{}{}", tab.title, indicator), "readme.md •");
    }
}
