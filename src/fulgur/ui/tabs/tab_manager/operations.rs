use crate::fulgur::Fulgur;
use gpui::{Context, Window};

impl Fulgur {
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
