use super::browse::{cannot_inspect_error, is_missing_path_error};
use super::paths::resolve_home_relative_path;
use super::version::{RemoteConflict, RemoteFileVersion, RemoteWriteGuard, RemoteWriteOutcome};
use crate::fulgur::sync::ssh::error::SshError;
use crate::fulgur::sync::ssh::session::SshSession;
use ssh2::{OpenFlags, OpenType, RenameFlags};
use std::io::Write;
use std::path::Path;

/// Fallback permission bits used when the remote destination mode cannot be read
const DEFAULT_REMOTE_FILE_MODE: u32 = 0o644;

/// Permission bits preserved across a save, including the setuid, setgid and sticky bits
const PRESERVED_MODE_BITS: u32 = 0o7777;

/// Numeric owner and group of a remote file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileOwner {
    uid: u32,
    gid: u32,
}

impl FileOwner {
    /// Read the owner and group reported in SFTP file attributes.
    ///
    /// ### Arguments
    /// - `stat`: Attributes returned by the server.
    ///
    /// ### Returns
    /// - `Some(FileOwner)`: The server reported both the uid and the gid.
    /// - `None`: Either id is missing from the attributes.
    fn from_stat(stat: &ssh2::FileStat) -> Option<Self> {
        Some(Self {
            uid: stat.uid?,
            gid: stat.gid?,
        })
    }
}

/// Destination attributes a save must carry over to the new content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DestAttributes {
    owner: Option<FileOwner>,
    mode: u32,
}

impl DestAttributes {
    /// Collect the attributes to preserve from the destination's current attributes.
    ///
    /// ### Arguments
    /// - `stat`: Attributes of the destination, or `None` when it does not exist or cannot be read.
    ///
    /// ### Returns
    /// - `DestAttributes`: The destination owner when known, and its permission bits, or
    ///   `DEFAULT_REMOTE_FILE_MODE` when they are unknown.
    fn from_stat(stat: Option<&ssh2::FileStat>) -> Self {
        Self {
            owner: stat.and_then(FileOwner::from_stat),
            mode: stat
                .and_then(|stat| stat.perm)
                .map_or(DEFAULT_REMOTE_FILE_MODE, |perm| perm & PRESERVED_MODE_BITS),
        }
    }
}

/// What a save found at its destination.
#[derive(Debug)]
enum Destination {
    /// It may be written; holds its attributes, or `None` when it does not exist or cannot
    /// be read.
    Writable(Option<ssh2::FileStat>),
    /// It must be left alone.
    Conflict(RemoteConflict),
}

/// Decide from the destination's attributes whether a save may replace it.
///
/// ### Arguments
/// - `stat`: Result of reading the destination's attributes.
/// - `guard`: What the write requires of the destination.
/// - `remote_path`: Path named in the error message.
///
/// ### Errors
/// - Returns an `SshError::SftpError` when the attributes of a destination that must be
///   unchanged cannot be read, unless it does not exist.
///
/// ### Returns
/// - `Ok(Destination)`: The destination may be written, or is in conflict.
/// - `Err(SshError::SftpError)`: The destination could not be checked.
fn check_destination(
    stat: Result<ssh2::FileStat, ssh2::Error>,
    guard: RemoteWriteGuard,
    remote_path: &str,
) -> Result<Destination, SshError> {
    match stat {
        Ok(stat) => {
            let current = RemoteFileVersion::from_stat(&stat);
            Ok(guard
                .conflict_with(&current)
                .map_or(Destination::Writable(Some(stat)), Destination::Conflict))
        }
        Err(e) if is_missing_path_error(&e) || guard == RemoteWriteGuard::Overwrite => {
            Ok(Destination::Writable(None))
        }
        Err(e) => Err(cannot_inspect_error(remote_path, &e)),
    }
}

/// Write bytes to a remote file via SFTP, unless it changed since the expected version.
///
/// ### Arguments
/// - `session`: Established SSH session with SFTP subsystem.
/// - `remote_path`: Absolute or home-relative (`~/...`) destination path on the remote host.
/// - `data`: File contents to write.
/// - `guard`: What the write requires of the destination.
/// - `before_write`: Called once the destination satisfies `guard`, before anything changes
///   on the remote host.
///
/// ### Errors
/// - Returns an `SshError::SftpError` when the remote home directory cannot be resolved,
///   when the destination cannot be checked against `guard`, or on any write, rename, or
///   permission failure against the temporary file or the destination path.
///
/// ### Returns
/// - `Ok(RemoteWriteOutcome::Written)`: File written and moved into place.
/// - `Ok(RemoteWriteOutcome::Refused)`: The destination does not satisfy `guard`; nothing
///   was written.
/// - `Err(SshError::SftpError)`: Check, write, rename, or permission error.
pub fn write_remote_file(
    session: &SshSession,
    remote_path: &str,
    data: &[u8],
    guard: RemoteWriteGuard,
    before_write: impl FnOnce(),
) -> Result<RemoteWriteOutcome, SshError> {
    let remote_path = resolve_home_relative_path(session, remote_path)?;
    // Resolve symlinks so the write targets the real file instead of replacing the link.
    let resolved = session
        .sftp
        .realpath(Path::new(&remote_path))
        .ok()
        .and_then(|path| path.to_str().map(str::to_string))
        .unwrap_or(remote_path);
    let dest_path = Path::new(&resolved);

    let dest_stat = match check_destination(session.sftp.stat(dest_path), guard, &resolved)? {
        Destination::Writable(stat) => stat,
        Destination::Conflict(conflict) => return Ok(RemoteWriteOutcome::Refused(conflict)),
    };
    before_write();
    replace_destination(
        session,
        &resolved,
        data,
        DestAttributes::from_stat(dest_stat.as_ref()),
    )?;
    Ok(RemoteWriteOutcome::Written(RemoteFileVersion::after_write(
        session.sftp.stat(dest_path).ok().as_ref(),
        data.len() as u64,
    )))
}

/// Replace a remote file's contents using a temp-then-rename approach.
///
/// ### Description
/// Writes to a `.fulgur.tmp.{pid}.{nanos}` sibling, then installs it with `install_temp`: the
/// temp file gets the destination's owner, group and permission bits, and `replace_with_temp`
/// moves it over the destination, keeping a complete copy of either the original or the new
/// content on the server at every step. A partial write therefore never corrupts the original.
/// The non-atomic truncating write is used when the temp file cannot be created, when it
/// cannot be given the destination's owner (so the file keeps its owner and group), or when
/// the server refuses every rename. Hard links cannot be detected because SFTP v3 attributes
/// carry no link count, so a hard-linked destination is still split by the rename.
///
/// ### Arguments
/// - `session`: Established SSH session with SFTP subsystem.
/// - `resolved`: Absolute destination path, with symlinks resolved.
/// - `data`: File contents to write.
/// - `dest_attributes`: Attributes of the destination to carry over.
///
/// ### Errors
/// - Returns an `SshError::SftpError` on any write, rename, or permission failure against
///   the temporary file or the destination path.
///
/// ### Returns
/// - `Ok(())`: File written and renamed successfully.
/// - `Err(SshError::SftpError)`: Write, rename, or permission error.
fn replace_destination(
    session: &SshSession,
    resolved: &str,
    data: &[u8],
    dest_attributes: DestAttributes,
) -> Result<(), SshError> {
    let dest_mode = dest_attributes.mode;

    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();

    let tmp_str = format!("{resolved}.fulgur.tmp.{pid}.{nanos}");
    let backup_str = format!("{resolved}.fulgur.bak.{pid}.{nanos}");
    let tmp_path = Path::new(&tmp_str);
    let backup_path = Path::new(&backup_str);
    let dest_path = Path::new(resolved);

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

    install_temp(
        &session.sftp,
        tmp_path,
        dest_path,
        backup_path,
        data,
        dest_attributes,
    )
    .map_err(|e| SshError::SftpError(format!("Cannot replace {resolved}: {e}")))
}

/// The SFTP primitives needed to move a fully written temp file over a destination.
trait RemoteReplaceOps {
    /// Read the owner and group of a remote file.
    ///
    /// ### Arguments
    /// - `path`: File to inspect.
    ///
    /// ### Returns
    /// - `Some(FileOwner)`: The server reported the file's uid and gid.
    /// - `None`: The file cannot be read or the server omitted either id.
    fn owner(&self, path: &Path) -> Option<FileOwner>;

    /// Change the owner and group of a remote file.
    ///
    /// ### Arguments
    /// - `path`: File to change.
    /// - `owner`: Requested uid and gid.
    ///
    /// ### Returns
    /// - `Ok(())`: The server accepted the request.
    /// - `Err(String)`: The server refused it, typically because the SSH user is neither root
    ///   nor a member of the requested group.
    fn set_owner(&self, path: &Path, owner: FileOwner) -> Result<(), String>;

    /// Set the permission bits of a remote file.
    ///
    /// ### Arguments
    /// - `path`: File to change.
    /// - `mode`: Permission bits to apply.
    ///
    /// ### Returns
    /// - `Ok(())`: The server accepted the request.
    /// - `Err(String)`: The server refused it.
    fn set_mode(&self, path: &Path, mode: u32) -> Result<(), String>;

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
    fn owner(&self, path: &Path) -> Option<FileOwner> {
        ssh2::Sftp::stat(self, path)
            .ok()
            .as_ref()
            .and_then(FileOwner::from_stat)
    }

    fn set_owner(&self, path: &Path, owner: FileOwner) -> Result<(), String> {
        self.setstat(
            path,
            ssh2::FileStat {
                size: None,
                uid: Some(owner.uid),
                gid: Some(owner.gid),
                perm: None,
                atime: None,
                mtime: None,
            },
        )
        .map_err(|e| e.to_string())
    }

    fn set_mode(&self, path: &Path, mode: u32) -> Result<(), String> {
        self.setstat(
            path,
            ssh2::FileStat {
                size: None,
                uid: None,
                gid: None,
                perm: Some(mode),
                atime: None,
                mtime: None,
            },
        )
        .map_err(|e| e.to_string())
    }

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

/// Give a fully written temp file the destination's attributes, then move it over the destination.
///
/// ### Description
/// When the temp file's owner or group differs from the destination's, it is changed to
/// match (a non-root user can only switch to a group it belongs to). If the ownership still
/// differs, the destination is written in place instead, which keeps its inode and therefore
/// its owner, group, mode, ACLs and hard links. If that in-place write fails, for example
/// because the SSH user cannot write the destination itself, the temp file is moved over it
/// anyway and the destination ends up owned by the SSH user. The permission bits are applied
/// after the ownership change, because changing ownership clears the setuid and setgid bits.
///
/// ### Arguments
/// - `ops`: SFTP primitives used to inspect, change, rename, delete, and write.
/// - `tmp_path`: Temporary file holding the complete new content.
/// - `dest_path`: Final destination path.
/// - `backup_path`: Unused sibling path that receives the original while swapping.
/// - `data`: New content, used by the in-place write paths.
/// - `dest`: Attributes of the destination to carry over.
///
/// ### Returns
/// - `Ok(())`: The destination holds the new content and no temp or backup file is left.
/// - `Err(String)`: The replacement failed. The message names any temp or backup file left
///   behind because it holds the only complete copy of the new or original content.
fn install_temp(
    ops: &impl RemoteReplaceOps,
    tmp_path: &Path,
    dest_path: &Path,
    backup_path: &Path,
    data: &[u8],
    dest: DestAttributes,
) -> Result<(), String> {
    if let Some(owner) = dest.owner
        && !temp_matches_owner(ops, tmp_path, owner)
    {
        match ops.write_truncating(dest_path, data, dest.mode) {
            Ok(()) => {
                log::info!(
                    "Wrote {} in place to keep its owner (uid {}, gid {})",
                    dest_path.display(),
                    owner.uid,
                    owner.gid
                );
                let _ = ops.unlink(tmp_path);
                return Ok(());
            }
            Err(write_err) => log::warn!(
                "Cannot write {} in place ({write_err}); replacing it, which changes its owner to the SSH user",
                dest_path.display()
            ),
        }
    }

    if let Err(mode_err) = ops.set_mode(tmp_path, dest.mode) {
        log::debug!(
            "Cannot set mode {:o} on {} ({mode_err})",
            dest.mode,
            tmp_path.display()
        );
    }
    replace_with_temp(ops, tmp_path, dest_path, backup_path, data, dest.mode)
}

/// Make sure the temp file has the destination's owner and group, changing them if needed.
///
/// ### Arguments
/// - `ops`: SFTP primitives used to inspect and change the ownership.
/// - `tmp_path`: Temporary file holding the new content.
/// - `owner`: Owner and group of the destination.
///
/// ### Returns
/// - `true`: The temp file is owned by `owner`, as confirmed by the server.
/// - `false`: The ownership differs and could not be changed or confirmed.
fn temp_matches_owner(ops: &impl RemoteReplaceOps, tmp_path: &Path, owner: FileOwner) -> bool {
    if ops.owner(tmp_path) == Some(owner) {
        return true;
    }
    if let Err(chown_err) = ops.set_owner(tmp_path, owner) {
        log::debug!(
            "Cannot give {} uid {} and gid {} ({chown_err})",
            tmp_path.display(),
            owner.uid,
            owner.gid
        );
        return false;
    }
    ops.owner(tmp_path) == Some(owner)
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
    use super::{
        DestAttributes, Destination, FileOwner, RemoteReplaceOps, check_destination, install_temp,
        replace_with_temp,
    };
    use crate::fulgur::sync::ssh::error::SshError;
    use crate::fulgur::sync::ssh::sftp::{RemoteConflict, RemoteFileVersion, RemoteWriteGuard};
    use ssh2::ErrorCode;
    use std::cell::{Cell, RefCell};
    use std::collections::{HashMap, HashSet};
    use std::path::{Path, PathBuf};

    const DEST: &str = "/srv/app/config.toml";
    const TMP: &str = "/srv/app/config.toml.fulgur.tmp.1.2";
    const BACKUP: &str = "/srv/app/config.toml.fulgur.bak.1.2";
    const ORIGINAL: &[u8] = b"original";
    const UPDATED: &[u8] = b"updated";
    const SSH_USER: FileOwner = FileOwner {
        uid: 1000,
        gid: 1000,
    };
    const SHARED_GROUP: FileOwner = FileOwner { uid: 1000, gid: 33 };
    const OTHER_USER: FileOwner = FileOwner { uid: 1001, gid: 33 };

    /// In-memory remote filesystem whose renames follow SFTP v3 (OpenSSH) semantics:
    /// renaming onto an existing path always fails, whatever flags are requested.
    #[derive(Default)]
    struct SftpV3Mock {
        files: RefCell<HashMap<PathBuf, Vec<u8>>>,
        owners: RefCell<HashMap<PathBuf, FileOwner>>,
        attribute_changes: RefCell<Vec<&'static str>>,
        refuse_owner_changes: bool,
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

        /// Give the destination and the temp file their owners.
        ///
        /// ### Arguments
        /// - `dest`: Owner of the original destination.
        /// - `tmp`: Owner of the temp file, i.e. the SSH user.
        ///
        /// ### Returns
        /// - `SftpV3Mock`: The mock with both owners recorded.
        fn owned_by(self, dest: FileOwner, tmp: FileOwner) -> Self {
            self.owners.borrow_mut().insert(PathBuf::from(DEST), dest);
            self.owners.borrow_mut().insert(PathBuf::from(TMP), tmp);
            self
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
            let mut owners = self.owners.borrow_mut();
            if let Some(owner) = owners.remove(src) {
                owners.insert(dst.to_path_buf(), owner);
            }
            Ok(())
        }

        fn unlink(&self, path: &Path) -> Result<(), String> {
            self.owners.borrow_mut().remove(path);
            self.files
                .borrow_mut()
                .remove(path)
                .map(|_| ())
                .ok_or_else(|| "SFTP(2) no such file".to_string())
        }

        fn owner(&self, path: &Path) -> Option<FileOwner> {
            self.owners.borrow().get(path).copied()
        }

        fn set_owner(&self, path: &Path, owner: FileOwner) -> Result<(), String> {
            self.attribute_changes.borrow_mut().push("owner");
            if self.refuse_owner_changes {
                return Err("SFTP(3) permission denied".to_string());
            }
            self.owners.borrow_mut().insert(path.to_path_buf(), owner);
            Ok(())
        }

        fn set_mode(&self, _path: &Path, _mode: u32) -> Result<(), String> {
            self.attribute_changes.borrow_mut().push("mode");
            Ok(())
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

    /// Run `install_temp` against the mock with the shared test paths.
    ///
    /// ### Arguments
    /// - `mock`: Mock remote filesystem to operate on.
    /// - `dest_owner`: Owner of the destination to preserve.
    ///
    /// ### Returns
    /// - `Result<(), String>`: The result of `install_temp`.
    fn install(mock: &SftpV3Mock, dest_owner: FileOwner) -> Result<(), String> {
        install_temp(
            mock,
            Path::new(TMP),
            Path::new(DEST),
            Path::new(BACKUP),
            UPDATED,
            DestAttributes {
                owner: Some(dest_owner),
                mode: 0o660,
            },
        )
    }

    /// Attributes of a 10-byte destination modified at the given time.
    ///
    /// ### Arguments
    /// - `mtime`: Modification time reported by the server.
    ///
    /// ### Returns
    /// - `ssh2::FileStat`: The attributes.
    fn dest_stat(mtime: u64) -> ssh2::FileStat {
        ssh2::FileStat {
            size: Some(10),
            uid: None,
            gid: None,
            perm: Some(0o100_644),
            atime: None,
            mtime: Some(mtime),
        }
    }

    /// Build the error a server returns for a failed metadata lookup.
    ///
    /// ### Arguments
    /// - `status`: SFTP status code.
    ///
    /// ### Returns
    /// - `ssh2::Error`: The error.
    fn sftp_error(status: i32) -> ssh2::Error {
        ssh2::Error::new(ErrorCode::SFTP(status), "stat failed")
    }

    /// Guard accepting only the destination described by `dest_stat(1_000)`.
    ///
    /// ### Returns
    /// - `RemoteWriteGuard`: The guard.
    fn loaded_version_guard() -> RemoteWriteGuard {
        RemoteWriteGuard::Unchanged(Some(RemoteFileVersion::from_stat(&dest_stat(1_000))))
    }

    #[test]
    fn unchanged_destination_is_writable_and_keeps_its_attributes() {
        let destination = check_destination(Ok(dest_stat(1_000)), loaded_version_guard(), DEST);

        assert!(
            matches!(destination, Ok(Destination::Writable(Some(stat))) if stat == dest_stat(1_000))
        );
    }

    #[test]
    fn destination_changed_on_the_server_is_a_conflict() {
        let destination = check_destination(Ok(dest_stat(2_000)), loaded_version_guard(), DEST);

        assert!(matches!(
            destination,
            Ok(Destination::Conflict(RemoteConflict::Changed))
        ));
    }

    #[test]
    fn existing_destination_with_unknown_baseline_is_a_conflict() {
        let guard = RemoteWriteGuard::Unchanged(None);

        assert!(matches!(
            check_destination(Ok(dest_stat(1_000)), guard, DEST),
            Ok(Destination::Conflict(RemoteConflict::UnknownBaseline))
        ));
    }

    #[test]
    fn missing_destination_is_writable_whatever_the_baseline() {
        for guard in [loaded_version_guard(), RemoteWriteGuard::Unchanged(None)] {
            assert!(matches!(
                check_destination(Err(sftp_error(2)), guard, DEST),
                Ok(Destination::Writable(None))
            ));
        }
    }

    #[test]
    fn destination_that_cannot_be_inspected_is_not_overwritten_unchecked() {
        let refused = check_destination(Err(sftp_error(3)), loaded_version_guard(), DEST);
        assert!(matches!(refused, Err(SshError::SftpError(message)) if message.contains(DEST)));

        let forced = check_destination(Err(sftp_error(3)), RemoteWriteGuard::Overwrite, DEST);
        assert!(matches!(forced, Ok(Destination::Writable(None))));
    }

    #[test]
    fn overwrite_guard_accepts_a_changed_destination() {
        let destination =
            check_destination(Ok(dest_stat(2_000)), RemoteWriteGuard::Overwrite, DEST);

        assert!(matches!(destination, Ok(Destination::Writable(Some(_)))));
    }

    #[test]
    fn dest_attributes_keep_special_mode_bits_and_owner() {
        let stat = ssh2::FileStat {
            size: None,
            uid: Some(SHARED_GROUP.uid),
            gid: Some(SHARED_GROUP.gid),
            perm: Some(0o100_4755),
            atime: None,
            mtime: None,
        };

        let attributes = DestAttributes::from_stat(Some(&stat));

        assert_eq!(attributes.owner, Some(SHARED_GROUP));
        assert_eq!(attributes.mode, 0o4755);
    }

    #[test]
    fn dest_attributes_default_when_destination_is_missing() {
        let attributes = DestAttributes::from_stat(None);

        assert_eq!(attributes.owner, None);
        assert_eq!(attributes.mode, 0o644);
    }

    #[test]
    fn install_with_matching_owner_swaps_without_owner_change() {
        let mock = SftpV3Mock::with_dest_and_tmp().owned_by(SSH_USER, SSH_USER);

        install(&mock, SSH_USER).expect("install should succeed");

        assert_eq!(mock.content(DEST).as_deref(), Some(UPDATED));
        assert_eq!(mock.content(TMP), None);
        assert_eq!(mock.owner(Path::new(DEST)), Some(SSH_USER));
        assert_eq!(*mock.attribute_changes.borrow(), ["mode"]);
        assert_eq!(mock.truncating_writes.get(), 0);
    }

    #[test]
    fn install_gives_temp_the_destination_group_before_its_mode() {
        let mock = SftpV3Mock::with_dest_and_tmp().owned_by(SHARED_GROUP, SSH_USER);

        install(&mock, SHARED_GROUP).expect("install should succeed");

        assert_eq!(mock.content(DEST).as_deref(), Some(UPDATED));
        assert_eq!(mock.owner(Path::new(DEST)), Some(SHARED_GROUP));
        assert_eq!(*mock.attribute_changes.borrow(), ["owner", "mode"]);
        assert_eq!(mock.truncating_writes.get(), 0);
    }

    #[test]
    fn install_writes_in_place_when_owner_cannot_be_matched() {
        let mock = SftpV3Mock {
            refuse_owner_changes: true,
            ..SftpV3Mock::with_dest_and_tmp()
        }
        .owned_by(OTHER_USER, SSH_USER);

        install(&mock, OTHER_USER).expect("in-place write should succeed");

        assert_eq!(mock.content(DEST).as_deref(), Some(UPDATED));
        assert_eq!(mock.owner(Path::new(DEST)), Some(OTHER_USER));
        assert_eq!(mock.content(TMP), None);
        assert_eq!(mock.content(BACKUP), None);
        assert_eq!(mock.truncating_writes.get(), 1);
    }

    #[test]
    fn install_replaces_destination_when_in_place_write_fails() {
        let mock = SftpV3Mock {
            refuse_owner_changes: true,
            fail_truncating_writes: true,
            ..SftpV3Mock::with_dest_and_tmp()
        }
        .owned_by(OTHER_USER, SSH_USER);

        install(&mock, OTHER_USER).expect("replacing should recover the save");

        assert_eq!(mock.content(DEST).as_deref(), Some(UPDATED));
        assert_eq!(mock.owner(Path::new(DEST)), Some(SSH_USER));
        assert_eq!(mock.content(TMP), None);
        assert_eq!(mock.content(BACKUP), None);
    }

    #[test]
    fn install_keeps_temp_when_every_write_path_fails() {
        let mock = SftpV3Mock {
            refuse_owner_changes: true,
            fail_truncating_writes: true,
            refuse_all_renames: true,
            ..SftpV3Mock::with_dest_and_tmp()
        }
        .owned_by(OTHER_USER, SSH_USER);

        let err = install(&mock, OTHER_USER).expect_err("install should fail");

        assert_eq!(mock.content(TMP).as_deref(), Some(UPDATED));
        assert!(err.contains(TMP));
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
