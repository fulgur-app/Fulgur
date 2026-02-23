use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};
use tree_sitter_haskell;

/// Add Haskell language support.
pub fn add_haskell_support() {
    LanguageRegistry::singleton().register(
        "haskell",
        &LanguageConfig::new(
            "haskell",
            tree_sitter_haskell::LANGUAGE.into(),
            vec![],
            tree_sitter_haskell::HIGHLIGHTS_QUERY,
            tree_sitter_haskell::INJECTIONS_QUERY,
            tree_sitter_haskell::LOCALS_QUERY,
        ),
    );
}
