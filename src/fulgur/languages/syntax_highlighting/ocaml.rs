use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};
use tree_sitter_ocaml;

/// Add OCaml language support.
pub fn add_ocaml_support() {
    LanguageRegistry::singleton().register(
        "ocaml",
        &LanguageConfig::new(
            "ocaml",
            tree_sitter_ocaml::LANGUAGE_OCAML.into(),
            vec![],
            tree_sitter_ocaml::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_ocaml::LOCALS_QUERY,
        ),
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_ocaml_support_registers_language() {
        super::add_ocaml_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("ocaml")
                .is_some()
        );
    }
}
