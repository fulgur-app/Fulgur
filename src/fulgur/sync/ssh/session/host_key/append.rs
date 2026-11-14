use crate::fulgur::sync::ssh::error::SshError;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Build the host string format used when storing entries in `known_hosts`.
///
/// ### Arguments
/// - `host`: Hostname or IP.
/// - `port`: SSH port.
///
/// ### Returns
/// - `String`: Plain host for port 22, or bracketed `[host]:port` for non-default ports.
pub(super) fn known_hosts_entry_host(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

/// Append one OpenSSH-format entry to `known_hosts`, leaving existing lines untouched.
///
/// ### Arguments
/// - `known_hosts_path`: Path to the `known_hosts` file, created if it does not exist.
/// - `known_host`: Host pattern to store, as produced by `known_hosts_entry_host`.
/// - `key`: Raw server host key in SSH wire format.
///
/// ### Errors
/// - Returns `SshError::ConnectionFailed` if the key blob is malformed or the file cannot be
///   opened, inspected, or written.
///
/// ### Returns
/// - `Ok(())`: The entry was appended.
/// - `Err(SshError::ConnectionFailed)`: The key was unusable or the file could not be updated.
pub(super) fn append_known_hosts_entry(
    known_hosts_path: &Path,
    known_host: &str,
    key: &[u8],
) -> Result<(), SshError> {
    let entry = known_hosts_entry_line(known_host, key)?;
    let separator = if known_hosts_ends_mid_line(known_hosts_path)? {
        "\n"
    } else {
        ""
    };

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(known_hosts_path)
        .map_err(|e| SshError::ConnectionFailed(format!("Failed to open known_hosts: {e}")))?;
    file.write_all(format!("{separator}{entry}\n").as_bytes())
        .map_err(|e| SshError::ConnectionFailed(format!("Failed to write known_hosts: {e}")))
}

/// Build a single OpenSSH `known_hosts` line for a host pattern and a raw host key.
///
/// ### Arguments
/// - `known_host`: Host pattern to store, as produced by `known_hosts_entry_host`.
/// - `key`: Raw server host key in SSH wire format.
///
/// ### Errors
/// - Returns `SshError::ConnectionFailed` if the algorithm name cannot be read from the key.
///
/// ### Returns
/// - `Ok(String)`: The `<host> <algorithm> <base64-key>` line, without a trailing newline.
/// - `Err(SshError::ConnectionFailed)`: The key blob is not in SSH wire format.
fn known_hosts_entry_line(known_host: &str, key: &[u8]) -> Result<String, SshError> {
    let algorithm = host_key_algorithm_name(key).ok_or_else(|| {
        SshError::ConnectionFailed(
            "Server host key is not in a recognizable SSH wire format".to_string(),
        )
    })?;
    Ok(format!("{known_host} {algorithm} {}", BASE64.encode(key)))
}

/// Read the algorithm name embedded at the start of an SSH wire-format public key.
///
/// ### Arguments
/// - `key`: Raw server host key in SSH wire format.
///
/// ### Returns
/// - `Some(&str)`: The algorithm name, for example `"ssh-ed25519"`.
/// - `None`: The blob is truncated or the name is not a printable ASCII token.
fn host_key_algorithm_name(key: &[u8]) -> Option<&str> {
    let length_prefix: [u8; 4] = key.get(..4)?.try_into().ok()?;
    let name_length = usize::try_from(u32::from_be_bytes(length_prefix)).ok()?;
    let name = std::str::from_utf8(key.get(4..)?.get(..name_length)?).ok()?;
    let is_single_token = !name.is_empty() && name.chars().all(|c| c.is_ascii_graphic());
    is_single_token.then_some(name)
}

/// Report whether `known_hosts` ends without a trailing newline.
///
/// ### Arguments
/// - `known_hosts_path`: Path to the `known_hosts` file.
///
/// ### Errors
/// - Returns `SshError::ConnectionFailed` if the file exists but cannot be inspected.
///
/// ### Returns
/// - `Ok(true)`: The file has content whose last byte is not a newline, so an appended entry
///   must be preceded by one.
/// - `Ok(false)`: The file is missing, empty, or already newline-terminated.
/// - `Err(SshError::ConnectionFailed)`: The file could not be opened or read.
fn known_hosts_ends_mid_line(known_hosts_path: &Path) -> Result<bool, SshError> {
    let mut file = match File::open(known_hosts_path) {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
        Err(e) => {
            return Err(SshError::ConnectionFailed(format!(
                "Failed to open known_hosts: {e}"
            )));
        }
    };

    let read_error =
        |e: std::io::Error| SshError::ConnectionFailed(format!("Failed to read known_hosts: {e}"));
    if file.seek(SeekFrom::End(0)).map_err(read_error)? == 0 {
        return Ok(false);
    }

    file.seek(SeekFrom::End(-1)).map_err(read_error)?;
    let mut last_byte = [0_u8; 1];
    file.read_exact(&mut last_byte).map_err(read_error)?;
    Ok(last_byte[0] != b'\n')
}

#[cfg(test)]
mod tests {
    use super::super::test_fixtures::server_key_bytes;
    use super::{
        append_known_hosts_entry, host_key_algorithm_name, known_hosts_entry_host,
        known_hosts_entry_line,
    };

    #[test]
    fn known_hosts_entry_uses_plain_host_for_default_port() {
        assert_eq!(known_hosts_entry_host("example.com", 22), "example.com");
    }

    #[test]
    fn known_hosts_entry_uses_bracket_host_for_custom_port() {
        assert_eq!(
            known_hosts_entry_host("example.com", 2222),
            "[example.com]:2222"
        );
    }

    #[test]
    fn algorithm_name_is_read_from_the_wire_format_prefix() {
        let key = server_key_bytes();
        assert_eq!(host_key_algorithm_name(&key), Some("ssh-ed25519"));
    }

    #[test]
    fn algorithm_name_rejects_a_truncated_key() {
        assert_eq!(host_key_algorithm_name(&[0, 0, 0, 11, b's']), None);
    }

    #[test]
    fn algorithm_name_rejects_a_non_printable_name() {
        assert_eq!(
            host_key_algorithm_name(&[0, 0, 0, 3, b'a', b' ', b'b']),
            None
        );
    }

    #[test]
    fn entry_line_uses_the_openssh_host_algorithm_key_layout() {
        let line = known_hosts_entry_line("[example.com]:2222", &server_key_bytes())
            .expect("failed to build known_hosts line");

        let mut fields = line.split(' ');
        assert_eq!(fields.next(), Some("[example.com]:2222"));
        assert_eq!(fields.next(), Some("ssh-ed25519"));
        assert_eq!(
            fields.next(),
            Some("AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ")
        );
        assert_eq!(fields.next(), None);
    }

    #[test]
    fn appending_preserves_lines_a_parser_cannot_represent() {
        let existing = "# a comment libssh2 drops\n\
             @cert-authority *.example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G1sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X\n\
             @revoked old.example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILIG2T/B0l0gaqj3puu510tu9N1OkQ4znY3LYuEm5zCF\n";
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("known_hosts");
        std::fs::write(&path, existing).expect("failed to seed known_hosts");

        append_known_hosts_entry(&path, "example.com", &server_key_bytes())
            .expect("failed to append known_hosts entry");

        let contents = std::fs::read_to_string(&path).expect("failed to read known_hosts");
        assert!(contents.starts_with(existing));
        assert!(contents.ends_with('\n'));
        assert_eq!(contents.lines().count(), 4);
    }

    #[test]
    fn appending_separates_an_entry_from_a_file_ending_mid_line() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("known_hosts");
        std::fs::write(&path, "old.example.com ssh-rsa AAAAB3NzaC1yc2E=")
            .expect("failed to seed known_hosts");

        append_known_hosts_entry(&path, "example.com", &server_key_bytes())
            .expect("failed to append known_hosts entry");

        let contents = std::fs::read_to_string(&path).expect("failed to read known_hosts");
        assert_eq!(contents.lines().count(), 2);
        assert_eq!(
            contents.lines().next(),
            Some("old.example.com ssh-rsa AAAAB3NzaC1yc2E=")
        );
    }

    #[test]
    fn appending_creates_a_missing_known_hosts_file() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("known_hosts");

        append_known_hosts_entry(&path, "example.com", &server_key_bytes())
            .expect("failed to append known_hosts entry");

        let contents = std::fs::read_to_string(&path).expect("failed to read known_hosts");
        assert_eq!(contents.lines().count(), 1);
    }
}
