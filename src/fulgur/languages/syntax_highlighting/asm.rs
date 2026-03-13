use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

/// Add Assembly language syntax highlighting support.
pub fn add_asm_support() {
    LanguageRegistry::singleton().register(
        "asm",
        &LanguageConfig::new(
            "asm",
            arborium_asm::language().into(),
            vec![],
            ASM_HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    );
}

/// Highlights query for Assembly, adapted from the arborium_asm bundled query.
///
/// Key mapping choices vs. the upstream tree-sitter-asm highlights.scm:
/// - `@variable.builtin` is used for registers — this capture is defined in all
///   bundled themes, giving registers a distinct color (typically blue).
/// - `@function.builtin` is used for instruction mnemonics (e.g. `mov`, `push`) and
///   assembler directives (e.g. `.section`, `.global`) — both use the `kind` field.
/// - `@label` is used for labels (e.g. `_start:`, `main:`) — this capture exists in
///   all bundled themes.
/// - `@number.float` is kept separate from `@number` so themes can color them differently.
/// - `@spell` from the upstream query is omitted — it is neovim-specific and has no
///   equivalent in gpui-component.
const ASM_HIGHLIGHTS_QUERY: &str = r#"
; Labels (e.g. _start:, .loop:, main:)
(label
  [(ident) (word)] @label)

; Registers (e.g. %rax, eax, rsp, x0)
(reg) @variable.builtin

; Instruction mnemonics (e.g. mov, push, call, ret)
(instruction
  kind: (_) @function.builtin)

; Assembler directives (e.g. .section, .global, .byte)
(meta
  kind: (_) @function.builtin)

; Named constants (const name = value)
(const
  name: (word) @constant)

; Comments
[
  (line_comment)
  (block_comment)
] @comment

; Integer literals (decimal, hex, binary, octal)
(int) @number

; Floating-point literals
(float) @number.float

; String literals
(string) @string

; Size specifiers and pointer keywords
[
  "byte"
  "word"
  "dword"
  "qword"
  "ptr"
  "rel"
  "label"
  "const"
] @keyword

; Arithmetic and bitwise operators
[
  "+"
  "-"
  "*"
  "/"
  "%"
  "|"
  "^"
  "&"
] @operator

; Brackets and parentheses
[
  "("
  ")"
  "["
  "]"
] @punctuation.bracket

; Commas and colons
[
  ","
  ":"
] @punctuation.delimiter
"#;

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_asm_support_registers_language() {
        super::add_asm_support();
        assert!(
            gpui_component::highlighter::LanguageRegistry::singleton()
                .language("asm")
                .is_some()
        );
    }
}
