use crate::fulgur::Fulgur;
use crate::fulgur::editor_tab::TabLocation;
use crate::fulgur::files::file_operations::DiskChangePolicy;
use crate::fulgur::sync::ssh::{sftp::RemoteConflict, url::RemoteSpec};
use crate::fulgur::ui::tabs::tab::TabId;
use gpui_kit::component::{
    WindowExt,
    button::{Button, ButtonVariant, ButtonVariants},
    h_flex, v_flex,
};
use gpui_kit::{
    App, ClickEvent, Context, InteractiveElement, ParentElement, Styled, Window, div, px,
};
use std::path::PathBuf;
use std::rc::Rc;

/// The file whose save was refused because it no longer holds the version the buffer
/// is based on.
pub(crate) enum ConflictedFile {
    /// A local file that changed on disk.
    Local(PathBuf),
    /// A file on a remote host.
    Remote {
        /// The remote file
        spec: RemoteSpec,
        /// Why its save was refused
        conflict: RemoteConflict,
    },
}

impl ConflictedFile {
    /// Check whether a tab is still backed by this file.
    ///
    /// ### Arguments
    /// - `location`: The current location of the tab
    ///
    /// ### Returns
    /// - `bool`: `true` when both designate the same file
    fn is_at(&self, location: &TabLocation) -> bool {
        match (self, location) {
            (Self::Local(path), TabLocation::Local(current)) => path == current,
            (Self::Remote { spec, .. }, TabLocation::Remote(current)) => {
                spec.same_location(current)
            }
            _ => false,
        }
    }

    /// The name of the file, without its directory.
    ///
    /// ### Returns
    /// - `&str`: The file name, or `"file"` when it has none
    fn file_name(&self) -> &str {
        match self {
            Self::Local(path) => path.file_name().and_then(|name| name.to_str()),
            Self::Remote { spec, .. } => spec.path.rsplit('/').next(),
        }
        .unwrap_or("file")
    }

    /// The title of the conflict dialog.
    ///
    /// ### Returns
    /// - `&'static str`: The title
    fn dialog_title(&self) -> &'static str {
        match self {
            Self::Local(_) => "File Changed on Disk",
            Self::Remote { conflict, .. } => match conflict {
                RemoteConflict::Changed => "File Changed on Server",
                RemoteConflict::UnknownBaseline => "File Exists on Server",
            },
        }
    }

    /// Explain why the file was not saved.
    ///
    /// ### Returns
    /// - `String`: The explanation, naming the file
    fn explanation(&self) -> String {
        let file_name = self.file_name();
        match self {
            Self::Local(_) => format!(
                "The file \"{file_name}\" was modified by another program since it was last loaded or saved, so it was not saved."
            ),
            Self::Remote { spec, conflict } => {
                let host = &spec.host;
                match conflict {
                    RemoteConflict::Changed => format!(
                        "The file \"{file_name}\" was modified on {host} since it was last loaded or saved, so it was not saved."
                    ),
                    RemoteConflict::UnknownBaseline => format!(
                        "Your changes to \"{file_name}\" were recovered from a previous session, and the file may have been modified on {host} since then, so it was not saved."
                    ),
                }
            }
        }
    }

    /// Ask which of the two versions to keep.
    ///
    /// ### Returns
    /// - `&'static str`: The question
    fn question(&self) -> &'static str {
        match self {
            Self::Local(_) => {
                "Overwrite it with your version, or load the version on disk and discard your unsaved changes?"
            }
            Self::Remote { .. } => {
                "Overwrite it with your version, or load the version on the server and discard your unsaved changes?"
            }
        }
    }

    /// The label of the button that loads the newer version.
    ///
    /// ### Returns
    /// - `&'static str`: The label
    fn load_label(&self) -> &'static str {
        match self {
            Self::Local(_) => "Load from file",
            Self::Remote { .. } => "Load from server",
        }
    }
}

/// The user's answer to a save refused because the file changed.
#[derive(Clone, Copy)]
enum SaveConflictChoice {
    /// Write the buffer anyway, replacing the newer version.
    Overwrite,
    /// Discard the buffer and load the newer version.
    LoadNewer,
    /// Keep both as they are; the buffer stays unsaved.
    Cancel,
}

impl Fulgur {
    /// Allow a save conflict dialog to be shown again for a file.
    ///
    /// ### Arguments
    /// - `file`: The file whose dialog was answered or closed
    fn release_save_conflict_guard(&mut self, file: &ConflictedFile) {
        if let ConflictedFile::Local(path) = file {
            self.file_watch_state.open_conflict_dialogs.remove(path);
        }
    }

    /// Apply the answer to a save conflict dialog and release its open-dialog guard.
    ///
    /// ### Arguments
    /// - `choice`: Whether to overwrite the newer version, load it, or do nothing
    /// - `tab_id`: The stable id of the tab whose save was refused
    /// - `file`: The file the tab is expected to still be backed by
    /// - `window`: The window context
    /// - `cx`: The application context
    fn resolve_save_conflict(
        &mut self,
        choice: SaveConflictChoice,
        tab_id: TabId,
        file: &ConflictedFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.release_save_conflict_guard(file);
        let still_backed_by_file = self
            .tab_entity_of(tab_id, cx)
            .and_then(|tab| Some(file.is_at(&tab.read(cx).as_editor()?.location)))
            .unwrap_or(false);
        if !still_backed_by_file {
            return;
        }
        match choice {
            SaveConflictChoice::Overwrite => {
                self.save_editor_tab(tab_id, DiskChangePolicy::Overwrite, window, cx);
            }
            SaveConflictChoice::LoadNewer => match file {
                ConflictedFile::Local(path) => {
                    self.reload_watched_tab_by_id(tab_id, path, window, cx);
                }
                ConflictedFile::Remote { spec, .. } => {
                    self.ensure_remote_tab_loaded(window, cx, tab_id, spec.clone());
                }
            },
            SaveConflictChoice::Cancel => {}
        }
    }

    /// Show the dialog raised when a save is refused because the file no longer holds
    /// the version the buffer is based on
    ///
    /// ### Arguments
    /// - `file`: The file the save was refused for
    /// - `tab_id`: The stable id of the tab whose save was refused
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    pub(crate) fn show_save_conflict_dialog(
        &mut self,
        file: ConflictedFile,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The file watcher raises its own dialog for a local file.
        if let ConflictedFile::Local(path) = &file
            && !self
                .file_watch_state
                .open_conflict_dialogs
                .insert(path.clone())
        {
            return;
        }

        let entity = cx.entity().clone();
        let file = Rc::new(file);

        window.open_alert_dialog(cx, move |modal, _, _| {
            let resolve_on_key = |choice: SaveConflictChoice| {
                let entity = entity.clone();
                let file = Rc::clone(&file);
                move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                    entity.update(cx, |this, cx| {
                        this.resolve_save_conflict(choice, tab_id, &file, window, cx);
                    });
                    true
                }
            };
            let resolve_on_click =
                |id: &'static str, label: &'static str, variant, choice: SaveConflictChoice| {
                    let entity = entity.clone();
                    let file = Rc::clone(&file);
                    Button::new(id)
                        .label(label)
                        .with_variant(variant)
                        .debug_selector(move || id.to_string())
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            entity.update(cx, |this, cx| {
                                this.resolve_save_conflict(choice, tab_id, &file, window, cx);
                            });
                        })
                };
            let file_for_close = Rc::clone(&file);
            let entity_for_close = entity.clone();
            modal
                .title(div().text_size(px(16.)).child(file.dialog_title()))
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
                            file.load_label(),
                            ButtonVariant::Danger,
                            SaveConflictChoice::LoadNewer,
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
                        .child(file.explanation())
                        .child(file.question()),
                )
                // Neither key may destroy one of the two versions.
                .on_ok(resolve_on_key(SaveConflictChoice::Cancel))
                .on_cancel(resolve_on_key(SaveConflictChoice::Cancel))
                .on_close(move |_, _, cx| {
                    entity_for_close.update(cx, |this, _| {
                        this.release_save_conflict_guard(&file_for_close);
                    });
                })
        });
    }
}
