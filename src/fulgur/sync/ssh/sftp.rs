use super::REMOTE_ROOT_PATH;
use super::error::SshError;
use super::session::SshSession;
use ssh2::{ErrorCode, OpenFlags, OpenType, RenameFlags};
use std::io::{Read, Write};
use std::path::Path;

/// Fallback permission bits used when the remote destination mode cannot be read
const DEFAULT_REMOTE_FILE_MODE: u32 = 0o644;

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

/// Write bytes to a remote file via SFTP using a temp-then-rename approach.
///
/// ### Description
/// Writes to a `.fulgur.tmp.{pid}.{nanos}` sibling, then moves it over the destination with
/// `replace_with_temp`, which keeps a complete copy of either the original or the new content
/// on the server at every step. A partial write therefore never corrupts the original. The
/// non-atomic truncating write is used only when the temp file cannot be created or when the
/// server refuses every rename.
///
/// ### Arguments
/// - `session`: Established SSH session with SFTP subsystem.
/// - `remote_path`: Absolute or home-relative (`~/...`) destination path on the remote host.
/// - `data`: File contents to write.
///
/// ### Errors
/// - Returns an `SshError::SftpError` when the remote home directory cannot be resolved,
///   or on any write, rename, or permission failure against the temporary file or the
///   destination path.
///
/// ### Returns
/// - `Ok(())`: File written and renamed successfully.
/// - `Err(SshError::SftpError)`: Write, rename, or permission error.
pub fn write_remote_file(
    session: &SshSession,
    remote_path: &str,
    data: &[u8],
) -> Result<(), SshError> {
    let remote_path = resolve_home_relative_path(session, remote_path)?;
    // Resolve symlinks so the write targets the real file instead of replacing the link.
    let resolved = session
        .sftp
        .realpath(Path::new(&remote_path))
        .ok()
        .and_then(|path| path.to_str().map(str::to_string))
        .unwrap_or(remote_path);

    // Preserve the destination permission bits across the temp-then-rename so a
    // private or executable remote file is not reset to a default mode.
    let dest_mode = session
        .sftp
        .stat(Path::new(&resolved))
        .ok()
        .and_then(|stat| stat.perm)
        .map_or(DEFAULT_REMOTE_FILE_MODE, |perm| perm & 0o777);

    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();

    let tmp_str = format!("{resolved}.fulgur.tmp.{pid}.{nanos}");
    let backup_str = format!("{resolved}.fulgur.bak.{pid}.{nanos}");
    let tmp_path = Path::new(&tmp_str);
    let backup_path = Path::new(&backup_str);
    let dest_path = Path::new(&resolved);

    let mut tmp = match session.sftp.open_mode(
        tmp_path,
        OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
        dest_mode.cast_signed(),
        OpenType::File,
    ) {
        Ok(tmp) => tmp,
        Err(create_err) => {
            log::warn!(
                "Cannot create temp file {tmp_str} ({create_err}); trying direct write fallback (non-atomic)"
            );
            return session
                .sftp
                .write_truncating(dest_path, data, dest_mode)
                .map_err(|write_err| {
                    SshError::SftpError(format!(
                        "Cannot create temp file {tmp_str}: {create_err}; direct write to {resolved} failed: {write_err}"
                    ))
                });
        }
    };

    if let Err(e) = tmp.write_all(data) {
        drop(tmp);
        let _ = session.sftp.unlink(tmp_path);
        return Err(SshError::SftpError(format!(
            "Write error on {tmp_str}: {e}"
        )));
    }
    drop(tmp);

    // Enforce the exact mode regardless of the remote umask applied at creation.
    let _ = session.sftp.setstat(
        tmp_path,
        ssh2::FileStat {
            size: None,
            uid: None,
            gid: None,
            perm: Some(dest_mode),
            atime: None,
            mtime: None,
        },
    );

    replace_with_temp(
        &session.sftp,
        tmp_path,
        dest_path,
        backup_path,
        data,
        dest_mode,
    )
    .map_err(|e| SshError::SftpError(format!("Cannot replace {resolved}: {e}")))
}

/// The SFTP primitives needed to move a fully written temp file over a destination.
trait RemoteReplaceOps {
    /// Rename `src` to `dst`, asking the server to overwrite `dst` atomically.
    ///
    /// ### Description
    /// The overwrite request is only honoured by SFTP v5+ servers. SFTP v3 servers (OpenSSH)
    /// ignore it and fail when `dst` already exists.
    ///
    /// ### Arguments
    /// - `src`: Existing path to move.
    /// - `dst`: New path.
    ///
    /// ### Returns
    /// - `Ok(())`: `src` now lives at `dst`.
    /// - `Err(String)`: The server refused or failed the rename.
    fn rename(&self, src: &Path, dst: &Path) -> Result<(), String>;

    /// Delete a remote file.
    ///
    /// ### Arguments
    /// - `path`: File to delete.
    ///
    /// ### Returns
    /// - `Ok(())`: File deleted.
    /// - `Err(String)`: Deletion failed.
    fn unlink(&self, path: &Path) -> Result<(), String>;

    /// Write `data` to `path` in place, truncating any existing content (non-atomic).
    ///
    /// ### Arguments
    /// - `path`: Destination file, created with `mode` when missing.
    /// - `data`: File contents to write.
    /// - `mode`: Permission bits applied when the file is created.
    ///
    /// ### Returns
    /// - `Ok(())`: Write succeeded.
    /// - `Err(String)`: Open or write failed, possibly leaving `path` truncated.
    fn write_truncating(&self, path: &Path, data: &[u8], mode: u32) -> Result<(), String>;
}

impl RemoteReplaceOps for ssh2::Sftp {
    fn rename(&self, src: &Path, dst: &Path) -> Result<(), String> {
        ssh2::Sftp::rename(
            self,
            src,
            dst,
            Some(RenameFlags::OVERWRITE | RenameFlags::ATOMIC),
        )
        .map_err(|e| e.to_string())
    }

    fn unlink(&self, path: &Path) -> Result<(), String> {
        ssh2::Sftp::unlink(self, path).map_err(|e| e.to_string())
    }

    fn write_truncating(&self, path: &Path, data: &[u8], mode: u32) -> Result<(), String> {
        let mut file = self
            .open_mode(
                path,
                OpenFlags::WRITE | OpenFlags::TRUNCATE | OpenFlags::CREATE,
                mode.cast_signed(),
                OpenType::File,
            )
            .map_err(|e| format!("cannot open {} for direct write: {e}", path.display()))?;
        file.write_all(data)
            .map_err(|e| format!("write error on {}: {e}", path.display()))
    }
}

/// Move a fully written temp file over the destination without ever losing a complete copy.
///
/// ### Description
/// Tries an overwriting rename first, which succeeds on SFTP v5+ servers and whenever the
/// destination does not exist yet. Otherwise (SFTP v3, e.g. OpenSSH) the original is moved
/// aside to `backup_path`, the temp file is moved into place, and the backup is deleted. If
/// moving the temp file into place fails, the original is restored from the backup. When the
/// server refuses every rename, the destination is written in place as a last resort, and the
/// temp file is kept unless that write succeeds.
///
/// ### Arguments
/// - `ops`: SFTP primitives used to rename, delete, and write.
/// - `tmp_path`: Temporary file holding the complete new content.
/// - `dest_path`: Final destination path.
/// - `backup_path`: Unused sibling path that receives the original while swapping.
/// - `data`: New content, used only by the direct-write fallback.
/// - `mode`: Permission bits applied if the direct-write fallback creates the file.
///
/// ### Returns
/// - `Ok(())`: The destination holds the new content and no temp or backup file is left.
/// - `Err(String)`: The replacement failed. The message names any temp or backup file left
///   behind because it holds the only complete copy of the new or original content.
fn replace_with_temp(
    ops: &impl RemoteReplaceOps,
    tmp_path: &Path,
    dest_path: &Path,
    backup_path: &Path,
    data: &[u8],
    mode: u32,
) -> Result<(), String> {
    let Err(overwrite_err) = ops.rename(tmp_path, dest_path) else {
        return Ok(());
    };
    log::debug!(
        "Overwriting rename to {} failed ({overwrite_err}); swapping through backup {}",
        dest_path.display(),
        backup_path.display()
    );

    if let Err(aside_err) = ops.rename(dest_path, backup_path) {
        log::warn!(
            "SFTP server refused to rename {} ({aside_err}); trying direct write fallback (non-atomic)",
            dest_path.display()
        );
        return match ops.write_truncating(dest_path, data, mode) {
            Ok(()) => {
                let _ = ops.unlink(tmp_path);
                Ok(())
            }
            Err(write_err) => Err(format!(
                "rename refused ({aside_err}); direct write failed ({write_err}); new content kept in {}",
                tmp_path.display()
            )),
        };
    }

    match ops.rename(tmp_path, dest_path) {
        Ok(()) => {
            let _ = ops.unlink(backup_path);
            Ok(())
        }
        Err(swap_err) => match ops.rename(backup_path, dest_path) {
            Ok(()) => {
                let _ = ops.unlink(tmp_path);
                Err(format!("cannot move temp file into place: {swap_err}"))
            }
            Err(restore_err) => Err(format!(
                "cannot move temp file into place ({swap_err}) nor restore the original ({restore_err}); original kept in {}, new content kept in {}",
                backup_path.display(),
                tmp_path.display()
            )),
        },
    }
}

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
    use super::{
        REMOTE_ROOT_PATH, RemoteReplaceOps, SshError, expand_home_relative_path, join_remote_path,
        normalize_remote_path, parent_remote_path, replace_with_temp,
    };
    use std::cell::{Cell, RefCell};
    use std::collections::{HashMap, HashSet};
    use std::path::{Path, PathBuf};

    const DEST: &str = "/srv/app/config.toml";
    const TMP: &str = "/srv/app/config.toml.fulgur.tmp.1.2";
    const BACKUP: &str = "/srv/app/config.toml.fulgur.bak.1.2";
    const ORIGINAL: &[u8] = b"original";
    const UPDATED: &[u8] = b"updated";

    /// In-memory remote filesystem whose renames follow SFTP v3 (OpenSSH) semantics:
    /// renaming onto an existing path always fails, whatever flags are requested.
    #[derive(Default)]
    struct SftpV3Mock {
        files: RefCell<HashMap<PathBuf, Vec<u8>>>,
        refused_renames: HashSet<(PathBuf, PathBuf)>,
        refuse_all_renames: bool,
        fail_truncating_writes: bool,
        truncating_writes: Cell<usize>,
    }

    impl SftpV3Mock {
        /// Build a mock holding the original destination and the fully written temp file.
        ///
        /// ### Returns
        /// - `SftpV3Mock`: Mock with `DEST` and `TMP` present and no failure injected.
        fn with_dest_and_tmp() -> Self {
            let mock = Self::default();
            mock.files
                .borrow_mut()
                .insert(PathBuf::from(DEST), ORIGINAL.to_vec());
            mock.files
                .borrow_mut()
                .insert(PathBuf::from(TMP), UPDATED.to_vec());
            mock
        }

        /// Make the server refuse one specific rename.
        ///
        /// ### Arguments
        /// - `src`: Source path of the refused rename.
        /// - `dst`: Destination path of the refused rename.
        ///
        /// ### Returns
        /// - `SftpV3Mock`: The mock with the rename refused.
        fn refusing(mut self, src: &str, dst: &str) -> Self {
            self.refused_renames
                .insert((PathBuf::from(src), PathBuf::from(dst)));
            self
        }

        /// Read the content currently stored at `path`.
        ///
        /// ### Arguments
        /// - `path`: Remote path to read.
        ///
        /// ### Returns
        /// - `Some(Vec<u8>)`: Content of the file.
        /// - `None`: The file does not exist.
        fn content(&self, path: &str) -> Option<Vec<u8>> {
            self.files.borrow().get(Path::new(path)).cloned()
        }
    }

    impl RemoteReplaceOps for SftpV3Mock {
        fn rename(&self, src: &Path, dst: &Path) -> Result<(), String> {
            let refused = self
                .refused_renames
                .contains(&(src.to_path_buf(), dst.to_path_buf()));
            let mut files = self.files.borrow_mut();
            if self.refuse_all_renames || refused || files.contains_key(dst) {
                return Err("SFTP(4) failure".to_string());
            }
            let content = files.remove(src).ok_or("SFTP(2) no such file")?;
            files.insert(dst.to_path_buf(), content);
            Ok(())
        }

        fn unlink(&self, path: &Path) -> Result<(), String> {
            self.files
                .borrow_mut()
                .remove(path)
                .map(|_| ())
                .ok_or_else(|| "SFTP(2) no such file".to_string())
        }

        fn write_truncating(&self, path: &Path, data: &[u8], _mode: u32) -> Result<(), String> {
            self.truncating_writes.set(self.truncating_writes.get() + 1);
            let mut files = self.files.borrow_mut();
            if self.fail_truncating_writes {
                files.insert(path.to_path_buf(), Vec::new());
                return Err("connection dropped".to_string());
            }
            files.insert(path.to_path_buf(), data.to_vec());
            Ok(())
        }
    }

    /// Run `replace_with_temp` against the mock with the shared test paths.
    ///
    /// ### Arguments
    /// - `mock`: Mock remote filesystem to operate on.
    ///
    /// ### Returns
    /// - `Result<(), String>`: The result of `replace_with_temp`.
    fn replace(mock: &SftpV3Mock) -> Result<(), String> {
        replace_with_temp(
            mock,
            Path::new(TMP),
            Path::new(DEST),
            Path::new(BACKUP),
            UPDATED,
            0o644,
        )
    }

    #[test]
    fn replace_existing_destination_on_v3_swaps_without_truncating() {
        let mock = SftpV3Mock::with_dest_and_tmp();

        replace(&mock).expect("replace should succeed");

        assert_eq!(mock.content(DEST).as_deref(), Some(UPDATED));
        assert_eq!(mock.content(TMP), None);
        assert_eq!(mock.content(BACKUP), None);
        assert_eq!(mock.truncating_writes.get(), 0);
    }

    #[test]
    fn replace_missing_destination_renames_directly() {
        let mock = SftpV3Mock::with_dest_and_tmp();
        mock.files.borrow_mut().remove(Path::new(DEST));

        replace(&mock).expect("replace should succeed");

        assert_eq!(mock.content(DEST).as_deref(), Some(UPDATED));
        assert_eq!(mock.content(TMP), None);
        assert_eq!(mock.truncating_writes.get(), 0);
    }

    #[test]
    fn replace_restores_original_when_temp_cannot_move_into_place() {
        let mock = SftpV3Mock::with_dest_and_tmp().refusing(TMP, DEST);

        assert!(replace(&mock).is_err());

        assert_eq!(mock.content(DEST).as_deref(), Some(ORIGINAL));
        assert_eq!(mock.content(BACKUP), None);
        assert_eq!(mock.content(TMP), None);
        assert_eq!(mock.truncating_writes.get(), 0);
    }

    #[test]
    fn replace_keeps_backup_and_temp_when_restore_fails() {
        let mock = SftpV3Mock::with_dest_and_tmp()
            .refusing(TMP, DEST)
            .refusing(BACKUP, DEST);

        let err = replace(&mock).expect_err("replace should fail");

        assert_eq!(mock.content(BACKUP).as_deref(), Some(ORIGINAL));
        assert_eq!(mock.content(TMP).as_deref(), Some(UPDATED));
        assert!(err.contains(BACKUP) && err.contains(TMP));
        assert_eq!(mock.truncating_writes.get(), 0);
    }

    #[test]
    fn replace_falls_back_to_direct_write_when_server_refuses_renames() {
        let mock = SftpV3Mock {
            refuse_all_renames: true,
            ..SftpV3Mock::with_dest_and_tmp()
        };

        replace(&mock).expect("direct write fallback should succeed");

        assert_eq!(mock.content(DEST).as_deref(), Some(UPDATED));
        assert_eq!(mock.content(TMP), None);
        assert_eq!(mock.truncating_writes.get(), 1);
    }

    #[test]
    fn replace_keeps_temp_when_direct_write_fallback_fails() {
        let mock = SftpV3Mock {
            refuse_all_renames: true,
            fail_truncating_writes: true,
            ..SftpV3Mock::with_dest_and_tmp()
        };

        let err = replace(&mock).expect_err("replace should fail");

        assert_eq!(mock.content(TMP).as_deref(), Some(UPDATED));
        assert!(err.contains(TMP));
    }

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
