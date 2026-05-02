use super::detect_encoding_and_decode;
use crate::fulgur::{
    Fulgur,
    editor_tab::{EditorTab, FromFileParams, TabLocation},
    sync::ssh::url::{RemoteSpec, parse_remote_url},
    tab::Tab,
    ui::menus,
    window_manager,
};
use gpui::{
    AsyncWindowContext, Context, ExternalPaths, PathPromptOptions, SharedString, WeakEntity, Window,
};
use gpui_component::{WindowExt, notification::NotificationType};
use std::{collections::HashSet, path::PathBuf};

impl Fulgur {
    /// Find the index of a tab with the given file path
    ///
    /// ### Arguments
    /// - `path`: The path to search for
    ///
    /// ### Returns
    /// - `Some(usize)`: The index of the tab if found
    /// - `None`: If the tab was not found
    pub fn find_tab_by_path(&self, path: &PathBuf) -> Option<usize> {
        self.tabs.iter().position(|tab| {
            if let Tab::Editor(editor_tab) = tab {
                editor_tab.file_path().is_some_and(|p| p == path)
            } else {
                false
            }
        })
    }

    /// Find the index of an editor tab opened from the same remote location.
    ///
    /// ### Arguments
    /// - `spec`: Remote location to search for.
    ///
    /// ### Returns
    /// - `Some(usize)`: The index of the matching remote tab.
    /// - `None`: If no tab matches this remote location.
    pub fn find_tab_by_remote_spec(&self, spec: &RemoteSpec) -> Option<usize> {
        self.tabs.iter().position(|tab| {
            if let Tab::Editor(editor_tab) = tab
                && let TabLocation::Remote(existing_spec) = &editor_tab.location
            {
                existing_spec.host == spec.host
                    && existing_spec.port == spec.port
                    && existing_spec.user == spec.user
                    && existing_spec.path == spec.path
            } else {
                false
            }
        })
    }

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
        let path = if let Some(Tab::Editor(editor_tab)) = self.tabs.get(tab_index) {
            editor_tab.file_path().cloned()
        } else {
            None
        };
        if let Some(path) = path {
            log::debug!("Reloading tab content from disk: {path:?}");
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let (encoding, contents) = detect_encoding_and_decode(&bytes);
                    if let Some(Tab::Editor(editor_tab)) = self.tabs.get_mut(tab_index) {
                        editor_tab.content.update(cx, |input_state, cx| {
                            input_state.set_value(&contents, window, cx);
                        });
                        editor_tab.set_original_content_from_str(&contents);
                        editor_tab.encoding = encoding;
                        editor_tab.modified = false;
                        editor_tab.update_file_tooltip_cache(bytes.len());
                        editor_tab.update_language(window, cx, &self.settings.editor_settings);
                        log::debug!("Tab reloaded successfully from disk: {path:?}");
                    }
                }
                Err(e) => {
                    log::error!("Failed to reload file {path:?}: {e}");
                }
            }
        }
    }

    /// Internal helper function to open a file from a path. This function handles reading the file, detecting encoding, and creating the editor tab
    ///
    /// ### Arguments
    /// - `view`: The view entity (WeakEntity)
    /// - `window`: The async window context
    /// - `path`: The path to the file to open
    ///
    /// ### Returns
    /// - `None`: If the file could not be opened
    /// - `Some(())`: If the file was opened successfully
    async fn open_file_from_path(
        view: WeakEntity<Self>,
        window: &mut AsyncWindowContext,
        path: PathBuf,
    ) -> Option<()> {
        log::debug!("Attempting to open file: {path:?}");
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => {
                log::debug!("Successfully read file: {:?} ({} bytes)", path, bytes.len());
                bytes
            }
            Err(e) => {
                log::error!("Failed to read file {path:?}: {e}");
                return None;
            }
        };
        let (encoding, contents) = detect_encoding_and_decode(&bytes);
        window
            .update(|window, cx| {
                _ = view.update(cx, |this, cx| {
                    let editor_tab = EditorTab::from_file(
                        FromFileParams {
                            id: this.next_tab_id,
                            path: path.clone(),
                            contents,
                            encoding,
                            is_modified: false,
                        },
                        window,
                        cx,
                        &this.settings.editor_settings,
                    );
                    let editor_tab_index = this.tabs.len();
                    this.tabs.push(Tab::Editor(editor_tab));
                    this.active_tab_index = Some(editor_tab_index);
                    this.pending_tab_scroll = Some(editor_tab_index);
                    this.next_tab_id += 1;
                    this.maybe_open_markdown_preview_for_editor(editor_tab_index);
                    this.watch_file(&path);
                    this.focus_active_tab(window, cx);
                    if let Err(e) = this.settings.add_file(path.clone()) {
                        log::error!("Failed to add file to recent files: {e}");
                    }
                    let shared = this.shared_state(cx);
                    let update_info = shared.update_info.lock().clone();
                    let update_link = update_info.as_ref().map(|info| info.download_url.clone());
                    let menus = menus::build_menus(
                        &this.settings.get_recent_files(),
                        update_link.as_deref(),
                    );
                    this.update_menus(menus, cx);
                    let title = path
                        .file_name()
                        .map(|file_name| file_name.to_string_lossy().to_string());
                    this.set_title(title, cx);
                    log::debug!("File opened successfully in new tab: {path:?}");
                    if let Err(e) = this.save_state(cx, window) {
                        log::error!("Failed to save app state after opening file: {e}");
                        this.pending_notification = Some((
                            NotificationType::Warning,
                            format!("File opened but failed to save state: {e}").into(),
                        ));
                    }
                    cx.notify();
                });
            })
            .ok();
        Some(())
    }

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
            let path = paths.first()?.clone();

            // Check if tab already exists for this path
            let should_open_new = window
                .update(|window, cx| {
                    view.update(cx, |this, cx| {
                        if let Some(tab_index) = this.find_tab_by_path(&path) {
                            log::debug!(
                                "Tab already exists for {path:?} at index {tab_index}, focusing and reloading"
                            );
                            if let Some(Tab::Editor(editor_tab)) = this.tabs.get(tab_index) {
                                if editor_tab.modified {
                                    log::debug!("Tab is modified, reloading content from disk");
                                    this.reload_tab_from_disk(tab_index, window, cx);
                                } else {
                                    log::debug!("Tab is not modified, just focusing it");
                                }
                            }
                            this.active_tab_index = Some(tab_index);
                            this.focus_active_tab(window, cx);
                            cx.notify();
                            false // Don't open new tab
                        } else {
                            true // Open new tab
                        }
                    })
                    .ok()
                })
                .ok()??;

            if should_open_new {
                Self::open_file_from_path(view, window, path).await
            } else {
                Some(())
            }
        })
        .detach();
    }

    /// Open a file from a given path
    ///
    /// ### Behavior
    /// First detects if the file is already open, and will focus on that tab if that's the case.
    ///
    /// ### Arguments
    /// - `window`: The window to open the file in
    /// - `cx`: The application context
    /// - `path`: The path to the file to open
    pub fn do_open_file(&mut self, window: &mut Window, cx: &mut Context<Self>, path: PathBuf) {
        if let Some(tab_index) = self.find_tab_by_path(&path) {
            log::debug!(
                "Tab already exists for {path:?} at index {tab_index}, focusing and reloading if modified"
            );
            if let Some(Tab::Editor(editor_tab)) = self.tabs.get(tab_index) {
                if editor_tab.modified {
                    log::debug!("Tab is modified, reloading content from disk");
                    self.reload_tab_from_disk(tab_index, window, cx);
                } else {
                    log::debug!("Tab is not modified, just focusing it");
                }
            }
            self.active_tab_index = Some(tab_index);
            self.focus_active_tab(window, cx);
            cx.notify();
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
            log::debug!("File {path:?} is already open in window {existing_window_id:?}");
            return;
        }
        cx.spawn_in(window, async move |view, window| {
            Self::open_file_from_path(view, window, path).await
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
                    self.pending_notification = Some((
                        NotificationType::Error,
                        format!(
                            "Failed to open remote recent file: {}",
                            error.user_message()
                        )
                        .into(),
                    ));
                    cx.notify();
                }
            }
            return;
        }
        self.do_open_file(window, cx, path);
    }

    /// Handle opening a file from the command line (double-click or "Open with")
    ///
    /// ### Behavior
    /// - If a tab exists for the file in this window: focus the tab (reload if modified)
    /// - If a tab exists in another window: show notification
    /// - If no tab exists: open a new tab and focus it
    ///
    /// ### Arguments
    /// - `window`: The window to open the file in
    /// - `cx`: The application context
    /// - `path`: The path to the file to open
    pub fn handle_open_file_from_cli(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        path: PathBuf,
    ) {
        log::debug!("Handling file open from CLI: {path:?}");
        self.do_open_file(window, cx, path);
    }

    /// Handle dropping external file system paths into this window.
    ///
    /// ### Behavior
    /// - Opens dropped files in new tabs (or focuses existing tabs via `do_open_file`)
    /// - Ignores non-file entries (e.g. directories)
    /// - Deduplicates duplicate paths within the same drop gesture
    ///
    /// ### Arguments
    /// - `paths`: Paths provided by GPUI external file drop
    /// - `window`: The target window
    /// - `cx`: The application context
    pub fn handle_external_paths_drop(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut dropped_files = Vec::new();
        let mut seen = HashSet::new();
        let mut skipped_non_files = 0usize;
        for path in paths.paths() {
            if !path.is_file() {
                skipped_non_files += 1;
                continue;
            }
            if seen.insert(path.clone()) {
                dropped_files.push(path.clone());
            }
        }
        if dropped_files.is_empty() {
            if skipped_non_files > 0 {
                window.push_notification(
                    (
                        NotificationType::Info,
                        SharedString::from("Dropped items contain no files to open"),
                    ),
                    cx,
                );
            }
            return;
        }
        log::info!(
            "Opening {} dropped file(s) in window {:?}",
            dropped_files.len(),
            self.window_id
        );
        for file_path in dropped_files {
            self.do_open_file(window, cx, file_path);
        }
        if skipped_non_files > 0 {
            window.push_notification(
                (
                    NotificationType::Info,
                    SharedString::from(format!(
                        "Ignored {skipped_non_files} dropped item(s) that are not files"
                    )),
                ),
                cx,
            );
        }
    }

    /// Process pending files from macOS "Open With" events
    ///
    /// ### Arguments
    /// - `window`: The window to open files in
    /// - `cx`: The application context
    pub fn process_pending_files_from_macos(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let shared = self.shared_state(cx);
        let should_process_files = cx
            .global::<window_manager::WindowManager>()
            .get_last_focused()
            .map(|id| id == self.window_id)
            .unwrap_or(true); // If no last focused window, allow this one to process
        let files_to_open = if should_process_files {
            if let Some(mut pending) = shared.pending_files_from_macos.try_lock() {
                if pending.is_empty() {
                    Vec::new()
                } else {
                    log::info!(
                        "Processing {} pending file(s) from macOS open event in window {:?}",
                        pending.len(),
                        self.window_id
                    );
                    pending.drain(..).collect()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        for file_path in files_to_open {
            self.handle_open_file_from_cli(window, cx, file_path);
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::{
        editor_tab::TabLocation,
        sync::ssh::url::{RemoteSpec, format_remote_url},
        tab::Tab,
    };
    #[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
    use crate::fulgur::{shared_state::SharedAppState, window_manager::WindowManager};
    #[cfg(feature = "gpui-test-support")]
    use std::path::PathBuf;

    #[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
    use crate::fulgur::files::file_operations::test_helpers::invoke_process_pending_files_from_macos;
    #[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
    use crate::fulgur::files::file_operations::test_helpers::{
        open_window_with_fulgur, setup_test_globals,
    };
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::files::file_operations::test_helpers::{setup_fulgur, temp_test_path};
    #[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
    use gpui::BorrowAppContext;
    #[cfg(feature = "gpui-test-support")]
    use gpui::TestAppContext;
    #[cfg(feature = "gpui-test-support")]
    use tempfile::TempDir;

    // ========== find_tab_by_path tests ==========

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_find_tab_by_path_returns_index_for_existing_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let path = temp_test_path("fulgur_find_tab_test.txt");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                if let Some(Tab::Editor(editor_tab)) = this.tabs.last_mut() {
                    editor_tab.location = TabLocation::Local(path.clone());
                }
                let expected_index = this.tabs.len() - 1;
                let result = this.find_tab_by_path(&path);
                assert_eq!(result, Some(expected_index));
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_find_tab_by_path_returns_none_for_unknown_path(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, _cx| {
                let result = this.find_tab_by_path(&PathBuf::from("/nonexistent/path.txt"));
                assert_eq!(result, None);
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_find_tab_by_path_ignores_settings_tabs(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.open_settings(window, cx);
                // Remove all editor tabs so only settings tabs remain
                this.tabs.retain(|t| matches!(t, Tab::Settings(_)));
                let result = this.find_tab_by_path(&PathBuf::from("/any/path.txt"));
                assert_eq!(result, None);
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_find_tab_by_remote_spec_returns_index_for_existing_remote_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let spec = RemoteSpec {
            host: "example.com".to_string(),
            port: 22,
            user: Some("alice".to_string()),
            path: "/var/log/syslog".to_string(),
            password_in_url: None,
        };

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                if let Some(Tab::Editor(editor_tab)) = this.tabs.last_mut() {
                    editor_tab.location = TabLocation::Remote(spec.clone());
                }
                let expected_index = this.tabs.len() - 1;
                let result = this.find_tab_by_remote_spec(&spec);
                assert_eq!(result, Some(expected_index));
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_find_tab_by_remote_spec_returns_none_for_unknown_remote_spec(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let spec = RemoteSpec {
            host: "example.com".to_string(),
            port: 22,
            user: Some("alice".to_string()),
            path: "/var/log/syslog".to_string(),
            password_in_url: None,
        };

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, _cx| {
                let result = this.find_tab_by_remote_spec(&spec);
                assert_eq!(result, None);
            });
        });
    }

    // ========== reload_tab_from_disk tests ==========

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_reload_tab_from_disk_updates_content_from_file(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("reload_test.txt");
        std::fs::write(&path, "initial content").expect("failed to write initial file");

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, _cx| {
                if let Some(Tab::Editor(editor_tab)) = this.tabs.last_mut() {
                    editor_tab.location = TabLocation::Local(path.clone());
                    editor_tab.set_original_content_from_str("initial content");
                }
            });
        });

        std::fs::write(&path, "updated content").expect("failed to overwrite file");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.reload_tab_from_disk(0, window, cx);
                let content = this
                    .tabs
                    .first()
                    .and_then(|t| t.as_editor())
                    .map(|e| e.content.read(cx).text().to_string())
                    .unwrap_or_default();
                assert_eq!(content, "updated content");
                let modified = this
                    .tabs
                    .first()
                    .and_then(|t| t.as_editor())
                    .map(|e| e.modified)
                    .unwrap_or(true);
                assert!(!modified, "tab should not be marked modified after reload");
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_reload_tab_from_disk_is_noop_without_file_path(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                // The default tab created on Fulgur::new has no file_path
                let initial_content = this
                    .tabs
                    .first()
                    .and_then(|t| t.as_editor())
                    .map(|e| e.content.read(cx).text().to_string())
                    .unwrap_or_default();
                this.reload_tab_from_disk(0, window, cx);
                let content_after = this
                    .tabs
                    .first()
                    .and_then(|t| t.as_editor())
                    .map(|e| e.content.read(cx).text().to_string())
                    .unwrap_or_default();
                assert_eq!(content_after, initial_content);
            });
        });
    }

    // ========== do_open_file tests ==========

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_do_open_file_focuses_existing_tab_when_already_open(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let path = temp_test_path("fulgur_already_open_test.txt");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(Tab::Editor(editor_tab)) = this.tabs.last_mut() {
                    editor_tab.location = TabLocation::Local(path.clone());
                }
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
    fn test_do_open_file_opens_new_tab_from_disk(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("open_new_tab.rs");
        std::fs::write(&path, "fn main() {}").expect("failed to write file");

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
            "a new tab should be opened for a file not yet open"
        );

        let tab_path = fulgur.read_with(&visual_cx, |this, _| {
            this.tabs
                .last()
                .and_then(|t| t.as_editor())
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
                if let Some(Tab::Editor(editor_tab)) = this.tabs.first_mut() {
                    editor_tab.location = TabLocation::Remote(spec.clone());
                }
                this.new_tab(window, cx);
                let tab_count_before = this.tabs.len();
                this.do_open_recent_file(window, cx, remote_recent.clone());
                assert_eq!(
                    this.tabs.len(),
                    tab_count_before,
                    "remote recent should focus existing tab instead of creating a duplicate"
                );
                assert_eq!(this.active_tab_index, Some(0));
            });
        });
    }

    #[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
    #[gpui::test]
    fn test_process_pending_files_from_macos_only_focused_window_drains_queue(
        cx: &mut TestAppContext,
    ) {
        setup_test_globals(cx);
        let (window_id_one, fulgur_one) = open_window_with_fulgur(cx);
        let (window_id_two, fulgur_two) = open_window_with_fulgur(cx);
        cx.update(|cx| {
            cx.update_global::<WindowManager, _>(|manager, _| {
                manager.register(window_id_one, fulgur_one.downgrade());
                manager.register(window_id_two, fulgur_two.downgrade());
            });
        });
        let dir = TempDir::new().expect("failed to create temp dir");
        let file_path = dir.path().join("macos-open-url-focus-test.txt");
        std::fs::write(&file_path, "from open-url event").expect("failed to write temp file");
        cx.update(|cx| {
            let shared = cx.global::<SharedAppState>();
            shared
                .pending_files_from_macos
                .lock()
                .push(file_path.clone());
        });
        // Window 1 is not last focused, so it must not drain the queue.
        invoke_process_pending_files_from_macos(cx, window_id_one, &fulgur_one);
        cx.update(|cx| {
            let shared = cx.global::<SharedAppState>();
            assert_eq!(
                shared.pending_files_from_macos.lock().len(),
                1,
                "non-focused windows must not consume pending macOS open-url files"
            );
        });
        invoke_process_pending_files_from_macos(cx, window_id_two, &fulgur_two);
        cx.run_until_parked();
        cx.update(|cx| {
            let shared = cx.global::<SharedAppState>();
            assert!(
                shared.pending_files_from_macos.lock().is_empty(),
                "focused window should consume pending macOS open-url files"
            );
            let tab_count = fulgur_two.read(cx).tabs.len();
            assert!(
                tab_count >= 2,
                "processing a queued file should open it in a new tab"
            );
        });
    }
}
