use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};
use tree_sitter_r;

/// Add R language support.
pub fn add_r_support() {
    LanguageRegistry::singleton().register(
        "r",
        &LanguageConfig::new(
            "r",
            tree_sitter_r::LANGUAGE.into(),
            vec![],
            tree_sitter_r::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_r::LOCALS_QUERY,
        ),
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_r_support_registers_language() {
        super::add_r_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("r")
                .is_some()
        );
    }
}
