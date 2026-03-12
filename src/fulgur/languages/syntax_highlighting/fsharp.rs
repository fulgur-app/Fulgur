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
  (type_name type_name: (_))
  (_type)
  (atomic_type)
] @type

(function_declaration_left . (_) @function)

(member_defn
  (method_or_prop_defn
    (property_or_ident) @function))

(application_expression . (_) @function)

(field_initializer field: (_) @property)

(record_fields
  (record_field . (identifier) @property))

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
  "event"
  "field"
  "fun"
  "function"
  "get"
  "set"
  "lazy"
  "new"
  "of"
  "param"
  "property"
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

(rules
  (rule
    pattern: (_) @constant
    block: (_)))

(identifier_pattern
  .
  (_) @constant
  .
  (_) @variable)

(ce_expression
  .
  (_) @constant)

(named_module name: (_) @type)
(namespace name: (_) @type)
(module_defn . (_) @type)
(import_decl . (_) @type)

(dot_expression base: (_)? @type)

((_type
  (long_identifier (identifier) @type))
 (#any-of? @type "bool" "byte" "sbyte" "int16" "uint16" "int" "uint" "int64" "uint64"
           "nativeint" "unativeint" "decimal" "float" "double" "float32" "single"
           "char" "string" "unit"))
"##;

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_fsharp_support_registers_language() {
        super::add_fsharp_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("fsharp")
                .is_some()
        );
    }
}
