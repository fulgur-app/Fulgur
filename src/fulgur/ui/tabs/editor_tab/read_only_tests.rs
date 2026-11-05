use super::EditorTab;
use crate::fulgur::{
    Fulgur,
    languages::supported_languages::SupportedLanguage,
    shared_state::{ExternalOpenFile, SharedAppState},
    tab::TabId,
};
use crate::test_support::{open_rendered_window, setup_fulgur_with_root, setup_test_globals};
use gpui_kit::{BorrowAppContext, Entity, EntityInputHandler, TestAppContext, VisualTestContext};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

/// Turn the current test process into a `--read-only` session.
///
/// ### Arguments
/// - `visual_cx`: The visual test context
fn enable_read_only_session(visual_cx: &mut VisualTestContext) {
    visual_cx.update(|_, cx| {
        cx.update_global::<SharedAppState, _>(|shared, _| shared.read_only_session = true);
    });
}

/// Write a file in a temporary directory.
///
/// ### Arguments
/// - `dir`: The temporary directory
/// - `name`: The file name
/// - `contents`: The file contents
///
/// ### Returns
/// - `PathBuf`: The canonical path of the new file
fn write_file(dir: &TempDir, name: &str, contents: &str) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, contents).expect("failed to write temp file");
    fs::canonicalize(&path).expect("failed to canonicalize temp file")
}

/// Open a file through the regular open path and wait for its tab.
///
/// ### Arguments
/// - `fulgur`: The window entity under test
/// - `visual_cx`: The visual test context
/// - `path`: The file to open
fn open_file(fulgur: &Entity<Fulgur>, visual_cx: &mut VisualTestContext, path: &Path) {
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.do_open_file(window, cx, path.to_path_buf());
        });
    });
    visual_cx.run_until_parked();
}

/// Read the lock state of the tab holding a file.
///
/// ### Arguments
/// - `fulgur`: The window entity under test
/// - `visual_cx`: The visual test context
/// - `path`: The canonical path of the file
///
/// ### Returns
/// - `(bool, bool)`: The tab's `read_only` flag and whether its buffer accepts user edits
fn file_tab_lock_state(
    fulgur: &Entity<Fulgur>,
    visual_cx: &mut VisualTestContext,
    path: &Path,
) -> (bool, bool) {
    visual_cx.update(|_, cx| {
        let this = fulgur.read(cx);
        let index = this
            .find_tab_by_path(&path.to_path_buf(), cx)
            .expect("expected a file tab");
        let editor = this.tabs[index].read(cx).as_editor().expect("editor tab");
        (editor.read_only, editor.content.read(cx).is_editable())
    })
}

/// Read the text of the active editor tab.
///
/// ### Arguments
/// - `fulgur`: The window entity under test
/// - `visual_cx`: The visual test context
///
/// ### Returns
/// - `String`: The buffer text
fn active_text(fulgur: &Entity<Fulgur>, visual_cx: &mut VisualTestContext) -> String {
    visual_cx.update(|_, cx| {
        fulgur
            .read(cx)
            .get_active_editor_tab(cx)
            .expect("expected an active editor tab")
            .content
            .read(cx)
            .text()
            .to_string()
    })
}

#[gpui_kit::test]
fn test_read_only_request_locks_only_the_forwarded_file(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
    let dir = TempDir::new().expect("failed to create temp dir");
    let read_only_path = write_file(&dir, "forwarded_read_only.txt", "locked");
    let editable_path = write_file(&dir, "forwarded_editable.txt", "free");

    visual_cx.update(|window, cx| {
        cx.global::<SharedAppState>()
            .pending_files_from_macos
            .lock()
            .extend([
                ExternalOpenFile {
                    path: read_only_path.clone(),
                    read_only: true,
                },
                ExternalOpenFile {
                    path: editable_path.clone(),
                    read_only: false,
                },
            ]);
        fulgur.update(cx, |this, cx| {
            this.process_pending_external_files(window, cx);
        });
    });
    visual_cx.run_until_parked();

    assert_eq!(
        file_tab_lock_state(&fulgur, &mut visual_cx, &read_only_path),
        (true, false)
    );
    assert_eq!(
        file_tab_lock_state(&fulgur, &mut visual_cx, &editable_path),
        (false, true)
    );
}

#[gpui_kit::test]
fn test_read_only_session_locks_files_opened_after_launch(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
    enable_read_only_session(&mut visual_cx);
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = write_file(&dir, "opened_later.txt", "content");

    open_file(&fulgur, &mut visual_cx, &path);

    assert_eq!(
        file_tab_lock_state(&fulgur, &mut visual_cx, &path),
        (true, false)
    );
}

#[gpui_kit::test]
fn test_read_only_session_keeps_new_blank_tabs_editable(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
    enable_read_only_session(&mut visual_cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.new_tab(window, cx);
            let editor = this.get_active_editor_tab(cx).expect("editor tab");
            assert!(!editor.read_only);
            assert!(editor.content.read(cx).is_editable());
        });
    });
}

#[gpui_kit::test]
fn test_read_only_request_leaves_an_already_open_tab_editable(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = write_file(&dir, "already_open.txt", "content");
    open_file(&fulgur, &mut visual_cx, &path);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.open_local_file(window, cx, path.clone(), true);
        });
    });
    visual_cx.run_until_parked();

    assert_eq!(
        file_tab_lock_state(&fulgur, &mut visual_cx, &path),
        (false, true)
    );
}

#[gpui_kit::test]
fn test_typing_is_rejected_on_a_read_only_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
    enable_read_only_session(&mut visual_cx);
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = write_file(&dir, "typing.txt", "content");
    open_file(&fulgur, &mut visual_cx, &path);

    visual_cx.update(|window, cx| {
        let content = fulgur
            .read(cx)
            .get_active_editor_tab(cx)
            .expect("editor tab")
            .content
            .clone();
        content.update(cx, |state, cx| {
            state.replace_text_in_range(None, "typed", window, cx);
        });
    });

    assert_eq!(active_text(&fulgur, &mut visual_cx), "content");
}

#[gpui_kit::test]
fn test_rendering_does_not_unlock_a_read_only_tab(cx: &mut TestAppContext) {
    setup_test_globals(cx);
    let (fulgur, _, mut visual_cx) = open_rendered_window(cx);
    enable_read_only_session(&mut visual_cx);
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = write_file(&dir, "rendered.txt", "content");
    open_file(&fulgur, &mut visual_cx, &path);

    visual_cx.update(|window, _| window.refresh());
    visual_cx.run_until_parked();

    assert_eq!(
        file_tab_lock_state(&fulgur, &mut visual_cx, &path),
        (true, false),
        "the rendered Editor re-applies its read-only flag on every frame"
    );
}

#[gpui_kit::test]
fn test_save_does_not_write_a_read_only_file(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
    enable_read_only_session(&mut visual_cx);
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = write_file(&dir, "save_blocked.txt", "on disk");
    open_file(&fulgur, &mut visual_cx, &path);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.update_active_editor_tab(cx, |editor, cx| {
                editor.content.update(cx, |state, cx| {
                    state.set_value("restored unsaved text", window, cx);
                });
            });
            this.save_file(window, cx);
        });
    });
    visual_cx.run_until_parked();

    assert_eq!(
        fs::read_to_string(&path).expect("failed to read file"),
        "on disk"
    );
}

#[gpui_kit::test]
fn test_leaving_log_view_keeps_a_read_only_tab_locked(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
    enable_read_only_session(&mut visual_cx);
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = write_file(&dir, "server.log", "line\n");
    open_file(&fulgur, &mut visual_cx, &path);

    // Asserted before the update ends: the next render would lock the active tab
    // again and hide a lock lost here, which an inactive tab would not recover from.
    visual_cx.update(|_, cx| {
        fulgur.update(cx, |this, cx| {
            let tab_id = this.active_tab_id.expect("expected an active tab");
            assert!(
                this.get_active_editor_tab(cx).expect("editor tab").log_view,
                "a .log file opens in log view"
            );
            this.deactivate_log_view(tab_id, cx);
            let editor = this.get_active_editor_tab(cx).expect("editor tab");
            assert!(editor.read_only);
            assert!(!editor.content.read(cx).is_editable());
        });
    });
}

#[gpui_kit::test]
fn test_transfer_to_another_window_keeps_the_tab_read_only(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
    enable_read_only_session(&mut visual_cx);
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = write_file(&dir, "transferred.txt", "content");
    open_file(&fulgur, &mut visual_cx, &path);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let tab_id = this.active_tab_id.expect("expected an active tab");
            let data = this
                .extract_tab_transfer_data(tab_id, cx)
                .expect("expected transfer data");
            let settings = this.settings.editor_settings.clone();
            let received = EditorTab::from_transfer(TabId(99), data, window, cx, &settings);
            assert!(received.read_only);
            assert!(!received.content.read(cx).is_editable());
        });
    });
}

#[gpui_kit::test]
fn test_duplicating_a_read_only_tab_stays_read_only(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
    enable_read_only_session(&mut visual_cx);
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = write_file(&dir, "duplicated.txt", "content");
    open_file(&fulgur, &mut visual_cx, &path);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let index = this.active_tab_index(cx).expect("expected an active tab");
            this.duplicate_tab(index, window, cx);
            let duplicate = this.get_active_editor_tab(cx).expect("editor tab");
            assert!(duplicate.location.is_untitled());
            assert!(duplicate.read_only);
            assert!(!duplicate.content.read(cx).is_editable());
        });
    });
}

#[gpui_kit::test]
fn test_markdown_toolbar_is_hidden_and_inert_on_a_read_only_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);

    visual_cx.update(|window, cx| {
        let toolbar = fulgur.read(cx).markdown_toolbar.clone();
        fulgur.update(cx, |this, cx| {
            this.update_active_editor_tab(cx, |editor, cx| {
                editor.language = SupportedLanguage::Markdown;
                editor.show_markdown_toolbar = true;
                editor.content.update(cx, |state, cx| {
                    state.set_value("hello", window, cx);
                });
            });
            assert!(this.markdown_toolbar_visible(cx));
            this.update_active_editor_tab(cx, |editor, cx| editor.make_read_only(cx));
            assert!(!this.markdown_toolbar_visible(cx));
        });
        toolbar.update(cx, |bar, cx| {
            bar.insert_or_surround("**", "**", window, cx);
        });
    });

    assert_eq!(active_text(&fulgur, &mut visual_cx), "hello");
}

#[gpui_kit::test]
fn test_csv_table_cannot_be_restructured_on_a_read_only_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let columns_movable = |this: &mut Fulgur, cx: &mut gpui_kit::Context<Fulgur>| {
                let editor = this.get_active_editor_tab(cx).expect("editor tab");
                let table = editor.csv_table.as_ref().expect("expected a CSV table");
                table.read(cx).col_movable
            };
            this.update_active_editor_tab(cx, |editor, cx| {
                editor.language = SupportedLanguage::Csv;
                editor.csv_delimiter = b',';
                editor.content.update(cx, |state, cx| {
                    state.set_value("a,b\n1,2", window, cx);
                });
                editor.ensure_csv_table(window, cx);
            });
            this.toggle_csv_view_mode(window, cx);
            assert!(this.csv_toolbar_visible(cx));
            assert!(columns_movable(this, cx));

            this.update_active_editor_tab(cx, |editor, cx| {
                editor.make_read_only(cx);
                editor.csv_table = None;
                editor.ensure_csv_table(window, cx);
            });
            assert!(!this.csv_toolbar_visible(cx));
            assert!(!columns_movable(this, cx));
        });
    });
}
