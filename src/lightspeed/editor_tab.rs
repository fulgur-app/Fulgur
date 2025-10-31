// Represents a single editor tab with its content
use gpui::*;
use gpui_component::input::{InputState, TabSize};

#[derive(Clone)]
pub struct EditorTab {
    pub id: usize,
    pub title: SharedString,
    pub content: Entity<InputState>,
    pub file_path: Option<std::path::PathBuf>,
    pub modified: bool,
    pub original_content: String,
    pub language: Language,
}

#[derive(Clone)]
pub enum Language {
    Rust,
    Python,
    Lua,
    Bash,
    JavaScript,
    TypeScript,
    JSON,
    YAML,
    TOML,
    PHP,
    C,
    CPP,
    CSharp,
    Text,
}

impl Language {
    // Convert an extension to a language
    // @param extension: The extension to convert
    // @return: The language
    fn from_extension(extension: &str) -> Self {
        match extension {
            "rs" => Self::Rust,
            "py" => Self::Python,
            "lua" => Self::Lua,
            "sh" => Self::Bash,
            "js" => Self::JavaScript,
            "ts" => Self::TypeScript,
            "json" => Self::JSON,
            "yaml" => Self::YAML,
            "yml" => Self::YAML,
            "toml" => Self::TOML,
            "php" => Self::PHP,
            "c" => Self::C,
            "cpp" => Self::CPP,
            "cs" => Self::CSharp,
            "_" => Self::Text,
            _ => Self::Text,
        }
    }

    // Convert the language to a string
    // @return: The string representation of the language
    fn to_string(&self) -> String {
        match self {
            Self::Rust => String::from("rust"),
            Self::Python => String::from("python"),
            Self::Lua => String::from("lua"),
            Self::Bash => String::from("bash"),
            Self::JavaScript => String::from("javascript"),
            Self::TypeScript => String::from("typescript"),
            Self::JSON => String::from("json"),
            Self::YAML => String::from("yaml"),
            Self::TOML => String::from("toml"),
            Self::PHP => String::from("php"),
            Self::C => String::from("c"),
            Self::CPP => String::from("cpp"),
            Self::CSharp => String::from("csharp"),
            Self::Text => String::from("text"),
        }
    }
}

// Create a new input state
// @param window: The window to create the input state in
// @param cx: The application context
// @param language: The language of the input state
// @param content: The content of the input state
// @return: The new input state
fn make_input_state(window: &mut Window, cx: &mut Context<InputState>, language: Language, content: Option<String>) -> InputState {
    InputState::new(window, cx)
        .code_editor(language.to_string())
        .line_number(true)
        .indent_guides(true)
        .tab_size(TabSize {
            tab_size: 4,
            hard_tabs: false,
        })
        .soft_wrap(false)
        .default_value(content.unwrap_or_default())
}

impl EditorTab {
    // Create a new tab
    // @param id: The ID of the tab
    // @param title: The title of the tab
    // @param window: The window to create the tab in
    // @param cx: The application context
    // @return: The new tab
    pub fn new(id: usize, title: impl Into<SharedString>, window: &mut Window, cx: &mut App) -> Self {
        let language = Language::Text;
        let content = cx.new(|cx| {
            make_input_state(window, cx, language.clone(), None)
        });
        
        Self {
            id,
            title: title.into(),
            content,
            file_path: None,
            modified: false,
            original_content: String::new(),
            language,
        }
    }

    // Create a new tab from a file
    // @param id: The ID of the tab
    // @param path: The path of the file
    // @param contents: The contents of the file
    // @param window: The window to create the tab in
    // @param cx: The application context
    // @return: The new tab
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

        let language = Language::from_extension(&file_name);
        let content = cx.new(|cx| {
            make_input_state(window, cx, language.clone(), Some(contents.clone()))
        });

        Self {
            id,
            title: file_name.into(),
            content,
            file_path: Some(path),
            modified: false,
            original_content: contents,
            language,
        }
    }
    
    // Check if the tab's content has been modified
    // @param cx: The application context
    // @return: True if the tab's content has been modified, false otherwise
    pub fn check_modified(&mut self, cx: &mut App) -> bool {
        let current_text = self.content.read(cx).text().to_string();
        self.modified = current_text != self.original_content;
        self.modified
    }
    
    // Mark the tab as saved
    // @param cx: The application context
    pub fn mark_as_saved(&mut self, cx: &mut App) {
        self.original_content = self.content.read(cx).text().to_string();
        self.modified = false;
    }
}
