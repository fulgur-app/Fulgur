/// Highlights query for Ada, remapped from the arborium_ada nvim-treesitter naming conventions
/// (`@include`, `@repeat`, `@conditional`, `@exception`, `@storageclass`) to the gpui-component
/// recognized names (`keyword`, `type`, `operator`, etc.).
pub const ADA_HIGHLIGHTS_QUERY: &str = r#"
[
  "abort" "abs" "abstract" "accept" "access" "all" "array" "at"
  "begin" "declare" "delay" "delta" "digits" "do" "end" "entry"
  "exit" "generic" "interface" "is" "limited" "null" "of" "others"
  "out" "pragma" "private" "range" "synchronized" "tagged" "task"
  "terminate" "until" "when"
] @keyword

[ "aliased" "constant" "renames" ] @keyword

[ "with" "use" ] @keyword

[ "body" "function" "overriding" "procedure" "package" "separate" ] @keyword

[ "and" "in" "not" "or" "xor" ] @operator

[ "while" "loop" "for" "parallel" "reverse" "some" ] @keyword

"return" @keyword

[ "case" "if" "else" "then" "elsif" "select" ] @keyword

[ "exception" "raise" ] @keyword

[ "mod" "new" "protected" "record" "subtype" "type" ] @type

(comment) @comment
(string_literal) @string
(character_literal) @string
(numeric_literal) @number

(procedure_specification name: (identifier) @function)
(function_specification name: (identifier) @function)
(package_declaration name: (identifier) @function)
(package_body name: (identifier) @function)
(generic_instantiation name: (identifier) @function)
(generic_instantiation generic_name: (identifier) @type)
(entry_declaration . (identifier) @function)

(subprogram_body endname: (identifier) @function)
(package_body endname: (identifier) @function)

(procedure_call_statement name: (identifier) @function)

(qualified_expression subtype_name: (identifier) @type)

(full_type_declaration . (identifier) @type)
(incomplete_type_declaration . (identifier) @type)
(private_type_declaration . (identifier) @type)
(task_type_declaration . (identifier) @type)
(protected_type_declaration . (identifier) @type)
(subtype_declaration . (identifier) @type)

(parameter_specification subtype_mark: (identifier) @type)
(result_profile subtype_mark: (identifier) @type)
(object_declaration subtype_mark: (identifier) @type)

(use_clause "use" @keyword "type" @type)
(with_clause "private" @keyword)
(with_clause "limited" @keyword)
(use_clause (_) @type)
(with_clause (_) @type)

(loop_statement "end" @keyword)
(if_statement "end" @keyword)
(loop_parameter_specification "in" @keyword)
(iterator_specification ["in" "of"] @keyword)
(range_attribute_designator "range" @keyword)

(raise_statement "with" @keyword)

(gnatprep_declarative_if_statement) @preproc
(gnatprep_if_statement) @preproc
(gnatprep_identifier) @preproc

(subprogram_declaration "is" @keyword "abstract" @keyword)
(aspect_specification "with" @keyword)

(full_type_declaration "is" @type)
(subtype_declaration "is" @type)
(record_definition "end" @type)
(full_type_declaration (_ "access" @type))
(array_type_definition "array" @type "of" @type)
(access_to_object_definition "access" @type)
(access_to_object_definition "access" @type
  [ (general_access_modifier "constant" @type) (general_access_modifier "all" @type) ]
)
(range_constraint "range" @type)
(signed_integer_type_definition "range" @type)
(index_subtype_definition "range" @type)
(record_type_definition "abstract" @type)
(record_type_definition "tagged" @type)
(record_type_definition "limited" @type)
(record_type_definition (record_definition "null" @type))
(private_type_declaration "is" @type "private" @type)
(private_type_declaration "tagged" @type)
(private_type_declaration "limited" @type)
(task_type_declaration "task" @type "is" @type)

(expression_function_declaration (function_specification) "is" (_) @attribute)
(subprogram_declaration (aspect_specification) @attribute)

((comment) @comment.doc
  . [ (entry_declaration) (subprogram_declaration) (parameter_specification) ])
(compilation_unit . (comment) @comment.doc)
(component_list (component_declaration) . (comment) @comment.doc)
(enumeration_type_definition (identifier) . (comment) @comment.doc)
"#;
