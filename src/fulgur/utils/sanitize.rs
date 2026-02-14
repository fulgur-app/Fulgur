/// Sanitize a filename to prevent path traversal and other security issues
///
/// This function:
/// - Extracts just the filename from paths (e.g., "/path/to/file.txt" → "file.txt")
/// - Preserves leading dots (hidden files like ".gitignore")
/// - Removes control characters and null bytes
///
/// ### Arguments
/// - `filename`: The filename or path to sanitize
///
/// ### Returns
/// - `String`: The sanitized filename, or "untitled" if the result is empty
///
/// ### Examples
/// ```
/// # use fulgur::fulgur::utils::sanitize::sanitize_filename;
/// assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
/// assert_eq!(sanitize_filename("normal.txt"), "normal.txt");
/// assert_eq!(sanitize_filename("path/to/file.txt"), "file.txt");
/// assert_eq!(sanitize_filename(".hidden"), ".hidden");
/// assert_eq!(sanitize_filename(""), "untitled");
/// ```
pub fn sanitize_filename(filename: &str) -> String {
    // Normalize path separators to Unix style, then split and take the last component
    let normalized = filename.replace('\\', "/");
    let base_name = normalized
        .split('/')
        .filter(|s| !s.is_empty())
        .next_back()
        .unwrap_or("");

    // Remove control characters and null bytes
    let sanitized = base_name
        .replace('\0', "")
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>();

    // If the result is empty or only whitespace, return a default name
    if sanitized.trim().is_empty() {
        "untitled".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_normal_filename() {
        assert_eq!(sanitize_filename("document.txt"), "document.txt");
        assert_eq!(sanitize_filename("My File.pdf"), "My File.pdf");
    }

    #[test]
    fn test_sanitize_path_traversal() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("../config.json"), "config.json");
        assert_eq!(sanitize_filename("..\\windows\\system32"), "system32");
    }

    #[test]
    fn test_sanitize_path_separators() {
        assert_eq!(sanitize_filename("path/to/file.txt"), "file.txt");
        assert_eq!(sanitize_filename("C:\\Users\\file.txt"), "file.txt");
        assert_eq!(sanitize_filename("/absolute/path/to/document.pdf"), "document.pdf");
    }

    #[test]
    fn test_sanitize_hidden_files() {
        assert_eq!(sanitize_filename(".hidden"), ".hidden");
        assert_eq!(sanitize_filename(".gitignore"), ".gitignore");
        assert_eq!(sanitize_filename("path/to/.hidden"), ".hidden");
    }

    #[test]
    fn test_sanitize_empty_and_whitespace() {
        assert_eq!(sanitize_filename(""), "untitled");
        assert_eq!(sanitize_filename("   "), "untitled");
        assert_eq!(sanitize_filename("/"), "untitled");
        assert_eq!(sanitize_filename("\\"), "untitled");
        assert_eq!(sanitize_filename("///"), "untitled");
    }

    #[test]
    fn test_sanitize_control_characters() {
        assert_eq!(sanitize_filename("file\x00name.txt"), "filename.txt");
        assert_eq!(sanitize_filename("test\nfile.txt"), "testfile.txt");
        assert_eq!(sanitize_filename("doc\r\nument.txt"), "document.txt");
    }

    #[test]
    fn test_sanitize_mixed_issues() {
        assert_eq!(
            sanitize_filename("../../.hidden/path/to/file\x00.txt"),
            "file.txt"
        );
        assert_eq!(
            sanitize_filename("/tmp/.config/app/settings.json"),
            "settings.json"
        );
    }

    #[test]
    fn test_sanitize_unicode() {
        assert_eq!(sanitize_filename("文档.txt"), "文档.txt");
        assert_eq!(sanitize_filename("émoji-😀.txt"), "émoji-😀.txt");
    }
}
