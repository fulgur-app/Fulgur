use super::super::error::SshError;
use std::path::{Path, PathBuf};

/// Return the platform-appropriate path to `~/.ssh/known_hosts`.
///
/// ### Returns
/// - `Ok(PathBuf)`: Absolute path derived from `home_dir()`.
/// - `Err(SshError::ConnectionFailed)`: Home directory could not be resolved safely.
pub(super) fn known_hosts_path() -> Result<PathBuf, SshError> {
    Ok(home_dir()?.join(".ssh").join("known_hosts"))
}

/// Create `~/.ssh` with mode `0700` on Unix if it does not already exist.
///
/// ### Returns
/// - `Ok(())`: Directory exists or was created successfully.
/// - `Err(SshError::IoError)`: Directory could not be created.
pub(super) fn ensure_ssh_dir() -> Result<(), SshError> {
    let ssh_dir = home_dir()?.join(".ssh");
    if !ssh_dir.exists() {
        std::fs::create_dir_all(&ssh_dir)
            .map_err(|e| SshError::IoError(format!("Failed to create ~/.ssh: {e}")))?;
        set_dir_permissions_700(&ssh_dir);
    }
    Ok(())
}

/// Return the current user's home directory from platform-specific environment variables.
///
/// ### Errors
/// Returns `SshError::ConnectionFailed` if the home directory cannot be resolved from
/// platform-specific environment variables or if the resolved path is not absolute.
///
/// ### Returns
/// - `Ok(PathBuf)`: Absolute home directory path.
/// - `Err(SshError::ConnectionFailed)`: Home path is missing or not absolute.
pub fn home_dir() -> Result<PathBuf, SshError> {
    #[cfg(windows)]
    let home = std::env::var("USERPROFILE")
        .or_else(|_| {
            std::env::var("HOMEDRIVE")
                .and_then(|d| std::env::var("HOMEPATH").map(|p| format!("{d}{p}")))
        })
        .map(PathBuf::from)
        .map_err(|_| {
            SshError::ConnectionFailed(
                "Cannot resolve the home directory for SSH trust storage. \
                 Set USERPROFILE or HOMEDRIVE/HOMEPATH and retry."
                    .to_string(),
            )
        })?;

    #[cfg(not(windows))]
    let home = std::env::var("HOME").map(PathBuf::from).map_err(|_| {
        SshError::ConnectionFailed(
            "Cannot resolve the home directory for SSH trust storage. \
             Set HOME and retry."
                .to_string(),
        )
    })?;

    ensure_absolute_home(home)
}

/// Validate that a resolved home directory is an absolute path.
///
/// ### Arguments
/// - `home`: Candidate home directory resolved from the environment.
///
/// ### Errors
/// Returns `SshError::ConnectionFailed` if `home` is not an absolute path.
///
/// ### Returns
/// - `Ok(PathBuf)`: The unchanged path when it is absolute.
/// - `Err(SshError::ConnectionFailed)`: When the path is relative.
fn ensure_absolute_home(home: PathBuf) -> Result<PathBuf, SshError> {
    if !home.is_absolute() {
        return Err(SshError::ConnectionFailed(
            "Resolved home directory is not an absolute path; cannot use it for SSH trust storage."
                .to_string(),
        ));
    }

    Ok(home)
}

/// Set file permissions to `0600` on Unix; no-op on Windows.
///
/// ### Arguments
/// - `path`: Path to the file whose permissions are to be set.
pub(super) fn set_file_permissions_600(path: &Path) {
    #[cfg(unix)]
    apply_unix_mode(path, 0o600);
    #[cfg(not(unix))]
    let _ = path;
}

/// Set directory permissions to `0700` on Unix; no-op on Windows.
///
/// ### Arguments
/// - `path`: Path to the directory whose permissions are to be set.
fn set_dir_permissions_700(path: &Path) {
    #[cfg(unix)]
    apply_unix_mode(path, 0o700);
    #[cfg(not(unix))]
    let _ = path;
}

/// Apply a Unix permission mode to a file or directory.
///
/// ### Arguments
/// - `path`: Path to the filesystem entry.
/// - `mode`: Unix permission bits, e.g. `0o600` or `0o700`.
#[cfg(unix)]
fn apply_unix_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        perms.set_mode(mode);
        let _ = std::fs::set_permissions(path, perms);
    }
}

#[cfg(test)]
mod tests {
    use super::{ensure_absolute_home, home_dir};
    use std::path::PathBuf;

    #[test]
    fn ensure_absolute_home_accepts_absolute_path() {
        #[cfg(windows)]
        let path = PathBuf::from(r"C:\Users\user");
        #[cfg(not(windows))]
        let path = PathBuf::from("/home/user");

        let result = ensure_absolute_home(path.clone()).expect("absolute path should be accepted");
        assert_eq!(result, path);
    }

    #[test]
    fn ensure_absolute_home_rejects_relative_path() {
        let result = ensure_absolute_home(PathBuf::from("relative/home"));
        assert!(result.is_err());
    }

    #[test]
    fn home_dir_returns_existing_absolute_path() {
        let home = home_dir().expect("home directory should resolve in the test environment");
        assert!(home.is_absolute());
        assert!(home.exists());
    }

    #[cfg(unix)]
    #[test]
    fn set_dir_permissions_700_sets_owner_only_mode() {
        use super::set_dir_permissions_700;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("failed to create temp dir");
        set_dir_permissions_700(dir.path());

        let mode = std::fs::metadata(dir.path())
            .expect("failed to read metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn set_file_permissions_600_sets_owner_only_mode() {
        use super::set_file_permissions_600;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let file_path = dir.path().join("secret");
        std::fs::write(&file_path, b"secret").expect("failed to write file");
        set_file_permissions_600(&file_path);

        let mode = std::fs::metadata(&file_path)
            .expect("failed to read metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn apply_unix_mode_sets_requested_mode() {
        use super::apply_unix_mode;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let file_path = dir.path().join("entry");
        std::fs::write(&file_path, b"data").expect("failed to write file");
        apply_unix_mode(&file_path, 0o640);

        let mode = std::fs::metadata(&file_path)
            .expect("failed to read metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o640);
    }
}
