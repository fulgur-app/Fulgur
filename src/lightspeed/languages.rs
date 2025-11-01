use gpui_component::highlighter::Language;

// Initialize the language registry with all supported languages
pub fn init_languages() {}

// Get the Language enum from a file extension 
// @param extension: The file extension
// @return: The Language enum
pub fn language_from_extension(extension: &str) -> Language {
    Language::from_str(extension)
}

// Get the language name as a string
// @param language: The Language enum
// @return: The language name as a string
pub fn language_name(language: &Language) -> &'static str {
    language.name()
}

