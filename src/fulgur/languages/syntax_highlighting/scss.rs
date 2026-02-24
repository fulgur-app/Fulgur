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

    format!("{}\n{}", css_base, SCSS_HIGHLIGHTS_QUERY_SCSS_LAYER)
});

#[cfg(test)]
mod tests {
    use super::{SCSS_HIGHLIGHTS_QUERY, SCSS_HIGHLIGHTS_QUERY_SCSS_LAYER, add_scss_support};
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
    fn test_combined_query_compiles_and_highlights() {
        // If the query is invalid, SyntaxHighlighter falls back to plain
        // text and produces a single default-style span.  We verify that
        // actual SCSS content produces more than one span (= highlighting).
        add_scss_support();
        let code = ".btn { color: #f00; }";
        let rope = gpui_component::Rope::from_str(code);
        let mut hl = gpui_component::highlighter::SyntaxHighlighter::new("scss");
        hl.update(None, &rope);
        let theme = gpui_component::highlighter::HighlightTheme::default_dark();
        let styles = hl.styles(&(0..code.len()), &theme);
        assert!(
            styles.len() > 1,
            "expected multiple highlight spans, got {} — query likely failed to compile",
            styles.len()
        );
    }

    #[test]
    fn test_scss_layer_contains_at_rule_keywords() {
        assert!(SCSS_HIGHLIGHTS_QUERY_SCSS_LAYER.contains("\"@mixin\""));
        assert!(SCSS_HIGHLIGHTS_QUERY_SCSS_LAYER.contains("\"@include\""));
        assert!(SCSS_HIGHLIGHTS_QUERY_SCSS_LAYER.contains("\"@each\""));
        assert!(SCSS_HIGHLIGHTS_QUERY_SCSS_LAYER.contains("\"@for\""));
        assert!(SCSS_HIGHLIGHTS_QUERY_SCSS_LAYER.contains("\"@if\""));
        assert!(SCSS_HIGHLIGHTS_QUERY_SCSS_LAYER.contains("\"@else\""));
        assert!(SCSS_HIGHLIGHTS_QUERY_SCSS_LAYER.contains("\"@function\""));
    }

    #[test]
    fn test_scss_layer_contains_contextual_loop_keywords() {
        assert!(SCSS_HIGHLIGHTS_QUERY_SCSS_LAYER.contains("each_statement \"in\""));
        assert!(SCSS_HIGHLIGHTS_QUERY_SCSS_LAYER.contains("for_statement \"from\""));
        assert!(SCSS_HIGHLIGHTS_QUERY_SCSS_LAYER.contains("for_statement \"through\""));
    }

    #[test]
    fn test_scss_layer_does_not_contain_bare_in_keyword() {
        // The bare "in" pattern (without parent context) must NOT appear.
        // It was the source of false positives in words like `inline-flex`.
        for line in SCSS_HIGHLIGHTS_QUERY_SCSS_LAYER.lines() {
            let trimmed = line.trim();
            assert!(
                trimmed != "\"in\"",
                "bare \"in\" pattern found — must be contextual"
            );
        }
    }

    #[test]
    fn test_scss_layer_captures_variables_as_constant() {
        assert!(SCSS_HIGHLIGHTS_QUERY_SCSS_LAYER.contains("(variable) @constant"));
    }

    #[test]
    fn test_combined_query_includes_css_base() {
        let q = &*SCSS_HIGHLIGHTS_QUERY;
        assert!(q.contains("(comment) @comment"), "missing CSS comment rule");
        assert!(q.contains("(tag_name) @tag"), "missing CSS tag_name rule");
        assert!(
            q.contains("(property_name) @property"),
            "missing CSS property_name rule"
        );
        assert!(
            q.contains("(to) @keyword"),
            "missing CSS (to) named node rule"
        );
        assert!(
            q.contains("(from) @keyword"),
            "missing CSS (from) named node rule"
        );
        assert!(
            q.contains("(integer_value) @number"),
            "missing CSS integer_value rule"
        );
    }

    #[test]
    fn test_combined_query_does_not_contain_upstream_scss_layer() {
        // The upstream SCSS layer contains @keyword.repeat which we
        // intentionally replace. Verify our split was correct.
        let q = &*SCSS_HIGHLIGHTS_QUERY;
        assert!(
            !q.contains("@keyword.repeat"),
            "upstream @keyword.repeat should have been stripped"
        );
    }

    #[test]
    fn test_add_scss_support_is_idempotent() {
        add_scss_support();
        add_scss_support();
        assert!(LanguageRegistry::singleton().language("scss").is_some());
    }

    #[test]
    fn test_css_base_extraction_has_content() {
        let full = &*arborium_scss::HIGHLIGHTS_QUERY;
        let split_pos = full.find("[\n  \"@at-root\"");
        assert!(
            split_pos.is_some(),
            "split pattern not found in upstream query"
        );
        let pos = split_pos.unwrap();
        assert!(
            pos > 100,
            "CSS base too short: only {} bytes before split point",
            pos
        );
        let css_base = &full[..pos];
        // Verify key CSS patterns are in the base
        assert!(
            css_base.contains("(property_name) @property"),
            "CSS base missing property_name"
        );
        assert!(
            css_base.contains("(tag_name) @tag"),
            "CSS base missing tag_name"
        );
        assert!(
            css_base.contains("(integer_value) @number"),
            "CSS base missing integer_value"
        );
    }

    #[test]
    fn test_css_patterns_produce_distinct_colors() {
        add_scss_support();
        let code = ".btn {\n  display: inline-flex;\n  font-weight: 600;\n  cursor: pointer;\n}\n";
        let rope = gpui_component::Rope::from_str(code);
        let mut hl = gpui_component::highlighter::SyntaxHighlighter::new("scss");
        hl.update(None, &rope);
        let theme = gpui_component::highlighter::HighlightTheme::default_dark();
        let styles = hl.styles(&(0..code.len()), &theme);

        // Collect colored (non-None, non-default) spans
        let colored_spans: Vec<_> = styles
            .iter()
            .filter(|(_, style)| style.color.is_some())
            .map(|(range, _)| &code[range.clone()])
            .collect();

        // CSS property names should be colored (@property)
        assert!(
            colored_spans.contains(&"display"),
            "property name 'display' missing"
        );
        // CSS values should be colored (@string via plain_value)
        assert!(
            colored_spans.contains(&"inline-flex"),
            "value 'inline-flex' missing"
        );
        assert!(
            colored_spans.contains(&"pointer"),
            "value 'pointer' missing"
        );
        // CSS numbers should be colored (@number)
        assert!(colored_spans.contains(&"600"), "number '600' missing");
    }

    /// CSS elements after SCSS constructs (mixins, loops) must still be colored.
    #[test]
    fn test_css_colored_after_scss_constructs() {
        add_scss_support();
        let code = "$c: #f00;\n@mixin m($x) { @content; }\n@each $s in a, b { .t-#{$s} { width: 1px; } }\n.btn {\n  display: flex;\n  font-weight: 600;\n  cursor: pointer;\n}\n";
        let rope = gpui_component::Rope::from_str(code);
        let mut hl = gpui_component::highlighter::SyntaxHighlighter::new("scss");
        hl.update(None, &rope);
        let theme = gpui_component::highlighter::HighlightTheme::default_dark();
        let styles = hl.styles(&(0..code.len()), &theme);

        let btn_offset = code.find(".btn").unwrap();
        let bottom_texts: Vec<&str> = styles
            .iter()
            .filter(|(range, style)| range.start >= btn_offset && style.color.is_some())
            .map(|(range, _)| &code[range.clone()])
            .collect();

        assert!(
            bottom_texts.contains(&"display"),
            "property 'display' not colored after SCSS constructs"
        );
        assert!(
            bottom_texts.contains(&"600"),
            "number '600' not colored after SCSS constructs"
        );
        assert!(
            bottom_texts.contains(&"pointer"),
            "value 'pointer' not colored after SCSS constructs"
        );
    }

    /// CSS colors must be identical whether at the top or after SCSS constructs.
    #[test]
    fn test_css_colors_identical_top_vs_bottom() {
        add_scss_support();
        let theme = gpui_component::highlighter::HighlightTheme::default_dark();

        let css_only = ".btn {\n  display: flex;\n  font-weight: 600;\n  cursor: pointer;\n}\n";
        let rope_top = gpui_component::Rope::from_str(css_only);
        let mut hl_top = gpui_component::highlighter::SyntaxHighlighter::new("scss");
        hl_top.update(None, &rope_top);
        let top_styles = hl_top.styles(&(0..css_only.len()), &theme);

        let with_scss = format!(
            "$c: #f00;\n@mixin m($x) {{ @content; }}\n@each $s in a, b {{ .t-#{{$s}} {{ width: 1px; }} }}\n{}",
            css_only
        );
        let rope_bot = gpui_component::Rope::from_str(&with_scss);
        let mut hl_bot = gpui_component::highlighter::SyntaxHighlighter::new("scss");
        hl_bot.update(None, &rope_bot);
        let bot_styles = hl_bot.styles(&(0..with_scss.len()), &theme);

        let find_color = |code: &str,
                          styles: &[(std::ops::Range<usize>, gpui::HighlightStyle)],
                          token: &str|
         -> Option<gpui::Hsla> {
            styles
                .iter()
                .find(|(range, _)| &code[range.clone()] == token)
                .and_then(|(_, style)| style.color)
        };

        for token in &["display", "flex", "600", "pointer"] {
            let top_color = find_color(css_only, &top_styles, token);
            let bot_color = find_color(&with_scss, &bot_styles, token);
            assert_eq!(
                top_color, bot_color,
                "color mismatch for {:?}: top={:?} vs bottom={:?}",
                token, top_color, bot_color
            );
        }
    }
}
