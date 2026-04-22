use crate::fulgur::Fulgur;
use crate::fulgur::sync::ssh::url::RemoteSpec;
use crate::fulgur::ui::tabs::editor_tab::TabLocation;
use crate::fulgur::{
    settings::Settings, shared_state::SharedAppState, tab::Tab, window_manager::WindowManager,
};
use gpui::{
    AppContext, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext, Window,
    div,
};
use parking_lot::Mutex;
use std::{cell::RefCell, path::PathBuf, sync::Arc};

struct EmptyView;

impl Render for EmptyView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Setup a test app with a fulgur instance and a visual test context.
///
/// ### Arguments
/// - `cx` - The test app context to setup.
///
/// ### Returns
/// - `Entity<Fulgur>` - The fulgur instance.
/// - `VisualTestContext` - The visual test context.
fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        let mut settings = Settings::new();
        settings.editor_settings.watch_files = false;
        let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
        cx.set_global(SharedAppState::new(settings, pending_files));
        cx.set_global(WindowManager::new());
    });
    let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
    let window = cx
        .update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
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

// ========== get_tab_display_title tests ==========

#[gpui::test]
fn test_get_tab_display_title_returns_filename_for_unique_path(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, _cx| {
            if let Some(Tab::Editor(e)) = this.tabs.first_mut() {
                e.location = TabLocation::Local(PathBuf::from("/projects/foo/main.rs"));
            }
            let filename_counts = this.build_tab_filename_counts();
            let tab = this.tabs.first().unwrap();
            let (filename, folder) = this.get_tab_display_title(tab, &filename_counts);
            assert_eq!(filename, "main.rs");
            assert!(
                folder.is_none(),
                "unique filename should have no parent folder suffix"
            );
        });
    });
}

#[gpui::test]
fn test_get_tab_display_title_shows_parent_folder_for_duplicate_filenames(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            if let Some(Tab::Editor(e)) = this.tabs.first_mut() {
                e.location = TabLocation::Local(PathBuf::from("/projects/a/main.rs"));
            }
            this.new_tab(window, cx);
            if let Some(Tab::Editor(e)) = this.tabs.get_mut(1) {
                e.location = TabLocation::Local(PathBuf::from("/projects/b/main.rs"));
            }
            let filename_counts = this.build_tab_filename_counts();
            let tab0 = this.tabs.first().unwrap();
            let (filename0, folder0) = this.get_tab_display_title(tab0, &filename_counts);
            assert_eq!(filename0, "main.rs");
            assert_eq!(
                folder0.as_deref(),
                Some("../a"),
                "first tab should show its parent folder when filename is shared"
            );
            let tab1 = this.tabs.get(1).unwrap();
            let (filename1, folder1) = this.get_tab_display_title(tab1, &filename_counts);
            assert_eq!(filename1, "main.rs");
            assert_eq!(
                folder1.as_deref(),
                Some("../b"),
                "second tab should show its own parent folder"
            );
        });
    });
}

#[gpui::test]
fn test_get_tab_display_title_returns_tab_title_for_untitled_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, _cx| {
            // The default tab has no file_path; its display title should be the tab's own title
            let filename_counts = this.build_tab_filename_counts();
            let tab = this.tabs.first().unwrap();
            let tab_title = tab.title().to_string();
            let (display_title, folder) = this.get_tab_display_title(tab, &filename_counts);
            assert_eq!(display_title, tab_title);
            assert!(folder.is_none());
        });
    });
}

#[gpui::test]
fn test_remote_tab_indicator_label_returns_ssh_for_remote_editor_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, _cx| {
            if let Some(Tab::Editor(e)) = this.tabs.first_mut() {
                e.location = TabLocation::Remote(RemoteSpec {
                    host: "example.com".to_string(),
                    port: 22,
                    user: Some("alice".to_string()),
                    path: "/tmp/test.txt".to_string(),
                    password_in_url: None,
                });
            }
            let tab = this.tabs.first().expect("default tab should exist");
            assert_eq!(this.remote_tab_indicator_label(tab), Some("R"));
        });
    });
}

#[gpui::test]
fn test_remote_tab_indicator_label_is_none_for_local_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, _cx| {
            if let Some(Tab::Editor(e)) = this.tabs.first_mut() {
                e.location = TabLocation::Local(PathBuf::from("/tmp/local.txt"));
            }
            let tab = this.tabs.first().expect("default tab should exist");
            assert_eq!(this.remote_tab_indicator_label(tab), None);
        });
    });
}

// ========== on_next_tab tests ==========

#[gpui::test]
fn test_on_next_tab_advances_active_index_by_one(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.new_tab(window, cx);
            this.new_tab(window, cx);
            // Three tabs: move to index 0, then advance
            this.set_active_tab(0, window, cx);
            this.on_next_tab(window, cx);
            assert_eq!(this.active_tab_index, Some(1));
        });
    });
}

#[gpui::test]
fn test_on_next_tab_wraps_around_from_last_to_first(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.new_tab(window, cx);
            this.new_tab(window, cx);
            let last = this.tabs.len() - 1;
            this.set_active_tab(last, window, cx);
            this.on_next_tab(window, cx);
            assert_eq!(this.active_tab_index, Some(0));
        });
    });
}

#[gpui::test]
fn test_on_next_tab_is_noop_when_no_active_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.active_tab_index = None;
            this.on_next_tab(window, cx);
            assert_eq!(this.active_tab_index, None);
        });
    });
}

// ========== on_previous_tab tests ==========

#[gpui::test]
fn test_on_previous_tab_moves_to_previous_index(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.new_tab(window, cx);
            this.new_tab(window, cx);
            let last = this.tabs.len() - 1;
            this.set_active_tab(last, window, cx);
            this.on_previous_tab(window, cx);
            assert_eq!(this.active_tab_index, Some(last - 1));
        });
    });
}

#[gpui::test]
fn test_on_previous_tab_wraps_around_from_first_to_last(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.new_tab(window, cx);
            this.new_tab(window, cx);
            this.set_active_tab(0, window, cx);
            this.on_previous_tab(window, cx);
            let last = this.tabs.len() - 1;
            assert_eq!(this.active_tab_index, Some(last));
        });
    });
}

#[gpui::test]
fn test_on_previous_tab_is_noop_when_no_active_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.active_tab_index = None;
            this.on_previous_tab(window, cx);
            assert_eq!(this.active_tab_index, None);
        });
    });
}
