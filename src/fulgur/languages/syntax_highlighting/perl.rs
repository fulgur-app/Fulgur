use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};
use ts_parser_perl;

/// Add Perl support to the editor
pub fn add_perl_support() {
    LanguageRegistry::singleton().register(
        "perl",
        &LanguageConfig::new(
            "perl",
            ts_parser_perl::LANGUAGE.into(),
            vec![],
            ts_parser_perl::HIGHLIGHTS_QUERY,
            ts_parser_perl::INJECTIONS_QUERY,
            "",
        ),
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_perl_support_registers_language() {
        super::add_perl_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("perl")
                .is_some()
        );
    }

    #[test]
    fn test_perl_highlights_query_compiles() {
        super::add_perl_support();
        let highlighter = gpui_component::highlighter::SyntaxHighlighter::new("perl");
        assert_eq!(highlighter.language().as_ref(), "perl");
    }
}
