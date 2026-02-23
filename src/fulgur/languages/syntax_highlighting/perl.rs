use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};
use tree_sitter_perl_next;

/// Add Perl support to the editor
pub fn add_perl_support() {
    LanguageRegistry::singleton().register(
        "perl",
        &LanguageConfig::new(
            "perl",
            tree_sitter_perl_next::LANGUAGE.into(),
            vec![],
            tree_sitter_perl_next::HIGHLIGHTS_QUERY,
            tree_sitter_perl_next::INJECTIONS_QUERY,
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
}
