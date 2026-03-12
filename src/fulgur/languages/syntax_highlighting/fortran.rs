use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

/// Add Fortran language support.
pub fn add_fortran_support() {
    LanguageRegistry::singleton().register(
        "fortran",
        &LanguageConfig::new(
            "fortran",
            tree_sitter_fortran::LANGUAGE.into(),
            vec![],
            FORTRAN_HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    );
}

/// Highlights query for Fortran, remapped from the tree-sitter-fortran nvim-treesitter naming
/// conventions (`@keyword.function`, `@conditional`, `@repeat`, `@include`, `@namespace`,
/// `@parameter`) to the gpui-component recognized names (`@keyword`, `@type`, `@variable`, etc.).
const FORTRAN_HIGHLIGHTS_QUERY: &str = r#"
(identifier) @variable
(string_literal) @string
(number_literal) @number
(boolean_literal) @boolean
(comment) @comment

[
 (intrinsic_type)
 "allocatable"
 "attributes"
 "device"
 "dimension"
 "endtype"
 "global"
 "grid_global"
 "host"
 "import"
 "in"
 "inout"
 "intent"
 "optional"
 "out"
 "pointer"
 "type"
 "value"
 ] @type

[
 "contains"
 "private"
 "public"
 ] @keyword

[
 (none)
 "implicit"
 ] @attribute

[
 "endfunction"
 "endprogram"
 "endsubroutine"
 "function"
 "procedure"
 "subroutine"
 ] @keyword

[
 (default)
 (procedure_qualifier)
 "abstract"
 "bind"
 "call"
 "class"
 "continue"
 "cycle"
 "endenum"
 "endinterface"
 "endmodule"
 "endprocedure"
 "endprogram"
 "endsubmodule"
 "enum"
 "enumerator"
 "equivalence"
 "exit"
 "extends"
 "format"
 "goto"
 "include"
 "interface"
 "intrinsic"
 "non_intrinsic"
 "module"
 "namelist"
 "only"
 "parameter"
 "print"
 "procedure"
 "program"
 "read"
 "stop"
 "submodule"
 "use"
 "write"
 ] @keyword

"return" @keyword

[
 "else"
 "elseif"
 "elsewhere"
 "endif"
 "endwhere"
 "if"
 "then"
 "where"
 ] @keyword

[
 "do"
 "enddo"
 "forall"
 "while"
 ] @keyword

[
 "*"
 "+"
 "-"
 "/"
 "="
 "<"
 ">"
 "<="
 ">="
 "=="
 "/="
 ] @operator

[
 "\\.and\\."
 "\\.or\\."
 "\\.lt\\."
 "\\.gt\\."
 "\\.ge\\."
 "\\.le\\."
 "\\.eq\\."
 "\\.eqv\\."
 "\\.neqv\\."
 ] @operator

[
 "("
 ")"
 "["
 "]"
 "<<<"
 ">>>"
 ] @punctuation.bracket

[
 "::"
 ","
 "%"
 ] @punctuation.delimiter

(parameters
  (identifier) @variable)

(program_statement
  (name) @type)

(module_statement
  (name) @type)

(submodule_statement
  (module_name) (name) @type)

(function_statement
  (name) @function)

(subroutine_statement
  (name) @function)

(module_procedure_statement
  (name) @function)

(end_program_statement
  (name) @type)

(end_module_statement
  (name) @type)

(end_submodule_statement
  (name) @type)

(end_function_statement
  (name) @function)

(end_subroutine_statement
  (name) @function)

(end_module_procedure_statement
  (name) @function)

(subroutine_call
  (identifier) @function)

(keyword_argument
  name: (identifier) @keyword)

(derived_type_member_expression
  (type_member) @property)
"#;

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_fortran_support_registers_language() {
        super::add_fortran_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("fortran")
                .is_some()
        );
    }
}
