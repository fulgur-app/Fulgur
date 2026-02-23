use arborium_dockerfile;
use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

/// Add Dockerfile language support.
pub fn add_dockerfile_support() {
    LanguageRegistry::singleton().register(
        "dockerfile",
        &LanguageConfig::new(
            "dockerfile",
            arborium_dockerfile::language().into(),
            vec![],
            arborium_dockerfile::HIGHLIGHTS_QUERY,
            arborium_dockerfile::INJECTIONS_QUERY,
            arborium_dockerfile::LOCALS_QUERY,
        ),
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_dockerfile_support_registers_language() {
        super::add_dockerfile_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("dockerfile")
                .is_some()
        );
    }
}
