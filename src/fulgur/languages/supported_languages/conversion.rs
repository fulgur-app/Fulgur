use super::SupportedLanguage;
use gpui_kit::component::highlighter::Language;

/// Convert a Language to a `SupportedLanguage`
///
/// ### Arguments
/// - `language`: The Language enum
///
/// ### Returns
/// - `SupportedLanguage`: The corresponding `SupportedLanguage`
#[must_use]
pub fn from_language(language: Language) -> SupportedLanguage {
    match language {
        Language::Astro => SupportedLanguage::Astro,
        Language::Bash => SupportedLanguage::Bash,
        Language::C => SupportedLanguage::C,
        Language::CMake => SupportedLanguage::CMake,
        Language::CSharp => SupportedLanguage::CSharp,
        Language::Cpp => SupportedLanguage::Cpp,
        Language::Css => SupportedLanguage::Css,
        Language::Diff => SupportedLanguage::Diff,
        Language::Ejs => SupportedLanguage::Ejs,
        Language::Elixir => SupportedLanguage::Elixir,
        Language::Erb => SupportedLanguage::Erb,
        Language::Go => SupportedLanguage::Go,
        Language::GraphQL => SupportedLanguage::GraphQl,
        Language::Html => SupportedLanguage::Html,
        Language::Java => SupportedLanguage::Java,
        Language::JavaScript => SupportedLanguage::JavaScript,
        Language::JsDoc => SupportedLanguage::JsDoc,
        Language::Json => SupportedLanguage::Json,
        Language::Kotlin => SupportedLanguage::Kotlin,
        Language::Lua => SupportedLanguage::Lua,
        Language::Make => SupportedLanguage::Make,
        Language::Markdown => SupportedLanguage::Markdown,
        Language::MarkdownInline => SupportedLanguage::MarkdownInline,
        Language::Php => SupportedLanguage::Php,
        Language::Plain => SupportedLanguage::Plain,
        Language::Proto => SupportedLanguage::Proto,
        Language::Python => SupportedLanguage::Python,
        Language::Ruby => SupportedLanguage::Ruby,
        Language::Rust => SupportedLanguage::Rust,
        Language::Scala => SupportedLanguage::Scala,
        Language::Sql => SupportedLanguage::Sql,
        Language::Svelte => SupportedLanguage::Svelte,
        Language::Swift => SupportedLanguage::Swift,
        Language::Toml => SupportedLanguage::Toml,
        Language::Tsx => SupportedLanguage::React,
        Language::TypeScript => SupportedLanguage::TypeScript,
        Language::Yaml => SupportedLanguage::Yaml,
        Language::Zig => SupportedLanguage::Zig,
    }
}

/// Convert a supported language to a language
///
/// ### Arguments
/// - `supported_language`: The supported language to convert
///
/// ### Returns
/// The corresponding language
#[must_use]
pub fn to_language(supported_language: &SupportedLanguage) -> Language {
    match supported_language {
        // Custom-registered languages: no native Language variant, plain text fallback.
        // The actual grammar is loaded by language_registry_name() at runtime.
        SupportedLanguage::Astro => Language::Astro,
        SupportedLanguage::Bash => Language::Bash,
        SupportedLanguage::C => Language::C,
        SupportedLanguage::CMake => Language::CMake,
        SupportedLanguage::CSharp => Language::CSharp,
        SupportedLanguage::Cpp => Language::Cpp,
        SupportedLanguage::Css => Language::Css,
        SupportedLanguage::Diff => Language::Diff,
        SupportedLanguage::Ejs => Language::Ejs,
        SupportedLanguage::Elixir => Language::Elixir,
        SupportedLanguage::Erb => Language::Erb,
        SupportedLanguage::Go => Language::Go,
        SupportedLanguage::GraphQl => Language::GraphQL,
        SupportedLanguage::Html | SupportedLanguage::Svg => Language::Html,
        SupportedLanguage::Java => Language::Java,
        SupportedLanguage::JavaScript => Language::JavaScript,
        SupportedLanguage::JsDoc => Language::JsDoc,
        SupportedLanguage::Json => Language::Json,
        SupportedLanguage::Kotlin => Language::Kotlin,
        SupportedLanguage::Lua => Language::Lua,
        SupportedLanguage::Make => Language::Make,
        SupportedLanguage::Markdown => Language::Markdown,
        SupportedLanguage::MarkdownInline => Language::MarkdownInline,
        SupportedLanguage::Php => Language::Php,
        SupportedLanguage::Proto => Language::Proto,
        SupportedLanguage::Python => Language::Python,
        SupportedLanguage::Ruby => Language::Ruby,
        SupportedLanguage::Rust => Language::Rust,
        SupportedLanguage::Scala => Language::Scala,
        SupportedLanguage::Sql => Language::Sql,
        SupportedLanguage::Svelte => Language::Svelte,
        SupportedLanguage::Swift => Language::Swift,
        SupportedLanguage::Toml => Language::Toml,
        SupportedLanguage::TypeScript => Language::TypeScript,
        SupportedLanguage::Yaml => Language::Yaml,
        SupportedLanguage::Zig => Language::Zig,
        SupportedLanguage::Ada
        | SupportedLanguage::Asm
        | SupportedLanguage::Clojure
        | SupportedLanguage::Csv
        | SupportedLanguage::D
        | SupportedLanguage::Dart
        | SupportedLanguage::Dockerfile
        | SupportedLanguage::Erlang
        | SupportedLanguage::Fortran
        | SupportedLanguage::FSharp
        | SupportedLanguage::Groovy
        | SupportedLanguage::Haskell
        | SupportedLanguage::Ini
        | SupportedLanguage::Jinja2
        | SupportedLanguage::Julia
        | SupportedLanguage::Matlab
        | SupportedLanguage::ObjectiveC
        | SupportedLanguage::Ocaml
        | SupportedLanguage::Pascal
        | SupportedLanguage::Perl
        | SupportedLanguage::Plain
        | SupportedLanguage::Powershell
        | SupportedLanguage::Prolog
        | SupportedLanguage::R
        | SupportedLanguage::React
        | SupportedLanguage::Scss
        | SupportedLanguage::Vue
        | SupportedLanguage::Xml => Language::Plain,
    }
}

/// Get the pretty name (human-readable) of a `SupportedLanguage`
///
/// ### Arguments
/// - `language`: The `SupportedLanguage` enum
///
/// ### Returns
/// - `&'static str`: The human-readable name of the language
#[must_use]
pub fn pretty_name(language: &SupportedLanguage) -> &'static str {
    match language {
        SupportedLanguage::Ada => "Ada",
        SupportedLanguage::Asm => "Assembly",
        SupportedLanguage::Astro => "Astro",
        SupportedLanguage::Bash => "Bash",
        SupportedLanguage::C => "C",
        SupportedLanguage::Clojure => "Clojure",
        SupportedLanguage::CMake => "CMake",
        SupportedLanguage::CSharp => "C#",
        SupportedLanguage::Cpp => "C++",
        SupportedLanguage::Css => "CSS",
        SupportedLanguage::Csv => "CSV",
        SupportedLanguage::D => "D",
        SupportedLanguage::Dart => "Dart",
        SupportedLanguage::Diff => "Diff",
        SupportedLanguage::Dockerfile => "Dockerfile",
        SupportedLanguage::Ejs => "EJS",
        SupportedLanguage::Elixir => "Elixir",
        SupportedLanguage::Erb => "ERB",
        SupportedLanguage::Erlang => "Erlang",
        SupportedLanguage::Fortran => "Fortran",
        SupportedLanguage::FSharp => "F#",
        SupportedLanguage::Go => "Go",
        SupportedLanguage::GraphQl => "GraphQL",
        SupportedLanguage::Groovy => "Groovy",
        SupportedLanguage::Haskell => "Haskell",
        SupportedLanguage::Html => "HTML",
        SupportedLanguage::Ini => "INI",
        SupportedLanguage::Jinja2 => "Jinja2",
        SupportedLanguage::Java => "Java",
        SupportedLanguage::JavaScript => "JavaScript",
        SupportedLanguage::JsDoc => "JSDoc",
        SupportedLanguage::Json => "JSON",
        SupportedLanguage::Julia => "Julia",
        SupportedLanguage::Kotlin => "Kotlin",
        SupportedLanguage::Lua => "Lua",
        SupportedLanguage::Make => "Make",
        SupportedLanguage::Markdown => "Markdown",
        SupportedLanguage::Matlab => "MATLAB",
        SupportedLanguage::MarkdownInline => "Markdown Inline",
        SupportedLanguage::ObjectiveC => "Objective-C",
        SupportedLanguage::Ocaml => "OCaml",
        SupportedLanguage::Pascal => "Pascal",
        SupportedLanguage::Perl => "Perl",
        SupportedLanguage::Php => "PHP",
        SupportedLanguage::Plain => "Plain Text",
        SupportedLanguage::Powershell => "PowerShell",
        SupportedLanguage::Prolog => "Prolog",
        SupportedLanguage::Proto => "Protocol Buffers",
        SupportedLanguage::Python => "Python",
        SupportedLanguage::R => "R",
        SupportedLanguage::React => "React",
        SupportedLanguage::Ruby => "Ruby",
        SupportedLanguage::Rust => "Rust",
        SupportedLanguage::Scala => "Scala",
        SupportedLanguage::Scss => "SCSS",
        SupportedLanguage::Sql => "SQL",
        SupportedLanguage::Svg => "SVG",
        SupportedLanguage::Svelte => "Svelte",
        SupportedLanguage::Swift => "Swift",
        SupportedLanguage::Toml => "TOML",
        SupportedLanguage::TypeScript => "TypeScript",
        SupportedLanguage::Vue => "Vue",
        SupportedLanguage::Xml => "XML",
        SupportedLanguage::Yaml => "YAML",
        SupportedLanguage::Zig => "Zig",
    }
}

/// Get the language registry name for a `SupportedLanguage`.
/// For languages built into gpui-component, this delegates to `to_language().name()`.
/// For custom-registered languages (e.g. Perl), this returns the registered name directly.
///
/// ### Arguments
/// - `supported_language`: The supported language to convert
///
/// ### Returns
/// - `&'static str`: The language name as registered in the `LanguageRegistry`
#[must_use]
pub fn language_registry_name(supported_language: &SupportedLanguage) -> &'static str {
    match supported_language {
        SupportedLanguage::Ada => "ada",
        SupportedLanguage::Asm => "asm",
        SupportedLanguage::Clojure => "clojure",
        SupportedLanguage::Erlang => "erlang",
        SupportedLanguage::Fortran => "fortran",
        SupportedLanguage::FSharp => "fsharp",
        SupportedLanguage::D => "d",
        SupportedLanguage::Dart => "dart",
        SupportedLanguage::Dockerfile => "dockerfile",
        SupportedLanguage::Groovy => "groovy",
        SupportedLanguage::Haskell => "haskell",
        SupportedLanguage::Ini => "ini",
        SupportedLanguage::Jinja2 => "jinja2",
        SupportedLanguage::Julia => "julia",
        SupportedLanguage::Matlab => "matlab",
        SupportedLanguage::ObjectiveC => "objective-c",
        SupportedLanguage::Ocaml => "ocaml",
        SupportedLanguage::Pascal => "pascal",
        SupportedLanguage::Perl => "perl",
        SupportedLanguage::Powershell => "powershell",
        SupportedLanguage::Prolog => "prolog",
        SupportedLanguage::R => "r",
        SupportedLanguage::React => "react",
        SupportedLanguage::Scss => "scss",
        SupportedLanguage::Vue => "vue",
        SupportedLanguage::Xml => "html",
        other => to_language(other).name(),
    }
}

#[cfg(test)]
mod tests {
    use super::{SupportedLanguage, from_language, pretty_name, to_language};
    use gpui_kit::component::highlighter::Language;
    use std::collections::HashSet;

    /// Supported languages that borrow another language's native grammar, so
    /// mapping them to a native language and back lands on the grammar's owner.
    const SHARED_NATIVE_GRAMMAR: &[(SupportedLanguage, SupportedLanguage)] =
        &[(SupportedLanguage::Svg, SupportedLanguage::Html)];

    /// Native languages Fulgur highlights with a custom-registered grammar
    /// instead, so their supported language maps back to `Language::Plain`.
    const CUSTOM_GRAMMAR_OVERRIDES: &[Language] = &[Language::Tsx];

    #[test]
    fn test_every_native_language_round_trips_through_supported_language() {
        for language in Language::all() {
            if CUSTOM_GRAMMAR_OVERRIDES.contains(&language) {
                continue;
            }
            assert_eq!(
                to_language(&from_language(language)),
                language,
                "{language:?} must map to a supported language that maps back to it"
            );
        }
    }

    #[test]
    fn test_natively_backed_supported_languages_round_trip() {
        for language in SupportedLanguage::all() {
            let native = to_language(language);
            if native == Language::Plain && *language != SupportedLanguage::Plain {
                continue;
            }
            let expected = SHARED_NATIVE_GRAMMAR
                .iter()
                .find(|(borrower, _)| borrower == language)
                .map_or(*language, |(_, owner)| *owner);
            assert_eq!(
                from_language(native),
                expected,
                "{language:?} maps to {native:?}, which must map back to {expected:?}"
            );
        }
    }

    #[test]
    fn test_custom_grammar_overrides_fall_back_to_plain() {
        for language in CUSTOM_GRAMMAR_OVERRIDES {
            assert_eq!(
                to_language(&from_language(*language)),
                Language::Plain,
                "{language:?} is listed as overridden, so its supported language must not map back to it"
            );
        }
    }

    #[test]
    fn test_all_languages_have_a_distinct_pretty_name() {
        let mut names = HashSet::new();
        for language in SupportedLanguage::all() {
            let name = pretty_name(language);
            assert!(
                names.insert(name),
                "duplicate pretty name in the language picker: {name}"
            );
        }
        assert_eq!(names.len(), SupportedLanguage::all().len());
    }
}
