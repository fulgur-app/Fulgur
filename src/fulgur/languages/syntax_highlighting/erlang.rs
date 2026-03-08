use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

/// Add Erlang syntax highlighting support.
pub fn add_erlang_support() {
    LanguageRegistry::singleton().register(
        "erlang",
        &LanguageConfig::new(
            "erlang",
            arborium_erlang::language().into(),
            vec![],
            arborium_erlang::HIGHLIGHTS_QUERY,
            arborium_erlang::INJECTIONS_QUERY,
            "",
        ),
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_erlang_support_registers_language() {
        super::add_erlang_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("erlang")
                .is_some()
        );
    }
}
