use crate::fulgur::{
    Fulgur, components_utils::UNTITLED, editor_tab::EditorTab, settings::SettingsTab, tab::Tab,
};
use gpui::*;
use gpui_component::WindowExt;
use std::ops::DerefMut;

impl Fulgur {
    /// Create a new tab
    ///
    /// ### Arguments
    /// - `window`: The window to create the tab in
    /// - `cx`: The application context
    pub(super) fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab = Tab::Editor(EditorTab::new(
            self.next_tab_id,
            format!("{} {}", UNTITLED, self.next_tab_id),
            window,
            cx,
            &self.settings.editor_settings,
        ));
        self.tabs.push(tab);
        self.active_tab_index = Some(self.tabs.len() - 1);
        self.next_tab_id += 1;
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    /// Open settings in a new tab or switch to existing settings tab
    ///
    /// ### Arguments
    /// - `window`: The window to open settings in
    /// - `cx`: The application context
    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.iter().position(|t| matches!(t, Tab::Settings(_))) {
            self.set_active_tab(index, window, cx);
        } else {
            let settings_tab = Tab::Settings(SettingsTab::new(self.next_tab_id, window, cx));
            self.tabs.push(settings_tab);
            self.active_tab_index = Some(self.tabs.len() - 1);
            self.next_tab_id += 1;
            cx.notify();
        }
    }

    /// Close a tab
    ///
    /// ### Arguments
    /// - `tab_id`: The ID of the tab to close
    /// - `window`: The window to close the tab in
    /// - `cx`: The application context
    pub(super) fn close_tab(&mut self, tab_id: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tabs.iter().any(|t| t.id() == tab_id) {
            return;
        }

        if self.check_tab_modified(tab_id) {
            self.show_unsaved_changes_dialog(window, cx, move |this, window, cx| {
                this.remove_tab_by_id(tab_id, window, cx);
                let _ = this.save_state(cx);
            });
        } else {
            self.remove_tab_by_id(tab_id, window, cx);
            self.focus_active_tab(window, cx);
            let _ = self.save_state(cx);
        }
    }

    /// Close a tab and manage the focus
    ///
    /// ### Arguments
    /// - `window`: The window to close the tab in
    /// - `cx`: The application context
    /// - `pos`: The position of the tab to close
    pub(super) fn close_tab_manage_focus(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        pos: usize,
    ) {
        if self.tabs.is_empty() {
            self.active_tab_index = None;
        } else {
            if self.active_tab_index.is_some() && self.active_tab_index.unwrap() >= self.tabs.len()
            {
                self.active_tab_index = Some(self.tabs.len() - 1);
            } else if self.active_tab_index.is_some() && pos < self.active_tab_index.unwrap() {
                self.active_tab_index = Some(self.active_tab_index.unwrap() - 1);
            }
        }
        self.focus_active_tab(window, cx);
    }

    /// Set the active tab. If search is open, re-run search on new tab.
    ///
    /// ### Arguments
    /// - `index`: The index of the tab to set as active
    /// - `window`: The window to set the active tab in
    /// - `cx`: The application context
    pub(super) fn set_active_tab(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index < self.tabs.len() {
            self.active_tab_index = Some(index);
            let pending_path = if let Some(Tab::Editor(editor_tab)) = self.tabs.get(index) {
                if let Some(path) = &editor_tab.file_path {
                    if self.pending_conflicts.contains_key(path) {
                        Some(path.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(path) = pending_path {
                self.pending_conflicts.remove(&path);
                self.show_file_conflict_dialog(path, index, window, cx);
            }
            self.focus_active_tab(window, cx);
            if self.show_search {
                self.perform_search(window, cx);
            }
            cx.notify();
        }
    }

    /// Focus the active tab's content
    ///
    /// ### Arguments
    /// - `window`: The window to focus the tab in
    /// - `cx`: The application context
    pub fn focus_active_tab(&self, window: &mut Window, cx: &App) {
        if let Some(active_tab_index) = self.active_tab_index {
            if let Some(active_tab) = self.tabs.get(active_tab_index) {
                match active_tab {
                    Tab::Editor(editor_tab) => {
                        let focus_handle = editor_tab.content.read(cx).focus_handle(cx);
                        window.focus(&focus_handle);
                    }
                    Tab::Settings(_) => {
                        // Settings don't have focusable content, just keep window focus
                    }
                }
            }
        }
    }

    /// Close all tabs
    ///
    /// ### Arguments
    /// - `window`: The window to close all tabs in
    /// - `cx`: The application context
    pub(super) fn close_all_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        let tab_ids: Vec<usize> = self.tabs.iter().map(|tab| tab.id()).collect();
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
                        this.close_all_tabs(window, cx);
                    } else {
                        this.active_tab_index = None;
                        this.next_tab_id = 1;
                        cx.notify();
                    }
                });
                return;
            } else {
                self.remove_tab_by_id(tab_id, window, cx);
            }
        }
        if self.tabs.is_empty() {
            self.active_tab_index = None;
            self.next_tab_id = 1;
        }
        let _ = self.save_state(cx);
        cx.notify();
    }

    /// Close all tabs to the left of the specified index
    ///
    /// ### Arguments
    /// - `index`: The index of the tab (tabs to the left will be closed)
    /// - `window`: The window to close tabs in
    /// - `cx`: The application context
    pub(super) fn close_tabs_to_left(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index == 0 || index >= self.tabs.len() {
            return;
        }
        let tab_ids: Vec<usize> = self.tabs[0..index].iter().map(|tab| tab.id()).collect();
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
                    if !this.tabs.is_empty() && index > 0 {
                        let new_index = this.tabs.len().min(index - 1);
                        if new_index > 0 {
                            this.close_tabs_to_left(new_index, window, cx);
                        }
                    }
                });
                return;
            } else {
                self.remove_tab_by_id(tab_id, window, cx);
            }
        }
        if let Some(active_idx) = self.active_tab_index {
            if active_idx >= self.tabs.len() {
                self.active_tab_index = Some(self.tabs.len().saturating_sub(1));
            }
        }
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    /// Close all tabs to the right of the specified index
    ///
    /// ### Arguments
    /// - `index`: The index of the tab (tabs to the right will be closed)
    /// - `window`: The window to close tabs in
    /// - `cx`: The application context
    pub(super) fn close_tabs_to_right(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index >= self.tabs.len() - 1 {
            return;
        }
        let tab_ids: Vec<usize> = self.tabs[(index + 1)..]
            .iter()
            .map(|tab| tab.id())
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
                    if !this.tabs.is_empty() {
                        let current_index = index.min(this.tabs.len() - 1);
                        if current_index < this.tabs.len() - 1 {
                            this.close_tabs_to_right(current_index, window, cx);
                        }
                    }
                });
                return;
            } else {
                self.remove_tab_by_id(tab_id, window, cx);
            }
        }
        if let Some(active_idx) = self.active_tab_index {
            if active_idx >= self.tabs.len() {
                self.active_tab_index = Some(self.tabs.len().saturating_sub(1));
            }
        }
        let _ = self.save_state(cx);
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    /// Close all tabs except the active one
    ///
    /// ### Arguments
    /// - `window`: The window to close tabs in
    /// - `cx`: The application context
    pub(super) fn close_other_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active_index) = self.active_tab_index else {
            return;
        };
        if self.tabs.len() <= 1 {
            return;
        }
        let active_tab_id = self.tabs.get(active_index).map(|t| t.id());
        let tab_ids: Vec<usize> = self
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(idx, tab)| {
                if idx != active_index {
                    Some(tab.id())
                } else {
                    None
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
                let active_tab_id_for_closure = active_tab_id;
                self.show_unsaved_changes_dialog(window, cx, move |this, window, cx| {
                    this.remove_tab_by_id(tab_id, window, cx);
                    if !this.tabs.is_empty() {
                        if let Some(remaining_active_id) = active_tab_id_for_closure {
                            if let Some(new_active_pos) =
                                this.tabs.iter().position(|t| t.id() == remaining_active_id)
                            {
                                this.active_tab_index = Some(new_active_pos);
                            }
                        }
                        this.close_other_tabs(window, cx);
                    }
                });
                return;
            } else {
                self.remove_tab_by_id(tab_id, window, cx);
            }
        }
        if let Some(remaining_active_id) = active_tab_id {
            if let Some(new_active_pos) =
                self.tabs.iter().position(|t| t.id() == remaining_active_id)
            {
                self.active_tab_index = Some(new_active_pos);
            }
        }
        self.focus_active_tab(window, cx);
        let _ = self.save_state(cx);
        cx.notify();
    }

    /// Update the modified status of the tabs
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub(super) fn update_modified_status(&mut self, cx: &mut Context<Self>) {
        for tab in self.tabs.iter_mut() {
            if let Tab::Editor(editor_tab) = tab {
                editor_tab.check_modified(cx);
            }
        }
    }

    /// Check if a tab has unsaved modifications
    ///
    /// ### Arguments
    /// - `tab_id`: The ID of the tab to check
    ///
    /// ### Returns
    /// - `True`: If the tab has unsaved changes, `False` otherwise
    fn check_tab_modified(&self, tab_id: usize) -> bool {
        if let Some(pos) = self.tabs.iter().position(|t| t.id() == tab_id) {
            if let Some(tab) = self.tabs.get(pos) {
                if let Tab::Editor(editor_tab) = tab {
                    return editor_tab.modified;
                }
            }
        }
        false
    }

    /// Remove a tab by ID and manage focus
    ///
    /// ### Arguments
    /// - `tab_id`: The ID of the tab to remove
    /// - `window`: The window context
    /// - `cx`: The application context
    fn remove_tab_by_id(&mut self, tab_id: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pos) = self.tabs.iter().position(|t| t.id() == tab_id) {
            let path_to_unwatch = if let Some(Tab::Editor(editor_tab)) = self.tabs.get(pos) {
                editor_tab.file_path.clone()
            } else {
                None
            };
            self.tabs.remove(pos);
            self.close_tab_manage_focus(window, cx, pos);
            if let Some(path) = path_to_unwatch {
                self.unwatch_file(&path);
            }
            cx.notify();
        }
    }

    /// Show confirmation dialog for unsaved changes
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    /// - `on_confirm`: Callback executed when user confirms closing
    fn show_unsaved_changes_dialog<F>(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        on_confirm: F,
    ) where
        F: Fn(&mut Fulgur, &mut Window, &mut Context<Fulgur>) + 'static + Clone,
    {
        let entity = cx.entity().clone();
        window.open_dialog(cx.deref_mut(), move |modal, _, _| {
            let entity_ok = entity.clone();
            let on_confirm_clone = on_confirm.clone();
            modal
                .title(div().text_size(px(16.)).child("Unsaved changed"))
                .keyboard(true)
                .confirm()
                .on_ok(move |_, window, cx| {
                    let entity_ok_footer = entity_ok.clone();
                    let on_confirm_inner = on_confirm_clone.clone();
                    entity_ok_footer.update(cx, |this, cx| {
                        on_confirm_inner(this, window, cx);
                    });
                    entity_ok_footer.update(cx, |_this, cx| {
                        cx.defer_in(window, move |this, window, cx| {
                            this.focus_active_tab(window, cx);
                        });
                    });
                    true
                })
                .on_cancel(move |_, _window, _cx| true)
                .child(
                    div().text_size(px(14.)).child(
                        "Are you sure you want to close this tab? Your changes will be lost.",
                    ),
                )
                .overlay_closable(false)
                .close_button(false)
        });
    }

    /// Quit the application. If confirm_exit is enabled, a modal will be shown to confirm the action.
    ///
    /// ### Arguments
    /// - `window`: The window to quit the application in
    /// - `cx`: The application context
    pub fn quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.app_settings.confirm_exit {
            let entity = cx.entity().clone();
            window.open_dialog(cx.deref_mut(), move |modal, _, _| {
                let entity_ok = entity.clone();
                modal
                    .title(div().text_size(px(16.)).child("Quit Fulgur"))
                    .keyboard(true)
                    .confirm()
                    .on_ok(move |_, _window, cx| {
                        let entity_ok_footer = entity_ok.clone();
                        entity_ok_footer.update(cx, |this, cx| {
                            if let Err(e) = this.save_state(cx) {
                                log::error!("Failed to save app state: {}", e);
                            }
                        });
                        cx.quit();
                        true
                    })
                    .on_cancel(move |_, _window, _cx| true)
                    .child(
                        div()
                            .text_size(px(14.))
                            .child("Are you sure you want to quit Fulgur?"),
                    )
                    .overlay_closable(false)
                    .close_button(false)
            });
            return;
        }
        if let Err(e) = self.save_state(cx) {
            log::error!("Failed to save app state: {}", e);
        }
        cx.quit();
    }
}
