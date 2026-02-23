use arborium_groovy;
use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

/// Custom highlights query for Groovy.
///
/// Extends the upstream arborium_groovy query with missing keywords
/// (`new`, `interface`, `do`) that are in the grammar but absent from
/// the default query.
const GROOVY_HIGHLIGHTS_QUERY: &str = r#"
[
  "!in"
  "!instanceof"
  "as"
  "assert"
  "case"
  "catch"
  "class"
  "def"
  "default"
  "do"
  "else"
  "extends"
  "finally"
  "for"
  "if"
  "import"
  "in"
  "instanceof"
  "interface"
  "new"
  "package"
  "pipeline"
  "return"
  "switch"
  "try"
  "while"
  (break)
  (continue)
] @keyword

[
  "true"
  "false"
] @boolean

(null) @constant
"this" @variable.builtin

[
  "int"
  "char"
  "short"
  "long"
  "boolean"
  "float"
  "double"
  "void"
] @type.builtin

[
  "final"
  "private"
  "protected"
  "public"
  "static"
  "synchronized"
] @keyword

(comment) @comment
(shebang) @comment

(string) @string
(string (escape_sequence) @operator)
(string (interpolation ([ "$" ]) @operator))

("(") @punctuation.bracket
(")") @punctuation.bracket
("[") @punctuation.bracket
("]") @punctuation.bracket
("{") @punctuation.bracket
("}") @punctuation.bracket
(":") @punctuation.delimiter
(",") @punctuation.delimiter
(".") @punctuation.delimiter

(number_literal) @number
(identifier) @variable
((identifier) @variable.parameter
  (#is? @variable.parameter "local.parameter"))

((identifier) @constant
  (#match? @constant "^[A-Z][A-Z_]+"))

[
  "%" "*" "/" "+" "-" "<<" ">>" ">>>" ".." "..<" "<..<" "<.." "<"
  "<=" ">" ">=" "==" "!=" "<=>" "===" "!==" "=~" "==~" "&" "^" "|"
  "&&" "||" "?:" "+" "*" ".&" ".@" "?." "*." "*" "*:" "++" "--" "!"
] @operator

(string ("/") @string)

(ternary_op ([ "?" ":" ]) @operator)

(map (map_item key: (identifier) @variable.parameter))

(parameter type: (identifier) @type name: (identifier) @variable.parameter)
(generic_param name: (identifier) @variable.parameter)

(declaration type: (identifier) @type)
(function_definition type: (identifier) @type)
(function_declaration type: (identifier) @type)
(class_definition name: (identifier) @type)
(class_definition superclass: (identifier) @type)
(generic_param superclass: (identifier) @type)

(type_with_generics (identifier) @type)
(type_with_generics (generics (identifier) @type))
(generics [ "<" ">" ] @punctuation.bracket)
(generic_parameters [ "<" ">" ] @punctuation.bracket)

(declaration ("=") @operator)
(assignment ("=") @operator)

(function_call
  function: (identifier) @function)
(function_call
  function: (dotted_identifier
    (identifier) @function . ))
(function_call (argument_list
    (map_item key: (identifier) @variable.parameter)))
(juxt_function_call
  function: (identifier) @function)
(juxt_function_call
  function: (dotted_identifier
    (identifier) @function . ))
(juxt_function_call (argument_list
    (map_item key: (identifier) @variable.parameter)))

(function_definition
  function: (identifier) @function)
(function_declaration
  function: (identifier) @function)

(annotation) @attribute
(annotation (identifier) @attribute)
"@interface" @attribute

"pipeline" @keyword

(groovy_doc) @comment
(groovy_doc
  [
    (groovy_doc_param)
    (groovy_doc_throws)
    (groovy_doc_tag)
  ] @string.special)
(groovy_doc (groovy_doc_param (identifier) @variable.parameter))
(groovy_doc (groovy_doc_throws (identifier) @type))
"#;

/// Add Groovy language support.
pub fn add_groovy_support() {
    LanguageRegistry::singleton().register(
        "groovy",
        &LanguageConfig::new(
            "groovy",
            arborium_groovy::language().into(),
            vec![],
            GROOVY_HIGHLIGHTS_QUERY,
            arborium_groovy::INJECTIONS_QUERY,
            "",
        ),
    );
}
