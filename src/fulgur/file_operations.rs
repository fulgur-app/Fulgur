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
        return (UTF_8.to_string(), text.to_string());
    }
    let mut detector = EncodingDetector::new();
    detector.feed(bytes, true);
    let encoding = detector.guess(None, true);
    let (decoded, _, had_errors) = encoding.decode(bytes);
    let encoding_name = if had_errors {
        match std::str::from_utf8(bytes) {
            Ok(text) => return (UTF_8.to_string(), text.to_string()),
            Err(_) => {
                let text = String::from_utf8_lossy(bytes).to_string();
                return (UTF_8.to_string(), text);
            }
        }
    } else {
        encoding.name().to_string()
    };
    (encoding_name, decoded.to_string())
}

impl Fulgur {
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
            let bytes = std::fs::read(&path).ok()?;
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
                        if let Err(e) = this.settings.add_file(path) {
                            eprintln!("Failed to add file to recent files: {}", e);
                        }
                        let menus = menus::build_menus(cx, &this.settings.get_recent_files());
                        cx.set_menus(menus);
                        cx.notify();
                    });
                })
                .ok();
            Some(())
        })
        .detach();
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
        if let Err(e) = std::fs::write(&path, contents) {
            eprintln!("Failed to save file: {}", e);
            return;
        }
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
            if let Err(e) = std::fs::write(&path, &contents) {
                eprintln!("Failed to save file: {}", e);
                return None;
            }
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
