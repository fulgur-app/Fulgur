use crate::fulgur::{
    Fulgur,
    tab::{Tab, TabId},
};
use gpui::{Context, Window};

impl Fulgur {
    /// Close a tab
    ///
    /// ### Arguments
    /// - `tab_id`: The ID of the tab to close
    /// - `window`: The window to close the tab in
    /// - `cx`: The application context
    pub fn close_tab(&mut self, tab_id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tabs.iter().any(|t| t.read(cx).id() == tab_id) {
            return;
        }

        if self.check_tab_modified(tab_id, cx) {
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
        if let Some(tab_id) = self.active_tab_id {
            self.close_tab(tab_id, window, cx);
        }
    }

    /// Re-anchor the active tab after a removal and manage the focus
    ///
    /// ### Arguments
    /// - `window`: The window to close the tab in
    /// - `cx`: The application context
    /// - `pos`: The position the removed tab occupied before removal
    pub fn close_tab_manage_focus(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        pos: usize,
    ) {
        if self.tabs.is_empty() {
            self.active_tab_id = None;
        } else if self.active_tab_index(cx).is_none() {
            let fallback = pos.min(self.tabs.len() - 1);
            self.active_tab_id = self.tabs.get(fallback).map(|t| t.read(cx).id());
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
        let tab_ids: Vec<TabId> = self.tabs.iter().map(|t| t.read(cx).id()).collect();
        for tab_id in tab_ids {
            if !self.tabs.iter().any(|t| t.read(cx).id() == tab_id) {
                continue;
            }
            if self.check_tab_modified(tab_id, cx) {
                if let Some(pos) = self.tab_index_of(tab_id, cx) {
                    self.set_active_tab(pos, window, cx);
                }
                self.show_unsaved_changes_dialog(window, cx, move |this, window, cx| {
                    this.remove_tab_by_id(tab_id, window, cx);
                    if this.tabs.is_empty() {
                        this.active_tab_id = None;
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
            self.active_tab_id = None;
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
        let keep_id = self.tabs[index].read(cx).id();
        let tab_ids: Vec<TabId> = self.tabs[0..index]
            .iter()
            .map(|t| t.read(cx).id())
            .collect();
        for tab_id in tab_ids {
            if !self.tabs.iter().any(|t| t.read(cx).id() == tab_id) {
                continue;
            }
            if self.check_tab_modified(tab_id, cx) {
                if let Some(pos) = self.tab_index_of(tab_id, cx) {
                    self.set_active_tab(pos, window, cx);
                }
                self.show_unsaved_changes_dialog(window, cx, move |this, window, cx| {
                    this.remove_tab_by_id(tab_id, window, cx);
                    if let Some(boundary_index) = this.tab_index_of(keep_id, cx)
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
        let keep_id = self.tabs[index].read(cx).id();
        let tab_ids: Vec<TabId> = self.tabs[(index + 1)..]
            .iter()
            .map(|t| t.read(cx).id())
            .collect();
        for tab_id in tab_ids {
            if !self.tabs.iter().any(|t| t.read(cx).id() == tab_id) {
                continue;
            }
            if self.check_tab_modified(tab_id, cx) {
                if let Some(pos) = self.tab_index_of(tab_id, cx) {
                    self.set_active_tab(pos, window, cx);
                }
                self.show_unsaved_changes_dialog(window, cx, move |this, window, cx| {
                    this.remove_tab_by_id(tab_id, window, cx);
                    if let Some(boundary_index) = this.tab_index_of(keep_id, cx)
                        && boundary_index < this.tabs.len() - 1
                    {
                        this.close_tabs_to_right(boundary_index, window, cx);
                        return;
                    }
                    this.save_state_async(cx, window);
                    cx.notify();
                });
                return;
            }
            self.remove_tab_by_id(tab_id, window, cx);
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
        let Some(active_tab_id) = self.active_tab_id else {
            return;
        };
        if self.tabs.len() <= 1 {
            return;
        }
        let tab_ids: Vec<TabId> = self
            .tabs
            .iter()
            .map(|t| t.read(cx).id())
            .filter(|id| *id != active_tab_id)
            .collect();
        for tab_id in tab_ids {
            if !self.tabs.iter().any(|t| t.read(cx).id() == tab_id) {
                continue;
            }
            if self.check_tab_modified(tab_id, cx) {
                if let Some(pos) = self.tab_index_of(tab_id, cx) {
                    self.set_active_tab(pos, window, cx);
                }
                self.show_unsaved_changes_dialog(window, cx, move |this, window, cx| {
                    this.remove_tab_by_id(tab_id, window, cx);
                    if this.tabs.iter().any(|t| t.read(cx).id() == active_tab_id) {
                        this.active_tab_id = Some(active_tab_id);
                    }
                    this.close_other_tabs(window, cx);
                });
                return;
            }
            self.remove_tab_by_id(tab_id, window, cx);
        }
        self.active_tab_id = Some(active_tab_id);
        self.focus_active_tab(window, cx);
        self.save_state_async(cx, window);
        cx.notify();
    }

    /// Check if a tab has unsaved modifications
    ///
    /// ### Arguments
    /// - `tab_id`: The ID of the tab to check
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `True`: If the tab has unsaved changes, `False` otherwise
    fn check_tab_modified(&self, tab_id: TabId, cx: &gpui::App) -> bool {
        if let Some(Tab::Editor(editor_tab)) = self
            .tabs
            .iter()
            .map(|t| t.read(cx))
            .find(|t| t.id() == tab_id)
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
    pub fn remove_tab_by_id(&mut self, tab_id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pos) = self.tab_index_of(tab_id, cx) {
            let removed_tab = self.tabs[pos].read(cx);
            let path_to_unwatch = match removed_tab {
                Tab::Editor(editor_tab) => editor_tab.file_path().cloned(),
                _ => None,
            };
            let linked_preview_id = if matches!(removed_tab, Tab::Editor(_)) {
                self.tabs.iter().map(|t| t.read(cx)).find_map(|t| match t {
                    Tab::MarkdownPreview(p) if p.source_tab_id == tab_id => Some(p.id),
                    _ => None,
                })
            } else {
                None
            };
            // Dropping the entity releases the tab's content subscription and
            // the rest of its owned state with it.
            self.tabs.remove(pos);
            self.close_tab_manage_focus(window, cx, pos);
            self.pending_remote_restore.remove(&tab_id);
            self.inflight_remote_restore.remove(&tab_id);
            self.latest_remote_open_request_by_tab.remove(&tab_id);
            self.latest_remote_save_request_by_tab.remove(&tab_id);
            self.clear_log_tail(tab_id);
            if let Some(path) = path_to_unwatch {
                self.unwatch_file(&path);
            }
            if let Some(preview_id) = linked_preview_id
                && let Some(preview_pos) = self.tab_index_of(preview_id, cx)
            {
                self.tabs.remove(preview_pos);
                self.close_tab_manage_focus(window, cx, preview_pos);
            }
            cx.notify();
        }
    }
}
