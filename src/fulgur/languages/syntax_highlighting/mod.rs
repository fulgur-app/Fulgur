pub mod ada;
pub mod asm;
pub mod clojure;
pub mod d;
pub mod dart;
pub mod dockerfile;
pub mod erlang;
pub mod fortran;
pub mod fsharp;
pub mod groovy;
pub mod haskell;
pub mod ini;
pub mod jinja2;
pub mod julia;
pub mod matlab;
pub mod objective_c;
pub mod ocaml;
pub mod pascal;
pub mod perl;
pub mod powershell;
pub mod prolog;
pub mod r;
pub mod react;
pub mod scss;
pub mod vue;

#[cfg(test)]
pub mod test_support {
    use gpui_kit::component::highlighter::LanguageRegistry;

    /// Assert that every Tree-sitter query registered for a language actually compiles.
    ///
    /// ### Arguments
    /// - `language`: Key the language was registered under in the `LanguageRegistry`.
    pub fn assert_queries_compile(language: &str) {
        let config = LanguageRegistry::singleton()
            .language(language)
            .unwrap_or_else(|| panic!("language `{language}` is not registered"));

        let grammar = config
            .language
            .as_ref()
            .unwrap_or_else(|| panic!("language `{language}` is registered without a grammar"));

        assert!(
            !config.highlights.is_empty(),
            "language `{language}` has an empty highlights query"
        );

        let queries = [
            ("highlights", config.highlights.as_ref()),
            ("injections", config.injections.as_ref()),
            ("locals", config.locals.as_ref()),
        ];

        for (kind, source) in queries {
            if source.is_empty() {
                continue;
            }

            if let Err(error) = tree_sitter::Query::new(grammar, source) {
                panic!("language `{language}` has an invalid {kind} query: {error}");
            }
        }
    }
}
