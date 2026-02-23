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
