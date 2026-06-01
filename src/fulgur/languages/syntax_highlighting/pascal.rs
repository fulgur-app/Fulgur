use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};
use tree_sitter_pascal;

const HIGHLIGHTS_QUERY: &str = include_str!("queries/pascal_highlights.scm");

/// Add Pascal / Delphi / `FreePascal` syntax highlighting support.
pub fn add_pascal_support() {
    LanguageRegistry::singleton().register(
        "pascal",
        &LanguageConfig::new(
            "pascal",
            tree_sitter_pascal::LANGUAGE.into(),
            vec![],
            HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_pascal_support_registers_language() {
        super::add_pascal_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("pascal")
                .is_some()
        );
    }

    #[test]
    fn test_pascal_highlights_query_compiles() {
        super::add_pascal_support();
        let highlighter = gpui_component::highlighter::SyntaxHighlighter::new("pascal");
        assert_eq!(highlighter.language().as_ref(), "pascal");
    }
}
