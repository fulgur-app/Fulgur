use arborium_tsx;
use gpui::SharedString;
use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

/// Add React language support.
pub fn add_react_support() {
    LanguageRegistry::singleton().register(
        "react",
        &LanguageConfig::new(
            "react",
            arborium_tsx::language().into(),
            vec![
                SharedString::new("typescript"),
                SharedString::new("javascript"),
                SharedString::new("css"),
            ],
            arborium_tsx::HIGHLIGHTS_QUERY.as_str(),
            arborium_tsx::INJECTIONS_QUERY,
            arborium_tsx::LOCALS_QUERY,
        ),
    );
}
