use gpui::*;
use std::ops::DerefMut;
use gpui_component::ContextModal;
use gpui_component::button::{Button, ButtonVariants};
use crate::lightspeed::{Lightspeed, editor_tab::EditorTab, settings::SettingsTab, tab::Tab};

impl Lightspeed {
    /// Create a new tab
    /// @param window: The window to create the tab in
    /// @param cx: The application context
    pub(super) fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab = Tab::Editor(EditorTab::new(
            self.next_tab_id,
            format!("Untitled {}", self.next_tab_id),
            window,
            cx,
        ));
        self.tabs.push(tab);
        self.active_tab_index = Some(self.tabs.len() - 1);
        self.next_tab_id += 1;
        
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    /// Open settings in a new tab or switch to existing settings tab
    /// @param window: The window to open settings in
    /// @param cx: The application context
    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Check if settings tab already exists
        if let Some(index) = self.tabs.iter().position(|t| matches!(t, Tab::Settings(_))) {
            // Settings tab exists, switch to it
            self.set_active_tab(index, window, cx);
        } else {
            // Create new settings tab with unique ID
            let settings_tab = Tab::Settings(SettingsTab::new(self.next_tab_id, window, cx));
            self.tabs.push(settings_tab);
            self.active_tab_index = Some(self.tabs.len() - 1);
            self.next_tab_id += 1;
            cx.notify();
        }
    }

    /// Close a tab
    /// @param tab_id: The ID of the tab to close
    /// @param window: The window to close the tab in
    /// @param cx: The application context
    pub(super) fn close_tab(&mut self, tab_id: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pos) = self.tabs.iter().position(|t| t.id() == tab_id) {
            if let Some(to_be_removed) = self.tabs.get_mut(pos) {
                // Check if the tab has been modified (only for editor tabs)
                let is_modified = if let Some(editor_tab) = to_be_removed.as_editor_mut() {
                    editor_tab.check_modified(cx)
                } else {
                    false // Settings tabs are never modified
                };
                if is_modified {
                    // Get the entity reference to use in the modal callbacks
                    let entity = cx.entity().clone();
                    
                    window.open_modal(cx.deref_mut(), move |modal, _, _| {
                        // Clone entity for on_ok closure
                        let entity_ok = entity.clone();
                        
                        // Return the modal builder
                        modal
                            .title(div().text_size(px(16.)).child("Unsaved changed"))
                            .child(div().text_size(px(14.)).child("Are you sure you want to close this tab? Your changes will be lost."))
                            .footer(move |_, _, _window, _cx| {
                                let entity_ok_footer = entity_ok.clone();
                                vec![
                                    Button::new("cancel")
                                        .label("Cancel")
                                        .on_click(move |_, window, cx| {
                                            window.close_modal(cx);
                                        })
                                        .into_any_element(),
                                    Button::new("ok")
                                        .label("Close")
                                        .primary()
                                        .on_click(move |_, window, cx| {
                                            // Remove the tab and adjust indices
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
                                            
                                            window.close_modal(cx);
                                        })
                                        .into_any_element(),
                                ]
                            })
                            .overlay_closable(false)
                            .show_close(false)
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

    /// Close a tab and manage the focus
    /// @param window: The window to close the tab in
    /// @param cx: The application context
    /// @param pos: The position of the tab to close
    pub(super) fn close_tab_manage_focus(&mut self, window: &mut Window, cx: &mut Context<Self>, pos: usize) {
        // If no tabs left, create a new one
        if self.tabs.is_empty() {
            self.active_tab_index = None;
        } else {
            // Adjust active index
            if self.active_tab_index.is_some() && self.active_tab_index.unwrap() >= self.tabs.len() {
                self.active_tab_index = Some(self.tabs.len() - 1);
            } else if self.active_tab_index.is_some() && pos < self.active_tab_index.unwrap() {
                self.active_tab_index = Some(self.active_tab_index.unwrap() - 1);
            }
        }
        
        self.focus_active_tab(window, cx);
    }

    /// Set the active tab
    /// @param index: The index of the tab to set as active
    /// @param window: The window to set the active tab in
    /// @param cx: The application context
    pub(super) fn set_active_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index < self.tabs.len() {
            self.active_tab_index = Some(index);
            self.focus_active_tab(window, cx);
            
            // If search is open, re-run search on new tab
            if self.show_search {
                self.perform_search(window, cx);
            }
            
            cx.notify();
        }
    }

    /// Focus the active tab's content
    /// @param window: The window to focus the tab in
    /// @param cx: The application context
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
    /// @param window: The window to close all tabs in
    /// @param cx: The application context
    pub(super) fn close_all_tabs(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.tabs.is_empty() {
            self.tabs.clear();
            self.active_tab_index = None;
            self.next_tab_id = 1;
            cx.notify();
        }
    }

    /// Update the modified status of the tabs
    /// @param cx: The application context
    pub(super) fn update_modified_status(&mut self, cx: &mut Context<Self>) {
        for tab in self.tabs.iter_mut() {
            if let Tab::Editor(editor_tab) = tab {
                editor_tab.check_modified(cx);
            }
        }
    }

    /// Quit the application
    /// @param window: The window to quit the application in
    /// @param cx: The application context
    pub(super) fn quit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // Save state before quitting
        if let Err(e) = self.save_state(cx) {
            eprintln!("Failed to save app state: {}", e);
        }
        cx.quit();
    }
}

