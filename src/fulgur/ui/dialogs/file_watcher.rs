use std::path::Path;

use gpui::{Context, ParentElement, Styled, Window, div, px};
use gpui_component::{WindowExt, button::ButtonVariant, dialog::DialogButtonProps, v_flex};

use crate::fulgur::Fulgur;
use crate::fulgur::ui::tabs::tab::Tab;

impl Fulgur {
    /// Reload a tab from disk after a watcher dialog, resolving the tab by stable id.
    ///
    /// ### Arguments
    /// - `tab_id`: The stable id of the tab to reload
    /// - `path`: The path the tab is expected to still point at
    /// - `window`: The window context
    /// - `cx`: The application context
    fn reload_watched_tab_by_id(
        &mut self,
        tab_id: usize,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_index) = self.tabs.iter().position(|tab| tab.id() == tab_id) else {
            return;
        };
        let path_matches = self
            .tabs
            .get(tab_index)
            .and_then(Tab::as_editor)
            .and_then(|editor_tab| editor_tab.file_path())
            .is_some_and(|p| p == path);
        if path_matches {
            self.reload_tab_from_disk(tab_index, window, cx);
        }
    }

    /// Show dialog when file is modified externally and has local changes
    ///
    /// ### Arguments
    /// - `path`: The path to the file that has local changes
    /// - `tab_id`: The stable id of the tab that has local changes
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    pub fn show_file_conflict_dialog(
        &self,
        path: &Path,
        tab_id: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity().clone();
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        let path = path.to_path_buf();

        window.open_alert_dialog(cx, move |modal, _, _| {
            let path = path.clone();
            let entity_for_ok = entity.clone();
            modal
                .title(div().text_size(px(16.)).child("File Modified Externally"))
                .keyboard(true)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text("Keep local changes")
                        .cancel_variant(ButtonVariant::Secondary)
                        .ok_text("Load from file")
                        .ok_variant(ButtonVariant::Primary),
                )
                .overlay_closable(false)
                .close_button(false)
                .child(
                    v_flex()
                        .gap_2()
                        .child(format!(
                            "The file \"{filename}\" has been modified externally."
                        ))
                        .child("You have unsaved changes in this file. Do you want to load the changes from the file?"),
                )
                .on_ok(move |_, window, cx| {
                    let path = path.clone();
                    entity_for_ok.update(cx, |this, cx| {
                        this.reload_watched_tab_by_id(tab_id, &path, window, cx);
                    });
                    true
                })
                .on_cancel(|_, _, _| true)
        });
    }

    /// Show dialog when re-opening an already-open modified file.
    ///
    /// ### Arguments
    /// - `path`: The file path that is already open in the editor
    /// - `tab_id`: The stable id of the already-open tab
    /// - `window`: The target window
    /// - `cx`: The application context
    pub fn show_reopen_modified_file_dialog(
        &self,
        path: &Path,
        tab_id: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity().clone();
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        let path = path.to_path_buf();

        window.open_alert_dialog(cx, move |modal, _, _| {
            let entity_for_ok = entity.clone();
            let path = path.clone();
            modal
                .title(div().text_size(px(16.)).child("File Already Open"))
                .keyboard(true)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text("Keep local changes")
                        .cancel_variant(ButtonVariant::Secondary)
                        .ok_text("Reload from disk")
                        .ok_variant(ButtonVariant::Primary),
                )
                .overlay_closable(false)
                .close_button(false)
                .child(
                    v_flex()
                        .gap_2()
                        .child(format!(
                            "The file \"{filename}\" is already open with unsaved local changes."
                        ))
                        .child("Choose which version to keep."),
                )
                .on_ok(move |_, window, cx| {
                    let path = path.clone();
                    entity_for_ok.update(cx, |this, cx| {
                        this.reload_watched_tab_by_id(tab_id, &path, window, cx);
                    });
                    true
                })
                .on_cancel(|_, _, _| true)
        });
    }
}
