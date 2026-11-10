use crate::fulgur::{Fulgur, tab::TabId};
use gpui_kit::{App, Context, Focusable, Window};

impl Fulgur {
    /// Set the active tab by position, running every side effect of a tab switch
    ///
    /// ### Arguments
    /// - `index`: The index of the tab to set as active
    /// - `window`: The window to set the active tab in
    /// - `cx`: The application context
    pub fn set_active_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab_id) = self.tabs.get(index).map(|tab| tab.read(cx).id()) {
            self.activate_tab(tab_id, window, cx);
        }
    }

    /// Make a tab active and run every side effect of a tab switch
    ///
    /// ### Arguments
    /// - `tab_id`: The id of the tab to activate
    /// - `window`: The window to set the active tab in
    /// - `cx`: The application context
    pub fn activate_tab(&mut self, tab_id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        if self.tab_index_of(tab_id, cx).is_none() {
            return;
        }
        self.pending_initial_active_tab = None;
        self.switch_active_tab(tab_id, cx);
        self.run_tab_entry_effects(tab_id, window, cx);
        self.focus_active_tab(window, cx);
    }

    /// Point the active tab at another tab without its entry effects
    ///
    /// ### Arguments
    /// - `tab_id`: The id of the tab becoming active
    /// - `cx`: The application context
    fn switch_active_tab(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        if let Some(previous_id) = self.active_tab_id
            && previous_id != tab_id
        {
            self.stop_log_poll_task(previous_id);
        }
        self.active_tab_id = Some(tab_id);
        // Deferred like every other tab bar scroll, so the latest request wins over an
        // older one (such as the restored tab's) still waiting for the bar's layout.
        self.request_tab_scroll(tab_id, cx);
        if self.search_bar.read(cx).is_visible() {
            let content = self
                .get_active_editor_tab(cx)
                .map(|editor_tab| editor_tab.content.clone());
            self.search_bar
                .update(cx, |bar, cx| bar.refresh_matches(content, cx));
        }
        cx.notify();
    }

    /// Run the effects a tab needs when it becomes the active one
    ///
    /// ### Arguments
    /// - `tab_id`: The id of the active tab
    /// - `window`: The window holding the tab
    /// - `cx`: The application context
    fn run_tab_entry_effects(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pending_conflict_path = self
            .editor_tab(tab_id, cx)
            .and_then(|editor_tab| editor_tab.file_path())
            .filter(|path| {
                self.file_watch_state
                    .pending_conflicts
                    .contains_key(path.as_path())
            })
            .cloned();
        if let Some(path) = pending_conflict_path {
            self.file_watch_state.pending_conflicts.remove(&path);
            self.show_file_conflict_dialog(&path, tab_id, window, cx);
        }
        self.load_active_tab_remote_content(window, cx);
        if self
            .editor_tab(tab_id, cx)
            .is_some_and(|editor_tab| editor_tab.log_view)
        {
            self.resume_log_view(tab_id, window, cx);
        }
    }

    /// Re-anchor the active tab on a neighbour after the active tab was removed
    ///
    /// ### Arguments
    /// - `tab_id`: The id of the neighbour taking over
    /// - `window`: The window holding the tab
    /// - `cx`: The application context
    pub(super) fn activate_fallback_tab(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.switch_active_tab(tab_id, cx);
        cx.defer_in(window, move |this, window, cx| {
            if this.active_tab_id == Some(tab_id) {
                this.run_tab_entry_effects(tab_id, window, cx);
            }
        });
    }

    /// Focus the active tab's content
    ///
    /// ### Arguments
    /// - `window`: The window to focus the tab in
    /// - `cx`: The application context
    pub fn focus_active_tab(&self, window: &mut Window, cx: &mut App) {
        let content = self
            .get_active_editor_tab(cx)
            .map(|editor_tab| editor_tab.content.clone());
        if let Some(content) = content {
            let focus_handle = content.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
        } else {
            window.focus(&self.focus_handle, cx);
        }
    }
}
