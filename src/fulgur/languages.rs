use gpui_component::highlighter::Language;

// Initialize the language registry with all supported languages
pub fn init_languages() {}

// Get the Language enum from a file extension. Checks if the extension is a known language, if not, it returns Plain.
// @param extension: The file extension
// @return: The Language enum
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

// Get the language name as a string
// @param language: The Language enum
// @return: The language name as a string
pub fn language_name(language: &Language) -> &'static str {
    language.name()
}

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
