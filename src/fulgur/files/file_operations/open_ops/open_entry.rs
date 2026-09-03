use crate::fulgur::{Fulgur, sync::ssh::url::parse_remote_url};
use gpui::{Context, PathPromptOptions, SharedString, Window};
use gpui_component::{WindowExt, notification::NotificationType};
use std::path::PathBuf;

impl Fulgur {
    /// Open a file
    ///
    /// ### Arguments
    /// - `window`: The window to open the file in
    /// - `cx`: The application context
    pub fn open_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path_future = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |view, window| {
            let paths = path_future.await.ok()?.ok()??;
            let raw_path = paths.first()?.clone();
            let path = std::fs::canonicalize(&raw_path).unwrap_or(raw_path);

            // Check if tab already exists for this path
            let should_open_new = window
                .update(|window, cx| {
                    view.update(cx, |this, cx| {
                        if let Some(tab_index) = this.find_tab_by_path(&path, cx) {
                            log::debug!(
                                "Tab already exists for {} at index {tab_index}, focusing existing tab",
                                path.display()
                            );
                            this.focus_existing_local_tab_for_open(&path, tab_index, window, cx);
                            false // Don't open new tab
                        } else {
                            true // Open new tab
                        }
                    })
                    .ok()
                })
                .ok()??;

            if should_open_new {
                Self::open_file_from_path(&view, window, &path).await
            } else {
                Some(())
            }
        })
        .detach();
    }

    /// Open a file from a given path. First detects if the file is already open, and will focus on that tab if that's the case.
    ///
    /// ### Arguments
    /// - `window`: The window to open the file in
    /// - `cx`: The application context
    /// - `path`: The path to the file to open
    pub fn do_open_file(&mut self, window: &mut Window, cx: &mut Context<Self>, path: PathBuf) {
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        if let Some(tab_index) = self.find_tab_by_path(&path, cx) {
            log::debug!(
                "Tab already exists for {} at index {tab_index}, focusing existing tab",
                path.display()
            );
            self.focus_existing_local_tab_for_open(&path, tab_index, window, cx);
            return;
        }
        let window_manager = cx.global::<crate::fulgur::window_manager::WindowManager>();
        if let Some(existing_window_id) =
            window_manager.find_window_with_file(&path, self.window_id, cx)
        {
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown file");
            let message = format!("File '{file_name}' is already open in another window");
            window.push_notification((NotificationType::Info, SharedString::from(message)), cx);
            log::debug!(
                "File {} is already open in window {existing_window_id:?}",
                path.display()
            );
            return;
        }
        cx.spawn_in(window, async move |view, window| {
            Self::open_file_from_path(&view, window, &path).await
        })
        .detach();
    }

    /// Open a recent entry, dispatching to local or remote open logic.
    ///
    /// ### Arguments
    /// - `window`: The target window
    /// - `cx`: The application context
    /// - `path`: The recent entry payload
    pub fn do_open_recent_file(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        path: PathBuf,
    ) {
        let recent_value = path.to_string_lossy();
        if recent_value.starts_with("ssh://") || recent_value.starts_with("sftp://") {
            match parse_remote_url(recent_value.as_ref()) {
                Ok(spec) => self.do_open_remote_file(window, cx, spec),
                Err(error) => {
                    window.push_notification(
                        (
                            NotificationType::Error,
                            SharedString::from(format!(
                                "Failed to open remote recent file: {}",
                                error.user_message()
                            )),
                        ),
                        cx,
                    );
                }
            }
            return;
        }
        self.do_open_file(window, cx, path);
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::{
        editor_tab::TabLocation,
        sync::ssh::url::{RemoteSpec, format_remote_url},
    };
    #[cfg(feature = "gpui-test-support")]
    use std::path::PathBuf;

    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::files::file_operations::test_helpers::{
        setup_fulgur, setup_fulgur_with_root, temp_test_path,
    };
    #[cfg(feature = "gpui-test-support")]
    use gpui::TestAppContext;
    #[cfg(feature = "gpui-test-support")]
    use gpui_component::input::InputEvent;
    #[cfg(feature = "gpui-test-support")]
    use tempfile::TempDir;

    // ========== do_open_file tests ==========

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_do_open_file_focuses_existing_tab_when_already_open(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let path = temp_test_path("fulgur_already_open_test.txt");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .last()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, _cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                        }
                    });
                let count_before = this.tabs.len();
                this.do_open_file(window, cx, path.clone());
                assert_eq!(
                    this.tabs.len(),
                    count_before,
                    "no new tab should be created for an already-open file"
                );
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_do_open_file_does_not_reload_modified_existing_tab_without_confirmation(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("already_open_modified.txt");
        std::fs::write(&path, "content on disk").expect("failed to write disk version");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                            editor_tab.content.update(cx, |input_state, cx| {
                                input_state.set_value("local unsaved edits", window, cx);
                            });
                            editor_tab.set_original_content_from_str("content on disk");
                            editor_tab.modified = true;
                        }
                    });

                this.do_open_file(window, cx, path.clone());

                let editor_tab = this.tabs[0]
                    .read(cx)
                    .as_editor()
                    .expect("expected editor tab");
                assert_eq!(
                    editor_tab.content.read(cx).text(),
                    "local unsaved edits",
                    "re-opening a modified tab should keep local edits until user confirms reload"
                );
                assert!(
                    editor_tab.modified,
                    "re-opening a modified tab should keep the modified flag set"
                );
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_do_open_file_reuses_empty_scratch_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("open_new_tab.rs");
        std::fs::write(&path, "fn main() {}").expect("failed to write file");

        // The default tab created on Fulgur::new is an empty, unsaved scratch buffer.
        let count_before = fulgur.read_with(&visual_cx, |this, _| this.tabs.len());

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.do_open_file(window, cx, path.clone());
            });
        });
        visual_cx.run_until_parked();

        let count_after = fulgur.read_with(&visual_cx, |this, _| this.tabs.len());
        assert_eq!(
            count_after, count_before,
            "opening a file should reuse the empty scratch tab instead of adding one"
        );

        let tab_path = fulgur.read_with(&visual_cx, |this, cx| {
            this.tabs
                .last()
                .and_then(|t| t.read(cx).as_editor())
                .and_then(|e| e.file_path().cloned())
        });
        // Canonicalize both sides since macOS may resolve /var/ -> /private/var/
        let canonical_expected = std::fs::canonicalize(&path).unwrap_or(path.clone());
        let canonical_actual = tab_path
            .as_ref()
            .and_then(|p| std::fs::canonicalize(p).ok())
            .unwrap_or_else(|| tab_path.clone().unwrap_or_default());
        assert_eq!(canonical_actual, canonical_expected);
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_do_open_file_does_not_reuse_tab_with_content(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("keep_scratch.rs");
        std::fs::write(&path, "fn main() {}").expect("failed to write file");

        // Type into the default scratch tab so it is no longer empty.
        let editor_content = visual_cx.update(|_window, cx| {
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
            editor_content.update(cx, |input_state, cx| {
                input_state.set_value("some work in progress", window, cx);
                cx.emit(InputEvent::Change);
            });
        });
        visual_cx.run_until_parked();

        let count_before = fulgur.read_with(&visual_cx, |this, _| this.tabs.len());
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.do_open_file(window, cx, path.clone());
            });
        });
        visual_cx.run_until_parked();

        let count_after = fulgur.read_with(&visual_cx, |this, _| this.tabs.len());
        assert_eq!(
            count_after,
            count_before + 1,
            "a tab with unsaved content must not be reused; a new tab should open"
        );
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_do_open_file_reuses_whitespace_only_scratch_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("reuse_blank.rs");
        std::fs::write(&path, "fn main() {}").expect("failed to write file");

        // Fill the default scratch tab with whitespace only; it should still be reused.
        let editor_content = visual_cx.update(|_window, cx| {
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
            editor_content.update(cx, |input_state, cx| {
                input_state.set_value("  \n\t\n", window, cx);
                cx.emit(InputEvent::Change);
            });
        });
        visual_cx.run_until_parked();

        let count_before = fulgur.read_with(&visual_cx, |this, _| this.tabs.len());
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.do_open_file(window, cx, path.clone());
            });
        });
        visual_cx.run_until_parked();

        let count_after = fulgur.read_with(&visual_cx, |this, _| this.tabs.len());
        assert_eq!(
            count_after, count_before,
            "a whitespace-only scratch tab should be reused instead of adding a tab"
        );
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_do_open_file_reuses_only_the_last_tab_position(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("reuse_last_position.rs");
        std::fs::write(&path, "fn main() {}").expect("failed to write file");

        // Two blank scratch tabs: only the last one in position should be replaced,
        // leaving the earlier blank tab untouched.
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
            });
        });

        let count_before = fulgur.read_with(&visual_cx, |this, _| this.tabs.len());
        let first_tab_id = fulgur.read_with(&visual_cx, |this, cx| this.tabs[0].read(cx).id());

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.do_open_file(window, cx, path.clone());
            });
        });
        visual_cx.run_until_parked();

        let count_after = fulgur.read_with(&visual_cx, |this, _| this.tabs.len());
        assert_eq!(
            count_after, count_before,
            "the trailing scratch tab should be reused, keeping the tab count stable"
        );

        let (first_id_after, first_is_blank, last_has_path) =
            fulgur.read_with(&visual_cx, |this, cx| {
                let first_blank = this.tabs[0].read(cx).as_editor().is_some_and(|e| {
                    e.location.is_untitled() && e.content.read(cx).text().len() == 0
                });
                let last_path = this
                    .tabs
                    .last()
                    .and_then(|t| t.read(cx).as_editor())
                    .and_then(|e| e.file_path().cloned())
                    .is_some();
                (this.tabs[0].read(cx).id(), first_blank, last_path)
            });
        assert_eq!(
            first_id_after, first_tab_id,
            "the earlier blank tab must be preserved, not the one replaced"
        );
        assert!(
            first_is_blank,
            "the earlier blank scratch tab must remain blank"
        );
        assert!(last_has_path, "the opened file must land in the last tab");
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_do_open_recent_file_focuses_existing_remote_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let spec = RemoteSpec {
            host: "example.com".to_string(),
            port: 22,
            user: Some("alice".to_string()),
            path: "/tmp/notes.md".to_string(),
            password_in_url: None,
        };
        let remote_recent = PathBuf::from(format_remote_url(&spec));

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, _cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Remote(spec.clone());
                        }
                    });
                this.new_tab(window, cx);
                let tab_count_before = this.tabs.len();
                this.do_open_recent_file(window, cx, remote_recent.clone());
                assert_eq!(
                    this.tabs.len(),
                    tab_count_before,
                    "remote recent should focus existing tab instead of creating a duplicate"
                );
                assert_eq!(this.active_tab_index(cx), Some(0));
            });
        });
    }
}
