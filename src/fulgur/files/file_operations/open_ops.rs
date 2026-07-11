use super::{DecodedContents, detect_encoding_and_decode, looks_binary};
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
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

/// Result of reading and classifying a file on the background executor.
enum FileReadOutcome {
    Decoded(DecodedContents),
    Binary,
    Failed,
}

impl Fulgur {
    /// Find the index of a tab with the given file path
    ///
    /// ### Arguments
    /// - `path`: The path to search for
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(usize)`: The index of the tab if found
    /// - `None`: If the tab was not found
    #[must_use]
    pub fn find_tab_by_path(&self, path: &PathBuf, cx: &gpui::App) -> Option<usize> {
        self.tabs.iter().position(|tab| {
            if let Tab::Editor(editor_tab) = tab.read(cx) {
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
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(usize)`: The index of the matching remote tab.
    /// - `None`: If no tab matches this remote location.
    #[must_use]
    pub fn find_tab_by_remote_spec(&self, spec: &RemoteSpec, cx: &gpui::App) -> Option<usize> {
        self.tabs.iter().position(|tab| {
            if let Tab::Editor(editor_tab) = tab.read(cx)
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
        let path = if let Some(Tab::Editor(editor_tab)) =
            self.tabs.get(tab_index).map(|tab| tab.read(cx))
        {
            editor_tab.file_path().cloned()
        } else {
            None
        };
        let Some(path) = path else {
            return;
        };
        log::debug!("Reloading tab content from disk: {}", path.display());
        cx.spawn_in(window, async move |view, window| {
            let read_path = path.clone();
            let read_result = window
                .background_executor()
                .spawn(async move { std::fs::read(&read_path).map(detect_encoding_and_decode) })
                .await;
            match read_result {
                Ok(decoded) => {
                    window
                        .update(|window, cx| {
                            _ = view.update(cx, |this, cx| {
                                this.apply_reloaded_contents(&path, decoded, window, cx);
                            });
                        })
                        .ok();
                }
                Err(e) => {
                    log::error!("Failed to reload file {}: {e}", path.display());
                }
            }
        })
        .detach();
    }

    /// Apply freshly decoded file contents to the editor tab backing a path.
    ///
    /// ### Arguments
    /// - `path`: The path whose tab should receive the reloaded content
    /// - `decoded`: The decoded file contents produced off the UI thread
    /// - `window`: The window context
    /// - `cx`: The application context
    fn apply_reloaded_contents(
        &mut self,
        path: &Path,
        decoded: DecodedContents,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_index) = self.find_tab_by_path(&path.to_path_buf(), cx) else {
            return;
        };
        let Some(tab_entity) = self.tabs.get(tab_index).cloned() else {
            return;
        };
        let settings = self.settings.editor_settings.clone();
        tab_entity.update(cx, |tab, cx| {
            let Some(editor_tab) = tab.as_editor_mut() else {
                return;
            };
            editor_tab.content.update(cx, |input_state, cx| {
                input_state.set_value(&decoded.content, window, cx);
            });
            editor_tab.set_original_content_from_str(&decoded.content);
            editor_tab.encoding = decoded.encoding;
            editor_tab.lossy_decode = decoded.lossy;
            editor_tab.modified = false;
            editor_tab.update_file_tooltip_cache(decoded.byte_len);
            tab.update_language(window, cx, &settings);
            log::debug!("Tab reloaded successfully from disk: {}", path.display());
        });
    }

    /// Focus an existing tab for a local path and resolve modified-content conflicts.
    ///
    /// ### Arguments
    /// - `path`: The path being opened again
    /// - `tab_index`: The index of the existing tab for this path
    /// - `window`: The active window context
    /// - `cx`: The application context
    fn focus_existing_local_tab_for_open(
        &mut self,
        path: &Path,
        tab_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_modified = self
            .tabs
            .get(tab_index)
            .and_then(|tab| tab.read(cx).as_editor())
            .is_some_and(|editor_tab| editor_tab.modified);

        if is_modified {
            log::debug!(
                "Tab for {} has unsaved changes; asking user which version to keep",
                path.display()
            );
            if let Some(tab_id) = self.tabs.get(tab_index).map(|tab| tab.read(cx).id()) {
                self.show_reopen_modified_file_dialog(path, tab_id, window, cx);
            }
        } else {
            log::debug!(
                "Tab for {} is already open and not modified; focusing existing tab",
                path.display()
            );
        }

        self.active_tab_id = self.tabs.get(tab_index).map(|tab| tab.read(cx).id());
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    /// Internal helper function to open a file from a path. This function handles reading the file, detecting encoding, and creating the editor tab
    ///
    /// ### Arguments
    /// - `view`: The view entity (`WeakEntity`)
    /// - `window`: The async window context
    /// - `path`: The path to the file to open
    ///
    /// ### Returns
    /// - `None`: If the file could not be opened
    /// - `Some(())`: If the file was opened successfully
    async fn open_file_from_path(
        view: &WeakEntity<Self>,
        window: &mut AsyncWindowContext,
        path: &Path,
    ) -> Option<()> {
        log::debug!("Attempting to open file: {}", path.display());
        let canonical_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let read_path = canonical_path.clone();
        let outcome = window
            .background_executor()
            .spawn(async move {
                match std::fs::read(&read_path) {
                    Ok(bytes) => {
                        log::debug!(
                            "Successfully read file: {} ({} bytes)",
                            read_path.display(),
                            bytes.len()
                        );
                        if looks_binary(&bytes) {
                            FileReadOutcome::Binary
                        } else {
                            FileReadOutcome::Decoded(detect_encoding_and_decode(bytes))
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to read file {}: {e}", read_path.display());
                        FileReadOutcome::Failed
                    }
                }
            })
            .await;
        let path = canonical_path.as_path();
        let decoded = match outcome {
            FileReadOutcome::Decoded(decoded) => decoded,
            FileReadOutcome::Failed => return None,
            FileReadOutcome::Binary => {
                log::warn!("Refusing to open binary file: {}", path.display());
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file")
                    .to_string();
                window
                    .update(|window, cx| {
                        window.push_notification(
                            (
                                NotificationType::Warning,
                                SharedString::from(format!(
                                    "Cannot open '{file_name}': appears to be a binary file"
                                )),
                            ),
                            cx,
                        );
                    })
                    .ok();
                return None;
            }
        };
        window
            .update(|window, cx| {
                _ = view.update(cx, |this, cx| {
                    let new_tab_id = this.allocate_tab_id();
                    let mut editor_tab = EditorTab::from_file(
                        FromFileParams {
                            id: new_tab_id,
                            path: path.to_path_buf(),
                            contents: decoded.content,
                            encoding: decoded.encoding,
                            is_modified: false,
                        },
                        window,
                        cx,
                        &this.settings.editor_settings,
                    );
                    editor_tab.lossy_decode = decoded.lossy;
                    let editor_tab_index =
                        this.place_editor_tab_reusing_scratch(Tab::Editor(editor_tab), window, cx);
                    this.maybe_open_markdown_preview_for_editor(editor_tab_index, cx);
                    this.watch_file(path);
                    if crate::fulgur::ui::log_view::opens_as_log_by_default(path)
                        && let Some(tab_id) =
                            this.tabs.get(editor_tab_index).map(|tab| tab.read(cx).id())
                    {
                        this.activate_log_view(tab_id, window, cx);
                    }
                    this.focus_active_tab(window, cx);
                    if let Err(e) = this.settings.add_file(path.to_path_buf()) {
                        log::error!("Failed to add file to recent files: {e}");
                    }
                    let shared = Fulgur::shared_state(cx);
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
                    log::debug!("File opened successfully in new tab: {}", path.display());
                    this.save_state_async(cx, window);
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

    /// Handle opening a file from the command line (double-click or "Open with")
    ///
    /// ### Behavior
    /// - If a tab exists for the file in this window: focus the tab and prompt when unsaved changes exist
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
        log::debug!("Handling file open from CLI: {}", path.display());
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
        let shared = Fulgur::shared_state(cx);
        let should_process_files = cx
            .global::<window_manager::WindowManager>()
            .get_last_focused()
            .is_none_or(|id| id == self.window_id); // If no last focused window, allow this one to process
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
    use crate::fulgur::files::file_operations::test_helpers::{
        setup_fulgur, setup_fulgur_with_root, temp_test_path,
    };
    #[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
    use gpui::BorrowAppContext;
    #[cfg(feature = "gpui-test-support")]
    use gpui::TestAppContext;
    #[cfg(feature = "gpui-test-support")]
    use gpui_component::input::InputEvent;
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
                this.tabs
                    .last()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, _cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                        }
                    });
                let expected_index = this.tabs.len() - 1;
                let result = this.find_tab_by_path(&path, cx);
                assert_eq!(result, Some(expected_index));
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_find_tab_by_path_returns_none_for_unknown_path(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let result = this.find_tab_by_path(&PathBuf::from("/nonexistent/path.txt"), cx);
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
                this.tabs.retain(|t| matches!(t.read(cx), Tab::Settings(_)));
                let result = this.find_tab_by_path(&PathBuf::from("/any/path.txt"), cx);
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
                this.tabs
                    .last()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, _cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Remote(spec.clone());
                        }
                    });
                let expected_index = this.tabs.len() - 1;
                let result = this.find_tab_by_remote_spec(&spec, cx);
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
            fulgur.update(cx, |this, cx| {
                let result = this.find_tab_by_remote_spec(&spec, cx);
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
    #[gpui::test]
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
            // The focused window starts with an empty scratch tab, which the queued file
            // reuses, so the file should be open without adding a new tab.
            let canonical_expected =
                std::fs::canonicalize(&file_path).unwrap_or_else(|_| file_path.clone());
            let has_file = fulgur_two.read(cx).tabs.iter().any(|tab| {
                tab.read(cx)
                    .as_editor()
                    .and_then(|e| e.file_path().cloned())
                    .and_then(|p| std::fs::canonicalize(&p).ok())
                    .is_some_and(|p| p == canonical_expected)
            });
            assert!(has_file, "processing a queued file should open it in a tab");
        });
    }
}
