use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

/// Add Julia language support.
pub fn add_julia_support() {
    LanguageRegistry::singleton().register(
        "julia",
        &LanguageConfig::new(
            "julia",
            arborium_julia::language().into(),
            vec![],
            JULIA_HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    );
}

/// Highlights query for Julia, remapped from the arborium_julia nvim-treesitter naming
/// conventions to the gpui-component recognized names
const JULIA_HIGHLIGHTS_QUERY: &str = r#"
(identifier) @variable

(field_expression
  (identifier) @property .)

(quote_expression
  ":" @string
  [
    (identifier)
    (operator)
  ] @string)

(call_expression
  (identifier) @function)

(call_expression
  (field_expression
    (identifier) @function .))

(broadcast_call_expression
  (identifier) @function)

(broadcast_call_expression
  (field_expression
    (identifier) @function .))

(binary_expression
  (_)
  (operator) @_pipe
  (identifier) @function
  (#any-of? @_pipe "|>" ".|>"))

(macro_identifier
  "@" @function
  (identifier) @function)

(macro_definition
  (signature
    (call_expression
      .
      (identifier) @function)))

((identifier) @function
  (#any-of? @function
    "applicable" "fieldtype" "getfield" "getglobal" "invoke" "isa" "isdefined" "isdefinedglobal"
    "modifyfield!" "modifyglobal!" "nfields" "replacefield!" "replaceglobal!" "setfield!"
    "setfieldonce!" "setglobal!" "setglobalonce!" "swapfield!" "swapglobal!" "throw" "tuple"
    "typeassert" "typeof"))

(type_head
  (_) @type)

(parametrized_type_expression
  [
    (identifier) @type
    (field_expression
      (identifier) @type .)
  ]
  (curly_expression
    (_) @type))

(typed_expression
  (identifier) @type .)

(unary_typed_expression
  (identifier) @type .)

(where_expression
  [
    (curly_expression
      (_) @type)
    (_) @type
  ] .)

(unary_expression
  (operator) @operator
  (_) @type
  (#any-of? @operator "<:" ">:"))

(binary_expression
  (_) @type
  (operator) @operator
  (_) @type
  (#any-of? @operator "<:" ">:"))

((identifier) @type
  (#any-of? @type
    "AbstractArray" "AbstractChar" "AbstractFloat" "AbstractString" "Any" "ArgumentError" "Array"
    "AssertionError" "AtomicMemory" "AtomicMemoryRef" "Bool" "BoundsError" "Char"
    "ConcurrencyViolationError" "Cvoid" "DataType" "DenseArray" "DivideError" "DomainError"
    "ErrorException" "Exception" "Expr" "FieldError" "Float16" "Float32" "Float64" "Function"
    "GenericMemory" "GenericMemoryRef" "GlobalRef" "IO" "InexactError" "InitError" "Int" "Int128"
    "Int16" "Int32" "Int64" "Int8" "Integer" "InterruptException" "LineNumberNode" "LoadError"
    "Memory" "MemoryRef" "Method" "MethodError" "Module" "NTuple" "NamedTuple" "Nothing" "Number"
    "OutOfMemoryError" "OverflowError" "Pair" "Ptr" "QuoteNode" "ReadOnlyMemoryError" "Real" "Ref"
    "SegmentationFault" "Signed" "StackOverflowError" "String" "Symbol" "Task" "Tuple" "Type"
    "TypeError" "TypeVar" "UInt" "UInt128" "UInt16" "UInt32" "UInt64" "UInt8" "UndefInitializer"
    "UndefKeywordError" "UndefRefError" "UndefVarError" "Union" "UnionAll" "Unsigned" "VecElement"
    "WeakRef"))

[
  "global"
  "local"
] @keyword

(compound_statement
  [
    "begin"
    "end"
  ] @keyword)

(quote_statement
  [
    "quote"
    "end"
  ] @keyword)

(let_statement
  [
    "let"
    "end"
  ] @keyword)

(if_statement
  [
    "if"
    "end"
  ] @keyword)

(elseif_clause
  "elseif" @keyword)

(else_clause
  "else" @keyword)

(ternary_expression
  [
    "?"
    ":"
  ] @keyword)

(try_statement
  [
    "try"
    "end"
  ] @keyword)

(catch_clause
  "catch" @keyword)

(finally_clause
  "finally" @keyword)

(for_statement
  [
    "for"
    "end"
  ] @keyword)

(for_binding
  "outer" @keyword)

(for_clause
  "for" @keyword)

(if_clause
  "if" @keyword)

(while_statement
  [
    "while"
    "end"
  ] @keyword)

[
  (break_statement)
  (continue_statement)
] @keyword

[
  "const"
  "mutable"
] @keyword

(function_definition
  [
    "function"
    "end"
  ] @keyword)

(do_clause
  [
    "do"
    "end"
  ] @keyword)

(macro_definition
  [
    "macro"
    "end"
  ] @keyword)

(return_statement
  "return" @keyword)

(module_definition
  [
    "module"
    "baremodule"
    "end"
  ] @keyword)

(export_statement
  "export" @keyword)

(public_statement
  "public" @keyword)

(import_statement
  "import" @keyword)

(using_statement
  "using" @keyword)

(import_alias
  "as" @keyword)

(selected_import
  ":" @punctuation.delimiter)

(struct_definition
  [
    "mutable"
    "struct"
    "end"
  ] @keyword)

(abstract_definition
  [
    "abstract"
    "type"
    "end"
  ] @keyword)

(primitive_definition
  [
    "primitive"
    "type"
    "end"
  ] @keyword)

(operator) @operator

(adjoint_expression
  "'" @operator)

(range_expression
  ":" @operator)

(arrow_function_expression
  "->" @operator)

[
  "."
  "..."
] @punctuation.delimiter

[
  ","
  ";"
  "::"
] @punctuation.delimiter

(typed_expression
  "::" @operator)

(unary_typed_expression
  "::" @operator)

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

(string_interpolation
  .
  "$" @punctuation.delimiter)

(interpolation_expression
  .
  "$" @punctuation.delimiter)

((operator) @keyword
  (#any-of? @keyword "in" "isa"))

(where_expression
  "where" @keyword)

(const_statement
  (assignment
    . (identifier) @constant))

(macrocall_expression
  (macro_argument_list
    . (identifier) @type))

(macrocall_expression
  (macro_argument_list
    (identifier) @constant))

((identifier) @constant
  (#any-of? @constant "nothing" "missing"))

(boolean_literal) @boolean

(integer_literal) @number

(float_literal) @number

((identifier) @number
  (#any-of? @number "NaN" "NaN16" "NaN32" "Inf" "Inf16" "Inf32"))

(character_literal) @string

(escape_sequence) @string

(string_literal) @string

(prefixed_string_literal
  prefix: (identifier) @function) @string

(command_literal) @string

(prefixed_command_literal
  prefix: (identifier) @function) @string

((string_literal) @comment
  .
  [
    (abstract_definition)
    (assignment)
    (const_statement)
    (function_definition)
    (macro_definition)
    (module_definition)
    (struct_definition)
  ])

(source_file
  (string_literal) @comment
  .
  [
    (identifier)
    (call_expression)
  ])

[
  (line_comment)
  (block_comment)
] @comment
"#;

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_julia_support_registers_language() {
        super::add_julia_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("julia")
                .is_some()
        );
    }
}
