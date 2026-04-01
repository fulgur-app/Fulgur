use crate::fulgur::{Fulgur, tab::Tab};
use gpui::*;

impl Fulgur {
    /// Update the modified status of the tabs
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub fn update_modified_status(&mut self, cx: &mut Context<Self>) {
        for tab in self.tabs.iter_mut() {
            if let Tab::Editor(editor_tab) = tab {
                editor_tab.check_modified(cx);
            }
        }
    }

    /// Reorder a tab from one index to another within this window.
    ///
    /// `to` is the logical insertion slot (0 = before all tabs, N = after all tabs).
    /// No-op when the operation would leave the tab in its current position.
    ///
    /// ### Arguments
    /// - `from`: The current index of the tab to move
    /// - `to`: The insertion slot index (0..=tabs.len())
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
        if let Some(active) = self.active_tab_index {
            self.active_tab_index = Some(if from == active {
                insert_at
            } else if from < active && insert_at >= active {
                active - 1
            } else if from > active && insert_at <= active {
                active + 1
            } else {
                active
            });
        }
        if let Err(e) = self.save_state(cx, window) {
            log::error!("Failed to save app state after reordering tab: {}", e);
            self.pending_notification = Some((
                gpui_component::notification::NotificationType::Warning,
                format!("Tab reordered but failed to save state: {}", e).into(),
            ));
        }
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
        self.reorder_tab(dragged.tab_index, slot_index, window, cx);
    }
}
