//! Per-language automatic pairing and indentation rules.

use gpui_kit::App;
use gpui_kit::component::input::language_config::LanguageConfig;
use gpui_kit::component::input::{
    AutoClosingPair, BracketPair, IndentationRules, SyntaxContext, set_language_config,
};
use regex::Regex;
use std::sync::LazyLock;

use super::supported_languages::{SupportedLanguage, language_registry_name};

/// Indentation increases after a line ending on an opening structural delimiter.
static STRUCTURAL_INDENT: LazyLock<IndentationRules> = LazyLock::new(|| {
    IndentationRules::new(
        Regex::new(r"[\{\(\[]\s*$").unwrap_or_else(|error| unreachable!("{error}")),
        Regex::new(r"^\s*[\}\)\]]").unwrap_or_else(|error| unreachable!("{error}")),
    )
});

/// Indentation increases after a mapping key, a sequence dash or an opening delimiter.
static YAML_INDENT: LazyLock<IndentationRules> = LazyLock::new(|| {
    IndentationRules::new(
        Regex::new(r"(:|-|[\{\(\[])\s*$").unwrap_or_else(|error| unreachable!("{error}")),
        Regex::new(r"^\s*[\}\)\]]").unwrap_or_else(|error| unreachable!("{error}")),
    )
});

/// A group of languages that share the same editing rules.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EditingFamily {
    /// Structural delimiters, both quote styles and a `/* */` block comment.
    Braces,
    /// [`EditingFamily::Braces`] without the apostrophe, which marks a lifetime
    /// or a type variable rather than a string.
    BracesNoApostrophe,
    /// Structural delimiters and both quote styles, for languages whose only
    /// comment form is a line comment.
    LineComments,
    /// [`EditingFamily::LineComments`] without the apostrophe.
    LineCommentsNoApostrophe,
    /// Flow delimiters and quotes, with a colon or a dash opening a block.
    Yaml,
    /// Table and inline-table delimiters, with quotes.
    Toml,
    /// Quotes only, for key and value configuration files.
    KeyValue,
    /// Attribute quotes and the `<!-- -->` comment pair.
    Markup,
    /// Link, emphasis and code delimiters, and never an apostrophe.
    Prose,
    /// No automatic pairing and no indentation rules.
    Inert,
}

/// Register Fulgur's editing rules for every language that has an explicit family.
///
/// ### Arguments
/// - `cx`: The application context
pub fn register_language_configs(cx: &mut App) {
    for language in SupportedLanguage::all() {
        if let Some(family) = editing_family(*language) {
            set_language_config(language_registry_name(language), family.config(), cx);
        }
    }
}

/// Resolve the editing family a language belongs to.
///
/// ### Arguments
/// - `language`: The language to classify
///
/// ### Returns
/// - `Option<EditingFamily>`: The family, or `None` to keep the editor's default
fn editing_family(language: SupportedLanguage) -> Option<EditingFamily> {
    let family = match language {
        SupportedLanguage::Astro
        | SupportedLanguage::C
        | SupportedLanguage::CSharp
        | SupportedLanguage::Cpp
        | SupportedLanguage::Css
        | SupportedLanguage::D
        | SupportedLanguage::Dart
        | SupportedLanguage::Ejs
        | SupportedLanguage::Erb
        | SupportedLanguage::Go
        | SupportedLanguage::Groovy
        | SupportedLanguage::Java
        | SupportedLanguage::JavaScript
        | SupportedLanguage::JsDoc
        | SupportedLanguage::Jinja2
        | SupportedLanguage::Kotlin
        | SupportedLanguage::ObjectiveC
        | SupportedLanguage::Php
        | SupportedLanguage::Proto
        | SupportedLanguage::React
        | SupportedLanguage::Scala
        | SupportedLanguage::Scss
        | SupportedLanguage::Sql
        | SupportedLanguage::Svelte
        | SupportedLanguage::Swift
        | SupportedLanguage::TypeScript
        | SupportedLanguage::Vue
        | SupportedLanguage::Zig => EditingFamily::Braces,
        SupportedLanguage::Rust => EditingFamily::BracesNoApostrophe,
        SupportedLanguage::Bash
        | SupportedLanguage::CMake
        | SupportedLanguage::Dockerfile
        | SupportedLanguage::Elixir
        | SupportedLanguage::GraphQl
        | SupportedLanguage::Julia
        | SupportedLanguage::Lua
        | SupportedLanguage::Make
        | SupportedLanguage::Perl
        | SupportedLanguage::Powershell
        | SupportedLanguage::R
        | SupportedLanguage::Ruby => EditingFamily::LineComments,
        SupportedLanguage::Ada
        | SupportedLanguage::Asm
        | SupportedLanguage::Clojure
        | SupportedLanguage::Erlang
        | SupportedLanguage::FSharp
        | SupportedLanguage::Fortran
        | SupportedLanguage::Haskell
        | SupportedLanguage::Matlab
        | SupportedLanguage::Ocaml
        | SupportedLanguage::Pascal
        | SupportedLanguage::Prolog => EditingFamily::LineCommentsNoApostrophe,
        SupportedLanguage::Yaml => EditingFamily::Yaml,
        SupportedLanguage::Toml => EditingFamily::Toml,
        SupportedLanguage::Ini => EditingFamily::KeyValue,
        SupportedLanguage::Html | SupportedLanguage::Svg | SupportedLanguage::Xml => {
            EditingFamily::Markup
        }
        SupportedLanguage::Markdown | SupportedLanguage::MarkdownInline => EditingFamily::Prose,
        SupportedLanguage::Csv | SupportedLanguage::Diff => EditingFamily::Inert,
        // JSON, Python and plain text keep the editor's reviewed defaults.
        SupportedLanguage::Json | SupportedLanguage::Python | SupportedLanguage::Plain => {
            return None;
        }
    };
    Some(family)
}

/// Build the three structural bracket pairs shared by most families.
///
/// ### Returns
/// - `Vec<BracketPair>`: Parentheses, square brackets and braces
fn structural_brackets() -> Vec<BracketPair> {
    vec![
        BracketPair::new("(", ")"),
        BracketPair::new("[", "]"),
        BracketPair::new("{", "}"),
    ]
}

/// Restrict automatic pairing to code, so delimiters typed inside a string or a
/// comment are left alone.
///
/// ### Arguments
/// - `pairs`: The pairs to restrict
///
/// ### Returns
/// - `Vec<AutoClosingPair>`: The pairs, each disabled in strings and comments
fn code_only(pairs: impl IntoIterator<Item = AutoClosingPair>) -> Vec<AutoClosingPair> {
    pairs
        .into_iter()
        .map(|pair| pair.not_in([SyntaxContext::String, SyntaxContext::Comment]))
        .collect()
}

impl EditingFamily {
    /// Build the editing configuration for this family.
    ///
    /// ### Returns
    /// - `LanguageConfig`: The brackets, automatic pairs and indentation rules
    fn config(self) -> LanguageConfig {
        match self {
            Self::Braces | Self::BracesNoApostrophe => {
                let mut pairs = vec![
                    AutoClosingPair::new("(", ")"),
                    AutoClosingPair::new("[", "]"),
                    AutoClosingPair::new("{", "}"),
                    AutoClosingPair::new("\"", "\""),
                    AutoClosingPair::new("/*", "*/"),
                ];
                if self == Self::Braces {
                    pairs.push(AutoClosingPair::new("'", "'"));
                }
                LanguageConfig::default()
                    .brackets(structural_brackets())
                    .auto_closing_pairs(code_only(pairs))
                    .indentation_rules(STRUCTURAL_INDENT.clone())
            }
            Self::LineComments | Self::LineCommentsNoApostrophe => {
                let mut pairs = vec![
                    AutoClosingPair::new("(", ")"),
                    AutoClosingPair::new("[", "]"),
                    AutoClosingPair::new("{", "}"),
                    AutoClosingPair::new("\"", "\""),
                ];
                if self == Self::LineComments {
                    pairs.push(AutoClosingPair::new("'", "'"));
                }
                LanguageConfig::default()
                    .brackets(structural_brackets())
                    .auto_closing_pairs(code_only(pairs))
                    .indentation_rules(STRUCTURAL_INDENT.clone())
            }
            Self::Yaml => LanguageConfig::default()
                .brackets([BracketPair::new("{", "}"), BracketPair::new("[", "]")])
                .auto_closing_pairs(code_only([
                    AutoClosingPair::new("{", "}"),
                    AutoClosingPair::new("[", "]"),
                    AutoClosingPair::new("\"", "\""),
                    AutoClosingPair::new("'", "'"),
                ]))
                .indentation_rules(YAML_INDENT.clone()),
            Self::Toml => LanguageConfig::default()
                .brackets([BracketPair::new("{", "}"), BracketPair::new("[", "]")])
                .auto_closing_pairs(code_only([
                    AutoClosingPair::new("{", "}"),
                    AutoClosingPair::new("[", "]"),
                    AutoClosingPair::new("\"", "\""),
                    AutoClosingPair::new("'", "'"),
                ]))
                .indentation_rules(STRUCTURAL_INDENT.clone()),
            Self::KeyValue => LanguageConfig::default()
                .brackets([])
                .auto_closing_pairs(code_only([AutoClosingPair::new("\"", "\"")])),
            Self::Markup => LanguageConfig::default()
                .brackets([BracketPair::new("{", "}")])
                .auto_closing_pairs(code_only([
                    AutoClosingPair::new("{", "}"),
                    AutoClosingPair::new("\"", "\""),
                    AutoClosingPair::new("'", "'"),
                    AutoClosingPair::new("<!--", "-->"),
                ])),
            Self::Prose => LanguageConfig::default().brackets([]).auto_closing_pairs([
                AutoClosingPair::new("[", "]"),
                AutoClosingPair::new("(", ")"),
                AutoClosingPair::new("`", "`"),
            ]),
            Self::Inert => LanguageConfig::default()
                .brackets([])
                .auto_closing_pairs([]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EditingFamily, SupportedLanguage, editing_family};

    fn opening_delimiters(language: SupportedLanguage) -> Vec<String> {
        editing_family(language)
            .expect("language should declare an editing family")
            .config()
            .auto_closing_pairs
            .expect("family should configure automatic pairs")
            .into_iter()
            .map(|pair| pair.open.to_string())
            .collect()
    }

    #[test]
    fn every_language_is_classified_or_deliberately_skipped() {
        let skipped: Vec<_> = SupportedLanguage::all()
            .iter()
            .filter(|language| editing_family(**language).is_none())
            .copied()
            .collect();
        assert_eq!(
            skipped,
            vec![
                SupportedLanguage::Json,
                SupportedLanguage::Plain,
                SupportedLanguage::Python,
            ]
        );
    }

    #[test]
    fn rust_does_not_close_the_lifetime_apostrophe() {
        let delimiters = opening_delimiters(SupportedLanguage::Rust);
        assert!(!delimiters.iter().any(|open| open == "'"));
        assert!(delimiters.iter().any(|open| open == "\""));
        assert!(delimiters.iter().any(|open| open == "/*"));
    }

    #[test]
    fn c_like_languages_close_both_quote_styles() {
        let delimiters = opening_delimiters(SupportedLanguage::TypeScript);
        assert!(delimiters.iter().any(|open| open == "'"));
        assert!(delimiters.iter().any(|open| open == "\""));
    }

    #[test]
    fn shell_scripts_have_no_block_comment_pair() {
        let delimiters = opening_delimiters(SupportedLanguage::Bash);
        assert!(!delimiters.iter().any(|open| open == "/*"));
        assert!(delimiters.iter().any(|open| open == "'"));
    }

    #[test]
    fn prose_never_closes_quotes() {
        let delimiters = opening_delimiters(SupportedLanguage::Markdown);
        assert!(!delimiters.iter().any(|open| open == "'" || open == "\""));
        assert!(delimiters.iter().any(|open| open == "["));
    }

    #[test]
    fn inert_languages_disable_every_pair() {
        assert!(opening_delimiters(SupportedLanguage::Csv).is_empty());
        assert!(opening_delimiters(SupportedLanguage::Diff).is_empty());
    }

    #[test]
    fn markup_closes_the_comment_pair() {
        let delimiters = opening_delimiters(SupportedLanguage::Html);
        assert!(delimiters.iter().any(|open| open == "<!--"));
    }

    #[test]
    fn structural_rules_dedent_on_a_closing_delimiter() {
        let rules = EditingFamily::Braces
            .config()
            .indentation_rules
            .expect("brace languages should declare indentation rules");
        let increase = rules
            .increase_indent_pattern
            .expect("an opening delimiter should increase indentation");
        let decrease = rules
            .decrease_indent_pattern
            .expect("a closing delimiter should decrease indentation");
        assert!(increase.is_match("fn main() {"));
        assert!(!increase.is_match("let value = 1;"));
        assert!(decrease.is_match("    }"));
        assert!(!decrease.is_match("    value"));
    }

    #[test]
    fn yaml_indents_after_a_mapping_key_or_a_sequence_dash() {
        let rules = EditingFamily::Yaml
            .config()
            .indentation_rules
            .expect("YAML should declare indentation rules");
        let increase = rules
            .increase_indent_pattern
            .expect("a mapping key should increase indentation");
        assert!(increase.is_match("jobs:"));
        assert!(increase.is_match("  -"));
        assert!(!increase.is_match("name: build"));
    }
}
