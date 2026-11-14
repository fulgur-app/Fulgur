use crate::fulgur::sync::ssh::REMOTE_ROOT_PATH;
use crate::fulgur::sync::ssh::error::SshError;
use crate::fulgur::sync::ssh::session::SshSession;
use std::path::Path;

/// Resolve a home-relative remote path (`~` or `~/...`) to an absolute path.
///
/// ### Arguments
/// - `session`: Established SSH session with SFTP subsystem.
/// - `path`: Remote path, possibly home-relative.
///
/// ### Errors
/// - Returns an `SshError::SftpError` when the remote home directory cannot be resolved.
///
/// ### Returns
/// - `Ok(String)`: Absolute path, or `path` unchanged when it is not home-relative.
/// - `Err(SshError::SftpError)`: The remote home directory could not be resolved.
pub fn resolve_home_relative_path(session: &SshSession, path: &str) -> Result<String, SshError> {
    expand_home_relative_path(path, || remote_home_directory(session))
}

/// Ask the SFTP server for the absolute path of its login directory.
///
/// ### Arguments
/// - `session`: Established SSH session with SFTP subsystem.
///
/// ### Errors
/// - Returns an `SshError::SftpError` when the server cannot resolve `.` or returns a
///   path that is not valid UTF-8.
///
/// ### Returns
/// - `Ok(String)`: Absolute remote home directory.
/// - `Err(SshError::SftpError)`: The lookup failed.
fn remote_home_directory(session: &SshSession) -> Result<String, SshError> {
    let home = session.sftp.realpath(Path::new(".")).map_err(|e| {
        SshError::SftpError(format!("Cannot resolve the remote home directory: {e}"))
    })?;
    home.to_str().map(str::to_string).ok_or_else(|| {
        SshError::SftpError(format!(
            "Remote home directory {} is not valid UTF-8",
            home.display()
        ))
    })
}

/// Expand a home-relative remote path against the remote home directory.
///
/// ### Arguments
/// - `path`: Remote path, possibly home-relative.
/// - `home_directory`: Looks up the absolute remote home directory; called only when
///   `path` is home-relative.
///
/// ### Errors
/// - Returns the error of `home_directory` when the lookup fails.
///
/// ### Returns
/// - `Ok(String)`: Absolute path, or `path` unchanged when it is not home-relative.
/// - `Err(SshError)`: The home directory lookup failed.
fn expand_home_relative_path(
    path: &str,
    home_directory: impl FnOnce() -> Result<String, SshError>,
) -> Result<String, SshError> {
    let below_home = if path == "~" {
        ""
    } else if let Some(below_home) = path.strip_prefix("~/") {
        below_home.trim_start_matches('/')
    } else {
        return Ok(path.to_string());
    };
    let home = home_directory()?;
    if below_home.is_empty() {
        Ok(home)
    } else {
        Ok(join_remote_path(&home, below_home))
    }
}

/// Normalize remote paths to forward-slash absolute form.
///
/// ### Arguments
/// - `path`: Raw remote path input.
///
/// ### Returns
/// - `String`: Normalized path, defaulting to `/` when empty.
pub(super) fn normalize_remote_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return REMOTE_ROOT_PATH.to_string();
    }
    if trimmed == "~" || trimmed.starts_with("~/") {
        return trimmed.to_string();
    }
    let slashes = trimmed.replace('\\', REMOTE_ROOT_PATH);
    if slashes.starts_with(REMOTE_ROOT_PATH) {
        slashes
    } else {
        format!("{REMOTE_ROOT_PATH}{slashes}")
    }
}

/// Compute the parent path for a normalized remote path.
///
/// ### Arguments
/// - `path`: Normalized remote path.
///
/// ### Returns
/// - `String`: Parent directory path (returns `/` for root or single-segment paths).
#[must_use]
pub fn parent_remote_path(path: &str) -> String {
    let trimmed = path.trim_end_matches(REMOTE_ROOT_PATH);
    if trimmed.is_empty() || trimmed == REMOTE_ROOT_PATH {
        return REMOTE_ROOT_PATH.to_string();
    }
    match trimmed.rfind(REMOTE_ROOT_PATH) {
        Some(0) | None => REMOTE_ROOT_PATH.to_string(),
        Some(index) => trimmed[..index].to_string(),
    }
}

/// Join a directory and entry name into a normalized remote path.
///
/// ### Arguments
/// - `directory`: Parent directory path.
/// - `name`: Entry name in that directory.
///
/// ### Returns
/// - `String`: Joined full path.
pub(super) fn join_remote_path(directory: &str, name: &str) -> String {
    let directory = normalize_remote_path(directory);
    if directory == REMOTE_ROOT_PATH {
        format!("{REMOTE_ROOT_PATH}{name}")
    } else {
        format!("{}/{}", directory.trim_end_matches(REMOTE_ROOT_PATH), name)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        REMOTE_ROOT_PATH, SshError, expand_home_relative_path, join_remote_path,
        normalize_remote_path, parent_remote_path,
    };

    #[test]
    fn parent_remote_path_of_root_is_root() {
        assert_eq!(parent_remote_path(REMOTE_ROOT_PATH), REMOTE_ROOT_PATH);
    }

    #[test]
    fn parent_remote_path_of_single_segment_returns_root() {
        assert_eq!(parent_remote_path("/tmp"), REMOTE_ROOT_PATH);
    }

    #[test]
    fn parent_remote_path_of_nested_path_returns_parent() {
        assert_eq!(parent_remote_path("/tmp/nested/file.txt"), "/tmp/nested");
    }

    #[test]
    fn normalize_remote_path_produces_forward_slash_absolute_paths() {
        let cases = [
            ("", "/"),
            ("  ", "/"),
            ("/", "/"),
            ("~", "~"),
            ("  ~  ", "~"),
            ("~/a", "~/a"),
            ("a\\b", "/a/b"),
            ("\\a\\b", "/a/b"),
            ("a/b", "/a/b"),
            ("/a/b", "/a/b"),
            ("/a/", "/a/"),
            (" /a/b ", "/a/b"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                normalize_remote_path(input),
                expected,
                "normalize_remote_path({input:?})"
            );
        }
    }

    #[test]
    fn expand_home_relative_path_resolves_against_the_home_directory() {
        let cases = [
            ("/home/alice", "~", "/home/alice"),
            ("/home/alice", "~/", "/home/alice"),
            ("/home/alice", "~/notes.txt", "/home/alice/notes.txt"),
            (
                "/home/alice",
                "~/notes/todo.txt",
                "/home/alice/notes/todo.txt",
            ),
            ("/home/alice", "~//notes.txt", "/home/alice/notes.txt"),
            ("/home/alice/", "~/notes.txt", "/home/alice/notes.txt"),
            ("/", "~/notes.txt", "/notes.txt"),
        ];
        for (home, path, expected) in cases {
            let expanded = expand_home_relative_path(path, || Ok(home.to_string()))
                .expect("home lookup succeeds");
            assert_eq!(expanded, expected, "expand_home_relative_path({path:?})");
        }
    }

    #[test]
    fn expand_home_relative_path_leaves_other_paths_without_a_home_lookup() {
        for path in [
            "/",
            "/etc/hosts",
            "/srv/~/notes.txt",
            "~notes.txt",
            "~bob/notes.txt",
        ] {
            let expanded = expand_home_relative_path(path, || {
                panic!("{path:?} must not look up the home directory")
            })
            .expect("no lookup, no error");
            assert_eq!(expanded, path);
        }
    }

    #[test]
    fn expand_home_relative_path_propagates_the_lookup_error() {
        let result = expand_home_relative_path("~/notes.txt", || {
            Err(SshError::SftpError("realpath refused".to_string()))
        });
        assert!(
            matches!(result, Err(SshError::SftpError(message)) if message == "realpath refused")
        );
    }

    #[test]
    fn join_remote_path_inserts_exactly_one_separator() {
        let cases = [
            ("/", "file.txt", "/file.txt"),
            ("", "file.txt", "/file.txt"),
            ("/a", "file.txt", "/a/file.txt"),
            ("/a/", "file.txt", "/a/file.txt"),
            ("a\\b", "file.txt", "/a/b/file.txt"),
            ("~", "file.txt", "~/file.txt"),
            ("~/a", "file.txt", "~/a/file.txt"),
        ];
        for (directory, name, expected) in cases {
            assert_eq!(
                join_remote_path(directory, name),
                expected,
                "join_remote_path({directory:?}, {name:?})"
            );
        }
    }
}
