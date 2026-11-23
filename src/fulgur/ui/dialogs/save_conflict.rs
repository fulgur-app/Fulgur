use crate::fulgur::Fulgur;
use crate::fulgur::ui::tabs::tab::TabId;
use gpui_kit::component::{
    WindowExt,
    button::{Button, ButtonVariant, ButtonVariants},
    h_flex, v_flex,
};
use gpui_kit::{
    App, ClickEvent, Context, InteractiveElement, ParentElement, Styled, Window, div, px,
};
use std::path::Path;

/// The user's answer to a save refused because the file changed on disk.
#[derive(Clone, Copy)]
enum SaveConflictChoice {
    /// Write the buffer anyway, replacing the version on disk.
    Overwrite,
    /// Discard the buffer and load the version on disk.
    LoadFromDisk,
    /// Keep both as they are; the buffer stays unsaved.
    Cancel,
}

impl Fulgur {
    /// Apply the answer to a save conflict dialog and release its open-dialog guard.
    ///
    /// ### Arguments
    /// - `choice`: Whether to overwrite the disk version, load it, or do nothing
    /// - `tab_id`: The stable id of the tab whose save was refused
    /// - `path`: The path the tab is expected to still point at
    /// - `window`: The window context
    /// - `cx`: The application context
    fn resolve_save_conflict(
        &mut self,
        choice: SaveConflictChoice,
        tab_id: TabId,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.file_watch_state.open_conflict_dialogs.remove(path);
        match choice {
            SaveConflictChoice::Overwrite => {
                self.save_tab_over_disk_changes(tab_id, path, window, cx);
            }
            SaveConflictChoice::LoadFromDisk => {
                self.reload_watched_tab_by_id(tab_id, path, window, cx);
            }
            SaveConflictChoice::Cancel => {}
        }
    }

    /// Show the dialog raised when a save is refused because the file changed on
    /// disk since the version the buffer is based on
    ///
    /// ### Arguments
    /// - `path`: The file the save was refused for
    /// - `tab_id`: The stable id of the tab whose save was refused
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    pub(crate) fn show_save_conflict_dialog(
        &mut self,
        path: &Path,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = path.to_path_buf();
        if !self
            .file_watch_state
            .open_conflict_dialogs
            .insert(path.clone())
        {
            return;
        }

        let entity = cx.entity().clone();
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();

        window.open_alert_dialog(cx, move |modal, _, _| {
            let resolve_on_key = |choice: SaveConflictChoice| {
                let entity = entity.clone();
                let path = path.clone();
                move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                    entity.update(cx, |this, cx| {
                        this.resolve_save_conflict(choice, tab_id, &path, window, cx);
                    });
                    true
                }
            };
            let resolve_on_click =
                |id: &'static str, label: &'static str, variant, choice: SaveConflictChoice| {
                    let entity = entity.clone();
                    let path = path.clone();
                    Button::new(id)
                        .label(label)
                        .with_variant(variant)
                        .debug_selector(move || id.to_string())
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            entity.update(cx, |this, cx| {
                                this.resolve_save_conflict(choice, tab_id, &path, window, cx);
                            });
                        })
                };
            let path_for_close = path.clone();
            let entity_for_close = entity.clone();
            modal
                .title(div().text_size(px(16.)).child("File Changed on Disk"))
                .keyboard(true)
                .close_button(false)
                .footer(
                    h_flex()
                        .gap_2()
                        .justify_end()
                        .child(resolve_on_click(
                            "save-conflict-cancel",
                            "Cancel",
                            ButtonVariant::Secondary,
                            SaveConflictChoice::Cancel,
                        ))
                        .child(resolve_on_click(
                            "save-conflict-load-from-file",
                            "Load from file",
                            ButtonVariant::Danger,
                            SaveConflictChoice::LoadFromDisk,
                        ))
                        .child(resolve_on_click(
                            "save-conflict-overwrite",
                            "Overwrite",
                            ButtonVariant::Primary,
                            SaveConflictChoice::Overwrite,
                        )),
                )
                .child(
                    v_flex()
                        .gap_2()
                        .child(format!(
                            "The file \"{filename}\" was modified by another program since it was last loaded or saved, so it was not saved."
                        ))
                        .child("Overwrite it with your version, or load the version on disk and discard your unsaved changes?"),
                )
                // Neither key may destroy one of the two versions.
                .on_ok(resolve_on_key(SaveConflictChoice::Cancel))
                .on_cancel(resolve_on_key(SaveConflictChoice::Cancel))
                .on_close(move |_, _, cx| {
                    let path = path_for_close.clone();
                    entity_for_close.update(cx, |this, _| {
                        this.file_watch_state.open_conflict_dialogs.remove(&path);
                    });
                })
        });
    }
}
