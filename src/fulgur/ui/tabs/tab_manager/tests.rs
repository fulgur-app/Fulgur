use crate::fulgur::{
    Fulgur,
    languages::supported_languages::SupportedLanguage,
    settings::Settings,
    shared_state::SharedAppState,
    tab::Tab,
    ui::tabs::editor_tab::{TabLocation, TabTransferData},
    window_manager::WindowManager,
};
use gpui::{
    AppContext, Context, Entity, IntoElement, Render, SharedString, TestAppContext,
    VisualTestContext, Window, WindowOptions, div,
};
use gpui_component::input::{InputEvent, Position};
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
        cx.set_global(SharedAppState::new(settings, pending_files, None));
        cx.set_global(WindowManager::new());
    });

    let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
    let window = cx
        .update(|cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
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
            assert_eq!(this.active_tab_index(cx), Some(this.tabs.len() - 1));
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
            assert_eq!(this.next_tab_id, id_before.next());
            this.new_tab(window, cx);
            assert_eq!(this.next_tab_id, id_before.next().next());
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
            let editor = last.read(cx).as_editor().expect("expected an editor tab");
            assert!(editor.file_path().is_none());
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
            assert!(matches!(
                this.tabs.last().map(|t| t.read(cx)),
                Some(Tab::Settings(_))
            ));
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
            let tab_id = this.tabs.last().expect("expected tab").read(cx).id();
            this.close_tab(tab_id, window, cx);
            assert_eq!(this.tabs.len(), count_before - 1);
            assert!(!this.tabs.iter().any(|t| t.read(cx).id() == tab_id));
        });
    });
}

#[gpui::test]
fn test_close_tab_is_noop_for_unknown_id(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let count_before = this.tabs.len();
            this.close_tab(crate::fulgur::tab::TabId(u64::MAX), window, cx);
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
            assert_eq!(this.active_tab_index(cx), Some(1));

            // Close the tab at index 0 (before the active one).
            let first_id = this.tabs[0].read(cx).id();
            this.close_tab(first_id, window, cx);

            // Active index must have shifted left by one.
            assert_eq!(this.active_tab_index(cx), Some(0));
        });
    });
}

#[gpui::test]
fn test_close_last_tab_leaves_no_active_index(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            assert_eq!(this.tabs.len(), 1);
            let tab_id = this.tabs[0].read(cx).id();
            this.close_tab(tab_id, window, cx);
            assert!(this.tabs.is_empty());
            assert_eq!(this.active_tab_index(cx), None);
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
            assert_eq!(this.active_tab_index(cx), Some(0));
            this.set_active_tab(1, window, cx);
            assert_eq!(this.active_tab_index(cx), Some(1));
        });
    });
}

#[gpui::test]
fn test_set_active_tab_is_noop_out_of_bounds(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let active_before = this.active_tab_index(cx);
            this.set_active_tab(usize::MAX, window, cx);
            assert_eq!(this.active_tab_index(cx), active_before);
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
            let active_id = this.tabs[1].read(cx).id();

            this.close_other_tabs(window, cx);

            assert_eq!(this.tabs.len(), 1);
            assert_eq!(this.tabs[0].read(cx).id(), active_id);
            assert_eq!(this.active_tab_index(cx), Some(0));
        });
    });
}

#[gpui::test]
fn test_close_other_tabs_is_noop_with_single_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            assert_eq!(this.tabs.len(), 1);
            let tab_id_before = this.tabs[0].read(cx).id();
            this.close_other_tabs(window, cx);
            assert_eq!(this.tabs.len(), 1);
            assert_eq!(this.tabs[0].read(cx).id(), tab_id_before);
        });
    });
}

// ========== duplicate_tab tests ==========

#[gpui::test]
fn test_duplicate_tab_inserts_copy_after_original_and_becomes_active(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let original_id = this.tabs[0].read(cx).id();
            this.duplicate_tab(0, window, cx);

            assert_eq!(this.tabs.len(), 2);
            assert_eq!(this.tabs[0].read(cx).id(), original_id);
            assert_ne!(this.tabs[1].read(cx).id(), original_id);
            assert_eq!(this.active_tab_index(cx), Some(1));
        });
    });
}

#[gpui::test]
fn test_duplicate_tab_preserves_content_and_language(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.update_active_editor_tab(cx, |editor, _| {
                editor.language = SupportedLanguage::Rust;
            });
            this.duplicate_tab(0, window, cx);

            let duplicate = this.tabs[1]
                .read(cx)
                .as_editor()
                .expect("expected editor tab");
            assert_eq!(duplicate.language, SupportedLanguage::Rust);
            assert!(duplicate.file_path().is_none());
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
                .position(|t| matches!(t.read(cx), Tab::Settings(_)))
                .expect("expected settings tab");
            let count_before = this.tabs.len();
            this.duplicate_tab(settings_index, window, cx);
            assert_eq!(this.tabs.len(), count_before);
        });
    });
}

// ========== open_markdown_preview_tab tests ==========

#[gpui::test]
fn test_open_markdown_preview_tab_creates_preview_tab_for_markdown_editor(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.update_active_editor_tab(cx, |editor, _| {
                editor.language = SupportedLanguage::Markdown;
            });
            let count_before = this.tabs.len();
            this.open_markdown_preview_tab(window, cx);
            assert_eq!(this.tabs.len(), count_before + 1);
            assert!(
                this.tabs
                    .iter()
                    .any(|t| t.read(cx).as_markdown_preview().is_some())
            );
        });
    });
}

#[gpui::test]
fn test_open_markdown_preview_tab_preview_is_inserted_after_editor(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.update_active_editor_tab(cx, |editor, _| {
                editor.language = SupportedLanguage::Markdown;
            });
            let editor_index = this.active_tab_index(cx).expect("expected active tab");
            this.open_markdown_preview_tab(window, cx);
            assert!(matches!(
                this.tabs.get(editor_index + 1).map(|t| t.read(cx)),
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
            this.update_active_editor_tab(cx, |editor, _| {
                editor.language = SupportedLanguage::Markdown;
            });
            let count_before = this.tabs.len();
            this.open_markdown_preview_tab(window, cx);
            assert_eq!(this.tabs.len(), count_before + 1);
            this.open_markdown_preview_tab(window, cx);
            assert_eq!(this.tabs.len(), count_before);
            assert!(
                !this
                    .tabs
                    .iter()
                    .any(|t| t.read(cx).as_markdown_preview().is_some())
            );
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
            this.update_active_editor_tab(cx, |editor, _| {
                editor.language = SupportedLanguage::Markdown;
            });
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
            this.active_tab_id = None;
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
        fulgur.update(cx, |this, cx| {
            this.tabs
                .first()
                .expect("expected at least one tab")
                .clone()
                .update(cx, |tab, _cx| {
                    if let Some(editor) = tab.as_editor_mut() {
                        editor.language = SupportedLanguage::Markdown;
                    }
                });
            let count_before = this.tabs.len();
            this.maybe_open_markdown_preview_for_editor(0, cx);
            assert_eq!(this.tabs.len(), count_before + 1);
            assert!(matches!(
                this.tabs.get(1).map(|t| t.read(cx)),
                Some(Tab::MarkdownPreview(_))
            ));
        });
    });
}

#[gpui::test]
fn test_maybe_open_markdown_preview_for_editor_skips_non_markdown(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            // Default language is Plain, no preview tab should be inserted
            let count_before = this.tabs.len();
            this.maybe_open_markdown_preview_for_editor(0, cx);
            assert_eq!(this.tabs.len(), count_before);
        });
    });
}

#[gpui::test]
fn test_maybe_open_markdown_preview_for_editor_is_noop_when_disabled(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            this.settings
                .editor_settings
                .markdown_settings
                .show_markdown_preview = false;
            this.tabs
                .first()
                .expect("expected at least one tab")
                .clone()
                .update(cx, |tab, _cx| {
                    if let Some(editor) = tab.as_editor_mut() {
                        editor.language = SupportedLanguage::Markdown;
                    }
                });
            let count_before = this.tabs.len();
            this.maybe_open_markdown_preview_for_editor(0, cx);
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
            this.tabs
                .first()
                .expect("expected at least one tab")
                .clone()
                .update(cx, |tab, _cx| {
                    if let Some(editor) = tab.as_editor_mut() {
                        editor.language = SupportedLanguage::Markdown;
                    }
                });
            this.new_tab(window, cx);
            this.tabs
                .last()
                .expect("expected at least one tab")
                .clone()
                .update(cx, |tab, _cx| {
                    if let Some(editor) = tab.as_editor_mut() {
                        editor.language = SupportedLanguage::Markdown;
                    }
                });
            assert_eq!(this.tabs.len(), 2);
            this.insert_preview_tabs_for_markdown(cx);
            assert_eq!(this.tabs.len(), 4);
            assert_eq!(
                this.tabs
                    .iter()
                    .filter(|t| t.read(cx).as_markdown_preview().is_some())
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
        fulgur.update(cx, |this, cx| {
            this.settings
                .editor_settings
                .markdown_settings
                .show_markdown_preview = false;
            this.tabs
                .first()
                .expect("expected at least one tab")
                .clone()
                .update(cx, |tab, _cx| {
                    if let Some(editor) = tab.as_editor_mut() {
                        editor.language = SupportedLanguage::Markdown;
                    }
                });
            let count_before = this.tabs.len();
            this.insert_preview_tabs_for_markdown(cx);
            assert_eq!(this.tabs.len(), count_before);
        });
    });
}

// ========== panel mode show_markdown_preview flag tests ==========

#[gpui::test]
fn test_panel_preview_flag_is_true_by_default(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            assert!(
                this.get_active_editor_tab(cx)
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
                .get_active_editor_tab(cx)
                .is_some_and(|e| e.show_markdown_preview);
            this.update_active_editor_tab(cx, |editor, _| {
                editor.show_markdown_preview = !editor.show_markdown_preview;
            });
            cx.notify();
            let after = this
                .get_active_editor_tab(cx)
                .is_some_and(|e| e.show_markdown_preview);
            assert_ne!(initial, after, "show_markdown_preview should toggle");
        });
    });
}

// ========== tab-entity modified tracking tests ==========

#[gpui::test]
fn test_tab_entity_updates_modified_on_input_change(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let editor_content = visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            let editor = this.tabs[0]
                .read(cx)
                .as_editor()
                .expect("expected editor tab");
            assert!(!editor.modified, "fresh tab should start as unmodified");
            editor.content.clone()
        })
    });

    visual_cx.update(|window, cx| {
        editor_content.update(cx, |input_state, cx| {
            input_state.set_value("changed in active tab", window, cx);
            cx.emit(InputEvent::Change);
        });
    });
    visual_cx.run_until_parked();

    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            let editor = this.tabs[0]
                .read(cx)
                .as_editor()
                .expect("expected editor tab");
            assert!(
                editor.modified,
                "InputEvent::Change should update modified state through the tab's own subscription"
            );
        });
    });
}

#[gpui::test]
fn test_tab_entity_resubscribes_after_content_rebuild(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    // Rebuild the content entity via the Tab-level `force_language` wrapper,
    // which must re-attach the tab's content subscription.
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let settings = this.settings.editor_settings.clone();
            this.tabs[0].clone().update(cx, |tab, cx| {
                tab.force_language(window, cx, SupportedLanguage::Rust, &settings);
            });
        });
    });

    let new_content = visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            this.tabs[0]
                .read(cx)
                .as_editor()
                .expect("expected editor tab")
                .content
                .clone()
        })
    });

    visual_cx.update(|window, cx| {
        new_content.update(cx, |input_state, cx| {
            input_state.set_value("edited after rebuild", window, cx);
            cx.emit(InputEvent::Change);
        });
    });
    visual_cx.run_until_parked();

    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            let editor = this.tabs[0]
                .read(cx)
                .as_editor()
                .expect("expected editor tab");
            assert!(
                editor.modified,
                "InputEvent::Change on the rebuilt content entity must update modified state"
            );
        });
    });
}

#[gpui::test]
fn test_large_file_tab_keeps_modified_after_restoring_original_content(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let editor_content = visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            this.update_active_editor_tab(cx, |editor, _| {
                editor.large_file = true;
            });
            this.tabs[0]
                .read(cx)
                .as_editor()
                .expect("expected editor tab")
                .content
                .clone()
        })
    });

    visual_cx.update(|window, cx| {
        editor_content.update(cx, |input_state, cx| {
            input_state.set_value("changed in large file", window, cx);
            cx.emit(InputEvent::Change);
        });
    });
    visual_cx.run_until_parked();

    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            let editor = this.tabs[0]
                .read(cx)
                .as_editor()
                .expect("expected editor tab");
            assert!(
                editor.modified,
                "large-file tab should be marked modified on change"
            );
        });
    });

    // Restore the original (empty) content: the large-file flag must stay set
    // because the per-change fingerprint is intentionally skipped.
    visual_cx.update(|window, cx| {
        editor_content.update(cx, |input_state, cx| {
            input_state.set_value("", window, cx);
            cx.emit(InputEvent::Change);
        });
    });
    visual_cx.run_until_parked();

    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            let editor = this.tabs[0]
                .read(cx)
                .as_editor()
                .expect("expected editor tab");
            assert!(
                editor.modified,
                "large-file modified flag must stay set until save or reload"
            );
        });
    });
}

#[gpui::test]
fn test_modified_detected_for_same_length_edit(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let editor_content = visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            this.update_active_editor_tab(cx, |editor, _| {
                editor.set_original_content_from_str("abc");
            });
            this.tabs[0]
                .read(cx)
                .as_editor()
                .expect("expected editor tab")
                .content
                .clone()
        })
    });

    // Same byte length as the baseline: the length fast path cannot answer,
    // so the content hash must still detect the difference.
    visual_cx.update(|window, cx| {
        editor_content.update(cx, |input_state, cx| {
            input_state.set_value("abd", window, cx);
            cx.emit(InputEvent::Change);
        });
    });
    visual_cx.run_until_parked();

    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            let editor = this.tabs[0]
                .read(cx)
                .as_editor()
                .expect("expected editor tab");
            assert!(
                editor.modified,
                "a same-length edit must still be detected as modified"
            );
        });
    });

    visual_cx.update(|window, cx| {
        editor_content.update(cx, |input_state, cx| {
            input_state.set_value("abc", window, cx);
            cx.emit(InputEvent::Change);
        });
    });
    visual_cx.run_until_parked();

    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            let editor = this.tabs[0]
                .read(cx)
                .as_editor()
                .expect("expected editor tab");
            assert!(
                !editor.modified,
                "restoring the baseline content must clear the modified flag"
            );
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
            let id_2 = this.tabs[2].read(cx).id();
            this.reorder_tab(2, 0, window, cx);
            assert_eq!(
                this.tabs[0].read(cx).id(),
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
            let id_0 = this.tabs[0].read(cx).id();
            this.reorder_tab(0, 3, window, cx);
            assert_eq!(
                this.tabs[2].read(cx).id(),
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
            let ids_before: Vec<crate::fulgur::tab::TabId> =
                this.tabs.iter().map(|t| t.read(cx).id()).collect();
            this.reorder_tab(1, 1, window, cx);
            let ids_after: Vec<crate::fulgur::tab::TabId> =
                this.tabs.iter().map(|t| t.read(cx).id()).collect();
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
            let ids_before: Vec<crate::fulgur::tab::TabId> =
                this.tabs.iter().map(|t| t.read(cx).id()).collect();
            // to == from+1 means inserting immediately after the tab, which is its current position
            this.reorder_tab(1, 2, window, cx);
            let ids_after: Vec<crate::fulgur::tab::TabId> =
                this.tabs.iter().map(|t| t.read(cx).id()).collect();
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
                this.active_tab_index(cx),
                Some(2),
                "active index should follow the moved tab"
            );
        });
    });
}

#[gpui::test]
fn test_reorder_tab_active_index_decrements_when_earlier_tab_moves_past(cx: &mut TestAppContext) {
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
                this.active_tab_index(cx),
                Some(0),
                "active index should decrement when a preceding tab moves past it"
            );
        });
    });
}

#[gpui::test]
fn test_reorder_tab_active_index_increments_when_later_tab_moves_before(cx: &mut TestAppContext) {
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
                this.active_tab_index(cx),
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
            let id_2 = this.tabs[2].read(cx).id();
            let dragged = DraggedTab {
                tab_id: id_2,
                title: "test.rs".into(),
                is_modified: false,
            };
            this.handle_tab_drop(&dragged, 0, window, cx);
            assert_eq!(
                this.tabs[0].read(cx).id(),
                id_2,
                "dropped tab should land at slot 0"
            );
        });
    });
}

// ========== send-to: helpers ==========

fn make_transfer_data() -> TabTransferData {
    TabTransferData {
        title: SharedString::from("sent.rs"),
        content: "let x = 42;".to_string(),
        location: crate::fulgur::ui::tabs::editor_tab::TabLocation::Untitled,
        modified: false,
        original_content_hash: crate::fulgur::ui::tabs::editor_tab::content_fingerprint_from_str(
            "let x = 42;",
        )
        .0,
        original_content_len: "let x = 42;".len(),
        encoding: "UTF-8".to_string(),
        lossy_decode: false,
        language: SupportedLanguage::Rust,
        show_markdown_toolbar: false,
        show_markdown_preview: false,
        file_size_bytes: None,
        file_last_modified: None,
        cursor_position: Position::default(),
        csv_view_mode: crate::fulgur::ui::tabs::editor_tab::CsvViewMode::Text,
        csv_delimiter: b',',
        log_view: false,
    }
}

// ========== extract_tab_transfer_data() tests ==========

#[gpui::test]
fn test_extract_transfer_data_returns_none_for_missing_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        let result = fulgur.update(cx, |this, cx| {
            this.extract_tab_transfer_data(crate::fulgur::tab::TabId(9999), cx)
        });
        assert!(result.is_none(), "unknown tab id must return None");
    });
}

#[gpui::test]
fn test_extract_transfer_data_captures_content_and_metadata(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        // The initial tab created by setup_fulgur has id=0 and empty content
        let data = fulgur
            .update(cx, |this, cx| {
                this.extract_tab_transfer_data(crate::fulgur::tab::TabId(0), cx)
            })
            .expect("should extract data from the initial tab");
        assert_eq!(data.content, "");
        assert!(data.location.is_untitled());
        assert!(!data.modified);
        assert_eq!(data.encoding, "UTF-8");
        assert_eq!(data.language, SupportedLanguage::Plain);
    });
}

// ========== handle_pending_tab_transfer() tests ==========

#[gpui::test]
fn test_handle_pending_tab_transfer_no_op_when_none(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let count_before = this.tabs.len();
            this.handle_pending_tab_transfer(window, cx);
            assert_eq!(
                this.tabs.len(),
                count_before,
                "no tab should be added when pending is None"
            );
        });
    });
}

#[gpui::test]
fn test_handle_pending_tab_transfer_adds_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let count_before = this.tabs.len();
            this.pending_tab_transfer = Some(make_transfer_data());
            this.handle_pending_tab_transfer(window, cx);
            assert_eq!(this.tabs.len(), count_before + 1);
        });
    });
}

#[gpui::test]
fn test_handle_pending_tab_transfer_sets_as_active(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.pending_tab_transfer = Some(make_transfer_data());
            this.handle_pending_tab_transfer(window, cx);
            assert_eq!(
                this.active_tab_index(cx),
                Some(this.tabs.len() - 1),
                "transferred tab must become the active tab"
            );
        });
    });
}

#[gpui::test]
fn test_handle_pending_tab_transfer_consumes_pending_field(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.pending_tab_transfer = Some(make_transfer_data());
            this.handle_pending_tab_transfer(window, cx);
            assert!(
                this.pending_tab_transfer.is_none(),
                "pending field must be consumed after handling"
            );
        });
    });
}

#[gpui::test]
fn test_handle_pending_tab_transfer_sets_deferred_scroll(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.pending_tab_transfer = Some(make_transfer_data());
            this.handle_pending_tab_transfer(window, cx);
            assert!(
                this.pending_transfer_scroll.is_some(),
                "cursor scroll must be deferred to the next frame"
            );
        });
    });
}

#[gpui::test]
fn test_handle_pending_tab_transfer_preserves_content(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.pending_tab_transfer = Some(make_transfer_data());
            this.handle_pending_tab_transfer(window, cx);
            let last = this.tabs.len() - 1;
            let editor = this.tabs[last]
                .read(cx)
                .as_editor()
                .expect("transferred tab must be an editor tab");
            assert_eq!(editor.content.read(cx).text().to_string(), "let x = 42;");
            assert_eq!(editor.language, SupportedLanguage::Rust);
        });
    });
}

#[gpui::test]
fn test_handle_pending_tab_transfer_increments_tab_id(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let id_before = this.next_tab_id;
            this.pending_tab_transfer = Some(make_transfer_data());
            this.handle_pending_tab_transfer(window, cx);
            assert_eq!(
                this.next_tab_id,
                id_before.next(),
                "next_tab_id must increment after transfer"
            );
        });
    });
}

// ========== handle_pending_tab_removal() tests ==========

#[gpui::test]
fn test_handle_pending_tab_removal_no_op_when_none(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let count_before = this.tabs.len();
            this.handle_pending_tab_removal(window, cx);
            assert_eq!(
                this.tabs.len(),
                count_before,
                "no tab should be removed when pending is None"
            );
        });
    });
}

#[gpui::test]
fn test_handle_pending_tab_removal_removes_correct_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            // Add a second tab so closing the first leaves one remaining
            this.new_tab(window, cx);
            let first_id = this.tabs[0].read(cx).id();
            this.pending_tab_removal = Some(first_id);
            this.handle_pending_tab_removal(window, cx);
            assert!(
                this.tabs.iter().all(|t| t.read(cx).id() != first_id),
                "removed tab must not appear in the tab list"
            );
        });
    });
}

#[gpui::test]
fn test_handle_pending_tab_removal_consumes_pending_field(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.new_tab(window, cx);
            let first_id = this.tabs[0].read(cx).id();
            this.pending_tab_removal = Some(first_id);
            this.handle_pending_tab_removal(window, cx);
            assert!(
                this.pending_tab_removal.is_none(),
                "pending field must be consumed after handling"
            );
        });
    });
}

#[gpui::test]
fn test_handle_pending_tab_removal_closes_window_when_last_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            // Exactly one tab exists by default; mark it for removal
            assert_eq!(this.tabs.len(), 1, "setup should start with one tab");
            let only_tab_id = this.tabs[0].read(cx).id();
            this.pending_tab_removal = Some(only_tab_id);
            // Should not panic and should empty the tab list before closing the window
            this.handle_pending_tab_removal(window, cx);
            assert!(
                this.tabs.is_empty(),
                "all tabs must be gone when the only tab is sent away"
            );
            assert!(
                this.pending_tab_removal.is_none(),
                "pending field must be consumed"
            );
        });
    });
}

// ========== handle_pending_transfer_scroll() tests ==========

#[gpui::test]
fn test_handle_pending_transfer_scroll_no_op_when_none(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            // Must not panic and pending must remain None
            this.handle_pending_transfer_scroll(window, cx);
            assert!(this.pending_transfer_scroll.is_none());
        });
    });
}

#[gpui::test]
fn test_handle_pending_transfer_scroll_consumes_position(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.pending_transfer_scroll = Some(Position::default());
            this.handle_pending_transfer_scroll(window, cx);
            assert!(
                this.pending_transfer_scroll.is_none(),
                "position must be consumed after scrolling"
            );
        });
    });
}

#[gpui::test]
fn test_large_modified_local_tabs_detects_modified_large_local_file(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let (tab_id, content) = visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            this.update_active_editor_tab(cx, |editor, _| {
                editor.large_file = true;
                editor.location = TabLocation::Local(PathBuf::from("/tmp/huge.log"));
            });
            let editor = this.tabs[0]
                .read(cx)
                .as_editor()
                .expect("expected editor tab");
            (editor.id, editor.content.clone())
        })
    });

    visual_cx.update(|window, cx| {
        content.update(cx, |input_state, cx| {
            input_state.set_value("edited large content", window, cx);
            cx.emit(InputEvent::Change);
        });
    });
    visual_cx.run_until_parked();

    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            assert_eq!(
                this.large_modified_local_tabs(cx),
                vec![tab_id],
                "a modified large local tab must be flagged for a close warning"
            );
        });
    });
}

#[gpui::test]
fn test_large_modified_local_tabs_excludes_untitled_and_unmodified(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let content = visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            this.update_active_editor_tab(cx, |editor, _| {
                editor.large_file = true;
            });
            this.tabs[0]
                .read(cx)
                .as_editor()
                .expect("expected editor tab")
                .content
                .clone()
        })
    });

    // An untitled large modified tab has no on-disk destination, so it is not
    // offered a save-before-close warning.
    visual_cx.update(|window, cx| {
        content.update(cx, |input_state, cx| {
            input_state.set_value("edited untitled large content", window, cx);
            cx.emit(InputEvent::Change);
        });
    });
    visual_cx.run_until_parked();

    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            assert!(
                this.large_modified_local_tabs(cx).is_empty(),
                "untitled large tabs must not be flagged for a close warning"
            );
        });
    });

    // Give it a local path but mark it saved: an unmodified tab is not flagged.
    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            this.update_active_editor_tab(cx, |editor, cx| {
                editor.location = TabLocation::Local(PathBuf::from("/tmp/huge.log"));
                editor.mark_as_saved(cx);
            });
            assert!(
                this.large_modified_local_tabs(cx).is_empty(),
                "an unmodified large local tab must not be flagged for a close warning"
            );
        });
    });
}
