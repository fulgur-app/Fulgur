use super::SupportedLanguage;
use super::filename_detection::{language_from_filename, lowercase_extension};

/// Detect whether a `.m` file contains Objective-C or MATLAB source by scanning
/// the first 50 lines for distinctive markers.
///
/// Returns `ObjectiveC` if any Objective-C marker is encountered first, `Matlab` if any
/// MATLAB marker is encountered first, and `ObjectiveC` as the default when no signal is found.
fn detect_m_file_language(content: &str) -> SupportedLanguage {
    for line in content.lines().take(50) {
        let trimmed = line.trim();
        // Objective-C markers
        if trimmed.starts_with("#import")
            || trimmed.starts_with("#include")
            || trimmed.starts_with("@interface")
            || trimmed.starts_with("@implementation")
            || trimmed.starts_with("@protocol")
            || trimmed.starts_with("@property")
            || trimmed.starts_with("- (")
            || trimmed.starts_with("+ (")
        {
            return SupportedLanguage::ObjectiveC;
        }
        // MATLAB markers
        if trimmed.starts_with('%')
            || trimmed.starts_with("function ")
            || trimmed == "function"
            || trimmed.starts_with("classdef ")
        {
            return SupportedLanguage::Matlab;
        }
    }
    SupportedLanguage::ObjectiveC
}

/// Check whether a line includes an extensionless system header such as `#include <vector>`,
/// which only exists in the C++ standard library (C headers end in `.h`).
///
/// ### Arguments
/// - `trimmed`: The source line, without leading whitespace
///
/// ### Returns
/// - `bool`: `true` if the line is an `#include <...>` of a header without an extension
fn is_extensionless_system_include(trimmed: &str) -> bool {
    trimmed
        .strip_prefix("#include")
        .map(str::trim_start)
        .and_then(|rest| rest.strip_prefix('<'))
        .and_then(|rest| rest.split_once('>'))
        .is_some_and(|(header, _)| !header.contains('.'))
}

/// Detect whether a `.h` header contains C, C++ or Objective-C source by scanning
/// the first 200 lines for distinctive markers.
///
/// ### Arguments
/// - `content`: The file content
///
/// ### Returns
/// - `SupportedLanguage`: `ObjectiveC` or `Cpp` for whichever marker is encountered first,
///   `C` when no marker is found
fn detect_h_file_language(content: &str) -> SupportedLanguage {
    for line in content.lines().take(200) {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#import")
            || trimmed.starts_with("@interface")
            || trimmed.starts_with("@protocol")
            || trimmed.starts_with("@class ")
            || trimmed.starts_with("@property")
        {
            return SupportedLanguage::ObjectiveC;
        }
        if trimmed.starts_with("class ")
            || trimmed.starts_with("namespace ")
            || trimmed.starts_with("using namespace ")
            || trimmed.starts_with("template<")
            || trimmed.starts_with("template <")
            || trimmed.starts_with("public:")
            || trimmed.starts_with("private:")
            || trimmed.starts_with("protected:")
            || is_extensionless_system_include(trimmed)
        {
            return SupportedLanguage::Cpp;
        }
    }
    SupportedLanguage::C
}

/// Get the language from a filename and file content.
///
/// Behaves like [`language_from_filename`] but additionally uses content-based heuristics
/// for extensions that are shared between multiple languages. Currently handles:
/// - `.m`: disambiguates between Objective-C and MATLAB by scanning the first 50 lines.
/// - `.h`: disambiguates between C, C++ and Objective-C by scanning the first 200 lines.
///
/// ### Arguments
/// - `filename`: The file name
/// - `content`: The file content
///
/// ### Returns
/// - `SupportedLanguage`: The detected language
#[must_use]
pub fn language_from_content(filename: &str, content: &str) -> SupportedLanguage {
    let base = language_from_filename(filename);
    match lowercase_extension(filename).as_str() {
        "m" if base == SupportedLanguage::ObjectiveC => detect_m_file_language(content),
        "h" if base == SupportedLanguage::C => detect_h_file_language(content),
        _ => base,
    }
}

#[cfg(test)]
mod tests {
    use super::{SupportedLanguage, language_from_content};

    #[test]
    fn test_m_file_objc_by_import() {
        let content = "#import <Foundation/Foundation.h>\n@interface Foo : NSObject\n@end\n";
        assert_eq!(
            language_from_content("foo.m", content),
            SupportedLanguage::ObjectiveC
        );
    }

    #[test]
    fn test_m_file_objc_by_interface() {
        let content = "@interface MyClass : NSObject\n- (void)doSomething;\n@end\n";
        assert_eq!(
            language_from_content("foo.m", content),
            SupportedLanguage::ObjectiveC
        );
    }

    #[test]
    fn test_m_file_objc_by_method() {
        let content = "- (void)viewDidLoad {\n    [super viewDidLoad];\n}\n";
        assert_eq!(
            language_from_content("foo.m", content),
            SupportedLanguage::ObjectiveC
        );
    }

    #[test]
    fn test_m_file_matlab_by_percent_comment() {
        let content = "% This is a MATLAB script\nx = 1 + 2;\n";
        assert_eq!(
            language_from_content("foo.m", content),
            SupportedLanguage::Matlab
        );
    }

    #[test]
    fn test_m_file_matlab_by_function_keyword() {
        let content = "function result = add(a, b)\n    result = a + b;\nend\n";
        assert_eq!(
            language_from_content("foo.m", content),
            SupportedLanguage::Matlab
        );
    }

    #[test]
    fn test_m_file_matlab_by_classdef() {
        let content = "classdef Animal\n    properties\n        Name\n    end\nend\n";
        assert_eq!(
            language_from_content("foo.m", content),
            SupportedLanguage::Matlab
        );
    }

    #[test]
    fn test_m_file_defaults_to_objc_when_ambiguous() {
        let content = "x = 1;\ny = 2;\n";
        assert_eq!(
            language_from_content("foo.m", content),
            SupportedLanguage::ObjectiveC
        );
    }

    #[test]
    fn test_mm_file_always_objc() {
        let content = "% looks like matlab but is .mm\n";
        assert_eq!(
            language_from_content("foo.mm", content),
            SupportedLanguage::ObjectiveC
        );
    }

    #[test]
    fn test_non_m_file_unaffected() {
        assert_eq!(
            language_from_content("foo.rs", "fn main() {}"),
            SupportedLanguage::Rust
        );
    }

    #[test]
    fn test_h_file_content_detection_table() {
        let cases = [
            (
                "#include <stdio.h>\nint add(int a, int b);\n",
                SupportedLanguage::C,
            ),
            ("", SupportedLanguage::C),
            (
                "#import <Foundation/Foundation.h>\n",
                SupportedLanguage::ObjectiveC,
            ),
            (
                "@interface Foo : NSObject\n@end\n",
                SupportedLanguage::ObjectiveC,
            ),
            ("@class Bar;\n", SupportedLanguage::ObjectiveC),
            ("#include <vector>\n", SupportedLanguage::Cpp),
            ("#include<string>\n", SupportedLanguage::Cpp),
            ("namespace fulgur {\n}\n", SupportedLanguage::Cpp),
            (
                "template <typename T>\nT max(T a, T b);\n",
                SupportedLanguage::Cpp,
            ),
            (
                "class Widget {\npublic:\n    Widget();\n};\n",
                SupportedLanguage::Cpp,
            ),
            (
                "struct Point {\n  public:\n  int x;\n};\n",
                SupportedLanguage::Cpp,
            ),
        ];
        for (content, expected) in cases {
            assert_eq!(
                language_from_content("util.h", content),
                expected,
                "{content}"
            );
        }
    }

    #[test]
    fn test_uppercase_h_file_still_disambiguates_content() {
        assert_eq!(
            language_from_content("UTIL.H", "namespace fulgur {}\n"),
            SupportedLanguage::Cpp
        );
    }

    #[test]
    fn test_uppercase_m_file_still_disambiguates_content() {
        let content = "% This is a MATLAB script\nx = 1 + 2;\n";
        assert_eq!(
            language_from_content("FOO.M", content),
            SupportedLanguage::Matlab
        );
    }
}
