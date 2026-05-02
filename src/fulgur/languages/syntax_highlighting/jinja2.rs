use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

/// Add Jinja2 template syntax highlighting support.
pub fn add_jinja2_support() {
    LanguageRegistry::singleton().register(
        "jinja2",
        &LanguageConfig::new(
            "jinja2",
            arborium_jinja2::language().into(),
            vec![],
            JINJA2_HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    );
}

/// Highlights query for Jinja2, remapped from the `arborium_jinja2` nvim-treesitter naming
/// conventions to the gpui-component recognized names.
///
/// Key points:
/// - `keyword` is a named node holding the template tag keyword (if, for, block, etc.),
///   captured directly rather than via anonymous string literals.
/// - `expression_begin/end` and `statement_begin/end` are named nodes wrapping `{{`, `}}`,
///   `{%`, `%}` delimiters (with optional whitespace-control `-`).
/// - `identifier` is the catch-all for variable names, filter names, and tag arguments.
/// - Delimiters use `@tag` because it is defined in all bundled themes; `@punctuation.bracket`
///   is only defined in Catppuccin and would be invisible elsewhere.
///
/// HTML injection is intentionally omitted. Injecting HTML into the full `source_file`
/// causes the HTML parser to mis-parse Jinja2 comparison operators (`< N`) as HTML tags,
/// bleeding `@attribute` highlights over all subsequent Jinja2 content. Since the Jinja2
/// grammar exposes no named nodes for the raw template text between tags, there is no
/// way to restrict the injection to non-Jinja2 regions.
const JINJA2_HIGHLIGHTS_QUERY: &str = r#"
; Comments: {# ... #}
(comment) @comment

; String literals within expressions and statements
(string) @string

; Template keywords: if, elif, else, endif, for, endfor, block, endblock, etc.
(keyword) @keyword

; Operators: |, ==, !=, <, >, and, or, not, is, in, ~, +, -, *, /
(operator) @operator

; Template delimiters: {{ }}, {% %}, with optional whitespace control (-/+)
; Using @tag because it is present in all bundled themes.
(expression_begin) @tag
(expression_end) @tag
(statement_begin) @tag
(statement_end) @tag

; Identifiers: variables, filter names, macro names, block names
(identifier) @variable
"#;

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_jinja2_support_registers_language() {
        super::add_jinja2_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("jinja2")
                .is_some()
        );
    }
}
