use super::error::SshError;
use super::session::SshSession;
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

    let rename_result = session
        .sftp
        .rename(
            tmp_path,
            dest_path,
            Some(ssh2::RenameFlags::OVERWRITE | ssh2::RenameFlags::ATOMIC),
        )
        .map_err(|e| SshError::SftpError(format!("Rename failed for {remote_path}: {e}")));

    if rename_result.is_err() {
        let _ = session.sftp.unlink(tmp_path);
    }

    rename_result
}
