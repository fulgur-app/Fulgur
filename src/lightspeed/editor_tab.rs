// Represents a single editor tab with its content
use gpui::*;
use gpui_component::input::InputState;

#[derive(Clone)]
pub struct EditorTab {
    pub id: usize,
    pub title: SharedString,
    pub content: Entity<InputState>,
    pub _file_path: Option<std::path::PathBuf>,
    pub _modified: bool,
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
            _file_path: None,
            _modified: false,
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
                .default_value(contents)
        });

        Self {
            id,
            title: file_name.into(),
            content,
            _file_path: Some(path),
            _modified: false,
        }
    }
}
