use arborium_scss;
use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

// arboretum_scss has a parsing issue with `@extend`, leading to partially parsed content

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

/// Custom SCSS highlights query built from the upstream CSS base plus a rewritten SCSS-specific layer.
const SCSS_HIGHLIGHTS_QUERY_SCSS_LAYER: &str = r#"
;; ── At-rule keywords ─────────────────────────────────────────────────────────

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

;; ── Loop connective keywords (contextual to avoid false positives) ────────────

(each_statement "in" @keyword)
(for_statement "from" @keyword)
(for_statement "through" @keyword)

;; ── Comments ──────────────────────────────────────────────────────────────────

(js_comment) @comment

;; ── Functions ─────────────────────────────────────────────────────────────────

(function_name) @function

(mixin_statement
  name: (identifier) @function)

(function_statement
  name: (identifier) @function)

(include_statement
  (identifier) @function)

(keyword_query) @function

;; ── Operators ─────────────────────────────────────────────────────────────────

[
  ">="
  "<="
] @operator

;; ── Parameters ────────────────────────────────────────────────────────────────

(mixin_statement
  (parameters
    (parameter) @variable))

(function_statement
  (parameters
    (parameter) @variable))

(argument) @variable

(arguments
  (variable) @constant)

;; ── Values ────────────────────────────────────────────────────────────────────

(plain_value) @string

(identifier) @string

;; ── SCSS variables ($var) ─────────────────────────────────────────────────────

(variable) @constant

;; ── Brackets ──────────────────────────────────────────────────────────────────

[
  "["
  "]"
] @punctuation.bracket
"#;

/// The full highlights query: the unmodified CSS base from arborium_css followed by our rewritten SCSS layer.
static SCSS_HIGHLIGHTS_QUERY: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    let full = &*arborium_scss::HIGHLIGHTS_QUERY;
    let css_base = full
        .find("[\n  \"@at-root\"")
        .map(|pos| &full[..pos])
        .unwrap_or(full);

    format!("{css_base}\n{SCSS_HIGHLIGHTS_QUERY_SCSS_LAYER}")
});

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_scss_support_registers_language() {
        super::add_scss_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("scss")
                .is_some()
        );
    }
}
