mod external_sources;
mod open_core;
mod open_entry;
mod reload;
mod tab_lookup;

use std::path::Path;

/// File name of a path as shown in user-facing notifications.
///
/// ### Arguments
/// - `path`: The path of the file
///
/// ### Returns
/// - `String`: The final path component, or `"file"` when it is missing or not UTF-8
fn notice_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file")
        .to_string()
}

/// Build the user-facing message shown when a local file cannot be read.
///
/// ### Arguments
/// - `verb`: The attempted action, such as `"open"` or `"reload"`
/// - `path`: The path of the file that could not be read
/// - `error`: The I/O error returned by the read
///
/// ### Returns
/// - `String`: The notification text
fn read_failure_notice(verb: &str, path: &Path, error: &std::io::Error) -> String {
    format!("Cannot {verb} '{}': {error}", notice_file_name(path))
}

#[cfg(test)]
mod tests {
    use super::read_failure_notice;
    use std::{io, path::Path};

    #[test]
    fn read_failure_notice_names_the_file_and_the_error() {
        let error = io::Error::new(io::ErrorKind::PermissionDenied, "permission denied");
        let notice = read_failure_notice("open", Path::new("/tmp/secret.txt"), &error);
        assert_eq!(notice, "Cannot open 'secret.txt': permission denied");
    }
}
