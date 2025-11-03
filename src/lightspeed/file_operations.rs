use crate::lightspeed::{Lightspeed, editor_tab::EditorTab, tab::Tab};
use chardetng::EncodingDetector;
use gpui::*;

/// Detect encoding from file bytes
/// @param bytes: The bytes to detect encoding from
/// @return: The detected encoding and decoded string
pub fn detect_encoding_and_decode(bytes: &[u8]) -> (String, String) {
    // Try UTF-8 first
    if let Ok(text) = std::str::from_utf8(bytes) {
        return ("UTF-8".to_string(), text.to_string());
    }

    // Use chardetng to detect encoding
    let mut detector = EncodingDetector::new();
    detector.feed(bytes, true);
    let encoding = detector.guess(None, true);

    // Decode the bytes using the detected encoding
    let (decoded, _, had_errors) = encoding.decode(bytes);

    // If there were errors, try to use a more lenient approach
    let encoding_name = if had_errors {
        // If decoding had errors, fall back to UTF-8 with replacement
        match std::str::from_utf8(bytes) {
            Ok(text) => return ("UTF-8".to_string(), text.to_string()),
            Err(_) => {
                let text = String::from_utf8_lossy(bytes).to_string();
                return ("UTF-8".to_string(), text);
            }
        }
    } else {
        encoding.name().to_string()
    };

    (encoding_name, decoded.to_string())
}

impl Lightspeed {
    /// Open a file
    /// @param window: The window to open the file in
    /// @param cx: The application context
    pub(super) fn open_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path_future = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });

        cx.spawn_in(window, async move |view, window| {
            // Wait for the user to select a path
            let paths = path_future.await.ok()?.ok()??;
            let path = paths.first()?.clone();

            // Read file contents as bytes first
            let bytes = std::fs::read(&path).ok()?;

            // Detect encoding and decode
            let (encoding, contents) = detect_encoding_and_decode(&bytes);

            // Update the view to add a new tab with the file
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
                        );
                        this.tabs.push(Tab::Editor(editor_tab));
                        this.active_tab_index = Some(this.tabs.len() - 1);
                        this.next_tab_id += 1;
                        this.focus_active_tab(window, cx);
                        cx.notify();
                    });
                })
                .ok();

            Some(())
        })
        .detach();
    }

    /// Save a file
    /// @param window: The window to save the file in
    /// @param cx: The application context
    pub(super) fn save_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() || self.active_tab_index.is_none() {
            return;
        }

        let active_tab = &self.tabs[self.active_tab_index.unwrap()];

        // Only save editor tabs
        let (path, content_entity) = match active_tab {
            Tab::Editor(editor_tab) => {
                // If no path exists, use save_as instead
                if editor_tab.file_path.is_none() {
                    self.save_file_as(window, cx);
                    return;
                }
                (
                    editor_tab.file_path.clone().unwrap(),
                    editor_tab.content.clone(),
                )
            }
            Tab::Settings(_) => return, // Can't save settings tabs
        };

        // Get the text content from the InputState
        let contents = content_entity.read(cx).text().to_string();

        // Write to file
        if let Err(e) = std::fs::write(&path, contents) {
            eprintln!("Failed to save file: {}", e);
            return;
        }

        // Mark as saved
        if let Tab::Editor(editor_tab) = &mut self.tabs[self.active_tab_index.unwrap()] {
            editor_tab.mark_as_saved(cx);
        }
        cx.notify();
    }

    /// Save a file as
    /// @param window: The window to save the file as in
    /// @param cx: The application context
    pub(super) fn save_file_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() || self.active_tab_index.is_none() {
            return;
        }

        let active_tab_index = self.active_tab_index;

        // Only save editor tabs
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
            Tab::Settings(_) => return, // Can't save settings tabs
        };

        let path_future = cx.prompt_for_new_path(&directory, None);

        cx.spawn_in(window, async move |view, window| {
            // Wait for the user to select a path
            let path = path_future.await.ok()?.ok()??;

            // Get the text content
            let contents = window
                .update(|_, cx| content_entity.read(cx).text().to_string())
                .ok()?;

            // Write to file
            if let Err(e) = std::fs::write(&path, &contents) {
                eprintln!("Failed to save file: {}", e);
                return None;
            }

            // Update the tab with the new path
            window
                .update(|_, cx| {
                    _ = view.update(cx, |this, cx| {
                        if let Some(Tab::Editor(editor_tab)) =
                            this.tabs.get_mut(active_tab_index.unwrap())
                        {
                            editor_tab.file_path = Some(path.clone());
                            editor_tab.title = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("Untitled")
                                .to_string()
                                .into();
                            editor_tab.mark_as_saved(cx);
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
