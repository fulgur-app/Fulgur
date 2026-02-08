use gpui::*;
use gpui_component::highlighter::Language;

use crate::fulgur::Fulgur;

/// Initialize the language registry with all supported languages
pub fn init_languages() {}

/// Lists all supported languages, including some that are not supported by the language registry but are close enough.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SupportedLanguage {
    Astro,
    Bash,
    C,
    CMake,
    CSharp,
    Cpp,
    Css,
    Diff,
    Ejs,
    Elixir,
    Erb,
    Go,
    GraphQl,
    Html,
    Java,
    JavaScript,
    JsDoc,
    Json,
    Make,
    Markdown,
    MarkdownInline,
    Php,
    Plain,
    Proto,
    Python,
    Ruby,
    Rust,
    Scala,
    Sql,
    Svelte,
    Svg,
    Swift,
    Toml,
    Tsx,
    TypeScript,
    Vue,
    Yaml,
    Zig,
}

/// Convert a Language to a SupportedLanguage
///
/// ### Arguments
/// - `language`: The Language enum
///
/// ### Returns
/// - `SupportedLanguage`: The corresponding SupportedLanguage
pub fn from_language(language: Language) -> SupportedLanguage {
    match language {
        Language::Bash => SupportedLanguage::Bash,
        Language::C => SupportedLanguage::C,
        Language::CMake => SupportedLanguage::CMake,
        Language::CSharp => SupportedLanguage::CSharp,
        Language::Cpp => SupportedLanguage::Cpp,
        Language::Css => SupportedLanguage::Css,
        Language::Diff => SupportedLanguage::Diff,
        Language::Ejs => SupportedLanguage::Ejs,
        Language::Elixir => SupportedLanguage::Elixir,
        Language::Erb => SupportedLanguage::Erb,
        Language::Go => SupportedLanguage::Go,
        Language::GraphQL => SupportedLanguage::GraphQl,
        Language::Html => SupportedLanguage::Html,
        Language::Java => SupportedLanguage::Java,
        Language::JavaScript => SupportedLanguage::JavaScript,
        Language::JsDoc => SupportedLanguage::JsDoc,
        Language::Json => SupportedLanguage::Json,
        Language::Make => SupportedLanguage::Make,
        Language::Markdown => SupportedLanguage::Markdown,
        Language::MarkdownInline => SupportedLanguage::MarkdownInline,
        Language::Plain => SupportedLanguage::Plain,
        Language::Proto => SupportedLanguage::Proto,
        Language::Python => SupportedLanguage::Python,
        Language::Ruby => SupportedLanguage::Ruby,
        Language::Rust => SupportedLanguage::Rust,
        Language::Scala => SupportedLanguage::Scala,
        Language::Sql => SupportedLanguage::Sql,
        Language::Swift => SupportedLanguage::Swift,
        Language::Toml => SupportedLanguage::Toml,
        Language::Tsx => SupportedLanguage::Tsx,
        Language::TypeScript => SupportedLanguage::TypeScript,
        Language::Yaml => SupportedLanguage::Yaml,
        Language::Zig => SupportedLanguage::Zig,
    }
}

/// Convert a supported language to a language
///
/// ### Arguments
/// - `supported_language`: The supported language to convert
///
/// ### Returns
/// The corresponding language
pub fn to_language(supported_language: &SupportedLanguage) -> Language {
    match supported_language {
        SupportedLanguage::Astro => Language::Html,
        SupportedLanguage::Bash => Language::Bash,
        SupportedLanguage::C => Language::C,
        SupportedLanguage::CMake => Language::CMake,
        SupportedLanguage::CSharp => Language::CSharp,
        SupportedLanguage::Cpp => Language::Cpp,
        SupportedLanguage::Css => Language::Css,
        SupportedLanguage::Diff => Language::Diff,
        SupportedLanguage::Ejs => Language::Ejs,
        SupportedLanguage::Elixir => Language::Elixir,
        SupportedLanguage::Erb => Language::Erb,
        SupportedLanguage::Go => Language::Go,
        SupportedLanguage::GraphQl => Language::GraphQL,
        SupportedLanguage::Html => Language::Html,
        SupportedLanguage::Java => Language::Java,
        SupportedLanguage::JavaScript => Language::JavaScript,
        SupportedLanguage::JsDoc => Language::JsDoc,
        SupportedLanguage::Json => Language::Json,
        SupportedLanguage::Make => Language::Make,
        SupportedLanguage::Markdown => Language::Markdown,
        SupportedLanguage::MarkdownInline => Language::MarkdownInline,
        SupportedLanguage::Php => Language::Html,
        SupportedLanguage::Plain => Language::Plain,
        SupportedLanguage::Proto => Language::Proto,
        SupportedLanguage::Python => Language::Python,
        SupportedLanguage::Ruby => Language::Ruby,
        SupportedLanguage::Rust => Language::Rust,
        SupportedLanguage::Scala => Language::Scala,
        SupportedLanguage::Sql => Language::Sql,
        SupportedLanguage::Svelte => Language::TypeScript,
        SupportedLanguage::Svg => Language::Html,
        SupportedLanguage::Swift => Language::Swift,
        SupportedLanguage::Toml => Language::Toml,
        SupportedLanguage::Tsx => Language::TypeScript,
        SupportedLanguage::TypeScript => Language::TypeScript,
        SupportedLanguage::Vue => Language::TypeScript,
        SupportedLanguage::Yaml => Language::Yaml,
        SupportedLanguage::Zig => Language::Zig,
    }
}

/// Get the pretty name (human-readable) of a SupportedLanguage
///
/// ### Arguments
/// - `language`: The SupportedLanguage enum
///
/// ### Returns
/// - `String`: The human-readable name of the language
pub fn pretty_name(language: &SupportedLanguage) -> String {
    match language {
        SupportedLanguage::Astro => "Astro".to_string(),
        SupportedLanguage::Bash => "Bash".to_string(),
        SupportedLanguage::C => "C".to_string(),
        SupportedLanguage::CMake => "CMake".to_string(),
        SupportedLanguage::CSharp => "C#".to_string(),
        SupportedLanguage::Cpp => "C++".to_string(),
        SupportedLanguage::Css => "CSS".to_string(),
        SupportedLanguage::Diff => "Diff".to_string(),
        SupportedLanguage::Ejs => "EJS".to_string(),
        SupportedLanguage::Elixir => "Elixir".to_string(),
        SupportedLanguage::Erb => "ERB".to_string(),
        SupportedLanguage::Go => "Go".to_string(),
        SupportedLanguage::GraphQl => "GraphQL".to_string(),
        SupportedLanguage::Html => "HTML".to_string(),
        SupportedLanguage::Java => "Java".to_string(),
        SupportedLanguage::JavaScript => "JavaScript".to_string(),
        SupportedLanguage::JsDoc => "JSDoc".to_string(),
        SupportedLanguage::Json => "JSON".to_string(),
        SupportedLanguage::Make => "Make".to_string(),
        SupportedLanguage::Markdown => "Markdown".to_string(),
        SupportedLanguage::MarkdownInline => "Markdown Inline".to_string(),
        SupportedLanguage::Php => "PHP".to_string(),
        SupportedLanguage::Plain => "Plain Text".to_string(),
        SupportedLanguage::Proto => "Protocol Buffers".to_string(),
        SupportedLanguage::Python => "Python".to_string(),
        SupportedLanguage::Ruby => "Ruby".to_string(),
        SupportedLanguage::Rust => "Rust".to_string(),
        SupportedLanguage::Scala => "Scala".to_string(),
        SupportedLanguage::Sql => "SQL".to_string(),
        SupportedLanguage::Svg => "SVG".to_string(),
        SupportedLanguage::Svelte => "Svelte".to_string(),
        SupportedLanguage::Swift => "Swift".to_string(),
        SupportedLanguage::Toml => "TOML".to_string(),
        SupportedLanguage::Tsx => "TSX".to_string(),
        SupportedLanguage::TypeScript => "TypeScript".to_string(),
        SupportedLanguage::Vue => "Vue".to_string(),
        SupportedLanguage::Yaml => "YAML".to_string(),
        SupportedLanguage::Zig => "Zig".to_string(),
    }
}

/// Get the Language enum from a file extension. Checks if the extension is a known language, if not, it returns Plain.
///
/// ### Arguments
/// - `extension`: The file extension
///
/// ### Returns
/// - `Language`: The detected language
pub fn language_from_extension(extension: &str) -> SupportedLanguage {
    if extension == "txt" {
        return SupportedLanguage::Plain;
    }
    let mut language = from_language(Language::from_str(extension));
    if language == SupportedLanguage::Plain {
        language = match extension {
            "astro" => SupportedLanguage::Html,
            "lock" => SupportedLanguage::Toml,
            "mjs" => SupportedLanguage::JavaScript,
            "php" | "php3" | "php4" | "php5" | "phhtml" => SupportedLanguage::Html,
            "svelte" => SupportedLanguage::Svelte,
            "svg" => SupportedLanguage::Html,
            "vue" => SupportedLanguage::Vue,
            _ => SupportedLanguage::Plain,
        };
    }
    language
}

impl SupportedLanguage {
    /// Lists all supported languages in alphabetical order, including some that are not supported by the language registry.
    ///
    /// ### Returns
    /// - `Vec<SupportedLanguage>`: A vector of all supported languages
    pub fn all() -> Vec<SupportedLanguage> {
        vec![
            SupportedLanguage::Astro,
            SupportedLanguage::Bash,
            SupportedLanguage::C,
            SupportedLanguage::Cpp,
            SupportedLanguage::CMake,
            SupportedLanguage::CSharp,
            SupportedLanguage::Css,
            SupportedLanguage::Diff,
            SupportedLanguage::Ejs,
            SupportedLanguage::Elixir,
            SupportedLanguage::Erb,
            SupportedLanguage::Go,
            SupportedLanguage::GraphQl,
            SupportedLanguage::Html,
            SupportedLanguage::Java,
            SupportedLanguage::JavaScript,
            SupportedLanguage::JsDoc,
            SupportedLanguage::Json,
            SupportedLanguage::Make,
            SupportedLanguage::Markdown,
            SupportedLanguage::MarkdownInline,
            SupportedLanguage::Php,
            SupportedLanguage::Plain,
            SupportedLanguage::Proto,
            SupportedLanguage::Python,
            SupportedLanguage::Ruby,
            SupportedLanguage::Rust,
            SupportedLanguage::Scala,
            SupportedLanguage::Sql,
            SupportedLanguage::Svelte,
            SupportedLanguage::Svg,
            SupportedLanguage::Swift,
            SupportedLanguage::Toml,
            SupportedLanguage::Tsx,
            SupportedLanguage::TypeScript,
            SupportedLanguage::Vue,
            SupportedLanguage::Yaml,
            SupportedLanguage::Zig,
        ]
    }
}

/// Get all languages as a vector of SharedString
///
/// ### Returns
/// - `Vec<SharedString>`: The list of all languages as SharedString
#[allow(dead_code)]
pub fn all_languages() -> Vec<SharedString> {
    let mut languages = SupportedLanguage::all()
        .iter()
        .map(|language| SharedString::new(pretty_name(language).as_str()))
        .collect::<Vec<SharedString>>();
    languages.sort();
    languages
}

impl Fulgur {
    /// Get the current language of the active tab
    ///
    /// ### Returns
    /// - `SupportedLanguage`: The active tab's language
    pub fn get_current_language(&self) -> SupportedLanguage {
        match self.active_tab_index {
            Some(index) => {
                if let Some(editor_tab) = self.tabs[index].as_editor() {
                    editor_tab.language
                } else {
                    SupportedLanguage::Plain
                }
            }
            None => SupportedLanguage::Plain,
        }
    }

    /// Check if the current active tab's language is a Markdown language
    ///
    /// ### Returns
    /// - `True` if the active tab's language is a Markdown language, `False` otherwise
    pub fn is_markdown(&self) -> bool {
        let current_language = self.get_current_language();
        current_language == SupportedLanguage::Markdown
            || current_language == SupportedLanguage::MarkdownInline
    }
}
