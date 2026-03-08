use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

/// Add Prolog syntax highlighting support.
pub fn add_prolog_support() {
    LanguageRegistry::singleton().register(
        "prolog",
        &LanguageConfig::new(
            "prolog",
            arborium_prolog::language().into(),
            vec![],
            arborium_prolog::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_prolog_support_registers_language() {
        super::add_prolog_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("prolog")
                .is_some()
        );
    }
}
