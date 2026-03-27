use crate::fulgur::{
    Fulgur,
    languages::supported_languages::SupportedLanguage,
    settings::MarkdownPreviewMode,
    tab::Tab,
    ui::{
        components_utils::UNTITLED,
        tabs::{
            editor_tab::{EditorTab, FromDuplicateParams},
            markdown_preview_tab::MarkdownPreviewTab,
            settings_tab::SettingsTab,
        },
    },
};
use gpui::*;
use gpui_component::{
    WindowExt,
    select::{SearchableVec, SelectEvent},
};
use std::{ops::DerefMut, path::PathBuf};

impl Fulgur {
    /// Create a new tab
    ///
    /// ### Arguments
    /// - `window`: The window to create the tab in
    /// - `cx`: The application context
    pub fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab = Tab::Editor(EditorTab::new(
            self.next_tab_id,
            format!("{} {}", UNTITLED, self.next_tab_id),
            window,
            cx,
            &self.settings.editor_settings,
        ));
        self.tabs.push(tab);
        self.active_tab_index = Some(self.tabs.len() - 1);
        self.pending_tab_scroll = Some(self.tabs.len() - 1);
        self.next_tab_id += 1;
        self.focus_active_tab(window, cx);
        if let Err(e) = self.save_state(cx, window) {
            log::error!("Failed to save app state after creating tab: {}", e);
            self.pending_notification = Some((
                gpui_component::notification::NotificationType::Warning,
                format!("Tab created but failed to save state: {}", e).into(),
            ));
        }
        cx.notify();
    }

    /// Open settings in a new tab or switch to existing settings tab
    ///
    /// ### Arguments
    /// - `window`: The window to open settings in
    /// - `cx`: The application context
    pub fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.iter().position(|t| matches!(t, Tab::Settings(_))) {
            self.set_active_tab(index, window, cx);
        } else {
            let tab = SettingsTab::new(
                self.next_tab_id,
                &self.settings.editor_settings.font_family,
                window,
                cx,
            );
            let font_select_subscription = cx.subscribe(
                &tab.font_family_select,
                |this: &mut Self,
                 _,
                 ev: &SelectEvent<SearchableVec<SharedString>>,
                 cx: &mut Context<Self>| {
                    if let SelectEvent::Confirm(Some(value)) = ev {
                        this.settings.editor_settings.font_family = value.to_string();
                        let _ = this.update_and_propagate_settings(cx);
                    }
                },
            );
            self._font_select_subscription = Some(font_select_subscription);
            let settings_tab = Tab::Settings(tab);
            self.tabs.push(settings_tab);
            self.active_tab_index = Some(self.tabs.len() - 1);
            self.pending_tab_scroll = Some(self.tabs.len() - 1);
            self.next_tab_id += 1;
            if let Err(e) = self.save_state(cx, window) {
                log::error!("Failed to save app state after opening settings: {}", e);
                self.pending_notification = Some((
                    gpui_component::notification::NotificationType::Warning,
                    format!("Settings opened but failed to save state: {}", e).into(),
                ));
            }
            cx.notify();
        }
    }

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
                if let Err(e) = this.save_state(cx, window) {
                    log::error!("Failed to save app state after closing tab: {}", e);
                    this.pending_notification = Some((
                        gpui_component::notification::NotificationType::Warning,
                        format!("Tab closed but failed to save state: {}", e).into(),
                    ));
                }
            });
        } else {
            self.remove_tab_by_id(tab_id, window, cx);
            self.focus_active_tab(window, cx);
            if let Err(e) = self.save_state(cx, window) {
                log::error!("Failed to save app state after closing tab: {}", e);
                self.pending_notification = Some((
                    gpui_component::notification::NotificationType::Warning,
                    format!("Tab closed but failed to save state: {}", e).into(),
                ));
            }
        }
    }

    /// Close the currently active tab
    ///
    /// This is a convenience method for the CloseFile action that closes
    /// whichever tab is currently active.
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

    /// Set the active tab. If search is open, re-run search on new tab.
    ///
    /// ### Arguments
    /// - `index`: The index of the tab to set as active
    /// - `window`: The window to set the active tab in
    /// - `cx`: The application context
    pub fn set_active_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index < self.tabs.len() {
            self.active_tab_index = Some(index);
            self.tab_scroll_handle.scroll_to_item(index);
            let pending_path = if let Some(Tab::Editor(editor_tab)) = self.tabs.get(index) {
                if let Some(path) = &editor_tab.file_path {
                    if self
                        .file_watch_state
                        .pending_conflicts
                        .contains_key::<PathBuf>(path)
                    {
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
                self.file_watch_state.pending_conflicts.remove(&path);
                self.show_file_conflict_dialog(path, index, window, cx);
            }
            self.focus_active_tab(window, cx);
            if self.search_state.show_search {
                self.search_state.search_matches.clear();
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
    pub fn focus_active_tab(&self, window: &mut Window, cx: &mut App) {
        if let Some(active_tab_index) = self.active_tab_index
            && let Some(active_tab) = self.tabs.get(active_tab_index)
        {
            match active_tab {
                Tab::Editor(editor_tab) => {
                    let focus_handle = editor_tab.content.read(cx).focus_handle(cx);
                    window.focus(&focus_handle, cx);
                }
                Tab::Settings(_) => {
                    // Settings don't have focusable content, just keep window focus
                }
                Tab::MarkdownPreview(_) => {
                    // Preview tabs are read-only, no focusable input content
                }
            }
        }
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
                        if let Err(e) = this.save_state(cx, window) {
                            log::error!("Failed to save state after closing all tabs: {}", e);
                            this.pending_notification = Some((
                                gpui_component::notification::NotificationType::Warning,
                                format!("Tabs closed but failed to save state: {}", e).into(),
                            ));
                        }
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
        if let Err(e) = self.save_state(cx, window) {
            log::error!("Failed to save app state after closing all tabs: {}", e);
            self.pending_notification = Some((
                gpui_component::notification::NotificationType::Warning,
                format!("Tabs closed but failed to save state: {}", e).into(),
            ));
        }
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
                            return;
                        }
                    }
                    if let Err(e) = this.save_state(cx, window) {
                        log::error!("Failed to save state after closing tabs to left: {}", e);
                        this.pending_notification = Some((
                            gpui_component::notification::NotificationType::Warning,
                            format!("Tabs closed but failed to save state: {}", e).into(),
                        ));
                    }
                    cx.notify();
                });
                return;
            } else {
                self.remove_tab_by_id(tab_id, window, cx);
            }
        }
        if let Some(active_idx) = self.active_tab_index
            && active_idx >= self.tabs.len()
        {
            self.active_tab_index = Some(self.tabs.len().saturating_sub(1));
        }
        if let Err(e) = self.save_state(cx, window) {
            log::error!("Failed to save app state after closing tabs to left: {}", e);
            self.pending_notification = Some((
                gpui_component::notification::NotificationType::Warning,
                format!("Tabs closed but failed to save state: {}", e).into(),
            ));
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
    pub fn close_tabs_to_right(
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
                            return;
                        }
                    }
                    if let Err(e) = this.save_state(cx, window) {
                        log::error!("Failed to save state after closing tabs to right: {}", e);
                        this.pending_notification = Some((
                            gpui_component::notification::NotificationType::Warning,
                            format!("Tabs closed but failed to save state: {}", e).into(),
                        ));
                    }
                    cx.notify();
                });
                return;
            } else {
                self.remove_tab_by_id(tab_id, window, cx);
            }
        }
        if let Some(active_idx) = self.active_tab_index
            && active_idx >= self.tabs.len()
        {
            self.active_tab_index = Some(self.tabs.len().saturating_sub(1));
        }
        if let Err(e) = self.save_state(cx, window) {
            log::error!(
                "Failed to save app state after closing tabs to right: {}",
                e
            );
            self.pending_notification = Some((
                gpui_component::notification::NotificationType::Warning,
                format!("Tabs closed but failed to save state: {}", e).into(),
            ));
        }
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
                        if let Some(remaining_active_id) = active_tab_id_for_closure
                            && let Some(new_active_pos) =
                                this.tabs.iter().position(|t| t.id() == remaining_active_id)
                        {
                            this.active_tab_index = Some(new_active_pos);
                        }
                        this.close_other_tabs(window, cx);
                    }
                });
                return;
            } else {
                self.remove_tab_by_id(tab_id, window, cx);
            }
        }
        if let Some(remaining_active_id) = active_tab_id
            && let Some(new_active_pos) =
                self.tabs.iter().position(|t| t.id() == remaining_active_id)
        {
            self.active_tab_index = Some(new_active_pos);
        }
        self.focus_active_tab(window, cx);
        if let Err(e) = self.save_state(cx, window) {
            log::error!("Failed to save app state after closing other tabs: {}", e);
            self.pending_notification = Some((
                gpui_component::notification::NotificationType::Warning,
                format!("Tabs closed but failed to save state: {}", e).into(),
            ));
        }
        cx.notify();
    }

    /// Duplicate a tab and insert it immediately to the right of the original
    ///
    /// The duplicate is an editor tab with the same content, language, and encoding, but no
    /// file path (it is treated as unsaved). Only editor tabs can be duplicated; calling this
    /// with the index of a non-editor tab is a no-op.
    ///
    /// ### Arguments
    /// - `index`: The index of the tab to duplicate
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn duplicate_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Tab::Editor(editor_tab)) = self.tabs.get(index) else {
            return;
        };
        let current_content = editor_tab.content.read(cx).text().to_string();
        let language = editor_tab.language;
        let raw_title = editor_tab.title.to_string();
        let encoding = editor_tab.encoding.clone();
        let settings = self.settings.editor_settings.clone();
        let clean_title: SharedString = raw_title.trim_end_matches(" •").trim().to_string().into();
        let new_tab = Tab::Editor(EditorTab::from_duplicate(
            FromDuplicateParams {
                id: self.next_tab_id,
                title: clean_title,
                current_content,
                encoding,
                language,
            },
            window,
            cx,
            &settings,
        ));
        let insert_pos = index + 1;
        self.tabs.insert(insert_pos, new_tab);
        self.active_tab_index = Some(insert_pos);
        self.pending_tab_scroll = Some(insert_pos);
        self.next_tab_id += 1;
        self.focus_active_tab(window, cx);
        if let Err(e) = self.save_state(cx, window) {
            log::error!("Failed to save app state after duplicating tab: {}", e);
            self.pending_notification = Some((
                gpui_component::notification::NotificationType::Warning,
                format!("Tab duplicated but failed to save state: {}", e).into(),
            ));
        }
        cx.notify();
    }

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
                editor_tab.file_path.clone()
            } else {
                None
            };
            let linked_preview_id = if matches!(self.tabs.get(pos), Some(Tab::Editor(_))) {
                self.tabs
                    .iter()
                    .find(|t| matches!(t, Tab::MarkdownPreview(p) if p.source_tab_id == tab_id))
                    .map(|t| t.id())
            } else {
                None
            };
            self.tabs.remove(pos);
            self.close_tab_manage_focus(window, cx, pos);
            if let Some(path) = path_to_unwatch {
                self.unwatch_file(&path);
            }
            if let Some(preview_id) = linked_preview_id
                && let Some(preview_pos) = self.tabs.iter().position(|t| t.id() == preview_id)
            {
                self.tabs.remove(preview_pos);
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
            cx.notify();
        }
    }

    /// Show confirmation dialog for unsaved changes
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    /// - `on_confirm`: Callback executed when user confirms closing
    pub fn show_unsaved_changes_dialog<F>(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        on_confirm: F,
    ) where
        F: Fn(&mut Fulgur, &mut Window, &mut Context<Fulgur>) + 'static + Clone,
    {
        let entity = cx.entity().clone();
        window.open_alert_dialog(cx.deref_mut(), move |modal, _, _| {
            let entity_ok = entity.clone();
            let on_confirm_clone = on_confirm.clone();
            modal
                .title(div().text_size(px(16.)).child("Unsaved changes"))
                .keyboard(true)
                .show_cancel(true)
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
            window.open_alert_dialog(cx.deref_mut(), move |modal, _, _| {
                let entity_ok = entity.clone();
                modal
                    .title(div().text_size(px(16.)).child("Quit Fulgur"))
                    .keyboard(true)
                    .show_cancel(true)
                    .on_ok(move |_, window: &mut Window, cx: &mut App| {
                        let entity_ok_footer = entity_ok.clone();
                        let save_result =
                            entity_ok_footer.update(cx, |this, cx| this.save_state(cx, window));
                        if let Err(e) = save_result {
                            log::error!("Failed to save app state on quit: {}", e);
                            entity_ok_footer.update(cx, |this, _cx| {
                                this.pending_notification = Some((
                                    gpui_component::notification::NotificationType::Error,
                                    format!(
                                        "Failed to save application state: {}. Quit anyway?",
                                        e
                                    )
                                    .into(),
                                ));
                            });
                            cx.refresh_windows();
                            return false; // Don't quit, show notification
                        }
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
        if let Err(e) = self.save_state(cx, window) {
            log::error!("Failed to save app state on quit: {}", e);
            self.pending_notification = Some((
                gpui_component::notification::NotificationType::Error,
                format!("Failed to save application state: {}. Try again or close the app to quit without saving.", e).into(),
            ));
            cx.notify();
            return; // Don't quit, show notification and let user try again
        }
        cx.quit();
    }

    /// Open or close the Markdown preview tab for the active editor tab.
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn open_markdown_preview_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.editor_settings.markdown_settings.preview_mode
            != MarkdownPreviewMode::DedicatedTab
        {
            return;
        }
        let Some(editor_tab) = self.get_active_editor_tab() else {
            return;
        };
        let editor_id = editor_tab.id;
        if let Some(preview_id) = self
            .tabs
            .iter()
            .find(|t| matches!(t, Tab::MarkdownPreview(p) if p.source_tab_id == editor_id))
            .map(|t| t.id())
        {
            self.remove_tab_by_id(preview_id, window, cx);
        } else {
            let Some(editor_tab) = self.get_active_editor_tab() else {
                return;
            };
            let title = SharedString::from(format!("Preview - {}", editor_tab.title));
            let content = editor_tab.content.clone();
            let editor_pos = self.active_tab_index.unwrap_or(0);
            let preview_tab = Tab::MarkdownPreview(MarkdownPreviewTab {
                id: self.next_tab_id,
                title,
                source_tab_id: editor_id,
                content,
            });
            self.tabs.insert(editor_pos + 1, preview_tab);
            self.pending_tab_scroll = Some(editor_pos + 1);
            self.next_tab_id += 1;
            cx.notify();
        }
    }

    /// Insert Markdown preview tabs for all eligible editor tabs.
    pub fn insert_preview_tabs_for_markdown(&mut self) {
        let settings = &self.settings.editor_settings.markdown_settings;
        if settings.preview_mode != MarkdownPreviewMode::DedicatedTab
            || !settings.show_markdown_preview
        {
            return;
        }
        let original_count = self.tabs.len();
        let mut offset = 0;
        for orig_idx in 0..original_count {
            let actual_idx = orig_idx + offset;
            let info = match self.tabs.get(actual_idx) {
                Some(Tab::Editor(et))
                    if et.language == SupportedLanguage::Markdown
                        || et.language == SupportedLanguage::MarkdownInline =>
                {
                    Some((et.id, et.title.clone(), et.content.clone()))
                }
                _ => None,
            };
            if let Some((editor_id, title, content)) = info {
                let preview_tab = Tab::MarkdownPreview(MarkdownPreviewTab {
                    id: self.next_tab_id,
                    title: SharedString::from(format!("Preview - {}", title)),
                    source_tab_id: editor_id,
                    content,
                });
                self.tabs.insert(actual_idx + 1, preview_tab);
                self.next_tab_id += 1;
                offset += 1;
            }
        }
    }

    /// Insert a Markdown preview tab after the given editor tab if conditions are met.
    ///
    /// ### Arguments
    /// - `editor_tab_index`: Index of the editor tab in `self.tabs`
    pub fn maybe_open_markdown_preview_for_editor(&mut self, editor_tab_index: usize) {
        let settings = &self.settings.editor_settings.markdown_settings;
        if settings.preview_mode != MarkdownPreviewMode::DedicatedTab
            || !settings.show_markdown_preview
        {
            return;
        }
        let info = match self.tabs.get(editor_tab_index) {
            Some(Tab::Editor(et))
                if et.language == SupportedLanguage::Markdown
                    || et.language == SupportedLanguage::MarkdownInline =>
            {
                Some((et.id, et.title.clone(), et.content.clone()))
            }
            _ => None,
        };
        if let Some((editor_id, title, content)) = info {
            let preview_tab = Tab::MarkdownPreview(MarkdownPreviewTab {
                id: self.next_tab_id,
                title: SharedString::from(format!("Preview - {}", title)),
                source_tab_id: editor_id,
                content,
            });
            self.tabs.insert(editor_tab_index + 1, preview_tab);
            self.next_tab_id += 1;
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

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::Fulgur;
    use crate::fulgur::{
        languages::supported_languages::SupportedLanguage, settings::Settings,
        shared_state::SharedAppState, tab::Tab, window_manager::WindowManager,
    };
    use gpui::{
        AppContext, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext,
        Window, div,
    };
    use parking_lot::Mutex;
    use std::{cell::RefCell, path::PathBuf, sync::Arc};

    struct EmptyView;

    impl Render for EmptyView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let mut settings = Settings::new();
            settings.editor_settings.watch_files = false;
            let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
            cx.set_global(SharedAppState::new(settings, pending_files));
            cx.set_global(WindowManager::new());
        });

        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        let window = cx
            .update(|cx| {
                cx.open_window(Default::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                    *fulgur_slot.borrow_mut() = Some(fulgur);
                    cx.new(|_| EmptyView)
                })
            })
            .expect("failed to open test window");

        let visual_cx = VisualTestContext::from_window(window.into(), cx);
        visual_cx.run_until_parked();
        let fulgur = fulgur_slot
            .into_inner()
            .expect("failed to capture Fulgur entity");
        (fulgur, visual_cx)
    }

    // ========== new_tab tests ==========

    #[gpui::test]
    fn test_new_tab_adds_tab_and_sets_as_active(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let initial_count = this.tabs.len();
                this.new_tab(window, cx);
                assert_eq!(this.tabs.len(), initial_count + 1);
                assert_eq!(this.active_tab_index, Some(this.tabs.len() - 1));
            });
        });
    }

    #[gpui::test]
    fn test_new_tab_increments_next_tab_id(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let id_before = this.next_tab_id;
                this.new_tab(window, cx);
                assert_eq!(this.next_tab_id, id_before + 1);
                this.new_tab(window, cx);
                assert_eq!(this.next_tab_id, id_before + 2);
            });
        });
    }

    #[gpui::test]
    fn test_new_tab_produces_untitled_editor_tab_without_file_path(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                let last = this.tabs.last().expect("expected at least one tab");
                let editor = last.as_editor().expect("expected an editor tab");
                assert!(editor.file_path.is_none());
                assert!(!editor.modified);
            });
        });
    }

    // ========== open_settings tests ==========

    #[gpui::test]
    fn test_open_settings_adds_settings_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let initial_count = this.tabs.len();
                this.open_settings(window, cx);
                assert_eq!(this.tabs.len(), initial_count + 1);
                assert!(matches!(this.tabs.last(), Some(Tab::Settings(_))));
            });
        });
    }

    #[gpui::test]
    fn test_open_settings_switches_to_existing_settings_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.open_settings(window, cx);
                let count_after_first = this.tabs.len();
                this.open_settings(window, cx);
                assert_eq!(this.tabs.len(), count_after_first);
            });
        });
    }

    // ========== close_tab tests ==========

    #[gpui::test]
    fn test_close_tab_removes_unmodified_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                let count_before = this.tabs.len();
                let tab_id = this.tabs.last().expect("expected tab").id();
                this.close_tab(tab_id, window, cx);
                assert_eq!(this.tabs.len(), count_before - 1);
                assert!(!this.tabs.iter().any(|t| t.id() == tab_id));
            });
        });
    }

    #[gpui::test]
    fn test_close_tab_is_noop_for_unknown_id(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let count_before = this.tabs.len();
                this.close_tab(usize::MAX, window, cx);
                assert_eq!(this.tabs.len(), count_before);
            });
        });
    }

    #[gpui::test]
    fn test_close_tab_keeps_active_index_valid_when_closing_before_active(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                // Start with one tab (index 0). Add a second tab (index 1) and switch to it.
                this.new_tab(window, cx);
                this.set_active_tab(1, window, cx);
                assert_eq!(this.active_tab_index, Some(1));

                // Close the tab at index 0 (before the active one).
                let first_id = this.tabs[0].id();
                this.close_tab(first_id, window, cx);

                // Active index must have shifted left by one.
                assert_eq!(this.active_tab_index, Some(0));
            });
        });
    }

    #[gpui::test]
    fn test_close_last_tab_leaves_no_active_index(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                assert_eq!(this.tabs.len(), 1);
                let tab_id = this.tabs[0].id();
                this.close_tab(tab_id, window, cx);
                assert!(this.tabs.is_empty());
                assert_eq!(this.active_tab_index, None);
            });
        });
    }

    // ========== set_active_tab tests ==========

    #[gpui::test]
    fn test_set_active_tab_changes_active_index(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.set_active_tab(0, window, cx);
                assert_eq!(this.active_tab_index, Some(0));
                this.set_active_tab(1, window, cx);
                assert_eq!(this.active_tab_index, Some(1));
            });
        });
    }

    #[gpui::test]
    fn test_set_active_tab_is_noop_out_of_bounds(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let active_before = this.active_tab_index;
                this.set_active_tab(usize::MAX, window, cx);
                assert_eq!(this.active_tab_index, active_before);
            });
        });
    }

    // ========== close_other_tabs tests ==========

    #[gpui::test]
    fn test_close_other_tabs_leaves_only_active_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.new_tab(window, cx);
                // Three tabs total; make the middle one (index 1) active.
                this.set_active_tab(1, window, cx);
                let active_id = this.tabs[1].id();

                this.close_other_tabs(window, cx);

                assert_eq!(this.tabs.len(), 1);
                assert_eq!(this.tabs[0].id(), active_id);
                assert_eq!(this.active_tab_index, Some(0));
            });
        });
    }

    #[gpui::test]
    fn test_close_other_tabs_is_noop_with_single_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                assert_eq!(this.tabs.len(), 1);
                let tab_id_before = this.tabs[0].id();
                this.close_other_tabs(window, cx);
                assert_eq!(this.tabs.len(), 1);
                assert_eq!(this.tabs[0].id(), tab_id_before);
            });
        });
    }

    // ========== duplicate_tab tests ==========

    #[gpui::test]
    fn test_duplicate_tab_inserts_copy_after_original_and_becomes_active(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let original_id = this.tabs[0].id();
                this.duplicate_tab(0, window, cx);

                assert_eq!(this.tabs.len(), 2);
                assert_eq!(this.tabs[0].id(), original_id);
                assert_ne!(this.tabs[1].id(), original_id);
                assert_eq!(this.active_tab_index, Some(1));
            });
        });
    }

    #[gpui::test]
    fn test_duplicate_tab_preserves_content_and_language(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(editor) = this.get_active_editor_tab_mut() {
                    editor.language = SupportedLanguage::Rust;
                }
                this.duplicate_tab(0, window, cx);

                let duplicate = this.tabs[1].as_editor().expect("expected editor tab");
                assert_eq!(duplicate.language, SupportedLanguage::Rust);
                assert!(duplicate.file_path.is_none());
            });
        });
    }

    #[gpui::test]
    fn test_duplicate_tab_is_noop_for_settings_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.open_settings(window, cx);
                let settings_index = this
                    .tabs
                    .iter()
                    .position(|t| matches!(t, Tab::Settings(_)))
                    .expect("expected settings tab");
                let count_before = this.tabs.len();
                this.duplicate_tab(settings_index, window, cx);
                assert_eq!(this.tabs.len(), count_before);
            });
        });
    }

    // ========== open_markdown_preview_tab tests ==========

    #[gpui::test]
    fn test_open_markdown_preview_tab_creates_preview_tab_for_markdown_editor(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(editor) = this.get_active_editor_tab_mut() {
                    editor.language = SupportedLanguage::Markdown;
                }
                let count_before = this.tabs.len();
                this.open_markdown_preview_tab(window, cx);
                assert_eq!(this.tabs.len(), count_before + 1);
                assert!(this.tabs.iter().any(|t| t.as_markdown_preview().is_some()));
            });
        });
    }

    #[gpui::test]
    fn test_open_markdown_preview_tab_preview_is_inserted_after_editor(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(editor) = this.get_active_editor_tab_mut() {
                    editor.language = SupportedLanguage::Markdown;
                }
                let editor_index = this.active_tab_index.expect("expected active tab");
                this.open_markdown_preview_tab(window, cx);
                assert!(matches!(
                    this.tabs.get(editor_index + 1),
                    Some(Tab::MarkdownPreview(_))
                ));
            });
        });
    }

    #[gpui::test]
    fn test_open_markdown_preview_tab_toggle_removes_preview_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(editor) = this.get_active_editor_tab_mut() {
                    editor.language = SupportedLanguage::Markdown;
                }
                let count_before = this.tabs.len();
                this.open_markdown_preview_tab(window, cx);
                assert_eq!(this.tabs.len(), count_before + 1);
                this.open_markdown_preview_tab(window, cx);
                assert_eq!(this.tabs.len(), count_before);
                assert!(!this.tabs.iter().any(|t| t.as_markdown_preview().is_some()));
            });
        });
    }

    #[gpui::test]
    fn test_open_markdown_preview_tab_is_noop_in_panel_mode(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.settings.editor_settings.markdown_settings.preview_mode =
                    crate::fulgur::settings::MarkdownPreviewMode::Panel;
                if let Some(editor) = this.get_active_editor_tab_mut() {
                    editor.language = SupportedLanguage::Markdown;
                }
                let count_before = this.tabs.len();
                this.open_markdown_preview_tab(window, cx);
                assert_eq!(this.tabs.len(), count_before);
            });
        });
    }

    #[gpui::test]
    fn test_open_markdown_preview_tab_is_noop_without_active_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.active_tab_index = None;
                let count_before = this.tabs.len();
                this.open_markdown_preview_tab(window, cx);
                assert_eq!(this.tabs.len(), count_before);
            });
        });
    }

    // ========== maybe_open_markdown_preview_for_editor tests ==========

    #[gpui::test]
    fn test_maybe_open_markdown_preview_for_editor_inserts_preview_for_markdown(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, _cx| {
                if let Some(Tab::Editor(editor)) = this.tabs.first_mut() {
                    editor.language = SupportedLanguage::Markdown;
                }
                let count_before = this.tabs.len();
                this.maybe_open_markdown_preview_for_editor(0);
                assert_eq!(this.tabs.len(), count_before + 1);
                assert!(matches!(this.tabs.get(1), Some(Tab::MarkdownPreview(_))));
            });
        });
    }

    #[gpui::test]
    fn test_maybe_open_markdown_preview_for_editor_skips_non_markdown(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, _cx| {
                // Default language is Plain — no preview tab should be inserted
                let count_before = this.tabs.len();
                this.maybe_open_markdown_preview_for_editor(0);
                assert_eq!(this.tabs.len(), count_before);
            });
        });
    }

    #[gpui::test]
    fn test_maybe_open_markdown_preview_for_editor_is_noop_when_disabled(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, _cx| {
                this.settings
                    .editor_settings
                    .markdown_settings
                    .show_markdown_preview = false;
                if let Some(Tab::Editor(editor)) = this.tabs.first_mut() {
                    editor.language = SupportedLanguage::Markdown;
                }
                let count_before = this.tabs.len();
                this.maybe_open_markdown_preview_for_editor(0);
                assert_eq!(this.tabs.len(), count_before);
            });
        });
    }

    // ========== insert_preview_tabs_for_markdown tests ==========

    #[gpui::test]
    fn test_insert_preview_tabs_for_markdown_adds_preview_tabs_for_all_markdown_editors(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(Tab::Editor(editor)) = this.tabs.first_mut() {
                    editor.language = SupportedLanguage::Markdown;
                }
                this.new_tab(window, cx);
                if let Some(Tab::Editor(editor)) = this.tabs.last_mut() {
                    editor.language = SupportedLanguage::Markdown;
                }
                assert_eq!(this.tabs.len(), 2);
                this.insert_preview_tabs_for_markdown();
                assert_eq!(this.tabs.len(), 4);
                assert_eq!(
                    this.tabs
                        .iter()
                        .filter(|t| t.as_markdown_preview().is_some())
                        .count(),
                    2
                );
            });
        });
    }

    #[gpui::test]
    fn test_insert_preview_tabs_for_markdown_is_noop_when_disabled(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, _cx| {
                this.settings
                    .editor_settings
                    .markdown_settings
                    .show_markdown_preview = false;
                if let Some(Tab::Editor(editor)) = this.tabs.first_mut() {
                    editor.language = SupportedLanguage::Markdown;
                }
                let count_before = this.tabs.len();
                this.insert_preview_tabs_for_markdown();
                assert_eq!(this.tabs.len(), count_before);
            });
        });
    }

    // ========== panel mode show_markdown_preview flag tests ==========

    #[gpui::test]
    fn test_panel_preview_flag_is_true_by_default(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, _cx| {
                assert!(
                    this.get_active_editor_tab()
                        .is_some_and(|e| e.show_markdown_preview),
                    "show_markdown_preview should default to true"
                );
            });
        });
    }

    #[gpui::test]
    fn test_panel_preview_flag_can_be_toggled(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let initial = this
                    .get_active_editor_tab()
                    .map(|e| e.show_markdown_preview)
                    .unwrap_or(false);
                if let Some(editor) = this.get_active_editor_tab_mut() {
                    editor.show_markdown_preview = !editor.show_markdown_preview;
                }
                cx.notify();
                let after = this
                    .get_active_editor_tab()
                    .map(|e| e.show_markdown_preview)
                    .unwrap_or(false);
                assert_ne!(initial, after, "show_markdown_preview should toggle");
            });
        });
    }

    // ========== reorder_tab tests ==========

    #[gpui::test]
    fn test_reorder_tab_moves_tab_backward(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.new_tab(window, cx);
                // tabs: [0, 1, 2]; move tab at index 2 to slot 0
                let id_2 = this.tabs[2].id();
                this.reorder_tab(2, 0, window, cx);
                assert_eq!(
                    this.tabs[0].id(),
                    id_2,
                    "tab moved backward should be at position 0"
                );
            });
        });
    }

    #[gpui::test]
    fn test_reorder_tab_moves_tab_forward(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.new_tab(window, cx);
                // tabs: [0, 1, 2]; move tab at index 0 to slot 3 (after last)
                let id_0 = this.tabs[0].id();
                this.reorder_tab(0, 3, window, cx);
                assert_eq!(
                    this.tabs[2].id(),
                    id_0,
                    "tab moved forward should be at last position"
                );
            });
        });
    }

    #[gpui::test]
    fn test_reorder_tab_noop_when_to_equals_from(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                let ids_before: Vec<usize> = this.tabs.iter().map(|t| t.id()).collect();
                this.reorder_tab(1, 1, window, cx);
                let ids_after: Vec<usize> = this.tabs.iter().map(|t| t.id()).collect();
                assert_eq!(ids_before, ids_after, "to == from should be a no-op");
            });
        });
    }

    #[gpui::test]
    fn test_reorder_tab_noop_when_to_equals_from_plus_one(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                let ids_before: Vec<usize> = this.tabs.iter().map(|t| t.id()).collect();
                // to == from+1 means inserting immediately after the tab, which is its current position
                this.reorder_tab(1, 2, window, cx);
                let ids_after: Vec<usize> = this.tabs.iter().map(|t| t.id()).collect();
                assert_eq!(ids_before, ids_after, "to == from+1 should be a no-op");
            });
        });
    }

    #[gpui::test]
    fn test_reorder_tab_noop_when_from_out_of_bounds(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let count_before = this.tabs.len();
                this.reorder_tab(usize::MAX, 0, window, cx);
                assert_eq!(this.tabs.len(), count_before);
            });
        });
    }

    #[gpui::test]
    fn test_reorder_tab_noop_when_to_out_of_bounds(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let count_before = this.tabs.len();
                this.reorder_tab(0, usize::MAX, window, cx);
                assert_eq!(this.tabs.len(), count_before);
            });
        });
    }

    #[gpui::test]
    fn test_reorder_tab_active_index_follows_moved_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.new_tab(window, cx);
                // tabs: [0*, 1, 2]; active = 0; move tab 0 to slot 3
                this.set_active_tab(0, window, cx);
                this.reorder_tab(0, 3, window, cx);
                // After remove: [1, 2]; insert_at = 3-1 = 2 → [1, 2, 0*]; active should be 2
                assert_eq!(
                    this.active_tab_index,
                    Some(2),
                    "active index should follow the moved tab"
                );
            });
        });
    }

    #[gpui::test]
    fn test_reorder_tab_active_index_decrements_when_earlier_tab_moves_past(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.new_tab(window, cx);
                // tabs: [0, 1*, 2]; active = 1; move tab 0 past active to slot 3
                this.set_active_tab(1, window, cx);
                this.reorder_tab(0, 3, window, cx);
                // from(0) < active(1), insert_at(2) >= active(1) → active - 1 = 0
                assert_eq!(
                    this.active_tab_index,
                    Some(0),
                    "active index should decrement when a preceding tab moves past it"
                );
            });
        });
    }

    #[gpui::test]
    fn test_reorder_tab_active_index_increments_when_later_tab_moves_before(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.new_tab(window, cx);
                // tabs: [0, 1*, 2]; active = 1; move tab 2 before active to slot 0
                this.set_active_tab(1, window, cx);
                this.reorder_tab(2, 0, window, cx);
                // from(2) > active(1), insert_at(0) <= active(1) → active + 1 = 2
                assert_eq!(
                    this.active_tab_index,
                    Some(2),
                    "active index should increment when a following tab moves before it"
                );
            });
        });
    }

    // ========== handle_tab_drop tests ==========

    #[gpui::test]
    fn test_handle_tab_drop_reorders_tab_to_target_slot(cx: &mut TestAppContext) {
        use crate::fulgur::ui::tabs::tab_drag::DraggedTab;
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.new_tab(window, cx);
                let id_2 = this.tabs[2].id();
                let dragged = DraggedTab {
                    tab_index: 2,
                    title: "test.rs".into(),
                    is_modified: false,
                };
                this.handle_tab_drop(&dragged, 0, window, cx);
                assert_eq!(this.tabs[0].id(), id_2, "dropped tab should land at slot 0");
            });
        });
    }
}
