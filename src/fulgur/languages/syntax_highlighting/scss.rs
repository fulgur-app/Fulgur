use arborium_scss;
use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

/// Add SCSS language support.
pub fn add_scss_support() {
    LanguageRegistry::singleton().register(
        "scss",
        &LanguageConfig::new(
            "scss",
            arborium_scss::language().into(),
            vec![],
            SCSS_HIGHLIGHTS_QUERY.as_str(),
            arborium_scss::INJECTIONS_QUERY,
            arborium_scss::LOCALS_QUERY,
        ),
    );
}

/// SCSS-specific fixes prepended before the full upstream query.
///
/// In tree-sitter, the **first** matching pattern for a node wins. By
/// prepending these rules before `arborium_scss::HIGHLIGHTS_QUERY` (which
/// itself prepends `arborium_css::HIGHLIGHTS_QUERY`), we guarantee that our
/// patterns take priority without having to duplicate or replace the full
/// upstream CSS + SCSS query.
///
/// Fixes applied:
///
/// 1. **Capture name mapping** — the upstream SCSS-specific part uses
///    `@keyword.repeat`, `@keyword.function`, `@keyword.import`, and
///    `@keyword.return`, which are absent from gpui-component's
///    `HIGHLIGHT_NAMES`. All at-rule and loop keywords are remapped to
///    `@keyword`.
///
/// 2. **Missing keywords** — `@if` and `@else` are absent from the upstream
///    SCSS-specific query.
///
/// 3. **`@each` loop variable** — `$size` in `@each $size in $sizes` is the
///    `value` named field of `each_statement`. An explicit field capture rule
///    guarantees it is highlighted even if the generic `(variable)` rule is
///    shadowed by a lower-priority ancestor match.
const SCSS_HIGHLIGHTS_PREFIX: &str = r#"
;; ── At-rule keywords (fix: map sub-named captures to @keyword) ───────────────

[
  "@at-root"
  "@charset"
  "@debug"
  "@error"
  "@extend"
  "@forward"
  "@function"
  "@if"
  "@else"
  "@import"
  "@include"
  "@keyframes"
  "@media"
  "@mixin"
  "@namespace"
  "@return"
  "@supports"
  "@use"
  "@warn"
  "@while"
  "@each"
  "@for"
] @keyword

;; ── Loop keywords (fix: @keyword.repeat → @keyword) ──────────────────────────

[
  "from"
  "through"
  "in"
] @keyword

;; ── @each loop variable (fix: explicit field capture for $size / $key) ───────

(each_statement value: (variable) @variable)
(each_statement key:   (variable) @variable)
"#;

/// The full highlights query: our fixes prepended before the upstream
/// `arborium_scss::HIGHLIGHTS_QUERY` (which already includes the CSS base).
static SCSS_HIGHLIGHTS_QUERY: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!(
        "{}\n{}",
        SCSS_HIGHLIGHTS_PREFIX,
        *arborium_scss::HIGHLIGHTS_QUERY
    )
});

#[cfg(test)]
mod tests {
    use super::{SCSS_HIGHLIGHTS_PREFIX, add_scss_support};
    use gpui_component::highlighter::LanguageRegistry;

    fn registered_scss() -> gpui_component::highlighter::LanguageConfig {
        add_scss_support();
        LanguageRegistry::singleton()
            .language("scss")
            .expect("SCSS should be registered after calling add_scss_support()")
    }

    #[test]
    fn test_add_scss_support_registers_language() {
        add_scss_support();
        assert!(LanguageRegistry::singleton().language("scss").is_some());
    }

    #[test]
    fn test_add_scss_support_language_name() {
        assert_eq!(registered_scss().name, "scss");
    }

    #[test]
    fn test_add_scss_support_highlights_are_not_empty() {
        assert!(!registered_scss().highlights.is_empty());
    }

    #[test]
    fn test_add_scss_support_no_injection_languages() {
        assert!(registered_scss().injection_languages.is_empty());
    }

    #[test]
    fn test_scss_highlights_prefix_contains_keyword_capture() {
        assert!(SCSS_HIGHLIGHTS_PREFIX.contains("@keyword"));
    }

    #[test]
    fn test_scss_highlights_prefix_contains_at_rule_keywords() {
        assert!(SCSS_HIGHLIGHTS_PREFIX.contains("@mixin"));
        assert!(SCSS_HIGHLIGHTS_PREFIX.contains("@include"));
        assert!(SCSS_HIGHLIGHTS_PREFIX.contains("@each"));
        assert!(SCSS_HIGHLIGHTS_PREFIX.contains("@for"));
        assert!(SCSS_HIGHLIGHTS_PREFIX.contains("@if"));
        assert!(SCSS_HIGHLIGHTS_PREFIX.contains("@else"));
        assert!(SCSS_HIGHLIGHTS_PREFIX.contains("@function"));
    }

    #[test]
    fn test_scss_highlights_prefix_contains_loop_keywords() {
        assert!(SCSS_HIGHLIGHTS_PREFIX.contains("\"from\""));
        assert!(SCSS_HIGHLIGHTS_PREFIX.contains("\"through\""));
        assert!(SCSS_HIGHLIGHTS_PREFIX.contains("\"in\""));
    }

    #[test]
    fn test_scss_highlights_prefix_contains_each_field_rules() {
        assert!(SCSS_HIGHLIGHTS_PREFIX.contains("each_statement value:"));
        assert!(SCSS_HIGHLIGHTS_PREFIX.contains("each_statement key:"));
    }

    #[test]
    fn test_add_scss_support_highlights_include_css_base() {
        // The combined query must include CSS base patterns.
        let h = registered_scss().highlights;
        assert!(h.contains("(comment)"));
        assert!(h.contains("(tag_name)"));
        assert!(h.contains("(property_name)"));
    }

    #[test]
    fn test_add_scss_support_is_idempotent() {
        add_scss_support();
        add_scss_support();
        assert!(LanguageRegistry::singleton().language("scss").is_some());
    }
}
