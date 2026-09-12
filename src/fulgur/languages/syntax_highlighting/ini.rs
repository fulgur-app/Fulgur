use gpui_kit::component::highlighter::{LanguageConfig, LanguageRegistry};

/// Add INI syntax highlighting support.
pub fn add_ini_support() {
    LanguageRegistry::singleton().register(
        "ini",
        &LanguageConfig::new(
            "ini",
            arborium_ini::language().into(),
            vec![],
            arborium_ini::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_ini_support_registers_language() {
        super::add_ini_support();
        assert!(
            gpui_kit::component::highlighter::LanguageRegistry::singleton()
                .language("ini")
                .is_some()
        );
    }

    #[test]
    fn test_ini_highlights_query_compiles() {
        super::add_ini_support();
        super::super::test_support::assert_queries_compile("ini");
    }
}
