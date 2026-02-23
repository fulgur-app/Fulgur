use arborium_vue;
use gpui::SharedString;
use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

/// Add Vue language support.
pub fn add_vue_support() {
    LanguageRegistry::singleton().register(
        "vue",
        &LanguageConfig::new(
            "vue",
            arborium_vue::language().into(),
            vec![
                SharedString::new("typescript"),
                SharedString::new("javascript"),
                SharedString::new("css"),
            ],
            arborium_vue::HIGHLIGHTS_QUERY.as_str(),
            arborium_vue::INJECTIONS_QUERY,
            arborium_vue::LOCALS_QUERY,
        ),
    );
}
