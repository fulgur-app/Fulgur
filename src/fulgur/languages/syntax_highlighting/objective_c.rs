use arborium_objc;
use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

/// Add Objective-C language support.
pub fn add_objective_c_support() {
    LanguageRegistry::singleton().register(
        "objective-c",
        &LanguageConfig::new(
            "objective-c",
            arborium_objc::language().into(),
            vec![],
            OBJC_HIGHLIGHTS_QUERY.as_str(),
            arborium_objc::INJECTIONS_QUERY,
            arborium_objc::LOCALS_QUERY,
        ),
    );
}

/// Objective-C highlights query layer, remapped from nvim-treesitter naming conventions
/// (`@method`, `@method.call`, `@exception`, `@storageclass`, `@type.qualifier`,
/// `@keyword.coroutine`, `@keyword.function`, `@keyword.operator`, `@include`,
/// `@type.builtin`, `@variable.builtin`, `@function.macro.builtin`, `@constant.macro`,
/// `@parameter`, `@parameter.builtin`, `@string.special`)
/// to the gpui-component recognized names (`keyword`, `function`, `type`, `operator`,
/// `variable`, `constant`, `attribute`, `property`, `string`, etc.).
const OBJC_HIGHLIGHTS_QUERY_LAYER: &str = r##"
;; ── Preprocs ────────────────────────────────────────────────────────────────

(preproc_undef
  name: (_) @constant) @preproc

;; ── Includes ────────────────────────────────────────────────────────────────

(module_import "@import" @keyword path: (identifier) @type)

((preproc_include
  _ @keyword path: (_))
  (#any-of? @keyword "#include" "#import"))

;; ── Type qualifiers ─────────────────────────────────────────────────────────

[
  "@optional"
  "@required"
  "__covariant"
  "__contravariant"
  (visibility_specification)
] @keyword

;; ── Storage classes ─────────────────────────────────────────────────────────

[
  "@autoreleasepool"
  "@synthesize"
  "@dynamic"
  "volatile"
  (protocol_qualifier)
] @keyword

;; ── Keywords ────────────────────────────────────────────────────────────────

[
  "@protocol"
  "@interface"
  "@implementation"
  "@compatibility_alias"
  "@property"
  "@selector"
  "@defs"
  "availability"
  "@end"
] @keyword

(class_declaration "@" @keyword "class" @keyword)

(method_definition ["+" "-"] @keyword)
(method_declaration ["+" "-"] @keyword)

[
  "__typeof__"
  "__typeof"
  "typeof"
  "in"
] @operator

[
  "@synchronized"
  "oneway"
] @keyword

;; ── Exceptions ──────────────────────────────────────────────────────────────

[
  "@try"
  "__try"
  "@catch"
  "__catch"
  "@finally"
  "__finally"
  "@throw"
] @keyword

;; ── Variables ───────────────────────────────────────────────────────────────

((identifier) @variable.special
  (#any-of? @variable.special "self" "super"))

;; ── Functions and methods ───────────────────────────────────────────────────

[
  "objc_bridge_related"
  "@available"
  "__builtin_available"
  "va_arg"
  "asm"
] @function

(method_definition (identifier) @function)

(method_declaration (identifier) @function)

(method_identifier (identifier)? @function ":" @function (identifier)? @function)

(message_expression method: (identifier) @function)

;; ── Constructors ────────────────────────────────────────────────────────────

((message_expression method: (identifier) @constructor)
  (#eq? @constructor "init"))

;; ── Attributes ──────────────────────────────────────────────────────────────

(availability_attribute_specifier
  [
    "CF_FORMAT_FUNCTION" "NS_AVAILABLE" "__IOS_AVAILABLE" "NS_AVAILABLE_IOS"
    "API_AVAILABLE" "API_UNAVAILABLE" "API_DEPRECATED" "NS_ENUM_AVAILABLE_IOS"
    "NS_DEPRECATED_IOS" "NS_ENUM_DEPRECATED_IOS" "NS_FORMAT_FUNCTION" "DEPRECATED_MSG_ATTRIBUTE"
    "__deprecated_msg" "__deprecated_enum_msg" "NS_SWIFT_NAME" "NS_SWIFT_UNAVAILABLE"
    "NS_EXTENSION_UNAVAILABLE_IOS" "NS_CLASS_AVAILABLE_IOS" "NS_CLASS_DEPRECATED_IOS" "__OSX_AVAILABLE_STARTING"
    "NS_ROOT_CLASS" "NS_UNAVAILABLE" "NS_REQUIRES_NIL_TERMINATION" "CF_RETURNS_RETAINED"
    "CF_RETURNS_NOT_RETAINED" "DEPRECATED_ATTRIBUTE" "UI_APPEARANCE_SELECTOR" "UNAVAILABLE_ATTRIBUTE"
  ]) @attribute

;; ── Memory management and nullability qualifiers ────────────────────────────

(type_qualifier
  [
    "_Complex"
    "_Nonnull"
    "_Nullable"
    "_Nullable_result"
    "_Null_unspecified"
    "__autoreleasing"
    "__block"
    "__bridge"
    "__bridge_retained"
    "__bridge_transfer"
    "__complex"
    "__kindof"
    "__nonnull"
    "__nullable"
    "__ptrauth_objc_class_ro"
    "__ptrauth_objc_isa_pointer"
    "__ptrauth_objc_super_pointer"
    "__strong"
    "__thread"
    "__unsafe_unretained"
    "__unused"
    "__weak"
  ]) @keyword

[ "__real" "__imag" ] @keyword

;; ── Macro type specifiers (NS_ENUM, NS_OPTIONS) ────────────────────────────
;; The grammar parses NS_ENUM(Type, Name) via macro_type_specifier but produces
;; ERROR nodes for the second argument and the enum body. When the enum body is
;; fully inside an ERROR node (NS_OPTIONS case), entries become bare identifiers
;; in assignment_expression / comma_expression instead of proper enumerator nodes.

;; Highlight the macro name (NS_ENUM, NS_OPTIONS, CF_ENUM, etc.)
(macro_type_specifier
  name: (identifier) @function)

;; Identifiers inside the ERROR child of macro_type_specifier (the second macro
;; argument, e.g. TaskFlags in NS_OPTIONS(NSUInteger, TaskFlags))
(macro_type_specifier
  (ERROR
    (identifier) @type))

;; Enum entries recovered as assignment_expression left-hand sides inside ERROR
(ERROR
  (assignment_expression
    left: (identifier) @constant))

;; Nested in comma_expression chains (second entry onward)
(ERROR
  (comma_expression
    left: (assignment_expression
      left: (identifier) @constant)))

;; ── Types ───────────────────────────────────────────────────────────────────

(class_declaration (identifier) @type)

(class_interface "@interface" . (identifier) @type superclass: _? @type)

(class_implementation "@implementation" . (identifier) @type superclass: _? @type)

(protocol_forward_declaration (identifier) @type)

(protocol_reference_list (identifier) @type)

[
  "BOOL"
  "IMP"
  "SEL"
  "Class"
  "id"
] @type

;; ── Constants ───────────────────────────────────────────────────────────────

(property_attribute (identifier) @constant "="?)

[ "__asm" "__asm__" ] @constant

;; ── Properties ──────────────────────────────────────────────────────────────

(property_implementation "@synthesize" (identifier) @property)

((identifier) @property
  (#has-ancestor? @property struct_declaration))

;; ── Parameters ──────────────────────────────────────────────────────────────

(method_parameter ":" @function (identifier) @variable)

(method_parameter declarator: (identifier) @variable)

(parameter_declaration
  declarator: (function_declarator
                declarator: (parenthesized_declarator
                              (block_pointer_declarator
                                declarator: (identifier) @variable))))

"..." @variable

;; ── Operators ───────────────────────────────────────────────────────────────

[
  "^"
] @operator

;; ── Literals ────────────────────────────────────────────────────────────────

(platform) @string

(version_number) @number

;; ── Punctuation ─────────────────────────────────────────────────────────────

"@" @punctuation.special

[ "<" ">" ] @punctuation.bracket
"##;

/// The full highlights query: the C base from arborium_c followed by our remapped Objective-C layer.
static OBJC_HIGHLIGHTS_QUERY: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    format!(
        "{}\n{}",
        arborium_c::HIGHLIGHTS_QUERY,
        OBJC_HIGHLIGHTS_QUERY_LAYER
    )
});

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_objective_c_support_registers_language() {
        super::add_objective_c_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("objective-c")
                .is_some()
        );
    }
}
