use super::paths::{
    join_remote_path, normalize_remote_path, parent_remote_path, resolve_home_relative_path,
};
use crate::fulgur::sync::ssh::REMOTE_ROOT_PATH;
use crate::fulgur::sync::ssh::error::SshError;
use crate::fulgur::sync::ssh::session::SshSession;
use ssh2::ErrorCode;
use std::io::Read;
use std::path::Path;

/// Read a file from the remote host via SFTP.
///
/// ### Arguments
/// - `session`: Established SSH session with SFTP subsystem.
/// - `remote_path`: Absolute or home-relative (`~/...`) path on the remote host.
///
/// ### Errors
/// - Returns an `SshError::SftpError` when the remote home directory cannot be resolved,
///   when the remote file cannot be opened (missing, permission denied), or when an I/O
///   error occurs while reading.
///
/// ### Returns
/// - `Ok(Vec<u8>)`: Raw file contents.
/// - `Err(SshError::SftpError)`: File not found, permission denied, or I/O error.
pub fn read_remote_file(session: &SshSession, remote_path: &str) -> Result<Vec<u8>, SshError> {
    let remote_path = resolve_home_relative_path(session, remote_path)?;
    let mut file = session
        .sftp
        .open(Path::new(&remote_path))
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
/// ### Errors
/// - Returns an `SshError::SftpError` if the remote home directory cannot be
///   resolved, or if the SFTP metadata lookup fails for a reason other than the
///   path being missing.
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
    let normalized = resolve_home_relative_path(session, &normalize_remote_path(remote_path))?;
    let path = Path::new(&normalized);
    match session.sftp.stat(path) {
        Ok(stat) => {
            if let Some(perm) = stat.perm {
                // POSIX mode bits where 0o040000 indicates a directory.
                if perm & 0o170_000 == 0o040_000 {
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
/// ### Errors
/// - Returns an `SshError::SftpError` if the remote home directory cannot be
///   resolved, or if a path classification check fails unexpectedly while walking
///   upward.
///
/// ### Returns
/// - `Ok(String)`: Closest existing directory path, always absolute.
/// - `Err(SshError::SftpError)`: Path checks failed unexpectedly.
pub fn closest_existing_remote_directory(
    session: &SshSession,
    path: &str,
) -> Result<String, SshError> {
    let mut candidate = resolve_home_relative_path(session, &normalize_remote_path(path))?;
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
/// ### Errors
/// - Returns an `SshError::SftpError` if the remote home directory cannot be
///   resolved, or if the remote directory cannot be read.
///
/// ### Returns
/// - `Ok(Vec<RemoteDirectoryEntry>)`: Directory entries sorted with directories first,
///   each with an absolute `full_path`.
/// - `Err(SshError::SftpError)`: Directory read failed.
pub fn list_remote_directory(
    session: &SshSession,
    directory: &str,
) -> Result<Vec<RemoteDirectoryEntry>, SshError> {
    let directory = resolve_home_relative_path(session, &normalize_remote_path(directory))?;
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
            let is_dir = stat.perm.is_some_and(|perm| perm & 0o170_000 == 0o040_000);
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
