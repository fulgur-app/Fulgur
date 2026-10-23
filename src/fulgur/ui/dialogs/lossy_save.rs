use crate::fulgur::ui::tabs::tab::TabId;
use std::{path::PathBuf, rc::Rc};

use gpui_kit::component::{WindowExt, button::ButtonVariant, dialog::DialogButtonProps, v_flex};
use gpui_kit::{Context, ParentElement, Styled, Window, div, px};

use crate::fulgur::{Fulgur, ui::components_utils::UTF_8};

impl Fulgur {
    /// Show a confirmation dialog when saving would lose data through encoding.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable id of the tab being saved
    /// - `encoding`: The current encoding label that cannot represent the text
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    pub fn show_lossy_save_dialog(
        &self,
        tab_id: TabId,
        encoding: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity().clone();
        let encoding = encoding.to_string();
        window.open_alert_dialog(cx, move |modal, _, _| {
            let entity_for_ok = entity.clone();
            modal
                .title(div().text_size(px(16.)).child("Cannot Save in Encoding"))
                .keyboard(true)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text("Cancel")
                        .cancel_variant(ButtonVariant::Secondary)
                        .ok_text("Save as UTF-8")
                        .ok_variant(ButtonVariant::Primary),
                )
                .close_button(false)
                .child(
                    v_flex()
                        .gap_2()
                        .child(format!(
                            "This file contains characters that cannot be represented in {encoding}."
                        ))
                        .child("Save it as UTF-8 instead? This changes the file's encoding."),
                )
                .on_ok(move |_, window, cx| {
                    entity_for_ok.update(cx, |this, cx| {
                        this.update_editor_tab(tab_id, cx, |editor_tab, _| {
                            editor_tab.encoding = UTF_8.to_string();
                            editor_tab.lossy_decode = false;
                        });
                        this.save_file(window, cx);
                    });
                    true
                })
                .on_cancel(|_, _, _| true)
        });
    }

    /// Show a confirmation dialog when "Save as" would lose data through encoding.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable id of the tab being saved
    /// - `path`: The chosen destination path
    /// - `contents`: The editor text to write as UTF-8 on confirm
    /// - `encoding`: The current encoding label that cannot represent the text
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    pub fn show_lossy_save_as_dialog(
        &self,
        tab_id: TabId,
        path: PathBuf,
        contents: String,
        encoding: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity().clone();
        let encoding = encoding.to_string();
        // The builder runs on every render, so it must share the buffer rather than copy it.
        let contents: Rc<str> = contents.into();
        window.open_alert_dialog(cx, move |modal, _, _| {
            let entity_for_ok = entity.clone();
            let path = path.clone();
            let contents = Rc::clone(&contents);
            modal
                .title(div().text_size(px(16.)).child("Cannot Save in Encoding"))
                .keyboard(true)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text("Cancel")
                        .cancel_variant(ButtonVariant::Secondary)
                        .ok_text("Save as UTF-8")
                        .ok_variant(ButtonVariant::Primary),
                )
                .close_button(false)
                .child(
                    v_flex()
                        .gap_2()
                        .child(format!(
                            "This file contains characters that cannot be represented in {encoding}."
                        ))
                        .child("Save it as UTF-8 instead? This changes the file's encoding."),
                )
                .on_ok(move |_, window, cx| {
                    entity_for_ok.update(cx, |this, cx| {
                        this.finalize_save_as(
                            tab_id,
                            &path,
                            (&contents, contents.as_bytes(), UTF_8.to_string()),
                            window,
                            cx,
                        );
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
        ui::components_utils::UTF_8,
    };
    use crate::test_support::{dispatch_dialog_action, has_active_dialog, setup_fulgur_with_root};
    use gpui_kit::base::actions::{Cancel, Confirm};
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    const LEGACY_ENCODING: &str = "windows-1252";
    /// "café" as stored in windows-1252, where the accented letter is the single byte 0xE9.
    const LEGACY_BYTES: &[u8] = b"caf\xE9";
    /// U+2603 has no windows-1252 mapping, so saving this text must not write silently.
    const UNMAPPABLE_TEXT: &str = "café ☃";

    /// The tab fields a lossy save is allowed to change.
    struct TabSnapshot {
        encoding: String,
        lossy_decode: bool,
        modified: bool,
        path: Option<PathBuf>,
    }

    /// Point the window's tab at a windows-1252 file on disk and type `text` into it.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state owning the tab
    /// - `visual_cx`: The visual test context of the window
    /// - `path`: The legacy-encoded file the tab is backed by
    /// - `text`: The edited buffer content
    /// - `lossy_decode`: Whether the file is flagged as decoded with replacement characters
    ///
    /// ### Returns
    /// - `TabId`: The id of the prepared tab
    fn open_legacy_tab(
        fulgur: &Entity<Fulgur>,
        visual_cx: &mut VisualTestContext,
        path: &Path,
        text: &str,
        lossy_decode: bool,
    ) -> TabId {
        std::fs::write(path, LEGACY_BYTES).expect("failed to write legacy file");
        let tab_id = visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let tab = this.tabs.last().expect("expected a tab").clone();
                tab.update(cx, |tab, cx| {
                    let editor_tab = tab.as_editor_mut().expect("expected an editor tab");
                    editor_tab.location = TabLocation::Local(path.to_path_buf());
                    editor_tab.encoding = LEGACY_ENCODING.to_string();
                    editor_tab.lossy_decode = lossy_decode;
                    editor_tab.content.update(cx, |state, cx| {
                        replace_editor_text(state, text, window, cx);
                    });
                });
                tab.read(cx).id()
            })
        });
        assert!(snapshot_tab(fulgur, visual_cx, tab_id).modified);
        tab_id
    }

    /// Read the encoding-related state of a tab.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state owning the tab
    /// - `visual_cx`: The visual test context of the window
    /// - `tab_id`: The tab to inspect
    ///
    /// ### Returns
    /// - `TabSnapshot`: The tab's encoding, lossy flag, dirty flag and backing path
    fn snapshot_tab(
        fulgur: &Entity<Fulgur>,
        visual_cx: &mut VisualTestContext,
        tab_id: TabId,
    ) -> TabSnapshot {
        visual_cx.update(|_, cx| {
            let tab = fulgur
                .read(cx)
                .tab_entity_of(tab_id, cx)
                .expect("tab should still exist");
            let tab = tab.read(cx);
            let editor_tab = tab.as_editor().expect("expected an editor tab");
            TabSnapshot {
                encoding: editor_tab.encoding.clone(),
                lossy_decode: editor_tab.lossy_decode,
                modified: editor_tab.modified,
                path: editor_tab.file_path().cloned(),
            }
        })
    }

    /// Trigger a plain save of the active tab.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state to save from
    /// - `visual_cx`: The visual test context of the window
    fn save_active_tab(fulgur: &Entity<Fulgur>, visual_cx: &mut VisualTestContext) {
        visual_cx.update(|window, cx| fulgur.update(cx, |this, cx| this.save_file(window, cx)));
        visual_cx.run_until_parked();
    }

    /// Trigger "Save as" on the active tab and answer the path prompt with `destination`.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state to save from
    /// - `visual_cx`: The visual test context of the window
    /// - `destination`: The path the simulated save panel returns
    fn save_active_tab_as(
        fulgur: &Entity<Fulgur>,
        visual_cx: &mut VisualTestContext,
        destination: &Path,
    ) {
        visual_cx.update(|window, cx| fulgur.update(cx, |this, cx| this.save_file_as(window, cx)));
        visual_cx.simulate_new_path_selection(|_| Some(destination.to_path_buf()));
        visual_cx.run_until_parked();
    }

    #[gpui_kit::test]
    fn test_lossy_save_cancel_keeps_file_and_dirty_flag(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("legacy.txt");
        let tab_id = open_legacy_tab(&fulgur, &mut visual_cx, &path, UNMAPPABLE_TEXT, false);

        save_active_tab(&fulgur, &mut visual_cx);
        assert!(
            has_active_dialog(&mut visual_cx),
            "an unmappable character must ask before saving"
        );
        assert_eq!(
            std::fs::read(&path).expect("file should exist"),
            LEGACY_BYTES,
            "the file must stay untouched while the dialog is open"
        );

        dispatch_dialog_action(&mut visual_cx, Cancel);
        assert!(!has_active_dialog(&mut visual_cx));
        assert_eq!(
            std::fs::read(&path).expect("file should exist"),
            LEGACY_BYTES
        );
        let tab = snapshot_tab(&fulgur, &mut visual_cx, tab_id);
        assert!(tab.modified, "cancelling must keep the unsaved edits dirty");
        assert_eq!(tab.encoding, LEGACY_ENCODING);
    }

    #[gpui_kit::test]
    fn test_lossy_save_confirm_rewrites_file_as_utf8(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("legacy.txt");
        let tab_id = open_legacy_tab(&fulgur, &mut visual_cx, &path, UNMAPPABLE_TEXT, false);

        save_active_tab(&fulgur, &mut visual_cx);
        assert!(has_active_dialog(&mut visual_cx));

        dispatch_dialog_action(&mut visual_cx, Confirm { secondary: false });
        assert!(!has_active_dialog(&mut visual_cx));
        assert_eq!(
            std::fs::read(&path).expect("file should exist"),
            UNMAPPABLE_TEXT.as_bytes(),
            "confirming must write UTF-8 bytes, not '?' replacements"
        );
        let tab = snapshot_tab(&fulgur, &mut visual_cx, tab_id);
        assert_eq!(tab.encoding, UTF_8);
        assert!(!tab.modified, "the UTF-8 write must mark the tab clean");
    }

    #[gpui_kit::test]
    fn test_lossy_decoded_tab_confirm_clears_flag_and_writes_utf8(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("lossy_decoded.txt");
        // The text is representable in windows-1252: only the lossy-decode flag forces the prompt.
        let tab_id = open_legacy_tab(&fulgur, &mut visual_cx, &path, "café", true);

        save_active_tab(&fulgur, &mut visual_cx);
        assert!(
            has_active_dialog(&mut visual_cx),
            "a lossily decoded file must ask before saving"
        );
        assert_eq!(
            std::fs::read(&path).expect("file should exist"),
            LEGACY_BYTES
        );

        dispatch_dialog_action(&mut visual_cx, Confirm { secondary: false });
        assert_eq!(
            std::fs::read(&path).expect("file should exist"),
            "café".as_bytes()
        );
        let tab = snapshot_tab(&fulgur, &mut visual_cx, tab_id);
        assert_eq!(tab.encoding, UTF_8);
        assert!(!tab.lossy_decode, "converting to UTF-8 must clear the flag");
        assert!(!tab.modified);
    }

    #[gpui_kit::test]
    fn test_lossy_save_as_confirm_writes_utf8_to_chosen_path(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let source = dir.path().join("legacy.txt");
        let destination = dir.path().join("converted.txt");
        let tab_id = open_legacy_tab(&fulgur, &mut visual_cx, &source, UNMAPPABLE_TEXT, false);

        save_active_tab_as(&fulgur, &mut visual_cx, &destination);
        assert!(has_active_dialog(&mut visual_cx));
        assert!(
            !destination.exists(),
            "nothing may be written while the dialog is open"
        );

        dispatch_dialog_action(&mut visual_cx, Confirm { secondary: false });
        assert_eq!(
            std::fs::read(&destination).expect("destination should exist"),
            UNMAPPABLE_TEXT.as_bytes()
        );
        assert_eq!(
            std::fs::read(&source).expect("source should exist"),
            LEGACY_BYTES,
            "Save As must leave the original file alone"
        );
        let tab = snapshot_tab(&fulgur, &mut visual_cx, tab_id);
        assert_eq!(
            tab.path,
            Some(
                destination
                    .canonicalize()
                    .expect("destination should resolve")
            )
        );
        assert_eq!(tab.encoding, UTF_8);
        assert!(!tab.modified);
    }

    #[gpui_kit::test]
    fn test_lossy_save_as_cancel_writes_nothing(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let source = dir.path().join("legacy.txt");
        let destination = dir.path().join("converted.txt");
        let tab_id = open_legacy_tab(&fulgur, &mut visual_cx, &source, UNMAPPABLE_TEXT, false);

        save_active_tab_as(&fulgur, &mut visual_cx, &destination);
        assert!(has_active_dialog(&mut visual_cx));

        dispatch_dialog_action(&mut visual_cx, Cancel);
        assert!(!has_active_dialog(&mut visual_cx));
        assert!(!destination.exists());
        let tab = snapshot_tab(&fulgur, &mut visual_cx, tab_id);
        assert_eq!(tab.path, Some(source));
        assert_eq!(tab.encoding, LEGACY_ENCODING);
        assert!(tab.modified);
    }
}
