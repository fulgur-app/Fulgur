use arborium_vue;
use gpui_kit::SharedString;
use gpui_kit::component::highlighter::{LanguageConfig, LanguageRegistry};

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

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_vue_support_registers_language() {
        super::add_vue_support();
        assert!(
            gpui_kit::component::highlighter::LanguageRegistry::singleton()
                .language("vue")
                .is_some()
        );
    }

    #[test]
    fn test_vue_highlights_query_compiles() {
        super::add_vue_support();
        let highlighter = gpui_kit::component::highlighter::SyntaxHighlighter::new("vue");
        assert_eq!(highlighter.language().as_ref(), "vue");
    }
}
