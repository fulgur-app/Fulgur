use gpui::*;
use gpui_component::{highlighter::Language, select::SelectState};

use crate::fulgur::{Fulgur, components_utils::create_select_state};

/// Initialize the language registry with all supported languages
pub fn init_languages() {}

/// Get the Language enum from a file extension. Checks if the extension is a known language, if not, it returns Plain.
///
/// @param extension: The file extension
///
/// @return: The Language enum
pub fn language_from_extension(extension: &str) -> Language {
    if extension == "txt" {
        return Language::Plain;
    }
    let mut language = Language::from_str(extension);
    // If the extension is not a known language, try to match it to a known language
    if language == Language::Plain {
        language = match extension {
            "astro" => Language::Html,
            "mjs" => Language::JavaScript,
            _ => Language::Plain,
        };
    }
    language
}

/// Get the language name as a string
///
/// @param language: The Language enum
///
/// @return: The language name as a string
pub fn language_name(language: &Language) -> &'static str {
    language.name()
}

/// Get the pretty name of a language
///
/// @param language: The Language enum
///
/// @return: The pretty name of the language
pub fn pretty_name(language: Language) -> String {
    let language_pretty = match language {
        Language::Plain => "Text",
        Language::Bash => "Bash",
        Language::C => "C",
        Language::CMake => "CMake",
        Language::CSharp => "C#",
        Language::Cpp => "C++",
        Language::Css => "CSS",
        Language::Diff => "Diff",
        Language::Ejs => "EJS",
        Language::Elixir => "Elixir",
        Language::Erb => "ERB",
        Language::Go => "Go",
        Language::GraphQL => "GraphQL",
        Language::Html => "HTML",
        Language::Java => "Java",
        Language::JavaScript => "JavaScript",
        Language::JsDoc => "JSdoc",
        Language::Json => "JSON",
        Language::Make => "Make",
        Language::Markdown => "Markdown",
        Language::MarkdownInline => "Markdown Inline",
        Language::Proto => "Proto",
        Language::Python => "Python",
        Language::Ruby => "Ruby",
        Language::Rust => "Rust",
        Language::Scala => "Scala",
        Language::Sql => "SQL",
        Language::Swift => "Swift",
        Language::Toml => "Toml",
        Language::Tsx => "TSX",
        Language::TypeScript => "TypeScript",
        Language::Yaml => "YAML",
        Language::Zig => "Zig",
    };
    language_pretty.to_string()
}

/// Get the Language enum from a pretty name (e.g., "JavaScript" -> Language::JavaScript)
///
/// @param pretty_name_str: The pretty name of the language
///
/// @return: The Language enum, or Language::Plain if not found
pub fn language_from_pretty_name(pretty_name_str: &str) -> Language {
    Language::all()
        .find(|&lang| pretty_name(lang) == pretty_name_str)
        .unwrap_or(Language::Plain)
}

/// Create the all languages select state
///
/// @param current_language: The current language
///
/// @param window: The window
///
/// @param cx: The app context
///
/// @return: The select state entity
pub fn create_all_languages_select_state(
    current_language: String,
    window: &mut Window,
    cx: &mut App,
) -> Entity<SelectState<Vec<SharedString>>> {
    let languages = all_languages();
    create_select_state(window, current_language, languages, cx)
}

/// Get all languages as a vector of SharedString
///
/// @return: The vector of SharedString
fn all_languages() -> Vec<SharedString> {
    let mut languages = Language::all()
        .map(|language| SharedString::new(pretty_name(language).as_str()))
        .collect::<Vec<SharedString>>();
    languages.sort();
    languages
}

impl Fulgur {
    /// Get the current language of the active tab
    ///
    /// @return: The current language
    pub fn get_current_language(&self) -> Language {
        let current_tab_language = match self.active_tab_index {
            Some(index) => {
                if let Some(editor_tab) = self.tabs[index].as_editor() {
                    editor_tab.language
                } else {
                    Language::Plain
                }
            }
            None => Language::Plain,
        };
        current_tab_language
    }

    /// Check if the current active tab's language is a Markdown language
    ///
    /// @return: True if the current active tab's language is a Markdown language
    pub fn is_markdown(&self) -> bool {
        let current_language = self.get_current_language();
        current_language == Language::Markdown || current_language == Language::MarkdownInline
    }
}
