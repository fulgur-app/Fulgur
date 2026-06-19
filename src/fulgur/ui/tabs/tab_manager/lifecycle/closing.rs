use crate::fulgur::{Fulgur, tab::Tab};
use gpui::{Context, Window};

impl Fulgur {
    /// Close a tab
    ///
    /// ### Arguments
    /// - `tab_id`: The ID of the tab to close
    /// - `window`: The window to close the tab in
    /// - `cx`: The application context
    pub fn close_tab(&mut self, tab_id: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tabs.iter().any(|t| t.id() == tab_id) {
            return;
        }

        if self.check_tab_modified(tab_id) {
            self.show_unsaved_changes_dialog(window, cx, move |this, window, cx| {
                this.remove_tab_by_id(tab_id, window, cx);
                this.save_state_async(cx, window);
            });
        } else {
            self.remove_tab_by_id(tab_id, window, cx);
            self.focus_active_tab(window, cx);
            self.save_state_async(cx, window);
        }
    }

    /// Close the currently active tab
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn close_active_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.active_tab_index
            && let Some(tab) = self.tabs.get(index)
        {
            self.close_tab(tab.id(), window, cx);
        }
    }

    /// Close a tab and manage the focus
    ///
    /// ### Arguments
    /// - `window`: The window to close the tab in
    /// - `cx`: The application context
    /// - `pos`: The position of the tab to close
    pub fn close_tab_manage_focus(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        pos: usize,
    ) {
        if self.tabs.is_empty() {
            self.active_tab_index = None;
        } else if let Some(active_index) = self.active_tab_index {
            if active_index >= self.tabs.len() {
                self.active_tab_index = Some(self.tabs.len() - 1);
            } else if pos < active_index {
                self.active_tab_index = Some(active_index - 1);
            }
        }

        self.focus_active_tab(window, cx);
    }

    /// Close all tabs
    ///
    /// ### Arguments
    /// - `window`: The window to close all tabs in
    /// - `cx`: The application context
    pub fn close_all_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        let tab_ids: Vec<usize> = self.tabs.iter().map(Tab::id).collect();
        for tab_id in tab_ids {
            if !self.tabs.iter().any(|t| t.id() == tab_id) {
                continue;
            }
            if self.check_tab_modified(tab_id) {
                if let Some(pos) = self.tabs.iter().position(|t| t.id() == tab_id) {
                    self.set_active_tab(pos, window, cx);
                }
                self.show_unsaved_changes_dialog(window, cx, move |this, window, cx| {
                    this.remove_tab_by_id(tab_id, window, cx);
                    if this.tabs.is_empty() {
                        this.active_tab_index = None;
                        this.save_state_async(cx, window);
                        cx.notify();
                    } else {
                        this.close_all_tabs(window, cx);
                    }
                });
                return;
            }
            self.remove_tab_by_id(tab_id, window, cx);
        }
        if self.tabs.is_empty() {
            self.active_tab_index = None;
        }
        self.save_state_async(cx, window);
        cx.notify();
    }

    /// Close all tabs to the left of the specified index
    ///
    /// ### Arguments
    /// - `index`: The index of the tab (tabs to the left will be closed)
    /// - `window`: The window to close tabs in
    /// - `cx`: The application context
    pub fn close_tabs_to_left(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index == 0 || index >= self.tabs.len() {
            return;
        }
        let keep_id = self.tabs[index].id();
        let tab_ids: Vec<usize> = self.tabs[0..index].iter().map(Tab::id).collect();
        for tab_id in tab_ids {
            if !self.tabs.iter().any(|t| t.id() == tab_id) {
                continue;
            }
            if self.check_tab_modified(tab_id) {
                if let Some(pos) = self.tabs.iter().position(|t| t.id() == tab_id) {
                    self.set_active_tab(pos, window, cx);
                }
                self.show_unsaved_changes_dialog(window, cx, move |this, window, cx| {
                    this.remove_tab_by_id(tab_id, window, cx);
                    if let Some(boundary_index) = this.tabs.iter().position(|t| t.id() == keep_id)
                        && boundary_index > 0
                    {
                        this.close_tabs_to_left(boundary_index, window, cx);
                        return;
                    }
                    this.save_state_async(cx, window);
                    cx.notify();
                });
                return;
            }
            self.remove_tab_by_id(tab_id, window, cx);
        }
        if let Some(active_idx) = self.active_tab_index
            && active_idx >= self.tabs.len()
        {
            self.active_tab_index = Some(self.tabs.len().saturating_sub(1));
        }
        self.save_state_async(cx, window);
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    /// Close all tabs to the right of the specified index
    ///
    /// ### Arguments
    /// - `index`: The index of the tab (tabs to the right will be closed)
    /// - `window`: The window to close tabs in
    /// - `cx`: The application context
    pub fn close_tabs_to_right(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index >= self.tabs.len() - 1 {
            return;
        }
        let tab_ids: Vec<usize> = self.tabs[(index + 1)..].iter().map(Tab::id).collect();
        for tab_id in tab_ids {
            if !self.tabs.iter().any(|t| t.id() == tab_id) {
                continue;
            }
            if self.check_tab_modified(tab_id) {
                if let Some(pos) = self.tabs.iter().position(|t| t.id() == tab_id) {
                    self.set_active_tab(pos, window, cx);
                }
                self.show_unsaved_changes_dialog(window, cx, move |this, window, cx| {
                    this.remove_tab_by_id(tab_id, window, cx);
                    if !this.tabs.is_empty() {
                        let current_index = index.min(this.tabs.len() - 1);
                        if current_index < this.tabs.len() - 1 {
                            this.close_tabs_to_right(current_index, window, cx);
                            return;
                        }
                    }
                    this.save_state_async(cx, window);
                    cx.notify();
                });
                return;
            }
            self.remove_tab_by_id(tab_id, window, cx);
        }
        if let Some(active_idx) = self.active_tab_index
            && active_idx >= self.tabs.len()
        {
            self.active_tab_index = Some(self.tabs.len().saturating_sub(1));
        }
        self.save_state_async(cx, window);
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    /// Close all tabs except the active one
    ///
    /// ### Arguments
    /// - `window`: The window to close tabs in
    /// - `cx`: The application context
    pub fn close_other_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active_index) = self.active_tab_index else {
            return;
        };
        if self.tabs.len() <= 1 {
            return;
        }
        let active_tab_id = self.tabs.get(active_index).map(Tab::id);
        let tab_ids: Vec<usize> = self
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(idx, tab)| {
                if idx == active_index {
                    None
                } else {
                    Some(tab.id())
                }
            })
            .collect();
        for tab_id in tab_ids {
            if !self.tabs.iter().any(|t| t.id() == tab_id) {
                continue;
            }
            if self.check_tab_modified(tab_id) {
                if let Some(pos) = self.tabs.iter().position(|t| t.id() == tab_id) {
                    self.set_active_tab(pos, window, cx);
                }
                self.show_unsaved_changes_dialog(window, cx, move |this, window, cx| {
                    this.remove_tab_by_id(tab_id, window, cx);
                    if let Some(remaining_active_id) = active_tab_id
                        && let Some(new_active_pos) =
                            this.tabs.iter().position(|t| t.id() == remaining_active_id)
                    {
                        this.active_tab_index = Some(new_active_pos);
                    }
                    this.close_other_tabs(window, cx);
                });
                return;
            }
            self.remove_tab_by_id(tab_id, window, cx);
        }
        if let Some(remaining_active_id) = active_tab_id
            && let Some(new_active_pos) =
                self.tabs.iter().position(|t| t.id() == remaining_active_id)
        {
            self.active_tab_index = Some(new_active_pos);
        }
        self.focus_active_tab(window, cx);
        self.save_state_async(cx, window);
        cx.notify();
    }

    /// Check if a tab has unsaved modifications
    ///
    /// ### Arguments
    /// - `tab_id`: The ID of the tab to check
    ///
    /// ### Returns
    /// - `True`: If the tab has unsaved changes, `False` otherwise
    fn check_tab_modified(&self, tab_id: usize) -> bool {
        if let Some(pos) = self.tabs.iter().position(|t| t.id() == tab_id)
            && let Some(tab) = self.tabs.get(pos)
            && let Tab::Editor(editor_tab) = tab
        {
            return editor_tab.modified;
        }
        false
    }

    /// Remove a tab by ID and manage focus
    ///
    /// ### Arguments
    /// - `tab_id`: The ID of the tab to remove
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn remove_tab_by_id(&mut self, tab_id: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pos) = self.tabs.iter().position(|t| t.id() == tab_id) {
            let path_to_unwatch = if let Some(Tab::Editor(editor_tab)) = self.tabs.get(pos) {
                editor_tab.file_path().cloned()
            } else {
                None
            };
            let linked_preview_id = if matches!(self.tabs.get(pos), Some(Tab::Editor(_))) {
                self.tabs
                    .iter()
                    .find(|t| matches!(t, Tab::MarkdownPreview(p) if p.source_tab_id == tab_id))
                    .map(Tab::id)
            } else {
                None
            };
            self.tabs.remove(pos);
            self.close_tab_manage_focus(window, cx, pos);
            self.pending_remote_restore.remove(&tab_id);
            self.inflight_remote_restore.remove(&tab_id);
            self.latest_remote_open_request_by_tab.remove(&tab_id);
            self.latest_remote_save_request_by_tab.remove(&tab_id);
            self.editor_modified_subscriptions.remove(&tab_id);
            self.markdown_preview_cache.remove(&tab_id);
            self.markdown_preview_to_refresh.remove(&tab_id);
            self.markdown_preview_subscriptions.remove(&tab_id);
            self.clear_log_tail(tab_id);
            if let Some(path) = path_to_unwatch {
                self.unwatch_file(&path);
            }
            if let Some(preview_id) = linked_preview_id
                && let Some(preview_pos) = self.tabs.iter().position(|t| t.id() == preview_id)
            {
                self.tabs.remove(preview_pos);
                self.editor_modified_subscriptions.remove(&preview_id);
                if let Some(ai) = self.active_tab_index {
                    if preview_pos < ai && ai > 0 {
                        self.active_tab_index = Some(ai - 1);
                    } else if preview_pos <= ai {
                        self.active_tab_index = if self.tabs.is_empty() {
                            None
                        } else {
                            Some(preview_pos.min(self.tabs.len() - 1))
                        };
                    }
                }
                if self.tabs.is_empty() {
                    self.active_tab_index = None;
                }
            }
            self.debug_assert_tab_maps_consistent();
            cx.notify();
        }
    }

    /// In debug builds, assert that tab-keyed maps do not retain entries for tabs
    /// that no longer exist (i.e. no subscription/cache leaks after a removal).
    fn debug_assert_tab_maps_consistent(&self) {
        if cfg!(debug_assertions) {
            let live: std::collections::HashSet<usize> = self.tabs.iter().map(Tab::id).collect();
            for &id in self.editor_modified_subscriptions.keys() {
                debug_assert!(
                    live.contains(&id),
                    "editor_modified_subscriptions retains entry for removed tab {id}",
                );
            }
            for &id in self.markdown_preview_cache.keys() {
                debug_assert!(
                    live.contains(&id),
                    "markdown_preview_cache retains entry for removed tab {id}",
                );
            }
            for &id in &self.markdown_preview_to_refresh {
                debug_assert!(
                    live.contains(&id),
                    "markdown_preview_to_refresh retains entry for removed tab {id}",
                );
            }
            for &id in self.markdown_preview_subscriptions.keys() {
                debug_assert!(
                    live.contains(&id),
                    "markdown_preview_subscriptions retains entry for removed tab {id}",
                );
            }
        }
    }
}
