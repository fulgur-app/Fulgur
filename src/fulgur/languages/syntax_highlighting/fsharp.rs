use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

/// Add F# language support.
pub fn add_fsharp_support() {
    LanguageRegistry::singleton().register(
        "fsharp",
        &LanguageConfig::new(
            "fsharp",
            tree_sitter_fsharp::LANGUAGE_FSHARP.into(),
            vec![],
            FSHARP_HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    );
}

/// Highlights query for F#, remapped from the tree-sitter-fsharp nvim-treesitter naming
/// conventions (`@keyword.conditional`, `@keyword.function`, `@keyword.type`, `@module`,
/// `@function.call`, `@variable.parameter`, etc.) to the gpui-component recognized names
/// (`@keyword`, `@function`, `@type`, `@variable`, etc.).
const FSHARP_HIGHLIGHTS_QUERY: &str = r##"
[
  (line_comment)
  (block_comment)
] @comment

(identifier) @variable

[
  (xint)
  (int)
  (int16)
  (uint16)
  (int32)
  (uint32)
  (int64)
  (uint64)
  (nativeint)
  (unativeint)
  (ieee32)
  (ieee64)
  (float)
  (decimal)
] @number

(bool) @boolean

[
  (string)
  (triple_quoted_string)
  (verbatim_string)
  (char)
] @string

(attribute) @attribute

[
  (_type)
  (atomic_type)
] @type

(value_declaration_left . (_) @variable)

(primary_constr_args (_) @variable)

[
  "|"
  "="
  ">"
  "<"
  "-"
  "~"
  "->"
  "<-"
  "&&"
  "||"
  ":>"
  ":?>"
  (prefix_op)
  (op_identifier)
] @operator

(infix_op) @keyword

[
  "("
  ")"
  "{"
  "}"
  "["
  "]"
  "[|"
  "|]"
  "{|"
  "|}"
] @punctuation.bracket

[
  "[<"
  ">]"
] @punctuation.special

[
  ","
  ";"
] @punctuation.delimiter

[
  "if"
  "then"
  "else"
  "elif"
  "when"
  "match"
  "match!"
  "and"
  "or"
  "not"
  "upcast"
  "downcast"
  "return"
  "return!"
  "yield"
  "yield!"
  "for"
  "while"
  "downto"
  "to"
  "open"
  "#r"
  "#load"
  "abstract"
  "delegate"
  "static"
  "inline"
  "mutable"
  "override"
  "rec"
  "global"
  (access_modifier)
  "let"
  "let!"
  "use"
  "use!"
  "member"
  "as"
  "assert"
  "begin"
  "end"
  "done"
  "default"
  "in"
  "do"
  "do!"
  "fun"
  "function"
  "get"
  "set"
  "lazy"
  "new"
  "of"
  "struct"
  "val"
  "module"
  "namespace"
  "with"
  "try"
  "finally"
] @keyword

[
  "enum"
  "type"
  "inherit"
  "interface"
  "class"
] @type

(compiler_directive_decl) @keyword
(preproc_line "#line" @keyword)

[
  "null"
  (union_type_case)
] @constant

(type_name type_name: (_)) @type

(named_module name: (_) @type)
(namespace name: (_) @type)
(import_decl . (_) @type)

((long_identifier (identifier) @type)
 (#any-of? @type "bool" "byte" "sbyte" "int16" "uint16" "int" "uint" "int64" "uint64"
           "nativeint" "unativeint" "decimal" "float" "double" "float32" "single"
           "char" "string" "unit"))
"##;

#[cfg(test)]
mod tests {
    use gpui_component::highlighter::SyntaxHighlighter;

    #[test]
    fn test_add_fsharp_support_registers_language() {
        super::add_fsharp_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("fsharp")
                .is_some()
        );
    }

    #[test]
    fn test_fsharp_highlights_query_compiles() {
        super::add_fsharp_support();
        let highlighter = SyntaxHighlighter::new("fsharp");
        assert_eq!(highlighter.language().as_ref(), "fsharp");
    }
}
