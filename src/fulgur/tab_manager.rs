use crate::fulgur::{
    Fulgur, components_utils::UNTITLED, editor_tab::EditorTab, settings::SettingsTab, tab::Tab,
};
use gpui::*;
use gpui_component::WindowExt;
use std::ops::DerefMut;

impl Fulgur {
    // Create a new tab
    // @param window: The window to create the tab in
    // @param cx: The application context
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

    // Open settings in a new tab or switch to existing settings tab
    // @param window: The window to open settings in
    // @param cx: The application context
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

    // Close a tab
    // @param tab_id: The ID of the tab to close
    // @param window: The window to close the tab in
    // @param cx: The application context
    pub(super) fn close_tab(&mut self, tab_id: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pos) = self.tabs.iter().position(|t| t.id() == tab_id) {
            if let Some(to_be_removed) = self.tabs.get_mut(pos) {
                let is_modified = if let Some(editor_tab) = to_be_removed.as_editor_mut() {
                    editor_tab.check_modified(cx)
                } else {
                    false
                };
                if is_modified {
                    let entity = cx.entity().clone();
                    window.open_dialog(cx.deref_mut(), move |modal, _, _| {
                        let entity_ok = entity.clone();
                        modal
                            .title(div().text_size(px(16.)).child("Unsaved changed"))
                            .keyboard(true)
                            .confirm()
                            .on_ok(move |_, window, cx| {
                                let entity_ok_footer = entity_ok.clone();
                                entity_ok_footer.update(cx, |this, cx| {
                                    if let Some(pos) = this.tabs.iter().position(|t| t.id() == tab_id) {
                                        this.tabs.remove(pos);
                                        this.close_tab_manage_focus(window, cx, pos);
                                        cx.notify();
                                    }
                                });
                                // Defer focus until after modal closes
                                entity_ok_footer.update(cx, |_this, cx| {
                                    cx.defer_in(window, move |this, window, cx| {
                                        this.focus_active_tab(window, cx);
                                    });
                                });
                                true
                            })
                            .on_cancel(move |_, _window, _cx| {
                                true
                            })
                            .child(div().text_size(px(14.)).child("Are you sure you want to close this tab? Your changes will be lost."))
                            .overlay_closable(false)
                            .close_button(false)
                    });
                    return;
                }
            }
            self.tabs.remove(pos);
            self.close_tab_manage_focus(window, cx, pos);
            self.focus_active_tab(window, cx);
            cx.notify();
        }
    }

    // Close a tab and manage the focus
    // @param window: The window to close the tab in
    // @param cx: The application context
    // @param pos: The position of the tab to close
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

    // Set the active tab. If search is open, re-run search on new tab.
    // @param index: The index of the tab to set as active
    // @param window: The window to set the active tab in
    // @param cx: The application context
    pub(super) fn set_active_tab(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index < self.tabs.len() {
            self.active_tab_index = Some(index);
            self.focus_active_tab(window, cx);
            if self.show_search {
                self.perform_search(window, cx);
            }
            cx.notify();
        }
    }

    // Focus the active tab's content
    // @param window: The window to focus the tab in
    // @param cx: The application context
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

    // Close all tabs
    // @param window: The window to close all tabs in
    // @param cx: The application context
    pub(super) fn close_all_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tabs.is_empty() {
            let tab_ids: Vec<usize> = self.tabs.iter().map(|tab| tab.id()).collect();
            for tab_id in tab_ids {
                // Check if tab still exists (might have been removed by previous close_tab)
                if !self.tabs.iter().any(|t| t.id() == tab_id) {
                    continue;
                }
                let is_modified = if let Some(pos) = self.tabs.iter().position(|t| t.id() == tab_id)
                {
                    if let Some(tab_mut) = self.tabs.get_mut(pos) {
                        if let Some(editor_tab_mut) = tab_mut.as_editor_mut() {
                            editor_tab_mut.check_modified(cx)
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };
                if is_modified {
                    if let Some(pos) = self.tabs.iter().position(|t| t.id() == tab_id) {
                        self.set_active_tab(pos, window, cx);
                    }
                    let entity = cx.entity().clone();
                    window.open_dialog(cx.deref_mut(), move |modal, _, _| {
                        let entity_ok = entity.clone();
                        modal
                            .title(div().text_size(px(16.)).child("Unsaved changed"))
                            .keyboard(true)
                            .confirm()
                            .on_ok(move |_, window, cx| {
                                let entity_ok_footer = entity_ok.clone();
                                entity_ok_footer.update(cx, |this, cx| {
                                    // Remove the confirmed tab
                                    if let Some(pos) = this.tabs.iter().position(|t| t.id() == tab_id) {
                                        this.tabs.remove(pos);
                                        this.close_tab_manage_focus(window, cx, pos);
                                        cx.notify();
                                    }
                                    // Continue closing remaining tabs
                                    if !this.tabs.is_empty() {
                                        this.close_all_tabs(window, cx);
                                    } else {
                                        this.active_tab_index = None;
                                        this.next_tab_id = 1;
                                        cx.notify();
                                    }
                                });
                                entity_ok_footer.update(cx, |_this, cx| {
                                    cx.defer_in(window, move |this, window, cx| {
                                        this.focus_active_tab(window, cx);
                                    });
                                });
                                true
                            })
                            .on_cancel(move |_, _window, _cx| {
                                true
                            })
                            .child(div().text_size(px(14.)).child("Are you sure you want to close this tab? Your changes will be lost."))
                            .overlay_closable(false)
                            .close_button(false)
                    });
                    return;
                } else {
                    if let Some(pos) = self.tabs.iter().position(|t| t.id() == tab_id) {
                        self.tabs.remove(pos);
                        self.close_tab_manage_focus(window, cx, pos);
                    }
                }
            }

            // All tabs closed (or none were modified)
            if self.tabs.is_empty() {
                self.active_tab_index = None;
                self.next_tab_id = 1;
            }
            cx.notify();
        }
    }

    // Close all tabs to the left of the specified index
    // @param index: The index of the tab (tabs to the left will be closed)
    // @param window: The window to close tabs in
    // @param cx: The application context
    pub(super) fn close_tabs_to_left(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index > 0 && index < self.tabs.len() {
            self.tabs.drain(0..index);
            if let Some(active_idx) = self.active_tab_index {
                if active_idx < index {
                    self.active_tab_index = Some(0);
                } else {
                    self.active_tab_index = Some(active_idx - index);
                }
            }
            self.focus_active_tab(window, cx);
            cx.notify();
        }
    }

    // Close all tabs to the right of the specified index
    // @param index: The index of the tab (tabs to the right will be closed)
    // @param window: The window to close tabs in
    // @param cx: The application context
    pub(super) fn close_tabs_to_right(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index < self.tabs.len() - 1 {
            self.tabs.truncate(index + 1);
            if let Some(active_idx) = self.active_tab_index {
                if active_idx > index {
                    // Active tab was closed, set to the rightmost remaining tab
                    self.active_tab_index = Some(index);
                }
            }
            self.focus_active_tab(window, cx);
            cx.notify();
        }
    }

    // Update the modified status of the tabs
    // @param cx: The application context
    pub(super) fn update_modified_status(&mut self, cx: &mut Context<Self>) {
        for tab in self.tabs.iter_mut() {
            if let Tab::Editor(editor_tab) = tab {
                editor_tab.check_modified(cx);
            }
        }
    }

    // Quit the application. If confirm_exit is enabled, a modal will be shown to confirm the action.
    // @param window: The window to quit the application in
    // @param cx: The application context
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
                                eprintln!("Failed to save app state: {}", e);
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
            eprintln!("Failed to save app state: {}", e);
        }
        cx.quit();
    }
}
