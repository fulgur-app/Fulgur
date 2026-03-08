use gpui_component::highlighter::Language;

use crate::fulgur::Fulgur;

/// Lists all supported languages, including some that are not supported by the language registry but are close enough.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SupportedLanguage {
    Ada,
    Astro,
    Bash,
    C,
    Clojure,
    CMake,
    CSharp,
    Cpp,
    Css,
    Dart,
    Diff,
    Dockerfile,
    Ejs,
    Elixir,
    Erb,
    Go,
    GraphQl,
    Groovy,
    Haskell,
    Html,
    Ini,
    Java,
    JavaScript,
    JsDoc,
    Json,
    Make,
    Markdown,
    MarkdownInline,
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

/// Convert a Language to a SupportedLanguage
///
/// ### Arguments
/// - `language`: The Language enum
///
/// ### Returns
/// - `SupportedLanguage`: The corresponding SupportedLanguage
pub fn from_language(language: Language) -> SupportedLanguage {
    match language {
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
        Language::Make => SupportedLanguage::Make,
        Language::Markdown => SupportedLanguage::Markdown,
        Language::MarkdownInline => SupportedLanguage::MarkdownInline,
        Language::Plain => SupportedLanguage::Plain,
        Language::Proto => SupportedLanguage::Proto,
        Language::Python => SupportedLanguage::Python,
        Language::Ruby => SupportedLanguage::Ruby,
        Language::Rust => SupportedLanguage::Rust,
        Language::Scala => SupportedLanguage::Scala,
        Language::Sql => SupportedLanguage::Sql,
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
pub fn to_language(supported_language: &SupportedLanguage) -> Language {
    match supported_language {
        SupportedLanguage::Astro => Language::Html,
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
        SupportedLanguage::Html => Language::Html,
        SupportedLanguage::Ini => Language::Plain,
        SupportedLanguage::Java => Language::Java,
        SupportedLanguage::JavaScript => Language::JavaScript,
        SupportedLanguage::JsDoc => Language::JsDoc,
        SupportedLanguage::Json => Language::Json,
        SupportedLanguage::Make => Language::Make,
        SupportedLanguage::Markdown => Language::Markdown,
        SupportedLanguage::MarkdownInline => Language::MarkdownInline,
        SupportedLanguage::Php => Language::Html,
        SupportedLanguage::Plain => Language::Plain,
        SupportedLanguage::Proto => Language::Proto,
        SupportedLanguage::Python => Language::Python,
        SupportedLanguage::Ruby => Language::Ruby,
        SupportedLanguage::Rust => Language::Rust,
        SupportedLanguage::Scala => Language::Scala,
        SupportedLanguage::Sql => Language::Sql,
        SupportedLanguage::Svelte => Language::TypeScript,
        SupportedLanguage::Svg => Language::Html,
        SupportedLanguage::Swift => Language::Swift,
        SupportedLanguage::Toml => Language::Toml,
        SupportedLanguage::TypeScript => Language::TypeScript,
        SupportedLanguage::Yaml => Language::Yaml,
        SupportedLanguage::Zig => Language::Zig,
        _ => Language::Plain,
    }
}

/// Get the pretty name (human-readable) of a SupportedLanguage
///
/// ### Arguments
/// - `language`: The SupportedLanguage enum
///
/// ### Returns
/// - `String`: The human-readable name of the language
pub fn pretty_name(language: &SupportedLanguage) -> String {
    match language {
        SupportedLanguage::Ada => "Ada".to_string(),
        SupportedLanguage::Astro => "Astro".to_string(),
        SupportedLanguage::Bash => "Bash".to_string(),
        SupportedLanguage::C => "C".to_string(),
        SupportedLanguage::Clojure => "Clojure".to_string(),
        SupportedLanguage::CMake => "CMake".to_string(),
        SupportedLanguage::CSharp => "C#".to_string(),
        SupportedLanguage::Cpp => "C++".to_string(),
        SupportedLanguage::Css => "CSS".to_string(),
        SupportedLanguage::Dart => "Dart".to_string(),
        SupportedLanguage::Diff => "Diff".to_string(),
        SupportedLanguage::Dockerfile => "Dockerfile".to_string(),
        SupportedLanguage::Ejs => "EJS".to_string(),
        SupportedLanguage::Elixir => "Elixir".to_string(),
        SupportedLanguage::Erb => "ERB".to_string(),
        SupportedLanguage::Go => "Go".to_string(),
        SupportedLanguage::GraphQl => "GraphQL".to_string(),
        SupportedLanguage::Groovy => "Groovy".to_string(),
        SupportedLanguage::Haskell => "Haskell".to_string(),
        SupportedLanguage::Html => "HTML".to_string(),
        SupportedLanguage::Ini => "INI".to_string(),
        SupportedLanguage::Java => "Java".to_string(),
        SupportedLanguage::JavaScript => "JavaScript".to_string(),
        SupportedLanguage::JsDoc => "JSDoc".to_string(),
        SupportedLanguage::Json => "JSON".to_string(),
        SupportedLanguage::Make => "Make".to_string(),
        SupportedLanguage::Markdown => "Markdown".to_string(),
        SupportedLanguage::MarkdownInline => "Markdown Inline".to_string(),
        SupportedLanguage::ObjectiveC => "Objective-C".to_string(),
        SupportedLanguage::Ocaml => "OCaml".to_string(),
        SupportedLanguage::Pascal => "Pascal".to_string(),
        SupportedLanguage::Perl => "Perl".to_string(),
        SupportedLanguage::Php => "PHP".to_string(),
        SupportedLanguage::Plain => "Plain Text".to_string(),
        SupportedLanguage::Powershell => "PowerShell".to_string(),
        SupportedLanguage::Prolog => "Prolog".to_string(),
        SupportedLanguage::Proto => "Protocol Buffers".to_string(),
        SupportedLanguage::Python => "Python".to_string(),
        SupportedLanguage::R => "R".to_string(),
        SupportedLanguage::React => "React".to_string(),
        SupportedLanguage::Ruby => "Ruby".to_string(),
        SupportedLanguage::Rust => "Rust".to_string(),
        SupportedLanguage::Scala => "Scala".to_string(),
        SupportedLanguage::Scss => "SCSS".to_string(),
        SupportedLanguage::Sql => "SQL".to_string(),
        SupportedLanguage::Svg => "SVG".to_string(),
        SupportedLanguage::Svelte => "Svelte".to_string(),
        SupportedLanguage::Swift => "Swift".to_string(),
        SupportedLanguage::Toml => "TOML".to_string(),
        SupportedLanguage::TypeScript => "TypeScript".to_string(),
        SupportedLanguage::Vue => "Vue".to_string(),
        SupportedLanguage::Xml => "XML".to_string(),
        SupportedLanguage::Yaml => "YAML".to_string(),
        SupportedLanguage::Zig => "Zig".to_string(),
    }
}

/// Get the language registry name for a SupportedLanguage.
/// For languages built into gpui-component, this delegates to `to_language().name()`.
/// For custom-registered languages (e.g. Perl), this returns the registered name directly.
///
/// ### Arguments
/// - `supported_language`: The supported language to convert
///
/// ### Returns
/// - `&'static str`: The language name as registered in the LanguageRegistry
pub fn language_registry_name(supported_language: &SupportedLanguage) -> &'static str {
    match supported_language {
        SupportedLanguage::Ada => "ada",
        SupportedLanguage::Clojure => "clojure",
        SupportedLanguage::Dart => "dart",
        SupportedLanguage::Dockerfile => "dockerfile",
        SupportedLanguage::Groovy => "groovy",
        SupportedLanguage::Haskell => "haskell",
        SupportedLanguage::Ini => "ini",
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

/// Get the Language enum from a filename or file extension.
/// Exact filename matches (e.g. `Dockerfile`, `Makefile`) are checked first,
/// then falls back to the file extension.
///
/// ### Arguments
/// - `filename`: The file name
///
/// ### Returns
/// - `SupportedLanguage`: The detected language
pub fn language_from_filename(filename: &str) -> SupportedLanguage {
    let lower = filename.to_lowercase();
    let exact = match lower.as_str() {
        "dockerfile" => Some(SupportedLanguage::Dockerfile),
        "makefile" | "gnumakefile" => Some(SupportedLanguage::Make),
        "gemfile" | "rakefile" | "guardfile" | "podfile" => Some(SupportedLanguage::Ruby),
        _ => None,
    };
    if let Some(lang) = exact {
        return lang;
    }

    let extension = std::path::Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");

    if extension == "txt" {
        return SupportedLanguage::Plain;
    }

    // Overriding gpui-component's default language mapping.
    if let Some(lang) = match extension {
        "scss" => Some(SupportedLanguage::Scss),
        _ => None,
    } {
        return lang;
    }

    let mut language = from_language(Language::from_str(extension));
    if language == SupportedLanguage::Plain {
        language = match extension {
            "ada" | "ads" | "adb" => SupportedLanguage::Ada,
            "astro" => SupportedLanguage::Html,
            "clojure" | "clj" | "cljs" => SupportedLanguage::Clojure,
            "dart" => SupportedLanguage::Dart,
            "dockerfile" => SupportedLanguage::Dockerfile,
            "hs" | "lhs" => SupportedLanguage::Haskell,
            "ini" | "cfg" | "conf" | "config" => SupportedLanguage::Ini,
            "lock" => SupportedLanguage::Toml,
            "groovy" | "gvy" | "gy" | "gsh" => SupportedLanguage::Groovy,
            "mjs" => SupportedLanguage::JavaScript,
            "pas" | "pp" | "dpr" | "dpk" | "lpr" => SupportedLanguage::Pascal,
            "perl" | "pl" | "pm" | "plx" => SupportedLanguage::Perl,
            "powershell" | "ps1" | "psm1" | "psd1" => SupportedLanguage::Powershell,
            "pro" | "prolog" => SupportedLanguage::Prolog,
            "php" | "php3" | "php4" | "php5" | "phhtml" => SupportedLanguage::Html,
            "m" | "M" | "mm" => SupportedLanguage::ObjectiveC,
            "ml" | "mli" => SupportedLanguage::Ocaml,
            "r" | "rmd" => SupportedLanguage::R,
            "svelte" => SupportedLanguage::Svelte,
            "svg" => SupportedLanguage::Html,
            "tsx" | "jsx" => SupportedLanguage::React,
            "vue" => SupportedLanguage::Vue,
            "xml" => SupportedLanguage::Xml,
            _ => SupportedLanguage::Plain,
        };
    }
    language
}

impl SupportedLanguage {
    /// Lists all supported languages in alphabetical order, including some that are not supported by the language registry.
    ///
    /// ### Returns
    /// - `Vec<SupportedLanguage>`: A vector of all supported languages
    pub fn all() -> Vec<SupportedLanguage> {
        vec![
            SupportedLanguage::Ada,
            SupportedLanguage::Astro,
            SupportedLanguage::Bash,
            SupportedLanguage::C,
            SupportedLanguage::Clojure,
            SupportedLanguage::Cpp,
            SupportedLanguage::CMake,
            SupportedLanguage::CSharp,
            SupportedLanguage::Css,
            SupportedLanguage::Dart,
            SupportedLanguage::Diff,
            SupportedLanguage::Dockerfile,
            SupportedLanguage::Ejs,
            SupportedLanguage::Elixir,
            SupportedLanguage::Erb,
            SupportedLanguage::Go,
            SupportedLanguage::GraphQl,
            SupportedLanguage::Groovy,
            SupportedLanguage::Haskell,
            SupportedLanguage::Html,
            SupportedLanguage::Ini,
            SupportedLanguage::Java,
            SupportedLanguage::JavaScript,
            SupportedLanguage::JsDoc,
            SupportedLanguage::Json,
            SupportedLanguage::Make,
            SupportedLanguage::Markdown,
            SupportedLanguage::MarkdownInline,
            SupportedLanguage::ObjectiveC,
            SupportedLanguage::Ocaml,
            SupportedLanguage::Pascal,
            SupportedLanguage::Perl,
            SupportedLanguage::Php,
            SupportedLanguage::Plain,
            SupportedLanguage::Powershell,
            SupportedLanguage::Prolog,
            SupportedLanguage::Proto,
            SupportedLanguage::Python,
            SupportedLanguage::R,
            SupportedLanguage::React,
            SupportedLanguage::Ruby,
            SupportedLanguage::Rust,
            SupportedLanguage::Scala,
            SupportedLanguage::Scss,
            SupportedLanguage::Sql,
            SupportedLanguage::Svelte,
            SupportedLanguage::Svg,
            SupportedLanguage::Swift,
            SupportedLanguage::Toml,
            SupportedLanguage::TypeScript,
            SupportedLanguage::Vue,
            SupportedLanguage::Xml,
            SupportedLanguage::Yaml,
            SupportedLanguage::Zig,
        ]
    }
}

impl Fulgur {
    /// Get the current language of the active tab
    ///
    /// ### Returns
    /// - `SupportedLanguage`: The active tab's language
    pub fn get_current_language(&self) -> SupportedLanguage {
        match self.active_tab_index {
            Some(index) => {
                if let Some(editor_tab) = self.tabs[index].as_editor() {
                    editor_tab.language
                } else {
                    SupportedLanguage::Plain
                }
            }
            None => SupportedLanguage::Plain,
        }
    }

    /// Check if the current active tab's language is a Markdown language
    ///
    /// ### Returns
    /// - `True` if the active tab's language is a Markdown language, `False` otherwise
    pub fn is_markdown(&self) -> bool {
        let current_language = self.get_current_language();
        current_language == SupportedLanguage::Markdown
            || current_language == SupportedLanguage::MarkdownInline
    }
}

/// Register external languages that are not supported by default by the editor
pub fn register_external_languages() {
    super::syntax_highlighting::ada::add_ada_support();
    super::syntax_highlighting::clojure::add_clojure_support();
    super::syntax_highlighting::dart::add_dart_support();
    super::syntax_highlighting::dockerfile::add_dockerfile_support();
    super::syntax_highlighting::groovy::add_groovy_support();
    super::syntax_highlighting::haskell::add_haskell_support();
    super::syntax_highlighting::ini::add_ini_support();
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
