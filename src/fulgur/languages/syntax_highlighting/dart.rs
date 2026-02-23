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
}
