mod content_detection;
mod conversion;
mod filename_detection;

pub use content_detection::language_from_content;
pub use conversion::{from_language, language_registry_name, pretty_name, to_language};
pub use filename_detection::language_from_filename;

use crate::fulgur::Fulgur;

/// Declares the `SupportedLanguage` enum and its `ALL` list from a single variant list.
///
/// Both artifacts come from the same declaration, so a variant cannot be added to the enum
/// without also appearing in `ALL` (and therefore in the language picker).
macro_rules! supported_languages {
    ($($variant:ident),+ $(,)?) => {
        /// Lists all supported languages, including some that are not supported by the language registry but are close enough.
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub enum SupportedLanguage {
            $($variant,)+
        }

        impl SupportedLanguage {
            /// Every supported language, in declaration order.
            pub const ALL: &[SupportedLanguage] = &[$(SupportedLanguage::$variant),+];
        }
    };
}

supported_languages! {
    Ada,
    Asm,
    Astro,
    Bash,
    C,
    Clojure,
    CMake,
    CSharp,
    Cpp,
    Css,
    Csv,
    D,
    Dart,
    Diff,
    Dockerfile,
    Ejs,
    Elixir,
    Erb,
    Erlang,
    Fortran,
    FSharp,
    Go,
    GraphQl,
    Groovy,
    Haskell,
    Html,
    Ini,
    Jinja2,
    Java,
    JavaScript,
    JsDoc,
    Json,
    Julia,
    Kotlin,
    Lua,
    Make,
    Markdown,
    MarkdownInline,
    Matlab,
    ObjectiveC,
    Ocaml,
    Pascal,
    Perl,
    Php,
    Plain,
    Powershell,
    Prolog,
    Proto,
    Python,
    R,
    React,
    Ruby,
    Rust,
    Scala,
    Scss,
    Sql,
    Svelte,
    Svg,
    Swift,
    Toml,
    TypeScript,
    Vue,
    Xml,
    Yaml,
    Zig,
}

impl SupportedLanguage {
    /// Lists all supported languages in alphabetical order, including some that are not supported by the language registry.
    ///
    /// ### Returns
    /// - `&'static [SupportedLanguage]`: Every supported language, in picker display order
    #[must_use]
    pub fn all() -> &'static [SupportedLanguage] {
        Self::ALL
    }
}

impl Fulgur {
    /// Get the current language of the active tab
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `SupportedLanguage`: The active tab's language
    #[must_use]
    pub fn get_current_language(&self, cx: &gpui_kit::App) -> SupportedLanguage {
        self.active_tab(cx)
            .and_then(crate::fulgur::tab::Tab::as_editor)
            .map_or(SupportedLanguage::Plain, |editor_tab| editor_tab.language)
    }

    /// Check if the current active tab's language is a Markdown language
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `True` if the active tab's language is a Markdown language, `False` otherwise
    #[must_use]
    pub fn is_markdown(&self, cx: &gpui_kit::App) -> bool {
        let current_language = self.get_current_language(cx);
        current_language == SupportedLanguage::Markdown
            || current_language == SupportedLanguage::MarkdownInline
    }
}

/// Register external languages that are not supported by default by the editor
pub fn register_external_languages() {
    super::syntax_highlighting::ada::add_ada_support();
    super::syntax_highlighting::asm::add_asm_support();
    super::syntax_highlighting::clojure::add_clojure_support();
    super::syntax_highlighting::d::add_d_support();
    super::syntax_highlighting::dart::add_dart_support();
    super::syntax_highlighting::dockerfile::add_dockerfile_support();
    super::syntax_highlighting::erlang::add_erlang_support();
    super::syntax_highlighting::fortran::add_fortran_support();
    super::syntax_highlighting::fsharp::add_fsharp_support();
    super::syntax_highlighting::groovy::add_groovy_support();
    super::syntax_highlighting::haskell::add_haskell_support();
    super::syntax_highlighting::ini::add_ini_support();
    super::syntax_highlighting::jinja2::add_jinja2_support();
    super::syntax_highlighting::julia::add_julia_support();
    super::syntax_highlighting::matlab::add_matlab_support();
    super::syntax_highlighting::objective_c::add_objective_c_support();
    super::syntax_highlighting::ocaml::add_ocaml_support();
    super::syntax_highlighting::pascal::add_pascal_support();
    super::syntax_highlighting::perl::add_perl_support();
    super::syntax_highlighting::powershell::add_powershell_support();
    super::syntax_highlighting::prolog::add_prolog_support();
    super::syntax_highlighting::r::add_r_support();
    super::syntax_highlighting::react::add_react_support();
    super::syntax_highlighting::scss::add_scss_support();
    super::syntax_highlighting::vue::add_vue_support();
}
