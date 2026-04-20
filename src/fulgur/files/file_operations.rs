use crate::fulgur::{
    Fulgur,
    editor_tab::{EditorTab, FromFileParams, TabLocation},
    sync::ssh::url::RemoteSpec,
    sync::ssh::{
        self,
        session::{HostKeyDecision, HostKeyRequest},
    },
    tab::Tab,
    ui::{
        components_utils::{UNTITLED, UTF_8},
        menus,
    },
    utils::atomic_write::atomic_write_file,
    window_manager,
};
use chardetng::EncodingDetector;
use gpui::{
    AsyncWindowContext, Context, ExternalPaths, Focusable, PathPromptOptions, SharedString,
    WeakEntity, Window,
};
use gpui_component::{WindowExt, notification::NotificationType};
use std::{collections::HashSet, path::PathBuf, sync::Arc, time::Duration};
use zeroize::Zeroizing;

/// Detect encoding from file bytes
///
/// ### Arguments
/// - `bytes`: The bytes to detect encoding from
///
/// ### Returns
/// - `(String, String)`: The detected encoding and decoded string
pub fn detect_encoding_and_decode(bytes: &[u8]) -> (String, String) {
    if let Ok(text) = std::str::from_utf8(bytes) {
        log::debug!("File encoding detected as UTF-8");
        return (UTF_8.to_string(), text.to_string());
    }
    let mut detector = EncodingDetector::new();
    detector.feed(bytes, true);
    let encoding = detector.guess(None, true);
    let (decoded, _, had_errors) = encoding.decode(bytes);
    let encoding_name = if had_errors {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                log::debug!("File encoding detected as UTF-8 (after error recovery)");
                return (UTF_8.to_string(), text.to_string());
            }
            Err(_) => {
                let text = String::from_utf8_lossy(bytes).to_string();
                log::warn!("File encoding detection failed, using UTF-8 lossy conversion");
                return (UTF_8.to_string(), text);
            }
        }
    } else {
        encoding.name().to_string()
    };
    log::debug!("File encoding detected as: {}", encoding_name);
    (encoding_name, decoded.to_string())
}

/// Result of a successfully loaded remote file, delivered by the SSH background thread.
pub struct RemoteFileResult {
    pub spec: RemoteSpec,
    pub content: String,
    pub encoding: String,
    pub file_size: usize,
}

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
            log::debug!("Reloading tab content from disk: {:?}", path);
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
                        log::debug!("Tab reloaded successfully from disk: {:?}", path);
                    }
                }
                Err(e) => {
                    log::error!("Failed to reload file {:?}: {}", path, e);
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
        log::debug!("Attempting to open file: {:?}", path);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => {
                log::debug!("Successfully read file: {:?} ({} bytes)", path, bytes.len());
                bytes
            }
            Err(e) => {
                log::error!("Failed to read file {:?}: {}", path, e);
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
                        log::error!("Failed to add file to recent files: {}", e);
                    }
                    let shared = this.shared_state(cx);
                    let update_info = shared.update_info.lock().clone();
                    let update_link = if let Some(info) = update_info {
                        Some(info.download_url.clone())
                    } else {
                        None
                    };
                    let menus = menus::build_menus(&this.settings.get_recent_files(), update_link);
                    this.update_menus(menus, cx);
                    let title = path
                        .file_name()
                        .map(|file_name| file_name.to_string_lossy().to_string());
                    this.set_title(title, cx);
                    log::debug!("File opened successfully in new tab: {:?}", path);
                    if let Err(e) = this.save_state(cx, window) {
                        log::error!("Failed to save app state after opening file: {}", e);
                        this.pending_notification = Some((
                            NotificationType::Warning,
                            format!("File opened but failed to save state: {}", e).into(),
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
                                "Tab already exists for {:?} at index {}, focusing and reloading",
                                path,
                                tab_index
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
                "Tab already exists for {:?} at index {}, focusing and reloading if modified",
                path,
                tab_index
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
            let message = format!("File '{}' is already open in another window", file_name);
            window.push_notification((NotificationType::Info, SharedString::from(message)), cx);
            log::debug!(
                "File {:?} is already open in window {:?}",
                path,
                existing_window_id
            );
            return;
        }
        cx.spawn_in(window, async move |view, window| {
            Self::open_file_from_path(view, window, path).await
        })
        .detach();
    }

    /// Open a remote file from a parsed `RemoteSpec`.
    ///
    /// ### Arguments
    /// - `window`: The window to open the tab in
    /// - `cx`: The application context
    /// - `spec`: The parsed remote file specification
    pub fn do_open_remote_file(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        spec: RemoteSpec,
    ) {
        let host = spec.host.clone();
        let port = spec.port;
        let user = spec.user.clone();
        let entity = cx.entity().downgrade();

        self.show_ssh_password_dialog(
            window,
            cx,
            host,
            port,
            user,
            move |resolved_user, password, window, cx| {
                let mut spec_with_user = spec.clone();
                spec_with_user.user = Some(resolved_user);
                spec_with_user.password_in_url = None;
                if let Some(entity) = entity.upgrade() {
                    entity.update(cx, |fulgur, cx| {
                        fulgur.spawn_ssh_open_task(window, cx, spec_with_user, password);
                    });
                }
            },
        );
    }

    /// Spawn the blocking SSH connect + file-read task plus a GPUI monitoring task.
    ///
    /// ### Arguments
    /// - `window`: The window context used to spawn the async monitoring task
    /// - `cx`: The application context
    /// - `spec`: The remote file specification with a resolved username
    /// - `password`: The session-scoped password (zeroed on drop)
    fn spawn_ssh_open_task(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        spec: RemoteSpec,
        password: Zeroizing<String>,
    ) {
        let pending_remote_open = Arc::clone(&self.pending_remote_open);

        let pending_host_key: Arc<parking_lot::Mutex<Option<HostKeyRequest>>> =
            Arc::new(parking_lot::Mutex::new(None));
        let pending_host_key_for_thread = Arc::clone(&pending_host_key);
        let pending_host_key_for_task = Arc::clone(&pending_host_key);

        let pending_remote_for_thread = Arc::clone(&pending_remote_open);
        let pending_remote_for_task = Arc::clone(&pending_remote_open);

        let spec_for_thread = spec.clone();
        let user = spec.user.clone().unwrap_or_default();

        std::thread::spawn(move || {
            let slot = pending_host_key_for_thread;
            let session_result = self::ssh::session::connect(
                &spec_for_thread,
                &user,
                &password,
                move |fingerprint, host, port| {
                    let (tx, rx) = std::sync::mpsc::channel();
                    *slot.lock() = Some(HostKeyRequest {
                        fingerprint: fingerprint.to_string(),
                        host: host.to_string(),
                        port,
                        decision_tx: tx,
                    });
                    match rx.recv() {
                        Ok(decision) => decision,
                        Err(_) => HostKeyDecision::Reject,
                    }
                },
            );

            let outcome = session_result
                .and_then(|session| {
                    self::ssh::sftp::read_remote_file(&session, &spec_for_thread.path)
                        .map(|bytes| (session, bytes))
                })
                .map(|(_, bytes)| {
                    let (encoding, content) = detect_encoding_and_decode(&bytes);
                    RemoteFileResult {
                        spec: spec_for_thread,
                        content,
                        encoding,
                        file_size: bytes.len(),
                    }
                })
                .map_err(|e| e.user_message());

            *pending_remote_for_thread.lock() = Some(outcome);
        });

        cx.spawn_in(window, async move |view, async_cx| {
            let deadline = std::time::Instant::now() + Duration::from_secs(60);
            loop {
                async_cx
                    .background_executor()
                    .timer(Duration::from_millis(100))
                    .await;

                if std::time::Instant::now() > deadline {
                    *pending_remote_for_task.lock() =
                        Some(Err("SSH connection timed out (60 s)".to_string()));
                    async_cx
                        .update(|_, cx| {
                            _ = view.update(cx, |_, cx| cx.notify());
                        })
                        .ok();
                    break;
                }

                let hk_req = pending_host_key_for_task.lock().take();
                if let Some(req) = hk_req {
                    async_cx
                        .update(|window, cx| {
                            _ = view.update(cx, |fulgur, cx| {
                                fulgur.show_ssh_host_fingerprint_dialog(window, cx, req);
                            });
                        })
                        .ok();
                }

                if pending_remote_for_task.lock().is_some() {
                    async_cx
                        .update(|_, cx| {
                            _ = view.update(cx, |_, cx| cx.notify());
                        })
                        .ok();
                    break;
                }
            }
        })
        .detach();
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
        log::debug!("Handling file open from CLI: {:?}", path);
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
                        "Ignored {} dropped item(s) that are not files",
                        skipped_non_files
                    )),
                ),
                cx,
            );
        }
    }

    /// Save a file
    ///
    /// ### Arguments
    /// - `window`: The window to save the file in
    /// - `cx`: The application context
    pub fn save_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        let Some(active_tab_index) = self.active_tab_index else {
            return;
        };
        let active_tab = &self.tabs[active_tab_index];
        let (path, content_entity) = match active_tab {
            Tab::Editor(editor_tab) => {
                let Some(file_path) = editor_tab.file_path().cloned() else {
                    self.save_file_as(window, cx);
                    return;
                };
                (file_path, editor_tab.content.clone())
            }
            Tab::Settings(_) | Tab::MarkdownPreview(_) => return,
        };
        let contents = content_entity.read(cx).text().to_string();
        log::debug!("Saving file: {:?} ({} bytes)", path, contents.len());
        if let Err(e) = atomic_write_file(&path, contents.as_bytes()) {
            log::error!("Failed to save file {:?}: {}", path, e);
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
            window.push_notification(
                (
                    NotificationType::Error,
                    SharedString::from(format!("Failed to save '{}': {}", file_name, e)),
                ),
                cx,
            );
            return;
        }
        log::debug!("File saved successfully: {:?}", path);
        self.file_watch_state
            .last_file_saves
            .insert(path.clone(), std::time::Instant::now());
        if let Tab::Editor(editor_tab) = &mut self.tabs[active_tab_index] {
            editor_tab.mark_as_saved(cx);
            editor_tab.update_file_tooltip_cache(contents.len());
        }
        cx.notify();
    }

    /// Save a file as
    ///
    /// ### Arguments
    /// - `window`: The window to save the file as in
    /// - `cx`: The application context
    pub fn save_file_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        let Some(active_tab_index) = self.active_tab_index else {
            return;
        };
        let (content_entity, directory, suggested_filename) = match &self.tabs[active_tab_index] {
            Tab::Editor(editor_tab) => {
                let dir = if let Some(path) = editor_tab.file_path() {
                    path.parent()
                        .unwrap_or(std::path::Path::new("."))
                        .to_path_buf()
                } else {
                    std::env::current_dir().unwrap_or_default()
                };
                let suggested = editor_tab.get_suggested_filename();
                (editor_tab.content.clone(), dir, suggested)
            }
            Tab::Settings(_) | Tab::MarkdownPreview(_) => return,
        };
        let path_future = cx.prompt_for_new_path(&directory, suggested_filename.as_deref());
        cx.spawn_in(window, async move |view, window| {
            let path = path_future.await.ok()?.ok()??;
            let contents = window
                .update(|_, cx| content_entity.read(cx).text().to_string())
                .ok()?;
            log::debug!("Saving file as: {:?} ({} bytes)", path, contents.len());
            if let Err(e) = atomic_write_file(&path, contents.as_bytes()) {
                log::error!("Failed to save file {:?}: {}", path, e);
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
                let message = SharedString::from(format!("Failed to save '{}': {}", file_name, e));
                window
                    .update(|_, cx| {
                        _ = view.update(cx, |this, cx| {
                            this.pending_notification = Some((NotificationType::Error, message));
                            cx.notify();
                        });
                    })
                    .ok()?;
                return None;
            }
            log::debug!("File saved successfully as: {:?}", path);
            window
                .update(|window, cx| {
                    _ = view.update(cx, |this, cx| {
                        let old_path = if let Some(Tab::Editor(editor_tab)) =
                            this.tabs.get(active_tab_index)
                        {
                            editor_tab.file_path().cloned()
                        } else {
                            None
                        };
                        if let Some(old_path) = old_path {
                            this.unwatch_file(&old_path);
                        }
                        this.file_watch_state
                            .last_file_saves
                            .insert(path.clone(), std::time::Instant::now());
                        if let Some(Tab::Editor(editor_tab)) = this.tabs.get_mut(active_tab_index) {
                            editor_tab.location = TabLocation::Local(path.clone());
                            editor_tab.title = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or(UNTITLED)
                                .to_string()
                                .into();
                            editor_tab.mark_as_saved(cx);
                            editor_tab.update_file_tooltip_cache(contents.len());
                            editor_tab.update_language(window, cx, &this.settings.editor_settings);
                            cx.notify();
                        }
                        this.watch_file(&path);
                    });
                })
                .ok()?;
            Some(())
        })
        .detach();
    }

    /// Show notification when file is reloaded
    ///
    /// ### Arguments
    /// - `path`: The path to the file that was reloaded
    /// - `window`: The window to show the notification in
    /// - `cx`: The application context
    pub(super) fn show_notification_file_reloaded(
        &self,
        path: &std::path::Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let message = SharedString::from(format!("File {} has been updated externally", filename));
        window.push_notification((NotificationType::Info, message), cx);
    }

    /// Show notification when file is deleted
    ///
    /// ### Arguments
    /// - `path`: The path to the file that was deleted
    /// - `window`: The window to show the notification in
    /// - `cx`: The application context
    pub(super) fn show_notification_file_deleted(
        &self,
        path: &std::path::Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let message = SharedString::from(format!("File '{}' deleted externally", filename));
        window.push_notification((NotificationType::Warning, message), cx);
    }

    /// Show notification when file is renamed
    ///
    /// ### Arguments
    /// - `from`: The path to the file that was renamed from
    /// - `to`: The path to the file that was renamed to
    /// - `window`: The window to show the notification in
    /// - `cx`: The application context
    pub(super) fn show_notification_file_renamed(
        &self,
        from: &std::path::Path,
        to: &std::path::Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old_name = from.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let new_name = to.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let message = SharedString::from(format!("File renamed: {} → {}", old_name, new_name));
        window.push_notification((NotificationType::Info, message), cx);
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

    /// Update search results if the search query has changed
    ///
    /// ### Arguments
    /// - `window`: The window containing the search bar and editor
    /// - `cx`: The application context
    pub fn update_search_if_needed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_state.show_search {
            let current_query = self.search_state.search_input.read(cx).text().to_string();
            if current_query != self.search_state.last_search_query {
                self.perform_search(window, cx);
                // Restore focus to the search input after perform_search
                let search_focus = self.search_state.search_input.read(cx).focus_handle(cx);
                window.focus(&search_focus, cx);
            }
        }
    }

    /// Open the native OS print dialog for the current document
    ///
    /// Writes the active tab's content to a temporary HTML file and opens it with
    /// the system's default browser, which automatically triggers the native print dialog.
    /// This approach works cross-platform without requiring OS-specific print APIs.
    ///
    /// ### Arguments
    /// - `window`: The window containing the editor
    /// - `cx`: The application context
    pub fn print_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active_tab_index) = self.active_tab_index else {
            return;
        };
        let (title, content) = match &self.tabs[active_tab_index] {
            Tab::Editor(editor_tab) => {
                let title = editor_tab.title.clone();
                let content = editor_tab.content.read(cx).text().to_string();
                (title, content)
            }
            Tab::Settings(_) | Tab::MarkdownPreview(_) => return,
        };
        let escaped_content = content
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let escaped_title = title
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>{title}</title>
<style>
  body {{ margin: 0; padding: 1em; font-family: monospace; white-space: pre-wrap; word-wrap: break-word; }}
  @media print {{ body {{ margin: 0; }} }}
</style>
</head>
<body>{content}</body>
<script>window.onload = function() {{ window.print(); }};</script>
</html>"#,
            title = escaped_title,
            content = escaped_content,
        );
        let temp_path =
            std::env::temp_dir().join(format!("fulgur_print_{}.html", std::process::id()));
        if let Err(e) = std::fs::write(&temp_path, html.as_bytes()) {
            log::error!("Failed to write print temp file: {}", e);
            window.push_notification(
                (
                    NotificationType::Error,
                    SharedString::from(format!("Failed to prepare print: {}", e)),
                ),
                cx,
            );
            return;
        }
        if let Err(e) = open::that(&temp_path) {
            log::error!("Failed to open print file: {}", e);
            window.push_notification(
                (
                    NotificationType::Error,
                    SharedString::from(format!("Failed to open print dialog: {}", e)),
                ),
                cx,
            );
        }
    }

    /// Handle pending jump-to-line action
    ///
    /// ### Arguments
    /// - `window`: The window containing the editor
    /// - `cx`: The application context
    pub fn handle_pending_jump_to_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(jump) = self.pending_jump.take()
            && let Some(index) = self.active_tab_index
            && let Some(Tab::Editor(editor_tab)) = self.tabs.get_mut(index)
        {
            editor_tab.jump_to_line(window, cx, jump);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::detect_encoding_and_decode;
    use crate::fulgur::ui::components_utils::UTF_8;

    // ========== detect_encoding_and_decode tests ==========

    #[test]
    fn test_detect_encoding_returns_utf8_for_valid_utf8_text() {
        let text = "Hello, world! Fulgur rocks.";
        let (encoding, decoded) = detect_encoding_and_decode(text.as_bytes());
        assert_eq!(encoding, UTF_8);
        assert_eq!(decoded, text);
    }

    #[test]
    fn test_detect_encoding_returns_utf8_for_ascii_content() {
        let text = "fn main() { println!(\"hi\"); }";
        let (encoding, decoded) = detect_encoding_and_decode(text.as_bytes());
        assert_eq!(encoding, UTF_8);
        assert_eq!(decoded, text);
    }

    #[test]
    fn test_detect_encoding_detects_non_utf8_encoding() {
        // 0xE9 is 'é' in Latin-1 but not a valid UTF-8 byte sequence on its own
        let bytes: &[u8] = &[0x63, 0x61, 0x66, 0xE9]; // "café" in Latin-1
        let (encoding, decoded) = detect_encoding_and_decode(bytes);
        assert_ne!(encoding, UTF_8);
        assert!(!decoded.is_empty());
    }

    // ========== GPUI-backed tests ==========

    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::{
        Fulgur, editor_tab::TabLocation, settings::Settings, shared_state::SharedAppState,
        tab::Tab, window_manager::WindowManager,
    };
    #[cfg(feature = "gpui-test-support")]
    use gpui::{
        AppContext, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext,
        Window, div,
    };
    #[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
    use gpui::{BorrowAppContext, WindowId};
    #[cfg(feature = "gpui-test-support")]
    use parking_lot::Mutex;
    #[cfg(feature = "gpui-test-support")]
    use std::{cell::RefCell, path::PathBuf, sync::Arc};
    #[cfg(feature = "gpui-test-support")]
    use tempfile::TempDir;

    #[cfg(feature = "gpui-test-support")]
    struct EmptyView;

    #[cfg(feature = "gpui-test-support")]
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
    #[cfg(feature = "gpui-test-support")]
    fn temp_test_path(file_name: &str) -> PathBuf {
        std::env::temp_dir().join(file_name)
    }

    #[cfg(feature = "gpui-test-support")]
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

    #[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
    fn setup_test_globals(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let mut settings = Settings::new();
            settings.editor_settings.watch_files = false;
            let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
            cx.set_global(SharedAppState::new(settings, pending_files));
            cx.set_global(WindowManager::new());
        });
    }

    #[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
    fn open_window_with_fulgur(cx: &mut TestAppContext) -> (WindowId, Entity<Fulgur>) {
        let window_id_slot: RefCell<Option<WindowId>> = RefCell::new(None);
        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        cx.update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
                let window_id = window.window_handle().window_id();
                let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                *window_id_slot.borrow_mut() = Some(window_id);
                *fulgur_slot.borrow_mut() = Some(fulgur.clone());
                cx.new(|_| EmptyView)
            })
            .expect("failed to open test window");
        });
        (
            window_id_slot
                .into_inner()
                .expect("failed to capture test window id"),
            fulgur_slot
                .into_inner()
                .expect("failed to capture test Fulgur entity"),
        )
    }

    #[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
    fn invoke_process_pending_files_from_macos(
        cx: &mut TestAppContext,
        window_id: WindowId,
        fulgur: &Entity<Fulgur>,
    ) {
        cx.update(|cx| {
            for handle in cx.windows() {
                if handle.window_id() == window_id {
                    handle
                        .update(cx, |_, window, cx| {
                            fulgur.update(cx, |this, cx| {
                                this.process_pending_files_from_macos(window, cx);
                            });
                        })
                        .expect("failed to run process_pending_files_from_macos on test window");
                    return;
                }
            }
            panic!("failed to locate target test window by id");
        });
    }

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

    // ========== save_file tests ==========

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_save_file_writes_content_to_disk(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("save_test.txt");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(Tab::Editor(editor_tab)) = this.tabs.last_mut() {
                    editor_tab.location = TabLocation::Local(path.clone());
                }
                this.save_file(window, cx);
            });
        });

        assert!(path.exists(), "file should exist after save_file");
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_save_file_marks_tab_as_not_modified(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("mark_saved_test.txt");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(Tab::Editor(editor_tab)) = this.tabs.last_mut() {
                    editor_tab.location = TabLocation::Local(path.clone());
                    editor_tab.modified = true;
                }
                this.save_file(window, cx);
                let modified = this
                    .tabs
                    .last()
                    .and_then(|t| t.as_editor())
                    .map(|e| e.modified)
                    .unwrap_or(true);
                assert!(!modified, "tab should be marked as not modified after save");
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_save_file_is_noop_when_no_active_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.active_tab_index = None;
                this.save_file(window, cx); // Must not panic
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
