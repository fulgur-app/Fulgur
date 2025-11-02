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
