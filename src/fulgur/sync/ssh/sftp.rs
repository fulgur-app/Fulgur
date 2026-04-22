use super::REMOTE_ROOT_PATH;
use super::error::SshError;
use super::session::SshSession;
use ssh2::{ErrorCode, OpenFlags, OpenType, RenameFlags};
use std::io::{Read, Write};
use std::path::Path;

/// Read a file from the remote host via SFTP.
///
/// ### Arguments
/// - `session`: Established SSH session with SFTP subsystem.
/// - `remote_path`: Absolute path on the remote host.
///
/// ### Returns
/// - `Ok(Vec<u8>)`: Raw file contents.
/// - `Err(SshError::SftpError)`: File not found, permission denied, or I/O error.
pub fn read_remote_file(session: &SshSession, remote_path: &str) -> Result<Vec<u8>, SshError> {
    let path = Path::new(remote_path);
    let mut file = session
        .sftp
        .open(path)
        .map_err(|e| SshError::SftpError(format!("Cannot open {remote_path}: {e}")))?;

    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .map_err(|e| SshError::SftpError(format!("Read error on {remote_path}: {e}")))?;

    Ok(buf)
}

/// Type classification for a remote SFTP path.
pub enum RemotePathKind {
    /// Path points to a regular file.
    File,
    /// Path points to a directory.
    Directory,
    /// Path does not exist on the remote host.
    Missing,
}

/// A single entry in a remote directory listing.
#[derive(Clone, Debug)]
pub struct RemoteDirectoryEntry {
    pub name: String,
    pub is_dir: bool,
    pub full_path: String,
}

/// Classify a remote path as file, directory, or missing.
///
/// ### Arguments
/// - `session`: Established SSH session with SFTP subsystem.
/// - `remote_path`: Path to classify on the remote host.
///
/// ### Returns
/// - `Ok(RemotePathKind::File)`: Path exists and is a file.
/// - `Ok(RemotePathKind::Directory)`: Path exists and is a directory.
/// - `Ok(RemotePathKind::Missing)`: Path does not exist.
/// - `Err(SshError::SftpError)`: Metadata lookup failed for another reason.
pub fn classify_remote_path(
    session: &SshSession,
    remote_path: &str,
) -> Result<RemotePathKind, SshError> {
    let normalized = normalize_remote_path(remote_path);
    let path = Path::new(&normalized);
    match session.sftp.stat(path) {
        Ok(stat) => {
            if let Some(perm) = stat.perm {
                // POSIX mode bits where 0o040000 indicates a directory.
                if perm & 0o170000 == 0o040000 {
                    return Ok(RemotePathKind::Directory);
                }
                return Ok(RemotePathKind::File);
            }

            // Fallback when the server omits mode bits.
            if session.sftp.opendir(path).is_ok() {
                Ok(RemotePathKind::Directory)
            } else {
                Ok(RemotePathKind::File)
            }
        }
        Err(err) => match err.code() {
            ErrorCode::SFTP(2) => Ok(RemotePathKind::Missing),
            _ => Err(SshError::SftpError(format!(
                "Cannot inspect {normalized}: {err}"
            ))),
        },
    }
}

/// Find the closest existing remote directory for an input path.
///
/// ### Description
/// Walks upward through parent paths until it finds an existing directory,
/// falling back to `/` if needed.
///
/// ### Arguments
/// - `session`: Established SSH session with SFTP subsystem.
/// - `path`: Candidate path to resolve.
///
/// ### Returns
/// - `Ok(String)`: Closest existing directory path.
/// - `Err(SshError::SftpError)`: Path checks failed unexpectedly.
pub fn closest_existing_remote_directory(
    session: &SshSession,
    path: &str,
) -> Result<String, SshError> {
    let mut candidate = normalize_remote_path(path);
    loop {
        match classify_remote_path(session, &candidate)? {
            RemotePathKind::Directory => return Ok(candidate),
            RemotePathKind::File | RemotePathKind::Missing => {
                if candidate == REMOTE_ROOT_PATH {
                    return Ok(REMOTE_ROOT_PATH.to_string());
                }
                candidate = parent_remote_path(&candidate);
            }
        }
    }
}

/// Read and sort a remote directory listing.
///
/// ### Arguments
/// - `session`: Established SSH session with SFTP subsystem.
/// - `directory`: Existing remote directory path.
///
/// ### Returns
/// - `Ok(Vec<RemoteDirectoryEntry>)`: Directory entries sorted with directories first.
/// - `Err(SshError::SftpError)`: Directory read failed.
pub fn list_remote_directory(
    session: &SshSession,
    directory: &str,
) -> Result<Vec<RemoteDirectoryEntry>, SshError> {
    let directory = normalize_remote_path(directory);
    let path = Path::new(&directory);
    let mut entries = session
        .sftp
        .readdir(path)
        .map_err(|e| SshError::SftpError(format!("Cannot list directory {directory}: {e}")))?
        .into_iter()
        .filter_map(|(entry_path, stat)| {
            let name = entry_path.file_name()?.to_string_lossy().to_string();
            if name == "." || name == ".." {
                return None;
            }
            let is_dir = stat
                .perm
                .map(|perm| perm & 0o170000 == 0o040000)
                .unwrap_or(false);
            let full_path = join_remote_path(&directory, &name);
            Some(RemoteDirectoryEntry {
                name,
                is_dir,
                full_path,
            })
        })
        .collect::<Vec<_>>();

    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries.truncate(500);
    Ok(entries)
}

/// Write bytes to a remote file via SFTP using an atomic temp-then-rename approach.
///
/// ### Description
/// Writes to a `.fulgur.tmp.{pid}.{nanos}` sibling then renames it over the destination
/// atomically, mirroring the local `atomic_write_file` pattern. A partial write therefore
/// never corrupts the original. The temp file is removed on any failure (best-effort).
///
/// ### Arguments
/// - `session`: Established SSH session with SFTP subsystem.
/// - `remote_path`: Absolute destination path on the remote host.
/// - `data`: File contents to write.
///
/// ### Returns
/// - `Ok(())`: File written and renamed successfully.
/// - `Err(SshError::SftpError)`: Write, rename, or permission error.
pub fn write_remote_file(
    session: &SshSession,
    remote_path: &str,
    data: &[u8],
) -> Result<(), SshError> {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();

    let tmp_str = format!("{remote_path}.fulgur.tmp.{pid}.{nanos}");
    let tmp_path = Path::new(&tmp_str);
    let dest_path = Path::new(remote_path);

    let write_result: Result<(), SshError> = (|| {
        let mut tmp = session
            .sftp
            .create(tmp_path)
            .map_err(|e| SshError::SftpError(format!("Cannot create temp file {tmp_str}: {e}")))?;

        tmp.write_all(data)
            .map_err(|e| SshError::SftpError(format!("Write error on {tmp_str}: {e}")))?;

        Ok(())
    })();

    if write_result.is_err() {
        let _ = session.sftp.unlink(tmp_path);
        return write_result;
    }

    let rename_result = rename_with_fallback(session, tmp_path, dest_path, remote_path, data)
        .map_err(|e| SshError::SftpError(format!("Rename failed for {remote_path}: {e}")));

    let _ = session.sftp.unlink(tmp_path);

    rename_result
}

/// Try to rename a temporary file over the destination with progressively more compatible modes.
///
/// ### Arguments
/// - `session`: Established SSH session with SFTP subsystem.
/// - `tmp_path`: Temporary file path containing fully written bytes.
/// - `dest_path`: Final destination path.
/// - `remote_path`: Destination path string for logging and error messages.
/// - `data`: File contents used by the direct-write fallback.
///
/// ### Returns
/// - `Ok(())`: Rename succeeded, or compatibility fallback direct-write succeeded.
/// - `Err(String)`: All rename attempts and fallback write failed.
fn rename_with_fallback(
    session: &SshSession,
    tmp_path: &Path,
    dest_path: &Path,
    remote_path: &str,
    data: &[u8],
) -> Result<(), String> {
    let attempts = [
        (
            "overwrite+atomic",
            Some(RenameFlags::OVERWRITE | RenameFlags::ATOMIC),
        ),
        ("overwrite", Some(RenameFlags::OVERWRITE)),
        ("default", None),
    ];

    let mut last_err = String::new();
    for (label, flags) in attempts {
        match session.sftp.rename(tmp_path, dest_path, flags) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = format!("{label}: {e}");
                log::warn!("SFTP rename mode '{label}' failed for {remote_path}: {e}");
            }
        }
    }

    log::warn!(
        "All SFTP rename modes failed for {remote_path}; trying direct write fallback (non-atomic)"
    );
    direct_write_fallback(session, dest_path, remote_path, data)
        .map_err(|fallback_err| format!("{last_err}; fallback direct write failed: {fallback_err}"))
}

/// Write file contents directly to the destination path when server-side rename is unsupported.
///
/// ### Description
/// This compatibility fallback is non-atomic and is only used when all rename modes fail.
///
/// ### Arguments
/// - `session`: Established SSH session with SFTP subsystem.
/// - `dest_path`: Final destination path.
/// - `remote_path`: Destination path string for error messages.
/// - `data`: File contents to write.
///
/// ### Returns
/// - `Ok(())`: Direct write succeeded.
/// - `Err(String)`: Destination open or write failed.
fn direct_write_fallback(
    session: &SshSession,
    dest_path: &Path,
    remote_path: &str,
    data: &[u8],
) -> Result<(), String> {
    let mut dest = session
        .sftp
        .open_mode(
            dest_path,
            OpenFlags::WRITE | OpenFlags::TRUNCATE | OpenFlags::CREATE,
            0o644,
            OpenType::File,
        )
        .map_err(|e| format!("cannot open destination for direct write: {e}"))?;
    dest.write_all(data)
        .map_err(|e| format!("write error during fallback for {remote_path}: {e}"))
}

/// Normalize remote paths to forward-slash absolute form.
///
/// ### Arguments
/// - `path`: Raw remote path input.
///
/// ### Returns
/// - `String`: Normalized path, defaulting to `/` when empty.
fn normalize_remote_path(path: &str) -> String {
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
fn join_remote_path(directory: &str, name: &str) -> String {
    let directory = normalize_remote_path(directory);
    if directory == REMOTE_ROOT_PATH {
        format!("{REMOTE_ROOT_PATH}{name}")
    } else {
        format!("{}/{}", directory.trim_end_matches(REMOTE_ROOT_PATH), name)
    }
}

#[cfg(test)]
mod tests {
    use super::{REMOTE_ROOT_PATH, parent_remote_path};

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
}
