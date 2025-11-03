use crate::lightspeed::{Lightspeed, editor_tab::EditorTab, state_persistence::*};
use gpui::*;
use std::fs;

impl Lightspeed {
    // Save the current app state to disk
    // @param cx: The application context
    // @return: The result of the save operation
    pub fn save_state(&self, cx: &App) -> Result<(), Box<dyn std::error::Error>> {
        let mut tab_states = Vec::new();

        for tab in &self.tabs {
            let current_content = tab.content.read(cx).text().to_string();
            let is_modified = current_content != tab.original_content;

            if tab.file_path.is_none() && current_content.is_empty() {
                continue;
            }

            let tab_state = if let Some(ref path) = tab.file_path {
                if is_modified {
                    TabState {
                        id: tab.id,
                        title: tab.title.to_string(),
                        file_path: Some(path.clone()),
                        content: Some(current_content),
                        last_saved: get_file_modified_time(path),
                    }
                } else {
                    TabState {
                        id: tab.id,
                        title: tab.title.to_string(),
                        file_path: Some(path.clone()),
                        content: None,
                        last_saved: None,
                    }
                }
            } else {
                TabState {
                    id: tab.id,
                    title: tab.title.to_string(),
                    file_path: None,
                    content: Some(current_content),
                    last_saved: None,
                }
            };

            tab_states.push(tab_state);
        }

        let app_state = AppState {
            tabs: tab_states,
            active_tab_index: self.active_tab_index,
            next_tab_id: self.next_tab_id,
        };

        app_state.save()?;
        Ok(())
    }

    // Load app state from disk and restore tabs
    // @param window: The window to load the state from
    // @param cx: The application context
    pub fn load_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(app_state) = AppState::load() {
            self.tabs.clear();

            for tab_state in app_state.tabs {
                let tab = self.restore_tab_from_state(tab_state, window, cx);
                if let Some(tab) = tab {
                    self.tabs.push(tab);
                }
            }

            if let Some(index) = app_state.active_tab_index {
                if index < self.tabs.len() {
                    self.active_tab_index = Some(index);
                } else if !self.tabs.is_empty() {
                    self.active_tab_index = Some(0);
                } else {
                    self.active_tab_index = None;
                }
            }

            self.next_tab_id = app_state.next_tab_id;

            if self.tabs.is_empty() {
                let initial_tab = EditorTab::new(0, "Untitled", window, cx);
                self.tabs.push(initial_tab);
                self.active_tab_index = Some(0);
                self.next_tab_id = 1;
            }

            cx.notify();
        }
    }

    // Restore a single tab from saved state:
    // - If a file exists, it will be loaded from the file.
    // - If a file exists and was modified but unsaved in the last state save and not modified after externally, it'll be loaded from the saved state.
    // - If a file does not exist, the saved content will be used.
    // - If no path and no content is provided, the tab will be skipped.
    // @param tab_state: The saved state of the tab
    // @param window: The window to restore the tab to
    // @param cx: The application context
    // @return: The restored tab
    fn restore_tab_from_state(
        &self,
        tab_state: TabState,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<EditorTab> {
        let (content, path) = if let Some(saved_path) = tab_state.file_path {
            if let Some(saved_content) = tab_state.content {
                if saved_path.exists() {
                    if let Some(ref saved_time) = tab_state.last_saved {
                        if let Some(file_time) = get_file_modified_time(&saved_path) {
                            if is_file_newer(&file_time, saved_time) {
                                if let Ok(file_content) = fs::read_to_string(&saved_path) {
                                    (file_content, Some(saved_path))
                                } else {
                                    (saved_content, Some(saved_path))
                                }
                            } else {
                                (saved_content, Some(saved_path))
                            }
                        } else {
                            (saved_content, Some(saved_path))
                        }
                    } else {
                        (saved_content, Some(saved_path))
                    }
                } else {
                    (saved_content, None)
                }
            } else {
                if saved_path.exists() {
                    if let Ok(file_content) = fs::read_to_string(&saved_path) {
                        (file_content, Some(saved_path))
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
        } else {
            if let Some(saved_content) = tab_state.content {
                (saved_content, None)
            } else {
                return None;
            }
        };

        let tab = if let Some(file_path) = path {
            EditorTab::from_file(tab_state.id, file_path, content, window, cx)
        } else {
            use gpui_component::highlighter::Language;
            use gpui_component::input::TabSize;

            let content_entity = cx.new(|cx| {
                gpui_component::input::InputState::new(window, cx)
                    .code_editor("Plain".to_string())
                    .line_number(true)
                    .indent_guides(true)
                    .tab_size(TabSize {
                        tab_size: 4,
                        hard_tabs: false,
                    })
                    .soft_wrap(false)
                    .default_value(content)
            });

            EditorTab {
                id: tab_state.id,
                title: tab_state.title.into(),
                content: content_entity,
                file_path: None,
                modified: true,
                original_content: String::new(),
                language: Language::Plain,
            }
        };

        Some(tab)
    }
}
