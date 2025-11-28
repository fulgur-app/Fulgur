use std::path::PathBuf;

use crate::fulgur::{
    Fulgur,
    components_utils::{UNTITLED, UTF_8},
    editor_tab::EditorTab,
    menus,
    tab::Tab,
};
use chardetng::EncodingDetector;
use gpui::*;

// Detect encoding from file bytes
// @param bytes: The bytes to detect encoding from
// @return: The detected encoding and decoded string
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
    // Find the index of a tab with the given file path
    // @param path: The path to search for
    // @return: The index of the tab if found, None otherwise
    fn find_tab_by_path(&self, path: &PathBuf) -> Option<usize> {
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

    // Reload tab content from disk
    // @param tab_index: The index of the tab to reload
    // @param window: The window context
    // @param cx: The application context
    fn reload_tab_from_disk(
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

    // Internal helper function to open a file from a path
    // This function handles reading the file, detecting encoding, and creating the editor tab
    // @param view: The view entity (WeakEntity)
    // @param window: The async window context
    // @param path: The path to the file to open
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
                        this.next_tab_id,
                        path.clone(),
                        contents,
                        encoding,
                        window,
                        cx,
                        &this.settings.editor_settings,
                        false,
                    );
                    this.tabs.push(Tab::Editor(editor_tab));
                    this.active_tab_index = Some(this.tabs.len() - 1);
                    this.next_tab_id += 1;
                    this.focus_active_tab(window, cx);
                    if let Err(e) = this.settings.add_file(path.clone()) {
                        log::error!("Failed to add file to recent files: {}", e);
                    }
                    let menus = menus::build_menus(cx, &this.settings.get_recent_files());
                    cx.set_menus(menus);
                    let title = match path.file_name() {
                        Some(file_name) => Some(file_name.to_string_lossy().to_string()),
                        None => None,
                    };
                    this.set_title(title, cx);
                    log::debug!("File opened successfully in new tab: {:?}", path);
                    cx.notify();
                });
            })
            .ok();
        Some(())
    }

    // Open a file
    // @param window: The window to open the file in
    // @param cx: The application context
    pub(super) fn open_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path_future = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |view, window| {
            let paths = path_future.await.ok()?.ok()??;
            let path = paths.first()?.clone();
            Self::open_file_from_path(view, window, path).await
        })
        .detach();
    }

    // Open a file from a given path
    // @param window: The window to open the file in
    // @param cx: The application context
    // @param path: The path to the file to open
    pub(super) fn do_open_file(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        path: PathBuf,
    ) {
        cx.spawn_in(window, async move |view, window| {
            Self::open_file_from_path(view, window, path).await
        })
        .detach();
    }

    // Handle opening a file from the command line (double-click or "Open with")
    // This method implements smart tab handling:
    // - If a tab exists for the file and is not modified: focus the tab
    // - If a tab exists for the file and is modified: reload content and focus the tab
    // - If no tab exists: open a new tab and focus it
    // @param window: The window to open the file in
    // @param cx: The application context
    // @param path: The path to the file to open
    pub fn handle_open_file_from_cli(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        path: PathBuf,
    ) {
        log::debug!("Handling file open from CLI: {:?}", path);
        if let Some(tab_index) = self.find_tab_by_path(&path) {
            log::debug!("Tab already exists for {:?} at index {}", path, tab_index);
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
        } else {
            log::debug!("No existing tab found, opening new tab for {:?}", path);
            self.do_open_file(window, cx, path);
        }
    }

    // Save a file
    // @param window: The window to save the file in
    // @param cx: The application context
    pub(super) fn save_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        if let Tab::Editor(editor_tab) = &mut self.tabs[self.active_tab_index.unwrap()] {
            editor_tab.mark_as_saved(cx);
        }
        cx.notify();
    }

    // Save a file as
    // @param window: The window to save the file as in
    // @param cx: The application context
    pub(super) fn save_file_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() || self.active_tab_index.is_none() {
            return;
        }
        let active_tab_index = self.active_tab_index;
        let (content_entity, directory) = match &self.tabs[active_tab_index.unwrap()] {
            Tab::Editor(editor_tab) => {
                let dir = if let Some(ref path) = editor_tab.file_path {
                    path.parent()
                        .unwrap_or(std::path::Path::new("."))
                        .to_path_buf()
                } else {
                    std::env::current_dir().unwrap_or_default()
                };
                (editor_tab.content.clone(), dir)
            }
            Tab::Settings(_) => return,
        };
        let path_future = cx.prompt_for_new_path(&directory, None);
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
                    });
                })
                .ok()?;
            Some(())
        })
        .detach();
    }
}
