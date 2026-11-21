use super::paths::{
    join_remote_path, normalize_remote_path, parent_remote_path, resolve_home_relative_path,
};
use crate::fulgur::sync::ssh::REMOTE_ROOT_PATH;
use crate::fulgur::sync::ssh::error::SshError;
use crate::fulgur::sync::ssh::session::SshSession;
use ssh2::ErrorCode;
use std::io::Read;
use std::path::Path;

/// Largest remote file, in bytes, that Fulgur reads into an editor buffer.
pub const MAX_REMOTE_FILE_BYTES: u64 = 1024 * 1024 * 1024;

/// POSIX mask selecting the file type bits of a mode.
const MODE_TYPE_MASK: u32 = 0o170_000;
/// POSIX file type bits of a directory.
const MODE_TYPE_DIRECTORY: u32 = 0o040_000;
/// POSIX file type bits of a regular file.
const MODE_TYPE_REGULAR: u32 = 0o100_000;

/// Read a regular file from the remote host via SFTP.
///
/// ### Arguments
/// - `session`: Established SSH session with SFTP subsystem.
/// - `remote_path`: Absolute or home-relative (`~/...`) path on the remote host.
/// - `before_read`: Called with the size reported by the server once the file is
///   known to be readable, before its contents are transferred.
///
/// ### Errors
/// - Returns an `SshError::SftpError` when the remote home directory cannot be resolved,
///   when the path cannot be inspected or opened (missing, permission denied), when it
///   is not a regular file, when it is larger than `MAX_REMOTE_FILE_BYTES`, or when an
///   I/O error occurs while reading.
///
/// ### Returns
/// - `Ok(Vec<u8>)`: Raw file contents.
/// - `Err(SshError::SftpError)`: The file was refused or could not be read.
pub fn read_remote_file(
    session: &SshSession,
    remote_path: &str,
    before_read: impl FnOnce(u64),
) -> Result<Vec<u8>, SshError> {
    let remote_path = resolve_home_relative_path(session, remote_path)?;
    let path = Path::new(&remote_path);
    let stat = session
        .sftp
        .stat(path)
        .map_err(|e| SshError::SftpError(format!("Cannot inspect {remote_path}: {e}")))?;
    if let Some(perm) = stat.perm
        && !matches!(remote_path_kind_from_mode(perm), RemotePathKind::File)
    {
        return Err(not_regular_file_error(&remote_path));
    }
    let reported_size = stat.size.unwrap_or(0);
    if reported_size > MAX_REMOTE_FILE_BYTES {
        return Err(file_too_large_error(&remote_path));
    }
    before_read(reported_size);

    let file = session
        .sftp
        .open(path)
        .map_err(|e| SshError::SftpError(format!("Cannot open {remote_path}: {e}")))?;
    read_bounded(file, MAX_REMOTE_FILE_BYTES, &remote_path)
}

/// Read a stream to its end, refusing it once it exceeds a byte limit.
///
/// ### Arguments
/// - `reader`: The stream to read.
/// - `limit`: Largest accepted length, in bytes.
/// - `remote_path`: Path named in the error messages.
///
/// ### Errors
/// - Returns an `SshError::SftpError` when the stream is longer than `limit`, or when
///   an I/O error occurs while reading.
///
/// ### Returns
/// - `Ok(Vec<u8>)`: The whole stream, at most `limit` bytes long.
/// - `Err(SshError::SftpError)`: The stream is too long or could not be read.
fn read_bounded(reader: impl Read, limit: u64, remote_path: &str) -> Result<Vec<u8>, SshError> {
    let mut buf = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut buf)
        .map_err(|e| SshError::SftpError(format!("Read error on {remote_path}: {e}")))?;
    if buf.len() as u64 > limit {
        return Err(file_too_large_error(remote_path));
    }
    Ok(buf)
}

/// Build the error returned for a remote path that is not a regular file.
///
/// ### Arguments
/// - `remote_path`: The refused path.
///
/// ### Returns
/// - `SshError::SftpError`: The error naming the path.
#[must_use]
pub fn not_regular_file_error(remote_path: &str) -> SshError {
    SshError::SftpError(format!(
        "Cannot open {remote_path}: not a regular file (device, FIFO or socket)"
    ))
}

/// Build the error returned for a remote file larger than `MAX_REMOTE_FILE_BYTES`.
///
/// ### Arguments
/// - `remote_path`: The refused path.
///
/// ### Returns
/// - `SshError::SftpError`: The error naming the path and the limit.
fn file_too_large_error(remote_path: &str) -> SshError {
    SshError::SftpError(format!(
        "Cannot open {remote_path}: larger than the {} MiB remote file limit",
        MAX_REMOTE_FILE_BYTES / (1024 * 1024)
    ))
}

/// Type classification for a remote SFTP path.
#[derive(Debug, PartialEq, Eq)]
pub enum RemotePathKind {
    /// Path points to a regular file.
    File,
    /// Path points to a directory.
    Directory,
    /// Path points to something else: a character or block device, a FIFO or a socket.
    Other,
    /// Path does not exist on the remote host.
    Missing,
}

/// Classify a path from the POSIX mode bits reported by the server.
///
/// ### Arguments
/// - `perm`: The mode bits of the path.
///
/// ### Returns
/// - `RemotePathKind::Directory`: The mode describes a directory.
/// - `RemotePathKind::File`: The mode describes a regular file.
/// - `RemotePathKind::Other`: The mode describes any other kind of file.
fn remote_path_kind_from_mode(perm: u32) -> RemotePathKind {
    match perm & MODE_TYPE_MASK {
        MODE_TYPE_DIRECTORY => RemotePathKind::Directory,
        MODE_TYPE_REGULAR => RemotePathKind::File,
        _ => RemotePathKind::Other,
    }
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
/// - `Ok(RemotePathKind::File)`: Path exists and is a regular file, or the server
///   omits the mode bits of something that is not a directory.
/// - `Ok(RemotePathKind::Directory)`: Path exists and is a directory.
/// - `Ok(RemotePathKind::Other)`: Path exists and is a device, a FIFO or a socket.
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
                return Ok(remote_path_kind_from_mode(perm));
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
            RemotePathKind::File | RemotePathKind::Other | RemotePathKind::Missing => {
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
            let is_dir = stat
                .perm
                .is_some_and(|perm| remote_path_kind_from_mode(perm) == RemotePathKind::Directory);
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

#[cfg(test)]
mod tests {
    use super::{RemotePathKind, read_bounded, remote_path_kind_from_mode};
    use std::io::{Cursor, Read};

    #[test]
    fn test_remote_path_kind_from_mode_accepts_only_regular_files_as_files() {
        assert_eq!(remote_path_kind_from_mode(0o100_644), RemotePathKind::File);
        assert_eq!(
            remote_path_kind_from_mode(0o040_755),
            RemotePathKind::Directory
        );
        for special in [0o020_666, 0o060_660, 0o010_644, 0o140_755] {
            assert_eq!(
                remote_path_kind_from_mode(special),
                RemotePathKind::Other,
                "mode {special:o} must not be read as a file"
            );
        }
    }

    #[test]
    fn test_read_bounded_returns_a_stream_at_the_limit() {
        let bytes = read_bounded(Cursor::new(vec![b'a'; 16]), 16, "/tmp/file").unwrap();
        assert_eq!(bytes.len(), 16);
    }

    #[test]
    fn test_read_bounded_refuses_an_endless_stream() {
        let endless = std::io::repeat(0);
        let error = read_bounded(endless.take(u64::MAX), 1024, "/dev/zero").unwrap_err();
        assert!(
            error.to_string().contains("remote file limit"),
            "unexpected error: {error}"
        );
    }
}
