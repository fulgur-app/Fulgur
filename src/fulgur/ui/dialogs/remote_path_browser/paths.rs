use crate::fulgur::sync::ssh;

use super::super::file_browser::truncate_to_parent_folder;

/// Parse a browser input path into `(directory_to_list, filter_prefix)`.
///
/// ### Arguments
/// - `raw`: Raw input path string.
///
/// ### Returns
/// - `(String, String)`: `(directory, filter)` pair.
pub(super) fn parse_remote_browser_input(raw: &str) -> (String, String) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (ssh::REMOTE_ROOT_PATH.to_string(), String::new());
    }

    if trimmed == "~" || trimmed.ends_with('/') {
        return (normalize_remote_browser_path(trimmed), String::new());
    }

    if let Some(idx) = trimmed.rfind('/') {
        let parent = if idx == 0 {
            ssh::REMOTE_ROOT_PATH.to_string()
        } else {
            normalize_remote_browser_path(&trimmed[..idx])
        };
        let filter = trimmed[idx + 1..].to_string();
        return (parent, filter);
    }

    (ssh::REMOTE_ROOT_PATH.to_string(), trimmed.to_string())
}

/// Normalize remote browser path text into an absolute or home-relative path.
///
/// ### Arguments
/// - `path`: Input path from the browser field.
///
/// ### Returns
/// - `String`: Normalized absolute or home-relative remote path.
pub(crate) fn normalize_remote_browser_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return ssh::REMOTE_ROOT_PATH.to_string();
    }
    if trimmed == "~" || trimmed.starts_with("~/") {
        return trimmed.trim_end_matches('/').to_string();
    }
    if trimmed.starts_with('/') {
        if trimmed.len() > 1 {
            return trimmed.trim_end_matches('/').to_string();
        }
        return ssh::REMOTE_ROOT_PATH.to_string();
    }
    format!("/{}", trimmed.trim_start_matches('/'))
}

/// Truncate a remote browser input to its parent folder.
///
/// ### Arguments
/// - `raw`: Raw input path string.
///
/// ### Returns
/// - `Some(String)`: The normalized input up to and including its previous `/`.
/// - `None`: When the input normalizes to the remote root or to the home folder `~`.
pub(super) fn remote_parent_folder(raw: &str) -> Option<String> {
    let path = normalize_remote_browser_path(raw);
    truncate_to_parent_folder(&path, |c| c == '/').map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{normalize_remote_browser_path, parse_remote_browser_input, remote_parent_folder};

    #[test]
    fn parse_remote_browser_input_handles_directory_path() {
        let (directory, filter) = parse_remote_browser_input("/var/log/");
        assert_eq!(directory, "/var/log");
        assert_eq!(filter, "");
    }

    #[test]
    fn parse_remote_browser_input_handles_file_prefix() {
        let (directory, filter) = parse_remote_browser_input("/var/log/sys");
        assert_eq!(directory, "/var/log");
        assert_eq!(filter, "sys");
    }

    #[test]
    fn parse_remote_browser_input_lists_the_home_folder() {
        for input in ["~", "~/"] {
            let (directory, filter) = parse_remote_browser_input(input);
            assert_eq!(directory, "~", "{input:?}");
            assert_eq!(filter, "", "{input:?}");
        }
        assert_eq!(
            parse_remote_browser_input("~/notes/"),
            ("~/notes".to_string(), String::new())
        );
        assert_eq!(
            parse_remote_browser_input("~/no"),
            ("~".to_string(), "no".to_string())
        );
    }

    #[test]
    fn normalize_remote_browser_path_defaults_to_root() {
        assert_eq!(
            normalize_remote_browser_path(""),
            super::ssh::REMOTE_ROOT_PATH
        );
    }

    #[test]
    fn normalize_remote_browser_path_keeps_home_relative_paths() {
        let cases = [
            ("~", "~"),
            ("~/", "~"),
            (" ~/tmp ", "~/tmp"),
            ("~/tmp/", "~/tmp"),
        ];
        for (input, expected) in cases {
            assert_eq!(normalize_remote_browser_path(input), expected, "{input:?}");
        }
    }

    #[test]
    fn remote_parent_folder_drops_the_last_folder_of_a_folder_path() {
        assert_eq!(remote_parent_folder("/var/log/").as_deref(), Some("/var/"));
        assert_eq!(remote_parent_folder("/var/").as_deref(), Some("/"));
    }

    #[test]
    fn remote_parent_folder_drops_only_a_partial_name() {
        assert_eq!(
            remote_parent_folder("/var/log/sys").as_deref(),
            Some("/var/log/")
        );
    }

    #[test]
    fn remote_parent_folder_normalizes_relative_input() {
        assert_eq!(remote_parent_folder("var/log").as_deref(), Some("/var/"));
    }

    #[test]
    fn remote_parent_folder_stops_at_the_root() {
        assert_eq!(remote_parent_folder("/"), None);
        assert_eq!(remote_parent_folder(""), None);
    }

    #[test]
    fn remote_parent_folder_stays_within_the_home_folder() {
        assert_eq!(
            remote_parent_folder("~/notes/todo").as_deref(),
            Some("~/notes/")
        );
        assert_eq!(remote_parent_folder("~/notes").as_deref(), Some("~/"));
        assert_eq!(remote_parent_folder("~/"), None);
        assert_eq!(remote_parent_folder("~"), None);
    }
}
