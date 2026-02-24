use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};
use tree_sitter_powershell;

/// Add Powershell language support.
pub fn add_powershell_support() {
    LanguageRegistry::singleton().register(
        "powershell",
        &LanguageConfig::new(
            "powershell",
            tree_sitter_powershell::LANGUAGE.into(),
            vec![],
            tree_sitter_powershell::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_powershell_support_registers_language() {
        super::add_powershell_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("powershell")
                .is_some()
        );
    }
}
