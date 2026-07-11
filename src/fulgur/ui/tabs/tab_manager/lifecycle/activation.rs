use crate::fulgur::{Fulgur, tab::Tab, ui::tabs::editor_tab::TabLocation};
use gpui::{App, Context, Focusable, Window};

impl Fulgur {
    /// Set the active tab. If search is open, re-run search on new tab.
    ///
    /// ### Arguments
    /// - `index`: The index of the tab to set as active
    /// - `window`: The window to set the active tab in
    /// - `cx`: The application context
    pub fn set_active_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index < self.tabs.len() {
            let previous_active_id = self.active_tab_id;
            let new_tab_id = self.tabs.get(index).map(|tab| tab.read(cx).id());
            if let Some(prev_id) = previous_active_id
                && previous_active_id != new_tab_id
            {
                self.stop_log_poll_task(prev_id);
            }
            self.active_tab_id = new_tab_id;
            self.tab_bar.update(cx, |bar, _| bar.scroll_to_index(index));
            let pending_path = if let Some(Tab::Editor(editor_tab)) =
                self.tabs.get(index).map(|tab| tab.read(cx))
            {
                editor_tab
                    .file_path()
                    .filter(|path| {
                        self.file_watch_state
                            .pending_conflicts
                            .contains_key(path.as_path())
                    })
                    .cloned()
            } else {
                None
            };
            if let Some(path) = pending_path
                && let Some(tab_id) = new_tab_id
            {
                self.file_watch_state.pending_conflicts.remove(&path);
                self.show_file_conflict_dialog(&path, tab_id, window, cx);
            }
            let pending_remote_reload = if let Some(Tab::Editor(editor_tab)) =
                self.tabs.get(index).map(|tab| tab.read(cx))
            {
                match &editor_tab.location {
                    TabLocation::Remote(spec)
                        if self.pending_remote_restore.contains(&editor_tab.id)
                            && !self.inflight_remote_restore.contains(&editor_tab.id)
                            && !editor_tab.modified =>
                    {
                        Some((editor_tab.id, spec.clone()))
                    }
                    _ => None,
                }
            } else {
                None
            };
            if let Some((tab_id, spec)) = pending_remote_reload {
                self.ensure_remote_tab_loaded(window, cx, tab_id, spec);
            }
            self.focus_active_tab(window, cx);
            if let Some(new_id) = new_tab_id
                && self
                    .tabs
                    .get(index)
                    .and_then(|tab| tab.read(cx).as_editor())
                    .is_some_and(|editor| editor.log_view)
            {
                self.resume_log_view(new_id, window, cx);
            }
            if self.search_bar.read(cx).is_visible() {
                let content = self
                    .get_active_editor_tab(cx)
                    .map(|editor_tab| editor_tab.content.clone());
                self.search_bar
                    .update(cx, |bar, cx| bar.refresh_matches(content, window, cx));
            }
            cx.notify();
        }
    }

    /// Focus the active tab's content
    ///
    /// ### Arguments
    /// - `window`: The window to focus the tab in
    /// - `cx`: The application context
    pub fn focus_active_tab(&self, window: &mut Window, cx: &mut App) {
        // Settings and preview tabs have no focusable input content, so only
        // editor tabs move the focus.
        let content = self
            .get_active_editor_tab(cx)
            .map(|editor_tab| editor_tab.content.clone());
        if let Some(content) = content {
            let focus_handle = content.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
        }
    }
}
