use super::{SupportedLanguage, from_language};
use gpui_kit::component::highlighter::Language;

/// Extract a file's extension in lowercase so detection is case-insensitive.
///
/// ### Arguments
/// - `filename`: The file name
///
/// ### Returns
/// - `String`: The lowercased extension, empty when the file has none
pub(super) fn lowercase_extension(filename: &str) -> String {
    std::path::Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
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
#[must_use]
pub fn language_from_filename(filename: &str) -> SupportedLanguage {
    let lower = filename.to_lowercase();
    let exact = match lower.as_str() {
        "dockerfile" | "containerfile" => Some(SupportedLanguage::Dockerfile),
        "makefile" | "gnumakefile" => Some(SupportedLanguage::Make),
        "cmakelists.txt" => Some(SupportedLanguage::CMake),
        ".bashrc" | ".bash_profile" | ".bash_aliases" | ".bash_logout" | ".profile" | ".zshrc"
        | ".zshenv" | ".zprofile" | ".zlogin" | ".zlogout" => Some(SupportedLanguage::Bash),
        "gemfile" | "rakefile" | "guardfile" | "podfile" | "vagrantfile" | "brewfile" => {
            Some(SupportedLanguage::Ruby)
        }
        "jenkinsfile" => Some(SupportedLanguage::Groovy),
        "cargo.lock" | "poetry.lock" | "uv.lock" | "pdm.lock" => Some(SupportedLanguage::Toml),
        "flake.lock" | "composer.lock" | "deno.lock" | "bun.lock" | "pipfile.lock" => {
            Some(SupportedLanguage::Json)
        }
        _ => None,
    };
    if let Some(lang) = exact {
        return lang;
    }

    let extension = lowercase_extension(filename);

    if extension == "txt" {
        return SupportedLanguage::Plain;
    }

    // Overriding gpui-component's default language mapping.
    if let Some(lang) = match extension.as_str() {
        "scss" => Some(SupportedLanguage::Scss),
        "csv" | "tsv" => Some(SupportedLanguage::Csv),
        _ => None,
    } {
        return lang;
    }

    let mut language = from_language(Language::from_str(&extension));
    if language == SupportedLanguage::Plain {
        language = match extension.as_str() {
            "ada" | "ads" | "adb" => SupportedLanguage::Ada,
            "asm" | "s" | "nasm" | "masm" => SupportedLanguage::Asm,
            "h" => SupportedLanguage::C,
            "hh" | "hpp" | "hxx" | "cc" | "cxx" | "inl" => SupportedLanguage::Cpp,
            "clojure" | "clj" | "cljs" => SupportedLanguage::Clojure,
            "d" | "di" => SupportedLanguage::D,
            "dart" => SupportedLanguage::Dart,
            "dockerfile" => SupportedLanguage::Dockerfile,
            "exs" => SupportedLanguage::Elixir,
            "erl" | "hrl" | "escript" => SupportedLanguage::Erlang,
            "f" | "f77" | "f90" | "f95" | "f03" | "f08" | "for" | "ftn" | "pf" => {
                SupportedLanguage::Fortran
            }
            "fs" | "fsi" | "fsx" | "fsscript" => SupportedLanguage::FSharp,
            "hs" | "lhs" => SupportedLanguage::Haskell,
            "htm" | "xhtml" => SupportedLanguage::Html,
            "ini" | "cfg" | "conf" | "config" => SupportedLanguage::Ini,
            "jinja" | "jinja2" | "j2" => SupportedLanguage::Jinja2,
            "jl" => SupportedLanguage::Julia,
            "groovy" | "gvy" | "gy" | "gsh" | "gradle" => SupportedLanguage::Groovy,
            "mjs" | "cjs" => SupportedLanguage::JavaScript,
            "mts" | "cts" => SupportedLanguage::TypeScript,
            "pas" | "pp" | "dpr" | "dpk" | "lpr" => SupportedLanguage::Pascal,
            "perl" | "pl" | "pm" | "plx" => SupportedLanguage::Perl,
            "powershell" | "ps1" | "psm1" | "psd1" => SupportedLanguage::Powershell,
            "pro" | "prolog" => SupportedLanguage::Prolog,
            "m" | "mm" => SupportedLanguage::ObjectiveC,
            "ml" | "mli" => SupportedLanguage::Ocaml,
            "pyi" | "pyw" => SupportedLanguage::Python,
            "r" | "rmd" => SupportedLanguage::R,
            "rake" | "gemspec" => SupportedLanguage::Ruby,
            "svg" => SupportedLanguage::Svg,
            "zsh" => SupportedLanguage::Bash,
            "tsx" | "jsx" => SupportedLanguage::React,
            "vue" => SupportedLanguage::Vue,
            "xml" => SupportedLanguage::Xml,
            _ => SupportedLanguage::Plain,
        };
    }
    language
}

#[cfg(test)]
mod tests {
    use super::{SupportedLanguage, language_from_filename};

    #[test]
    fn test_uppercase_extension_detects_registry_language() {
        assert_eq!(language_from_filename("FOO.RS"), SupportedLanguage::Rust);
        assert_eq!(language_from_filename("Foo.Py"), SupportedLanguage::Python);
    }

    #[test]
    fn test_uppercase_extension_detects_overridden_language() {
        assert_eq!(
            language_from_filename("styles.SCSS"),
            SupportedLanguage::Scss
        );
        assert_eq!(
            language_from_filename("notes.TXT"),
            SupportedLanguage::Plain
        );
    }

    #[test]
    fn test_uppercase_extension_detects_fallback_language() {
        assert_eq!(language_from_filename("boot.S"), SupportedLanguage::Asm);
        assert_eq!(language_from_filename("main.ADB"), SupportedLanguage::Ada);
    }

    #[test]
    fn test_extension_fallback_table() {
        let cases = [
            ("util.h", SupportedLanguage::C),
            ("util.hh", SupportedLanguage::Cpp),
            ("util.hpp", SupportedLanguage::Cpp),
            ("util.hxx", SupportedLanguage::Cpp),
            ("util.cc", SupportedLanguage::Cpp),
            ("util.cxx", SupportedLanguage::Cpp),
            ("util.inl", SupportedLanguage::Cpp),
            ("index.htm", SupportedLanguage::Html),
            ("index.xhtml", SupportedLanguage::Html),
            ("config.cjs", SupportedLanguage::JavaScript),
            ("config.mjs", SupportedLanguage::JavaScript),
            ("config.cts", SupportedLanguage::TypeScript),
            ("config.mts", SupportedLanguage::TypeScript),
            ("mix.exs", SupportedLanguage::Elixir),
            ("setup.zsh", SupportedLanguage::Bash),
            ("stubs.pyi", SupportedLanguage::Python),
            ("gui.pyw", SupportedLanguage::Python),
            ("build.gradle", SupportedLanguage::Groovy),
            ("tasks.rake", SupportedLanguage::Ruby),
            ("fulgur.gemspec", SupportedLanguage::Ruby),
            ("logo.svg", SupportedLanguage::Svg),
        ];
        for (filename, expected) in cases {
            assert_eq!(language_from_filename(filename), expected, "{filename}");
        }
    }

    #[test]
    fn test_exact_filename_table() {
        let cases = [
            ("CMakeLists.txt", SupportedLanguage::CMake),
            (".bashrc", SupportedLanguage::Bash),
            (".bash_profile", SupportedLanguage::Bash),
            (".profile", SupportedLanguage::Bash),
            (".zshrc", SupportedLanguage::Bash),
            (".zprofile", SupportedLanguage::Bash),
            ("Containerfile", SupportedLanguage::Dockerfile),
            ("Vagrantfile", SupportedLanguage::Ruby),
            ("Brewfile", SupportedLanguage::Ruby),
            ("Jenkinsfile", SupportedLanguage::Groovy),
            ("Cargo.lock", SupportedLanguage::Toml),
            ("poetry.lock", SupportedLanguage::Toml),
            ("uv.lock", SupportedLanguage::Toml),
            ("flake.lock", SupportedLanguage::Json),
            ("composer.lock", SupportedLanguage::Json),
            ("Pipfile.lock", SupportedLanguage::Json),
        ];
        for (filename, expected) in cases {
            assert_eq!(language_from_filename(filename), expected, "{filename}");
        }
    }

    #[test]
    fn test_lock_files_without_known_format_are_plain() {
        assert_eq!(
            language_from_filename("yarn.lock"),
            SupportedLanguage::Plain
        );
        assert_eq!(
            language_from_filename("Gemfile.lock"),
            SupportedLanguage::Plain
        );
    }

    #[test]
    fn test_other_txt_files_stay_plain() {
        assert_eq!(
            language_from_filename("requirements.txt"),
            SupportedLanguage::Plain
        );
    }
}
