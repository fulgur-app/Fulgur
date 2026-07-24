use crate::fulgur::{
    Fulgur,
    tab::{Tab, TabId},
    ui::components_utils::MAX_TAB_NAME_LENGTH,
};
use gpui::{Context, Window};

impl Fulgur {
    /// Rename a tab identified by its stable identifier
    ///
    /// ### Arguments
    /// - `tab_id`: The identifier of the tab to rename
    /// - `name`: The requested new name
    /// - `window`: The window context
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `bool`: `true` when the tab was renamed, `false` when the name was blank
    ///   or the tab is missing or not renameable
    pub fn rename_tab(
        &mut self,
        tab_id: TabId,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let new_name: String = name.trim().chars().take(MAX_TAB_NAME_LENGTH).collect();
        if new_name.is_empty() {
            return false;
        }
        let Some(tab_entity) = self.tab_entity_of(tab_id, cx) else {
            return false;
        };
        let settings = self.settings.editor_settings.clone();
        let renamed = tab_entity.update(cx, |tab, cx| {
            tab.rename_editor(&new_name, window, cx, &settings)
        });
        if renamed {
            self.retitle_preview_tabs_of(tab_id, &new_name, cx);
            self.save_state_async(cx, window);
            cx.notify();
        }
        renamed
    }

    /// Refresh the title of the Markdown preview tabs of a renamed editor tab
    ///
    /// ### Arguments
    /// - `tab_id`: The identifier of the renamed editor tab
    /// - `new_name`: The new name of the renamed editor tab
    /// - `cx`: The application context
    fn retitle_preview_tabs_of(&self, tab_id: TabId, new_name: &str, cx: &mut Context<Self>) {
        let previews: Vec<_> = self
            .tabs
            .iter()
            .filter(|tab| {
                tab.read(cx)
                    .as_markdown_preview()
                    .is_some_and(|preview| preview.source_tab_id == tab_id)
            })
            .cloned()
            .collect();
        let title = gpui::SharedString::from(format!("Preview - {new_name}"));
        for preview in previews {
            preview.update(cx, |tab, cx| {
                if let Tab::MarkdownPreview(preview_tab) = tab {
                    preview_tab.title = title.clone();
                    cx.notify();
                }
            });
        }
    }

    /// Reorder a tab from one index to another within this window.
    ///
    /// `to` is the logical insertion slot (0 = before all tabs, N = after all tabs).
    /// No-op when the operation would leave the tab in its current position.
    ///
    /// ### Arguments
    /// - `from`: The current index of the tab to move
    /// - `to`: The insertion slot index (`0..=tabs.len()`)
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn reorder_tab(
        &mut self,
        from: usize,
        to: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if from >= self.tabs.len() || to > self.tabs.len() {
            return;
        }
        // Inserting at slot `to` or `to-1` when `to > from` is equivalent to no move.
        if to == from || to == from + 1 {
            return;
        }
        let tab = self.tabs.remove(from);
        // After removing `from`, the effective insert position shifts down by 1 when to > from.
        let insert_at = if to > from { to - 1 } else { to };
        self.tabs.insert(insert_at, tab);
        self.save_state_async(cx, window);
        cx.notify();
    }

    /// Handle a tab drop onto an insertion slot.
    ///
    /// Called by `on_drop` handlers on the slot divs in the tab bar.
    ///
    /// ### Arguments
    /// - `dragged`: The drag payload
    /// - `slot_index`: The insertion slot (0 = before first tab, N = after last tab)
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn handle_tab_drop(
        &mut self,
        dragged: &crate::fulgur::ui::tabs::tab_drag::DraggedTab,
        slot_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(from) = self.tab_index_of(dragged.tab_id, cx) {
            self.reorder_tab(from, slot_index, window, cx);
        }
    }
}
