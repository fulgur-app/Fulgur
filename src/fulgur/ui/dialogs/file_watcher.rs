use crate::fulgur::state::file_modified_system_time;
use crate::fulgur::ui::tabs::tab::TabId;
use std::path::Path;

use gpui_kit::component::{
    WindowExt,
    button::{Button, ButtonVariant, ButtonVariants},
    dialog::DialogButtonProps,
    h_flex, v_flex,
};
use gpui_kit::{
    App, ClickEvent, Context, InteractiveElement, ParentElement, Styled, Window, div, px,
};

use crate::fulgur::Fulgur;

/// The user's answer to the unprompted external-modification conflict dialog.
#[derive(Clone, Copy)]
enum FileConflictChoice {
    KeepLocal,
    LoadFromDisk,
}

impl Fulgur {
    /// Apply the answer to a file conflict dialog and release its open-dialog guard.
    ///
    /// ### Arguments
    /// - `choice`: Whether to keep the local edits or reload the disk version
    /// - `tab_id`: The stable id of the conflicting tab
    /// - `path`: The path the tab is expected to still point at
    /// - `window`: The window context
    /// - `cx`: The application context
    fn resolve_file_conflict(
        &mut self,
        choice: FileConflictChoice,
        tab_id: TabId,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.file_watch_state.open_conflict_dialogs.remove(path);
        match choice {
            FileConflictChoice::KeepLocal => self.keep_local_changes_over_disk(tab_id, path, cx),
            FileConflictChoice::LoadFromDisk => {
                self.reload_watched_tab_by_id(tab_id, path, window, cx);
            }
        }
    }

    /// Reload a tab from disk after a watcher dialog, resolving the tab by stable id.
    ///
    /// ### Arguments
    /// - `tab_id`: The stable id of the tab to reload
    /// - `path`: The path the tab is expected to still point at
    /// - `window`: The window context
    /// - `cx`: The application context
    fn reload_watched_tab_by_id(
        &mut self,
        tab_id: TabId,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_index) = self.tab_index_of(tab_id, cx) else {
            return;
        };
        let path_matches = self
            .tabs
            .get(tab_index)
            .and_then(|tab| tab.read(cx).as_editor())
            .and_then(|editor_tab| editor_tab.file_path())
            .is_some_and(|p| p == path);
        if path_matches {
            self.reload_tab_from_disk(tab_index, window, cx);
        }
    }

    /// Accept the current disk version as the baseline of a tab whose local edits
    /// the user chose to keep, so a restart does not raise the same conflict again.
    ///
    /// ### Arguments
    /// - `tab_id`: The stable id of the tab keeping its local changes
    /// - `path`: The path the tab is expected to still point at
    /// - `cx`: The application context
    pub(crate) fn keep_local_changes_over_disk(
        &mut self,
        tab_id: TabId,
        path: &Path,
        cx: &mut Context<Self>,
    ) {
        let disk_modified = file_modified_system_time(path);
        self.update_editor_tab(tab_id, cx, |editor_tab, _| {
            if editor_tab.file_path().is_some_and(|p| p == path) {
                editor_tab.file_last_modified = disk_modified;
            }
        });
    }

    /// Show dialog when file is modified externally and has local changes
    ///
    /// ### Arguments
    /// - `path`: The path to the file that has local changes
    /// - `tab_id`: The stable id of the tab that has local changes
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    pub fn show_file_conflict_dialog(
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
            let resolve_on_key = |choice: FileConflictChoice| {
                let entity = entity.clone();
                let path = path.clone();
                move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                    entity.update(cx, |this, cx| {
                        this.resolve_file_conflict(choice, tab_id, &path, window, cx);
                    });
                    true
                }
            };
            let resolve_on_click =
                |id: &'static str, label: &'static str, variant, choice: FileConflictChoice| {
                    let entity = entity.clone();
                    let path = path.clone();
                    Button::new(id)
                        .label(label)
                        .with_variant(variant)
                        .debug_selector(move || id.to_string())
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            entity.update(cx, |this, cx| {
                                this.resolve_file_conflict(choice, tab_id, &path, window, cx);
                            });
                        })
                };
            let path_for_close = path.clone();
            let entity_for_close = entity.clone();
            modal
                .title(div().text_size(px(16.)).child("File Modified Externally"))
                .keyboard(true)
                .close_button(false)
                .footer(
                    h_flex()
                        .gap_2()
                        .justify_end()
                        .child(resolve_on_click(
                            "file-conflict-load-from-file",
                            "Load from file",
                            ButtonVariant::Danger,
                            FileConflictChoice::LoadFromDisk,
                        ))
                        .child(resolve_on_click(
                            "file-conflict-keep-local",
                            "Keep local changes",
                            ButtonVariant::Primary,
                            FileConflictChoice::KeepLocal,
                        )),
                )
                .child(
                    v_flex()
                        .gap_2()
                        .child(format!(
                            "The file \"{filename}\" has been modified externally."
                        ))
                        .child("You have unsaved changes in this file. Do you want to load the changes from the file?"),
                )
                .on_ok(resolve_on_key(FileConflictChoice::KeepLocal))
                .on_cancel(resolve_on_key(FileConflictChoice::KeepLocal))
                .on_close(move |_, _, cx| {
                    let path = path_for_close.clone();
                    entity_for_close.update(cx, |this, _| {
                        this.file_watch_state.open_conflict_dialogs.remove(&path);
                    });
                })
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
        tab_id: TabId,
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

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use crate::fulgur::{
        Fulgur,
        editor_tab::{TabLocation, replace_editor_text},
        tab::TabId,
    };
    use crate::test_support::{dispatch_dialog_action, has_active_dialog, setup_fulgur_with_root};
    use gpui_kit::base::actions::{Cancel, Confirm};
    use gpui_kit::{Action, Entity, Modifiers, TestAppContext, VisualTestContext};
    use std::path::Path;
    use tempfile::TempDir;

    const DISK_TEXT: &str = "content-from-disk";
    const LOCAL_TEXT: &str = "local-edits";

    /// Back the active tab with a file on disk, give it unsaved edits, and raise
    /// the external-modification conflict dialog for it.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state owning the tab
    /// - `visual_cx`: The visual test context of the window
    /// - `path`: The file backing the tab
    ///
    /// ### Returns
    /// - `TabId`: The id of the conflicting tab
    fn open_conflict_dialog(
        fulgur: &Entity<Fulgur>,
        visual_cx: &mut VisualTestContext,
        path: &Path,
    ) -> TabId {
        std::fs::write(path, DISK_TEXT).expect("failed to write test file");
        let tab_id = visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let tab = this.active_tab_entity(cx).expect("expected an active tab");
                tab.update(cx, |tab, cx| {
                    let editor_tab = tab.as_editor_mut().expect("expected an editor tab");
                    editor_tab.location = TabLocation::Local(path.to_path_buf());
                    editor_tab.content.update(cx, |state, cx| {
                        replace_editor_text(state, LOCAL_TEXT, window, cx);
                    });
                });
                let tab_id = tab.read(cx).id();
                this.show_file_conflict_dialog(path, tab_id, window, cx);
                tab_id
            })
        });
        visual_cx.run_until_parked();
        assert!(has_active_dialog(visual_cx));
        tab_id
    }

    /// Read a tab's buffer text and dirty flag.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state owning the tab
    /// - `visual_cx`: The visual test context of the window
    /// - `tab_id`: The tab to inspect
    ///
    /// ### Returns
    /// - `(String, bool)`: The buffer text and whether the tab is modified
    fn tab_text_and_modified(
        fulgur: &Entity<Fulgur>,
        visual_cx: &mut VisualTestContext,
        tab_id: TabId,
    ) -> (String, bool) {
        visual_cx.update(|_, cx| {
            let tab = fulgur
                .read(cx)
                .tab_entity_of(tab_id, cx)
                .expect("tab should still exist");
            let editor_tab = tab.read(cx).as_editor().expect("expected an editor tab");
            (
                editor_tab.content.read(cx).text().to_string(),
                editor_tab.modified,
            )
        })
    }

    /// Answer the conflict dialog from the keyboard and check the local edits survive.
    ///
    /// ### Arguments
    /// - `cx`: The test application context
    /// - `action`: The dialog action the key press dispatches
    fn assert_keyboard_answer_keeps_local_edits(cx: &mut TestAppContext, action: impl Action) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("conflict.txt");
        let tab_id = open_conflict_dialog(&fulgur, &mut visual_cx, &path);

        dispatch_dialog_action(&mut visual_cx, action);

        assert!(!has_active_dialog(&mut visual_cx));
        assert_eq!(
            tab_text_and_modified(&fulgur, &mut visual_cx, tab_id),
            (LOCAL_TEXT.to_string(), true),
            "a key press on the unprompted dialog must never discard the local edits"
        );
        visual_cx.update(|_, cx| {
            assert!(
                fulgur
                    .read(cx)
                    .file_watch_state
                    .open_conflict_dialogs
                    .is_empty()
            );
        });
    }

    #[gpui_kit::test]
    fn test_conflict_dialog_enter_keeps_local_edits(cx: &mut TestAppContext) {
        assert_keyboard_answer_keeps_local_edits(cx, Confirm { secondary: false });
    }

    #[gpui_kit::test]
    fn test_conflict_dialog_escape_keeps_local_edits(cx: &mut TestAppContext) {
        assert_keyboard_answer_keeps_local_edits(cx, Cancel);
    }

    #[gpui_kit::test]
    fn test_conflict_dialog_load_from_file_click_reloads_disk_version(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("conflict.txt");
        let tab_id = open_conflict_dialog(&fulgur, &mut visual_cx, &path);

        visual_cx.update(|window, cx| window.draw(cx).clear(cx));
        let bounds = visual_cx
            .debug_bounds("file-conflict-load-from-file")
            .expect("the Load from file button should be rendered");
        visual_cx.simulate_click(bounds.center(), Modifiers::none());
        visual_cx.run_until_parked();

        assert!(!has_active_dialog(&mut visual_cx));
        assert_eq!(
            tab_text_and_modified(&fulgur, &mut visual_cx, tab_id),
            (DISK_TEXT.to_string(), false)
        );
        visual_cx.update(|_, cx| {
            assert!(
                fulgur
                    .read(cx)
                    .file_watch_state
                    .open_conflict_dialogs
                    .is_empty()
            );
        });
    }
}
