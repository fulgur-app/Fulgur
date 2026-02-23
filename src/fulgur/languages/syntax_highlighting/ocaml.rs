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
