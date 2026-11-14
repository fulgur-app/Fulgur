use super::paths::resolve_home_relative_path;
use crate::fulgur::sync::ssh::error::SshError;
use crate::fulgur::sync::ssh::session::SshSession;
use ssh2::{OpenFlags, OpenType, RenameFlags};
use std::io::Write;
use std::path::Path;

/// Fallback permission bits used when the remote destination mode cannot be read
const DEFAULT_REMOTE_FILE_MODE: u32 = 0o644;

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

#[cfg(test)]
mod tests {
    use super::{RemoteReplaceOps, replace_with_temp};
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
}
