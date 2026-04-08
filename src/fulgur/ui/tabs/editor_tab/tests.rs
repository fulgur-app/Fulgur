use super::{
    EditorTab, FromDuplicateParams, FromFileParams, TabTransferData, content_fingerprint_from_str,
};
use crate::fulgur::languages::supported_languages::SupportedLanguage;
use crate::fulgur::settings::EditorSettings;
use gpui::{AppContext, Context, IntoElement, Render, SharedString, TestAppContext, Window, div};
use gpui_component::input::Position;
use std::path::PathBuf;

struct EmptyView;

impl Render for EmptyView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Build an OS-agnostic temporary test path.
///
/// ### Parameters
/// - `file_name`: The file name to append to the platform temp directory.
///
/// ### Returns
/// - `PathBuf`: A path under `std::env::temp_dir()` suitable for cross-platform tests.
fn temp_test_path(file_name: &str) -> PathBuf {
    std::env::temp_dir().join(file_name)
}

fn make_transfer_data() -> TabTransferData {
    TabTransferData {
        title: SharedString::from("transfer.rs"),
        content: "fn main() {}".to_string(),
        file_path: Some(std::path::PathBuf::from("/tmp/transfer.rs")),
        modified: false,
        original_content_hash: content_fingerprint_from_str("fn main() {}").0,
        original_content_len: "fn main() {}".len(),
        encoding: "UTF-8".to_string(),
        language: SupportedLanguage::Rust,
        show_markdown_toolbar: true,
        show_markdown_preview: false,
        file_size_bytes: Some(12),
        file_last_modified: None,
        cursor_position: Position::default(),
    }
}

#[gpui::test]
fn test_editor_tab_new_construction(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let settings = EditorSettings::new();

    cx.update(|cx| {
        cx.open_window(Default::default(), |window, cx| {
            let tab = EditorTab::new(7, "Scratch", window, cx, &settings);
            assert_eq!(tab.id, 7);
            assert_eq!(tab.title, SharedString::from("Scratch"));
            assert!(tab.file_path.is_none());
            assert!(!tab.modified);
            assert_eq!(
                tab.original_content_hash,
                content_fingerprint_from_str("").0
            );
            assert_eq!(tab.original_content_len, 0);
            assert_eq!(tab.encoding, "UTF-8");
            assert_eq!(tab.language, SupportedLanguage::Plain);
            assert_eq!(
                tab.show_markdown_toolbar,
                settings.markdown_settings.show_markdown_toolbar
            );
            assert_eq!(
                tab.show_markdown_preview,
                settings.markdown_settings.show_markdown_preview
            );
            assert_eq!(tab.file_size_bytes, None);
            assert_eq!(tab.file_last_modified, None);
            assert_eq!(tab.content.read(cx).text().to_string(), "");
            cx.new(|_| EmptyView)
        })
        .expect("failed to open test window");
    });
}

#[gpui::test]
fn test_editor_tab_from_content_construction(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let mut settings = EditorSettings::new();
    settings.markdown_settings.show_markdown_toolbar = true;
    settings.markdown_settings.show_markdown_preview = false;

    let contents = "fn main() {\n    println!(\"hi\");\n}".to_string();
    cx.update(|cx| {
        cx.open_window(Default::default(), |window, cx| {
            let tab = EditorTab::from_content(
                9,
                contents.clone(),
                "shared.rs".to_string(),
                window,
                cx,
                &settings,
            );
            assert_eq!(tab.id, 9);
            assert_eq!(tab.title, SharedString::from("shared.rs"));
            assert!(tab.file_path.is_none());
            assert!(tab.modified);
            assert_eq!(
                tab.original_content_hash,
                content_fingerprint_from_str("").0
            );
            assert_eq!(tab.original_content_len, 0);
            assert_eq!(tab.encoding, "UTF-8");
            assert_eq!(tab.language, SupportedLanguage::Rust);
            assert!(tab.show_markdown_toolbar);
            assert!(!tab.show_markdown_preview);
            assert_eq!(tab.file_size_bytes, None);
            assert_eq!(tab.file_last_modified, None);
            assert_eq!(tab.content.read(cx).text().to_string(), contents);
            cx.new(|_| EmptyView)
        })
        .expect("failed to open test window");
    });
}

#[gpui::test]
fn test_editor_tab_from_file_construction(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let settings = EditorSettings::new();
    let path = temp_test_path("test_file.md");
    let contents = "# title\nbody".to_string();
    let params = FromFileParams {
        id: 13,
        path: path.clone(),
        contents: contents.clone(),
        encoding: "UTF-8".to_string(),
        is_modified: true,
    };

    cx.update(|cx| {
        cx.open_window(Default::default(), |window, cx| {
            let tab = EditorTab::from_file(params, window, cx, &settings);
            assert_eq!(tab.id, 13);
            assert_eq!(tab.title, SharedString::from("test_file.md •"));
            assert_eq!(tab.file_path, Some(path));
            assert!(tab.modified);
            assert_eq!(
                tab.original_content_hash,
                content_fingerprint_from_str(&contents).0
            );
            assert_eq!(tab.original_content_len, contents.len());
            assert_eq!(tab.encoding, "UTF-8");
            assert_eq!(tab.language, SupportedLanguage::Markdown);
            assert_eq!(tab.file_size_bytes, Some(12));
            assert!(tab.file_last_modified.is_some());
            assert_eq!(tab.content.read(cx).text().to_string(), "# title\nbody");
            cx.new(|_| EmptyView)
        })
        .expect("failed to open test window");
    });
}

#[gpui::test]
fn test_editor_tab_from_duplicate_construction(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let settings = EditorSettings::new();
    let params = FromDuplicateParams {
        id: 22,
        title: SharedString::from("copy.rs"),
        current_content: "let value = 42;".to_string(),
        encoding: "UTF-8".to_string(),
        language: SupportedLanguage::Rust,
    };

    cx.update(|cx| {
        cx.open_window(Default::default(), |window, cx| {
            let tab = EditorTab::from_duplicate(params, window, cx, &settings);
            assert_eq!(tab.id, 22);
            assert_eq!(tab.title, SharedString::from("copy.rs"));
            assert!(tab.file_path.is_none());
            assert!(tab.modified);
            assert_eq!(
                tab.original_content_hash,
                content_fingerprint_from_str("").0
            );
            assert_eq!(tab.original_content_len, 0);
            assert_eq!(tab.encoding, "UTF-8");
            assert_eq!(tab.language, SupportedLanguage::Rust);
            assert_eq!(tab.file_size_bytes, None);
            assert_eq!(tab.file_last_modified, None);
            assert_eq!(tab.content.read(cx).text().to_string(), "let value = 42;");
            cx.new(|_| EmptyView)
        })
        .expect("failed to open test window");
    });
}

#[gpui::test]
fn test_editor_tab_check_modified_and_mark_as_saved(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let settings = EditorSettings::new();
    let params = FromFileParams {
        id: 31,
        path: temp_test_path("modified_state.md"),
        contents: "original".to_string(),
        encoding: "UTF-8".to_string(),
        is_modified: false,
    };

    cx.update(|cx| {
        cx.open_window(Default::default(), |window, cx| {
            let mut tab = EditorTab::from_file(params, window, cx, &settings);
            assert!(!tab.modified);
            assert_eq!(
                tab.original_content_hash,
                content_fingerprint_from_str("original").0
            );
            assert_eq!(tab.original_content_len, "original".len());

            tab.content.update(cx, |content, cx| {
                content.set_value("changed", window, cx);
            });

            assert!(tab.check_modified(cx));
            assert!(tab.modified);

            tab.mark_as_saved(cx);
            assert!(!tab.modified);
            assert_eq!(
                tab.original_content_hash,
                content_fingerprint_from_str("changed").0
            );
            assert_eq!(tab.original_content_len, "changed".len());
            assert!(!tab.check_modified(cx));

            cx.new(|_| EmptyView)
        })
        .expect("failed to open test window");
    });
}

#[gpui::test]
fn test_editor_tab_check_modified_detects_same_length_content_change(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let settings = EditorSettings::new();
    let params = FromFileParams {
        id: 32,
        path: temp_test_path("modified_same_len.md"),
        contents: "abcd".to_string(),
        encoding: "UTF-8".to_string(),
        is_modified: false,
    };
    cx.update(|cx| {
        cx.open_window(Default::default(), |window, cx| {
            let mut tab = EditorTab::from_file(params, window, cx, &settings);
            tab.content.update(cx, |content, cx| {
                content.set_value("abce", window, cx);
            });
            assert!(tab.check_modified(cx));
            assert!(tab.modified);
            cx.new(|_| EmptyView)
        })
        .expect("failed to open test window");
    });
}

#[gpui::test]
fn test_editor_tab_check_modified_handles_multibyte_utf8_content(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let settings = EditorSettings::new();
    let params = FromFileParams {
        id: 33,
        path: temp_test_path("modified_utf8.md"),
        contents: "\u{00E9}\u{1F642}\u{6F22}".to_string(),
        encoding: "UTF-8".to_string(),
        is_modified: false,
    };
    cx.update(|cx| {
        cx.open_window(Default::default(), |window, cx| {
            let mut tab = EditorTab::from_file(params, window, cx, &settings);
            assert!(
                !tab.check_modified(cx),
                "unchanged UTF-8 content must remain unmodified"
            );
            tab.content.update(cx, |content, cx| {
                content.set_value("\u{00E9}\u{1F643}\u{6F22}", window, cx);
            });
            assert!(
                tab.check_modified(cx),
                "changing a multibyte character must set modified=true"
            );
            cx.new(|_| EmptyView)
        })
        .expect("failed to open test window");
    });
}

#[gpui::test]
fn test_editor_tab_from_file_title_indicator_clean_and_dirty(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let settings = EditorSettings::new();

    let clean = FromFileParams {
        id: 41,
        path: temp_test_path("clean.md"),
        contents: "clean".to_string(),
        encoding: "UTF-8".to_string(),
        is_modified: false,
    };
    let dirty = FromFileParams {
        id: 42,
        path: temp_test_path("dirty.md"),
        contents: "dirty".to_string(),
        encoding: "UTF-8".to_string(),
        is_modified: true,
    };

    cx.update(|cx| {
        cx.open_window(Default::default(), |window, cx| {
            let clean_tab = EditorTab::from_file(clean, window, cx, &settings);
            let dirty_tab = EditorTab::from_file(dirty, window, cx, &settings);

            assert_eq!(clean_tab.title, SharedString::from("clean.md"));
            assert_eq!(dirty_tab.title, SharedString::from("dirty.md •"));

            cx.new(|_| EmptyView)
        })
        .expect("failed to open test window");
    });
}

#[gpui::test]
fn test_editor_tab_get_suggested_filename_trims_modified_indicator(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let settings = EditorSettings::new();
    let params = FromFileParams {
        id: 51,
        path: temp_test_path("suggested_name.md"),
        contents: "content".to_string(),
        encoding: "UTF-8".to_string(),
        is_modified: true,
    };

    cx.update(|cx| {
        cx.open_window(Default::default(), |window, cx| {
            let tab = EditorTab::from_file(params, window, cx, &settings);
            assert_eq!(tab.title, SharedString::from("suggested_name.md •"));
            assert_eq!(
                tab.get_suggested_filename(),
                Some("suggested_name.md".to_string())
            );

            cx.new(|_| EmptyView)
        })
        .expect("failed to open test window");
    });
}

// ========== from_transfer() tests ==========

#[gpui::test]
fn test_from_transfer_preserves_all_fields(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let settings = EditorSettings::new();
    cx.update(|cx| {
        cx.open_window(Default::default(), |window, cx| {
            let tab = EditorTab::from_transfer(99, make_transfer_data(), window, cx, &settings);
            assert_eq!(tab.id, 99);
            assert_eq!(tab.title, SharedString::from("transfer.rs"));
            assert_eq!(
                tab.file_path,
                Some(std::path::PathBuf::from("/tmp/transfer.rs"))
            );
            assert!(!tab.modified);
            assert_eq!(
                tab.original_content_hash,
                content_fingerprint_from_str("fn main() {}").0
            );
            assert_eq!(tab.original_content_len, "fn main() {}".len());
            assert_eq!(tab.encoding, "UTF-8");
            assert_eq!(tab.language, SupportedLanguage::Rust);
            assert!(tab.show_markdown_toolbar);
            assert!(!tab.show_markdown_preview);
            assert_eq!(tab.file_size_bytes, Some(12));
            assert!(tab.file_last_modified.is_none());
            assert_eq!(tab.content.read(cx).text().to_string(), "fn main() {}");
            cx.new(|_| EmptyView)
        })
        .expect("failed to open test window");
    });
}

#[gpui::test]
fn test_from_transfer_assigns_new_id(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let settings = EditorSettings::new();
    cx.update(|cx| {
        cx.open_window(Default::default(), |window, cx| {
            let tab = EditorTab::from_transfer(77, make_transfer_data(), window, cx, &settings);
            assert_eq!(
                tab.id, 77,
                "id must be the parameter, not from transfer data"
            );
            cx.new(|_| EmptyView)
        })
        .expect("failed to open test window");
    });
}

#[gpui::test]
fn test_from_transfer_untitled_no_file_metadata(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let settings = EditorSettings::new();
    let data = TabTransferData {
        title: SharedString::from("Untitled"),
        content: "scratch content".to_string(),
        file_path: None,
        modified: false,
        original_content_hash: content_fingerprint_from_str("").0,
        original_content_len: 0,
        encoding: "UTF-8".to_string(),
        language: SupportedLanguage::Plain,
        show_markdown_toolbar: false,
        show_markdown_preview: false,
        file_size_bytes: None,
        file_last_modified: None,
        cursor_position: Position::default(),
    };
    cx.update(|cx| {
        cx.open_window(Default::default(), |window, cx| {
            let tab = EditorTab::from_transfer(1, data, window, cx, &settings);
            assert!(tab.file_path.is_none());
            assert!(tab.file_size_bytes.is_none());
            assert!(tab.file_last_modified.is_none());
            assert_eq!(tab.content.read(cx).text().to_string(), "scratch content");
            cx.new(|_| EmptyView)
        })
        .expect("failed to open test window");
    });
}

#[gpui::test]
fn test_from_transfer_modified_state_preserved(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let settings = EditorSettings::new();
    let data = TabTransferData {
        title: SharedString::from("changed.md"),
        content: "edited content".to_string(),
        file_path: Some(std::path::PathBuf::from("/tmp/changed.md")),
        modified: true,
        original_content_hash: content_fingerprint_from_str("original content").0,
        original_content_len: "original content".len(),
        encoding: "UTF-8".to_string(),
        language: SupportedLanguage::Markdown,
        show_markdown_toolbar: false,
        show_markdown_preview: false,
        file_size_bytes: None,
        file_last_modified: None,
        cursor_position: Position::default(),
    };
    cx.update(|cx| {
        cx.open_window(Default::default(), |window, cx| {
            let tab = EditorTab::from_transfer(2, data, window, cx, &settings);
            assert!(tab.modified, "modified flag must be preserved");
            assert_eq!(
                tab.original_content_hash,
                content_fingerprint_from_str("original content").0,
                "original content must differ from current"
            );
            assert_eq!(tab.content.read(cx).text().to_string(), "edited content");
            cx.new(|_| EmptyView)
        })
        .expect("failed to open test window");
    });
}

#[gpui::test]
fn test_from_transfer_preserves_language(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let settings = EditorSettings::new();
    let data = TabTransferData {
        title: SharedString::from("script.py"),
        content: "print('hello')".to_string(),
        file_path: None,
        modified: false,
        original_content_hash: content_fingerprint_from_str("print('hello')").0,
        original_content_len: "print('hello')".len(),
        encoding: "UTF-8".to_string(),
        language: SupportedLanguage::Python,
        show_markdown_toolbar: false,
        show_markdown_preview: false,
        file_size_bytes: None,
        file_last_modified: None,
        cursor_position: Position::default(),
    };
    cx.update(|cx| {
        cx.open_window(Default::default(), |window, cx| {
            let tab = EditorTab::from_transfer(3, data, window, cx, &settings);
            assert_eq!(tab.language, SupportedLanguage::Python);
            cx.new(|_| EmptyView)
        })
        .expect("failed to open test window");
    });
}

#[gpui::test]
fn test_from_transfer_preserves_markdown_flags(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let settings = EditorSettings::new();
    let data = TabTransferData {
        title: SharedString::from("note.md"),
        content: "# Note".to_string(),
        file_path: None,
        modified: false,
        original_content_hash: content_fingerprint_from_str("# Note").0,
        original_content_len: "# Note".len(),
        encoding: "UTF-8".to_string(),
        language: SupportedLanguage::Markdown,
        show_markdown_toolbar: true,
        show_markdown_preview: true,
        file_size_bytes: None,
        file_last_modified: None,
        cursor_position: Position::default(),
    };
    cx.update(|cx| {
        cx.open_window(Default::default(), |window, cx| {
            let tab = EditorTab::from_transfer(4, data, window, cx, &settings);
            assert!(tab.show_markdown_toolbar);
            assert!(tab.show_markdown_preview);
            cx.new(|_| EmptyView)
        })
        .expect("failed to open test window");
    });
}
