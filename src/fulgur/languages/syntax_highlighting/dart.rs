use arborium_dart;
use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

/// Add Dart language support.
pub fn add_dart_support() {
    LanguageRegistry::singleton().register(
        "dart",
        &LanguageConfig::new(
            "dart",
            arborium_dart::language().into(),
            vec![],
            arborium_dart::HIGHLIGHTS_QUERY,
            arborium_dart::INJECTIONS_QUERY,
            arborium_dart::LOCALS_QUERY,
        ),
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_dart_support_registers_language() {
        super::add_dart_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("dart")
                .is_some()
        );
    }

    #[test]
    fn test_dart_highlights_query_compiles() {
        super::add_dart_support();
        let highlighter = gpui_component::highlighter::SyntaxHighlighter::new("dart");
        assert_eq!(highlighter.language().as_ref(), "dart");
    }
}
