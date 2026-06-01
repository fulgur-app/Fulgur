use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

/// Add D language syntax highlighting support.
pub fn add_d_support() {
    LanguageRegistry::singleton().register(
        "d",
        &LanguageConfig::new(
            "d",
            arborium_d::language().into(),
            vec![],
            arborium_d::HIGHLIGHTS_QUERY,
            arborium_d::INJECTIONS_QUERY,
            "",
        ),
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_d_support_registers_language() {
        super::add_d_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("d")
                .is_some()
        );
    }

    #[test]
    fn test_d_highlights_query_compiles() {
        super::add_d_support();
        let highlighter = gpui_component::highlighter::SyntaxHighlighter::new("d");
        assert_eq!(highlighter.language().as_ref(), "d");
    }
}
