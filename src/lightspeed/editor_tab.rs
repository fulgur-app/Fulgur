// Represents a single editor tab with its content
use gpui::*;
use gpui_component::input::InputState;

#[derive(Clone)]
pub struct EditorTab {
    pub id: usize,
    pub title: SharedString,
    pub content: Entity<InputState>,
    pub file_path: Option<std::path::PathBuf>,
    pub modified: bool,
    pub original_content: String,
}

impl EditorTab {
    pub fn new(id: usize, title: impl Into<SharedString>, window: &mut Window, cx: &mut App) -> Self {
        let content = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line()
                .placeholder("Start typing...")
        });
        
        Self {
            id,
            title: title.into(),
            content,
            file_path: None,
            modified: false,
            original_content: String::new(),
        }
    }

    pub fn from_file(
        id: usize,
        path: std::path::PathBuf,
        contents: String,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Untitled")
            .to_string();

        let content = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line()
                .default_value(contents.clone())
        });

        Self {
            id,
            title: file_name.into(),
            content,
            file_path: Some(path),
            modified: false,
            original_content: contents,
        }
    }
    
    pub fn check_modified(&mut self, cx: &mut App) -> bool {
        let current_text = self.content.read(cx).text().to_string();
        self.modified = current_text != self.original_content;
        self.modified
    }
    
    pub fn mark_as_saved(&mut self, cx: &mut App) {
        self.original_content = self.content.read(cx).text().to_string();
        self.modified = false;
    }
}
