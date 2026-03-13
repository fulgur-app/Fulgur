use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

/// Add MATLAB language support.
pub fn add_matlab_support() {
    LanguageRegistry::singleton().register(
        "matlab",
        &LanguageConfig::new(
            "matlab",
            arborium_matlab::language().into(),
            vec![],
            MATLAB_HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    );
}

/// Highlights query for MATLAB, remapped from the arborium_matlab nvim-treesitter naming
/// conventions to the gpui-component recognized names.
///
/// Key corrections vs. the bundled arborium_matlab highlights.scm:
/// - Uses `name:` field selectors on `function_call`, `function_definition`, and
///   `class_definition` instead of bare child patterns — the grammar exposes these as
///   named fields, so field-qualified selectors are more reliable.
/// - `break_statement`, `continue_statement`, and `return_statement` are **named leaf
///   nodes** (the entire node IS the keyword token); they must be captured by node type,
///   not by the anonymous string literal `"break"` / `"continue"` / `"return"`.
/// - `field_expression` uses the `field:` selector to capture only the accessed property,
///   not the object expression.
const MATLAB_HIGHLIGHTS_QUERY: &str = r#"
; Comments — covers both % and %% styles
(comment) @comment

; Strings
(string) @string
(string_content) @string
(formatting_sequence) @string.special
(escape_sequence) @string.escape

; Numbers
(number) @number

; Function definitions — name: field holds the function identifier
(function_definition
  name: (identifier) @function)

; Function calls — name: field holds the callee identifier
(function_call
  name: (identifier) @function)

; Nested method calls: obj.method(args)
(function_call
  name: (function_call
    name: (identifier) @function))

; Command syntax (e.g. `disp text` without parens)
(command
  (command_name) @function)

; Class definitions — name: field holds the class identifier
(class_definition
  name: (identifier) @type)

; Classdef property declarations (name: field)
(property
  name: (identifier) @property)
(property
  name: (property_name) @property)

; Class property access in expressions (class_property node)
(class_property
  (identifier) @property)

; Field/member access: only the accessed field, not the object
(field_expression
  field: (identifier) @property)

; Identifiers fall back to variable
(identifier) @variable

; Ignored argument (~) used as output placeholder
(ignored_argument) @comment

; Keywords — named leaf nodes (the whole node IS the keyword token)
(break_statement) @keyword
(continue_statement) @keyword
(return_statement) @keyword

; Keywords — anonymous tokens inside statement nodes
[
  "function"
  "end"
  "classdef"
  "properties"
  "methods"
  "events"
  "enumeration"
] @keyword

[
  "if"
  "elseif"
  "else"
] @keyword

[
  "switch"
  "case"
  "otherwise"
] @keyword

[
  "for"
  "parfor"
  "while"
  "spmd"
] @keyword

[
  "try"
  "catch"
] @keyword

[
  "global"
  "persistent"
] @keyword

; Operators
[
  "="
  "+"
  "-"
  "*"
  "/"
  "^"
  "'"
  "@"
  ":"
  "~"
] @operator

; Punctuation
[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

[
  ","
  ";"
  "."
] @punctuation.delimiter

; Line continuation (...)
(line_continuation) @punctuation.special

; Attributes in classdef sections
(attribute) @attribute
(attributes) @attribute
"#;

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_matlab_support_registers_language() {
        super::add_matlab_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("matlab")
                .is_some()
        );
    }
}
