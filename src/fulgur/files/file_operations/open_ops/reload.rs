use super::super::{DecodedContents, detect_encoding_and_decode};
use crate::fulgur::{Fulgur, editor_tab::ContentRevision, tab::Tab, ui::tabs::tab::TabId};
use gpui_kit::{Context, Window};
use std::path::PathBuf;

/// Editor state that must remain unchanged while a local reload is in flight.
struct LocalReloadGuard {
    tab_id: TabId,
    path: PathBuf,
    content_revision: ContentRevision,
}

impl Fulgur {
    /// Reload tab content from disk
    ///
    /// ### Arguments
    /// - `tab_index`: The index of the tab to reload
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn reload_tab_from_disk(
        &mut self,
        tab_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let guard = if let Some(Tab::Editor(editor_tab)) =
            self.tabs.get(tab_index).map(|tab| tab.read(cx))
        {
            editor_tab
                .file_path()
                .cloned()
                .map(|path| LocalReloadGuard {
                    tab_id: editor_tab.id,
                    path,
                    content_revision: editor_tab.content_revision(cx),
                })
        } else {
            None
        };
        let Some(guard) = guard else {
            return;
        };
        log::debug!("Reloading tab content from disk: {}", guard.path.display());
        cx.spawn_in(window, async move |view, window| {
            let read_path = guard.path.clone();
            let read_result = window
                .background_executor()
                .spawn(async move { std::fs::read(&read_path).map(detect_encoding_and_decode) })
                .await;
            match read_result {
                Ok(decoded) => {
                    window
                        .update(|window, cx| {
                            _ = view.update(cx, |this, cx| {
                                this.apply_reloaded_contents(guard, decoded, window, cx);
                            });
                        })
                        .ok();
                }
                Err(e) => {
                    log::error!("Failed to reload file {}: {e}", guard.path.display());
                }
            }
        })
        .detach();
    }

    /// Apply freshly decoded file contents to the editor tab backing a path.
    ///
    /// ### Arguments
    /// - `guard`: Stable tab, path, and content revision captured before the read
    /// - `decoded`: The decoded file contents produced off the UI thread
    /// - `window`: The window context
    /// - `cx`: The application context
    fn apply_reloaded_contents(
        &mut self,
        guard: LocalReloadGuard,
        decoded: DecodedContents,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_index) = self.tab_index_of(guard.tab_id, cx) else {
            return;
        };
        let Some(tab_entity) = self.tabs.get(tab_index).cloned() else {
            return;
        };
        let Some((current_path, current_revision)) =
            tab_entity.read(cx).as_editor().and_then(|editor| {
                editor
                    .file_path()
                    .cloned()
                    .map(|path| (path, editor.content_revision(cx)))
            })
        else {
            return;
        };
        if current_path != guard.path {
            return;
        }
        if current_revision != guard.content_revision {
            log::warn!(
                "Discarding stale reload for {} because the buffer changed while reading",
                guard.path.display()
            );
            tab_entity.update(cx, |tab, cx| {
                if let Some(editor) = tab.as_editor_mut() {
                    editor.check_modified(cx);
                    cx.notify();
                }
            });
            if self.active_tab_index(cx) == Some(tab_index) {
                self.show_file_conflict_dialog(&guard.path, guard.tab_id, window, cx);
            } else {
                self.file_watch_state
                    .pending_conflicts
                    .insert(guard.path, tab_index);
            }
            return;
        }
        tab_entity.update(cx, |tab, cx| {
            let Some(editor_tab) = tab.as_editor_mut() else {
                return;
            };
            let (cursor, scroll_offset) = {
                let input_state = editor_tab.content.read(cx);
                (input_state.cursor(), input_state.scroll_offset())
            };
            editor_tab.content.update(cx, |input_state, cx| {
                input_state.set_value(&decoded.content, window, cx);
            });
            editor_tab.set_original_content_from_str(&decoded.content);
            editor_tab.encoding = decoded.encoding;
            editor_tab.lossy_decode = decoded.lossy;
            editor_tab.modified = false;
            editor_tab.update_file_tooltip_cache(decoded.byte_len);
            tab.update_language(cx);
            if let Some(editor_tab) = tab.as_editor_mut() {
                editor_tab.content.update(cx, |input_state, cx| {
                    input_state.set_selected_range(cursor..cursor, cx);
                    input_state.set_scroll_offset(scroll_offset, cx);
                });
            }
            log::debug!(
                "Tab reloaded successfully from disk: {}",
                guard.path.display()
            );
        });
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::{
        editor_tab::{EditorTab, TabLocation},
        tab::Tab,
    };

    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::files::file_operations::test_helpers::{
        setup_fulgur, setup_fulgur_with_root,
    };
    #[cfg(feature = "gpui-test-support")]
    use gpui_kit::TestAppContext;
    #[cfg(feature = "gpui-test-support")]
    use std::fmt::Write as _;
    #[cfg(feature = "gpui-test-support")]
    use tempfile::TempDir;

    // ========== reload_tab_from_disk tests ==========

    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
    fn test_reload_tab_from_disk_updates_content_from_file(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("reload_test.txt");
        std::fs::write(&path, "initial content").expect("failed to write initial file");

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .last()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, _cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                            editor_tab.set_original_content_from_str("initial content");
                        }
                    });
            });
        });

        std::fs::write(&path, "updated content").expect("failed to overwrite file");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.reload_tab_from_disk(0, window, cx);
            });
        });
        // The read and decode now run on the background executor and are applied
        // asynchronously, so let the spawned task complete before asserting.
        visual_cx.run_until_parked();

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let content = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .map(|e| e.content.read(cx).text().to_string())
                    .unwrap_or_default();
                assert_eq!(content, "updated content");
                let modified = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .is_none_or(|e| e.modified);
                assert!(!modified, "tab should not be marked modified after reload");
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
    fn test_reload_tab_from_disk_keeps_caret_and_input_state(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("reload_position.txt");
        let numbered_lines = |suffix: &str| {
            (0..200).fold(String::new(), |mut acc, i| {
                writeln!(acc, "line {i}{suffix}").expect("writing to a String cannot fail");
                acc
            })
        };
        let initial = numbered_lines("");
        std::fs::write(&path, &initial).expect("failed to write initial file");

        let caret = initial
            .find("line 120")
            .expect("expected the anchor line in the fixture");

        let input_before = visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let tab_entity = this.tabs.last().expect("expected at least one tab").clone();
                tab_entity.update(cx, |tab, cx| {
                    let editor_tab = tab.as_editor_mut().expect("expected an editor tab");
                    editor_tab.location = TabLocation::Local(path.clone());
                    editor_tab.content.update(cx, |input_state, cx| {
                        input_state.set_value(&initial, window, cx);
                        input_state.set_selected_range(caret..caret, cx);
                    });
                    editor_tab.set_original_content_from_str(&initial);
                    editor_tab.modified = false;
                    editor_tab.content.entity_id()
                })
            })
        });

        // Rewrite the file with the same shape, as an external editor would.
        let updated = numbered_lines(" edited");
        std::fs::write(&path, &updated).expect("failed to overwrite file");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.reload_tab_from_disk(0, window, cx);
            });
        });
        visual_cx.run_until_parked();

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let editor_tab = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .expect("expected an editor tab");
                assert_eq!(
                    editor_tab.content.entity_id(),
                    input_before,
                    "an unchanged language must not rebuild the input state, which would drop the scroll offset"
                );
                assert_eq!(
                    editor_tab.content.read(cx).cursor(),
                    caret,
                    "the caret must survive an external edit instead of snapping back to the top"
                );
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
    fn test_reload_tab_from_disk_is_noop_without_file_path(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                // The default tab created on Fulgur::new has no file_path
                let initial_content = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .map(|e| e.content.read(cx).text().to_string())
                    .unwrap_or_default();
                this.reload_tab_from_disk(0, window, cx);
                let content_after = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .map(|e| e.content.read(cx).text().to_string())
                    .unwrap_or_default();
                assert_eq!(content_after, initial_content);
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
    fn test_reload_tab_from_disk_keeps_edits_made_while_reading(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("reload_race.txt");
        std::fs::write(&path, "content-from-disk").expect("failed to write test file");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let tab_entity = this.tabs.first().expect("expected an editor tab").clone();
                tab_entity.update(cx, |tab, _cx| {
                    let editor = tab.as_editor_mut().expect("expected an editor tab");
                    editor.location = TabLocation::Local(path.clone());
                    editor.set_original_content_from_str("");
                    editor.modified = false;
                });

                this.reload_tab_from_disk(0, window, cx);

                tab_entity.update(cx, |tab, cx| {
                    let editor = tab.as_editor_mut().expect("expected an editor tab");
                    editor.content.update(cx, |state, cx| {
                        state.set_value("local edit", window, cx);
                    });
                });
            });
        });
        visual_cx.run_until_parked();

        visual_cx.update(|_window, cx| {
            let editor = fulgur
                .read(cx)
                .tabs
                .first()
                .and_then(|tab| tab.read(cx).as_editor())
                .expect("expected an editor tab");
            assert_eq!(editor.content.read(cx).text().to_string(), "local edit");
            assert!(editor.modified, "the intervening edit must remain dirty");
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
    fn test_reload_tab_from_disk_does_not_target_reopened_path(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("reopened_path.txt");
        std::fs::write(&path, "stale async result").expect("failed to write test file");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let old_tab = this.tabs.first().expect("expected an editor tab").clone();
                old_tab.update(cx, |tab, _cx| {
                    tab.as_editor_mut()
                        .expect("expected an editor tab")
                        .location = TabLocation::Local(path.clone());
                });
                this.reload_tab_from_disk(0, window, cx);

                let new_id = this.allocate_tab_id();
                let mut replacement = EditorTab::new(
                    new_id,
                    "reopened_path.txt",
                    window,
                    cx,
                    &this.settings.editor_settings,
                );
                replacement.location = TabLocation::Local(path.clone());
                replacement.content.update(cx, |state, cx| {
                    state.set_value("new tab content", window, cx);
                });
                replacement.set_original_content_from_str("new tab content");
                this.tabs[0] = Tab::Editor(replacement).into_entity(cx);
                this.active_tab_id = Some(new_id);
            });
        });
        visual_cx.run_until_parked();

        visual_cx.update(|_window, cx| {
            let editor = fulgur
                .read(cx)
                .tabs
                .first()
                .and_then(|tab| tab.read(cx).as_editor())
                .expect("expected an editor tab");
            assert_eq!(
                editor.content.read(cx).text().to_string(),
                "new tab content"
            );
        });
    }
}
