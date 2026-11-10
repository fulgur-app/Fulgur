use super::LogFilePosition;
use super::tail::LogTailChunk;
use crate::fulgur::Fulgur;
use crate::fulgur::editor_tab::TabLocation;
use crate::fulgur::files::file_watcher::FileWatchEvent;
use crate::fulgur::files::file_watcher::test_helpers::setup_fulgur;
use crate::fulgur::ui::tabs::editor_tab::content_fingerprint_from_str;
use crate::fulgur::ui::tabs::tab::TabId;
use crate::test_support::{notification_count, setup_fulgur_with_root};
use gpui_kit::component::input::RopeExt;
use gpui_kit::{Entity, EntityInputHandler, Pixels, TestAppContext, VisualTestContext, point, px};
use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

/// Point the first tab at a log file on disk, seed its buffer with the file
/// content, and activate log view on it.
///
/// ### Arguments
/// - `fulgur`: The window entity under test
/// - `visual_cx`: The visual test context
/// - `content`: The initial file and buffer content
///
/// ### Returns
/// - `(TempDir, TabId)`: The directory holding the log file and the tab id
fn setup_log_tab(
    fulgur: &Entity<Fulgur>,
    visual_cx: &mut VisualTestContext,
    content: &str,
) -> (TempDir, TabId) {
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("tail.log");
    std::fs::write(&path, content).expect("write log file");
    let tab_id = visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let tab = this.tabs.first().expect("expected one tab").clone();
            let tab_id = tab.update(cx, |tab, cx| {
                let editor_tab = tab.as_editor_mut().expect("editor tab");
                editor_tab.location = TabLocation::Local(path.clone());
                editor_tab.content.update(cx, |state, cx| {
                    state.set_value(content, window, cx);
                });
                editor_tab.set_original_content_from_str(content);
                editor_tab.modified = false;
                editor_tab.id
            });
            this.active_tab_id = Some(tab.read(cx).id());
            this.activate_log_view(tab_id, window, cx);
            // The poll task is exercised by the tail unit tests; chunks are
            // applied directly here to keep the tests deterministic.
            this.stop_log_poll_task(tab_id);
            tab_id
        })
    });
    (dir, tab_id)
}

/// Read the buffer text and modified flag of an editor tab.
///
/// ### Arguments
/// - `fulgur`: The window entity under test
/// - `visual_cx`: The visual test context
/// - `tab_id`: The tab to read
///
/// ### Returns
/// - `(String, bool)`: The buffer text and whether the tab reads as modified
fn read_tab(
    fulgur: &Entity<Fulgur>,
    visual_cx: &mut VisualTestContext,
    tab_id: TabId,
) -> (String, bool) {
    visual_cx.update(|_, cx| {
        let this = fulgur.read(cx);
        let editor = this.editor_tab(tab_id, cx).expect("editor tab");
        (editor.content.read(cx).text().to_string(), editor.modified)
    })
}

/// Build an appended chunk advancing the consumed offset by the text length.
///
/// ### Arguments
/// - `text`: The appended text
/// - `offset_before`: The consumed offset before the append
///
/// ### Returns
/// - `LogTailChunk`: A non-reset chunk carrying the text
fn appended(text: &str, offset_before: u64) -> LogTailChunk {
    LogTailChunk {
        text: text.to_string(),
        position: LogFilePosition {
            byte_offset: offset_before + text.len() as u64,
            identity: None,
        },
        reset: false,
    }
}

#[gpui_kit::test]
fn test_activate_log_view_rejects_typing_but_keeps_selection(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let (_dir, tab_id) = setup_log_tab(&fulgur, &mut visual_cx, "line1\n");

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let editor = this.editor_tab(tab_id, cx).expect("editor tab");
            assert!(editor.log_view);
            assert!(editor.log_follow);
            let content = editor.content.clone();
            content.update(cx, |state, cx| {
                assert!(!state.is_editable());
                state.replace_text_in_range(None, "typed", window, cx);
                state.set_selected_range(0..5, cx);
                assert_eq!(state.selected_range(), 0..5);
            });
        });
    });

    let (text, modified) = read_tab(&fulgur, &mut visual_cx, tab_id);
    assert_eq!(text, "line1\n", "user typing must be rejected in log view");
    assert!(!modified);
}

#[gpui_kit::test]
fn test_tail_append_lands_in_buffer_and_keeps_tab_unmodified(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let (_dir, tab_id) = setup_log_tab(&fulgur, &mut visual_cx, "line1\n");

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.apply_log_tail_chunk(tab_id, &appended("line2\n", 6), window, cx);
        });
    });
    visual_cx.run_until_parked();

    let (text, modified) = read_tab(&fulgur, &mut visual_cx, tab_id);
    assert_eq!(text, "line1\nline2\n");
    assert!(!modified, "a tail write must not dirty the tab");
    visual_cx.update(|_, cx| {
        let this = fulgur.read(cx);
        let state = this.log_tail_state.get(&tab_id).expect("tail state");
        assert_eq!(state.byte_offset, 12);
        let editor = this.editor_tab(tab_id, cx).expect("editor tab");
        let end = editor.content.read(cx).text().len();
        assert_eq!(
            editor.content.read(cx).selected_range(),
            end..end,
            "following snaps the caret to the end"
        );
    });
}

#[gpui_kit::test]
fn test_tail_append_without_follow_preserves_selection(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let (_dir, tab_id) = setup_log_tab(&fulgur, &mut visual_cx, "line1\nline2\n");

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.update_editor_tab(tab_id, cx, |editor, cx| {
                editor.log_follow = false;
                editor.content.update(cx, |state, cx| {
                    state.set_selected_range(0..5, cx);
                });
            });
            this.apply_log_tail_chunk(tab_id, &appended("line3\n", 12), window, cx);
        });
    });

    visual_cx.update(|_, cx| {
        let this = fulgur.read(cx);
        let editor = this.editor_tab(tab_id, cx).expect("editor tab");
        assert_eq!(
            editor.content.read(cx).text().to_string(),
            "line1\nline2\nline3\n"
        );
        assert_eq!(
            editor.content.read(cx).selected_range(),
            0..5,
            "paused follow keeps the user's selection"
        );
    });
}

#[gpui_kit::test]
fn test_reset_chunk_replaces_buffer(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let (_dir, tab_id) = setup_log_tab(&fulgur, &mut visual_cx, "old1\nold2\n");

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let chunk = LogTailChunk {
                text: "fresh\n".to_string(),
                position: LogFilePosition {
                    byte_offset: 6,
                    identity: None,
                },
                reset: true,
            };
            this.apply_log_tail_chunk(tab_id, &chunk, window, cx);
        });
    });

    let (text, modified) = read_tab(&fulgur, &mut visual_cx, tab_id);
    assert_eq!(text, "fresh\n");
    assert!(!modified);
}

#[gpui_kit::test]
fn test_deactivate_log_view_restores_editability(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let (_dir, tab_id) = setup_log_tab(&fulgur, &mut visual_cx, "line1\n");

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.deactivate_log_view(tab_id, cx);
            let editor = this.editor_tab(tab_id, cx).expect("editor tab");
            assert!(!editor.log_view);
            assert!(!this.log_tail_state.contains_key(&tab_id));
            editor.content.clone().update(cx, |state, cx| {
                assert!(state.is_editable());
                state.replace_text_in_range(None, "typed", window, cx);
            });
        });
    });

    let (text, _) = read_tab(&fulgur, &mut visual_cx, tab_id);
    assert_eq!(
        text, "line1\ntyped",
        "the caret was left at the end by log view"
    );
}

#[gpui_kit::test]
fn test_watch_event_on_log_tab_neither_reloads_nor_conflicts(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let (dir, tab_id) = setup_log_tab(&fulgur, &mut visual_cx, "line1\n");
    let path = dir.path().join("tail.log");
    std::fs::write(&path, "line1\nfrom-disk\n").expect("append to log file");

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.handle_file_watch_event(FileWatchEvent::Modified(path.clone()), window, cx);
        });
    });
    visual_cx.run_until_parked();

    let (text, _) = read_tab(&fulgur, &mut visual_cx, tab_id);
    assert_eq!(
        text, "line1\n",
        "the watcher must leave a log tab to the poller"
    );
    visual_cx.update(|_, cx| {
        let this = fulgur.read(cx);
        assert!(this.file_watch_state.open_conflict_dialogs.is_empty());
        assert!(!this.file_watch_state.pending_conflicts.contains_key(&path));
    });
}

#[gpui_kit::test]
fn test_activate_log_view_rereads_a_buffer_that_lags_the_file(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("lagging.log");
    std::fs::write(&path, "line1\nline2\n").expect("write log file");

    let tab_id = visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let tab = this.tabs.first().expect("expected one tab").clone();
            let tab_id = tab.update(cx, |tab, cx| {
                let editor_tab = tab.as_editor_mut().expect("editor tab");
                editor_tab.location = TabLocation::Local(path.clone());
                editor_tab.content.update(cx, |state, cx| {
                    state.set_value("line1\n", window, cx);
                });
                editor_tab.set_original_content_from_str("line1\n");
                editor_tab.modified = false;
                editor_tab.id
            });
            this.activate_log_view(tab_id, window, cx);
            let editor = this.editor_tab(tab_id, cx).expect("editor tab");
            assert_eq!(
                editor.content.read(cx).text().to_string(),
                "line1\n",
                "the reread must not run on the UI thread during activation"
            );
            let state = this.log_tail_state.get(&tab_id).expect("tail state");
            assert!(state.reseed_pending);
            tab_id
        })
    });
    visual_cx.run_until_parked();

    visual_cx.update(|_, cx| {
        fulgur.update(cx, |this, cx| {
            let editor = this.editor_tab(tab_id, cx).expect("editor tab");
            assert_eq!(editor.content.read(cx).text().to_string(), "line1\nline2\n");
            assert!(!editor.modified);
            let state = this.log_tail_state.get(&tab_id).expect("tail state");
            assert_eq!(state.byte_offset, 12);
            assert!(!state.reseed_pending);
            this.stop_log_poll_task(tab_id);
        });
    });
}

#[gpui_kit::test]
fn test_tail_keeps_a_length_baseline_and_leaving_log_view_fingerprints_it(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let (_dir, tab_id) = setup_log_tab(&fulgur, &mut visual_cx, "line1\n");

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.apply_log_tail_chunk(tab_id, &appended("line2\n", 6), window, cx);
            let editor = this.editor_tab(tab_id, cx).expect("editor tab");
            assert_eq!(
                editor.original_content_hash, 0,
                "a tail chunk must not fingerprint the whole buffer"
            );
            assert_eq!(editor.original_content_len, 12);
            assert!(!editor.content_differs_from_original(cx));

            this.deactivate_log_view(tab_id, cx);
            let editor = this.editor_tab(tab_id, cx).expect("editor tab");
            assert_eq!(
                editor.original_content_hash,
                content_fingerprint_from_str("line1\nline2\n").0
            );
            assert!(!editor.content_differs_from_original(cx));
            assert!(!editor.modified);
        });
    });
}

#[gpui_kit::test]
fn test_toggle_log_view_refuses_a_modified_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("edited.log");
    std::fs::write(&path, "line1\n").expect("write log file");

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let tab = this.tabs.first().expect("expected one tab").clone();
            tab.update(cx, |tab, cx| {
                let editor_tab = tab.as_editor_mut().expect("editor tab");
                editor_tab.location = TabLocation::Local(path.clone());
                editor_tab.content.update(cx, |state, cx| {
                    state.set_value("line1\nunsaved", window, cx);
                });
                editor_tab.set_original_content_from_str("line1\n");
                editor_tab.modified = true;
            });
            this.active_tab_id = Some(tab.read(cx).id());
            this.toggle_log_view(window, cx);
            let editor = this.get_active_editor_tab(cx).expect("editor tab");
            assert!(!editor.log_view, "a modified tab must not enter log view");
            assert!(editor.modified);
            assert_eq!(editor.content.read(cx).text().to_string(), "line1\nunsaved");
        });
    });
}

#[gpui_kit::test]
fn test_log_view_suspends_color_highlighting(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let (_dir, tab_id) = setup_log_tab(&fulgur, &mut visual_cx, "#ff0000\n");

    visual_cx.update(|_, cx| {
        fulgur.update(cx, |this, cx| {
            assert!(this.settings.editor_settings.highlight_colors);
            let editor = this.editor_tab(tab_id, cx).expect("editor tab");
            assert!(
                !editor.highlight_colors,
                "the whole-buffer colour scan must not run on a tailed log"
            );
            this.deactivate_log_view(tab_id, cx);
            let editor = this.editor_tab(tab_id, cx).expect("editor tab");
            assert!(
                editor.highlight_colors,
                "leaving log view restores the setting"
            );
        });
    });
}

/// Encode text as UTF-16LE code units, independently of the code under test.
///
/// ### Arguments
/// - `text`: The text to encode
///
/// ### Returns
/// - `Vec<u8>`: The UTF-16LE code units, without a byte order mark
fn utf16le_bytes(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

#[gpui_kit::test]
fn test_utf16_log_is_tailed_decoded_and_saves_back_unchanged(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("utf16.log");
    let mut on_disk = vec![0xFF, 0xFE];
    on_disk.extend(utf16le_bytes("caf\u{e9}\n"));
    std::fs::write(&path, &on_disk).expect("write UTF-16 log");

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| this.do_open_file(window, cx, path.clone()));
    });
    visual_cx.run_until_parked();

    let tab_id = fulgur.read_with(&visual_cx, |this, cx| {
        let editor = this.get_active_editor_tab(cx).expect("editor tab");
        assert!(editor.log_view, "a .log file opens in log view");
        assert_eq!(editor.encoding, "UTF-16LE");
        assert_eq!(
            editor.content.read(cx).text().to_string(),
            "caf\u{e9}\n",
            "the activation reread must decode UTF-16, not UTF-8"
        );
        editor.id
    });

    let appended = utf16le_bytes("line2 \u{1f600}\n");
    on_disk.extend(&appended);
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("open log for append");
    std::io::Write::write_all(&mut file, &appended).expect("append to log");
    drop(file);
    visual_cx
        .background_executor
        .advance_clock(Duration::from_secs(1));
    visual_cx.run_until_parked();

    let (text, modified) = read_tab(&fulgur, &mut visual_cx, tab_id);
    assert_eq!(text, "caf\u{e9}\nline2 \u{1f600}\n");
    assert!(!modified);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.deactivate_log_view(tab_id, cx);
            this.save_file(window, cx);
        });
    });
    visual_cx.run_until_parked();
    let saved = std::fs::read(&path).expect("read saved log");
    assert_eq!(saved, on_disk, "saving must write back the original bytes");
}

/// Point the first tab at a file with the given encoding label and make it active.
///
/// ### Arguments
/// - `fulgur`: The window entity under test
/// - `visual_cx`: The visual test context
/// - `path`: The file path the tab points at
/// - `encoding`: The encoding label the tab reports
/// - `log_view`: The initial log-view flag, as a restore or a transfer would set it
///
/// ### Returns
/// - `TabId`: The id of the prepared tab
fn prepare_encoded_tab(
    fulgur: &Entity<Fulgur>,
    visual_cx: &mut VisualTestContext,
    path: &Path,
    encoding: &str,
    log_view: bool,
) -> TabId {
    visual_cx.update(|_, cx| {
        fulgur.update(cx, |this, cx| {
            let tab = this.tabs.first().expect("expected one tab").clone();
            let tab_id = tab.update(cx, |tab, _| {
                let editor_tab = tab.as_editor_mut().expect("editor tab");
                editor_tab.location = TabLocation::Local(path.to_path_buf());
                editor_tab.encoding = encoding.to_string();
                editor_tab.log_view = log_view;
                editor_tab.id
            });
            this.active_tab_id = Some(tab_id);
            tab_id
        })
    })
}

#[gpui_kit::test]
fn test_toggle_log_view_refuses_a_multibyte_legacy_encoding_with_a_notice(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("sjis.log");
    std::fs::write(&path, b"line1\n").expect("write log file");
    let tab_id = prepare_encoded_tab(&fulgur, &mut visual_cx, &path, "Shift_JIS", false);

    let notifications_before = notification_count(&mut visual_cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| this.toggle_log_view(window, cx));
    });

    fulgur.read_with(&visual_cx, |this, cx| {
        let editor = this.editor_tab(tab_id, cx).expect("editor tab");
        assert!(!editor.log_view, "a Shift_JIS tab must stay in the editor");
        assert!(!this.log_tail_state.contains_key(&tab_id));
    });
    assert_eq!(
        notification_count(&mut visual_cx),
        notifications_before + 1,
        "the refusal must tell the user why"
    );
}

#[gpui_kit::test]
fn test_activate_log_view_clears_a_restored_flag_for_a_multibyte_legacy_encoding(
    cx: &mut TestAppContext,
) {
    let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
    let dir = TempDir::new().expect("temp dir");
    let path = dir.path().join("big5.log");
    std::fs::write(&path, b"line1\n").expect("write log file");
    let tab_id = prepare_encoded_tab(&fulgur, &mut visual_cx, &path, "Big5", true);

    let notifications_before = notification_count(&mut visual_cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| this.activate_log_view(tab_id, window, cx));
    });

    fulgur.read_with(&visual_cx, |this, cx| {
        let editor = this.editor_tab(tab_id, cx).expect("editor tab");
        assert!(
            !editor.log_view,
            "an untailable tab falls back to the editor"
        );
        assert!(!this.log_tail_state.contains_key(&tab_id));
        assert!(!this.log_tail_cancel.contains_key(&tab_id), "no poll task");
    });
    assert_eq!(
        notification_count(&mut visual_cx),
        notifications_before,
        "automatic activation (open, restore, transfer) must stay silent"
    );
}

/// Build a buffer of `count` newline-terminated numbered lines.
///
/// ### Arguments
/// - `count`: The number of lines to generate
///
/// ### Returns
/// - `String`: The generated text
fn numbered_lines(count: usize) -> String {
    (0..count).fold(String::new(), |mut text, index| {
        let _ = writeln!(text, "line {index}");
        text
    })
}

/// Read the scroll offset of a tab's buffer together with the offset that
/// would show its last line at the bottom of the viewport.
///
/// ### Arguments
/// - `fulgur`: The window entity under test
/// - `visual_cx`: The visual test context
/// - `tab_id`: The tab to read
///
/// ### Returns
/// - `(Pixels, Pixels)`: The current vertical scroll offset and the bottom offset
fn scroll_and_bottom(
    fulgur: &Entity<Fulgur>,
    visual_cx: &mut VisualTestContext,
    tab_id: TabId,
) -> (Pixels, Pixels) {
    visual_cx.update(|_, cx| {
        let this = fulgur.read(cx);
        let editor = this.editor_tab(tab_id, cx).expect("editor tab");
        let state = editor.content.read(cx);
        let line_height = state.line_height().expect("laid out");
        let rows = u16::try_from(state.text().lines_len()).expect("small test buffer");
        let content_height = line_height * f32::from(rows);
        let bottom = (state.input_bounds().size.height - content_height).min(px(0.));
        (state.scroll_offset().y, bottom)
    })
}

#[gpui_kit::test]
fn test_follow_keeps_the_viewport_at_the_bottom(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let seed = numbered_lines(200);
    let (_dir, tab_id) = setup_log_tab(&fulgur, &mut visual_cx, &seed);
    visual_cx.run_until_parked();

    let (offset, bottom) = scroll_and_bottom(&fulgur, &mut visual_cx, tab_id);
    assert!(bottom < px(0.), "the seed must overflow the viewport");
    assert_eq!(offset, bottom, "activation reveals the last line");

    let mut consumed = seed.len() as u64;
    for index in 0..3 {
        let text = format!("appended {index}\n");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.apply_log_tail_chunk(tab_id, &appended(&text, consumed), window, cx);
            });
        });
        consumed += text.len() as u64;
        visual_cx.run_until_parked();
        let (offset, bottom) = scroll_and_bottom(&fulgur, &mut visual_cx, tab_id);
        assert_eq!(offset, bottom, "append {index} must land at the new bottom");
    }
}

#[gpui_kit::test]
fn test_paused_follow_keeps_the_viewport_in_place(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let seed = numbered_lines(200);
    let (_dir, tab_id) = setup_log_tab(&fulgur, &mut visual_cx, &seed);
    visual_cx.run_until_parked();

    let parked = px(-210.);
    visual_cx.update(|_, cx| {
        fulgur.update(cx, |this, cx| {
            this.update_editor_tab(tab_id, cx, |editor, cx| {
                editor.log_follow = false;
                editor.content.update(cx, |state, cx| {
                    state.set_scroll_offset(point(px(0.), parked), cx);
                });
            });
        });
    });
    visual_cx.run_until_parked();

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.apply_log_tail_chunk(tab_id, &appended("late\n", seed.len() as u64), window, cx);
        });
    });
    visual_cx.run_until_parked();
    let (offset, _) = scroll_and_bottom(&fulgur, &mut visual_cx, tab_id);
    assert_eq!(
        offset, parked,
        "a paused log must not move under the reader"
    );
}
