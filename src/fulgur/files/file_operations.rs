use std::path::PathBuf;

use crate::fulgur::{
    Fulgur,
    editor_tab::{EditorTab, FromFileParams},
    tab::Tab,
    ui::components_utils::{UNTITLED, UTF_8},
    ui::menus,
};
use chardetng::EncodingDetector;
use gpui::*;
use gpui_component::{WindowExt, notification::NotificationType};

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
                if let Some(ref tab_path) = editor_tab.file_path {
                    tab_path == path
                } else {
                    false
                }
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
            editor_tab.file_path.clone()
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
                        editor_tab.original_content = contents;
                        editor_tab.encoding = encoding;
                        editor_tab.modified = false;
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
                    this.tabs.push(Tab::Editor(editor_tab));
                    this.active_tab_index = Some(this.tabs.len() - 1);
                    this.next_tab_id += 1;
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
                    cx.set_menus(menus);
                    let title = path
                        .file_name()
                        .map(|file_name| file_name.to_string_lossy().to_string());
                    this.set_title(title, cx);
                    log::debug!("File opened successfully in new tab: {:?}", path);
                    let _ = this.save_state(cx, window);
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

    /// Save a file
    ///
    /// ### Arguments
    /// - `window`: The window to save the file in
    /// - `cx`: The application context
    pub fn save_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() || self.active_tab_index.is_none() {
            return;
        }
        let active_tab = &self.tabs[self.active_tab_index.unwrap()];
        let (path, content_entity) = match active_tab {
            Tab::Editor(editor_tab) => {
                if editor_tab.file_path.is_none() {
                    self.save_file_as(window, cx);
                    return;
                }
                (
                    editor_tab.file_path.clone().unwrap(),
                    editor_tab.content.clone(),
                )
            }
            Tab::Settings(_) => return,
        };
        let contents = content_entity.read(cx).text().to_string();
        log::debug!("Saving file: {:?} ({} bytes)", path, contents.len());
        if let Err(e) = std::fs::write(&path, contents) {
            log::debug!("Failed to save file {:?}: {}", path, e);
            return;
        }
        log::debug!("File saved successfully: {:?}", path);
        self.last_file_saves
            .insert(path.clone(), std::time::Instant::now());
        if let Tab::Editor(editor_tab) = &mut self.tabs[self.active_tab_index.unwrap()] {
            editor_tab.mark_as_saved(cx);
        }
        cx.notify();
    }

    /// Save a file as
    ///
    /// ### Arguments
    /// - `window`: The window to save the file as in
    /// - `cx`: The application context
    pub fn save_file_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() || self.active_tab_index.is_none() {
            return;
        }
        let active_tab_index = self.active_tab_index;
        let (content_entity, directory, suggested_filename) =
            match &self.tabs[active_tab_index.unwrap()] {
                Tab::Editor(editor_tab) => {
                    let dir = if let Some(ref path) = editor_tab.file_path {
                        path.parent()
                            .unwrap_or(std::path::Path::new("."))
                            .to_path_buf()
                    } else {
                        std::env::current_dir().unwrap_or_default()
                    };
                    let suggested = editor_tab.get_suggested_filename();
                    (editor_tab.content.clone(), dir, suggested)
                }
                Tab::Settings(_) => return,
            };
        let path_future = cx.prompt_for_new_path(&directory, suggested_filename.as_deref());
        cx.spawn_in(window, async move |view, window| {
            let path = path_future.await.ok()?.ok()??;
            let contents = window
                .update(|_, cx| content_entity.read(cx).text().to_string())
                .ok()?;
            log::debug!("Saving file as: {:?} ({} bytes)", path, contents.len());
            if let Err(e) = std::fs::write(&path, &contents) {
                log::error!("Failed to save file {:?}: {}", path, e);
                return None;
            }
            log::debug!("File saved successfully as: {:?}", path);
            window
                .update(|window, cx| {
                    _ = view.update(cx, |this, cx| {
                        let old_path = if let Some(Tab::Editor(editor_tab)) =
                            this.tabs.get(active_tab_index.unwrap())
                        {
                            editor_tab.file_path.clone()
                        } else {
                            None
                        };
                        if let Some(old_path) = old_path {
                            this.unwatch_file(&old_path);
                        }
                        this.last_file_saves
                            .insert(path.clone(), std::time::Instant::now());
                        if let Some(Tab::Editor(editor_tab)) =
                            this.tabs.get_mut(active_tab_index.unwrap())
                        {
                            editor_tab.file_path = Some(path.clone());
                            editor_tab.title = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or(UNTITLED)
                                .to_string()
                                .into();
                            editor_tab.mark_as_saved(cx);
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
}
