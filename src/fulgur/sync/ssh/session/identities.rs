use super::paths::home_dir;
use super::ssh_config::SshHostConfig;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

/// Key names OpenSSH tries by default, in its preference order.
const DEFAULT_IDENTITY_FILES: &[&str] = &["id_ed25519", "id_ecdsa", "id_rsa"];

/// List the private key files to try for a host, in OpenSSH order.
///
/// Like OpenSSH, the default key names in `~/.ssh` are only used when `~/.ssh/config`
/// configures no `IdentityFile` for the host.
///
/// ### Arguments
/// - `host_config`: Settings from `~/.ssh/config` for the host.
/// - `alias`: Host as typed in the remote URL.
/// - `user`: Remote username, substituted for `%r` in `IdentityFile` values.
///
/// ### Returns
/// - `Vec<PathBuf>`: Existing key files without duplicates; empty when the home directory
///   cannot be resolved.
pub(super) fn identity_files_for_host(
    host_config: &SshHostConfig,
    alias: &str,
    user: &str,
) -> Vec<PathBuf> {
    let Ok(home) = home_dir() else {
        return Vec::new();
    };
    let mut candidates = host_config.identity_files(alias, user, &home);
    if candidates.is_empty() {
        let ssh_dir = home.join(".ssh");
        candidates.extend(DEFAULT_IDENTITY_FILES.iter().map(|name| ssh_dir.join(name)));
    }

    let mut identity_files: Vec<PathBuf> = Vec::new();
    for candidate in candidates {
        if candidate.is_file() && !identity_files.contains(&candidate) {
            identity_files.push(candidate);
        }
    }
    identity_files
}

/// Read the SSH wire-format public key blob of a key file, as reported by ssh-agent.
///
/// ### Arguments
/// - `private_key`: Private key file; `<private_key>.pub` is read when present, otherwise
///   the public half is taken from an OpenSSH-format private key, encrypted or not.
///
/// ### Returns
/// - `Some(Vec<u8>)`: The public key blob.
/// - `None`: No readable public key could be found.
pub(super) fn public_key_blob(private_key: &Path) -> Option<Vec<u8>> {
    let mut public_key_path = private_key.as_os_str().to_owned();
    public_key_path.push(".pub");
    if let Ok(public_key) = std::fs::read_to_string(PathBuf::from(public_key_path)) {
        return ssh_key::PublicKey::from_openssh(public_key.trim())
            .ok()?
            .to_bytes()
            .ok();
    }
    let private_key = Zeroizing::new(std::fs::read_to_string(private_key).ok()?);
    ssh_key::PrivateKey::from_openssh(private_key.as_str())
        .ok()?
        .public_key()
        .to_bytes()
        .ok()
}

/// Replace a leading `~` with the home directory.
///
/// ### Arguments
/// - `raw`: Path as typed, e.g. `~/.ssh/id_work`.
/// - `home`: Home directory.
///
/// ### Returns
/// - `PathBuf`: The path with `~` expanded, or `raw` unchanged when it has no leading `~`.
#[must_use]
pub fn expand_tilde(raw: &str, home: &Path) -> PathBuf {
    if raw == "~" {
        return home.to_path_buf();
    }
    match raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        Some(rest) => home.join(rest),
        None => PathBuf::from(raw),
    }
}

/// Report whether a private key file is protected by a passphrase.
///
/// ### Arguments
/// - `path`: Private key file.
///
/// ### Returns
/// - `true`: The key is encrypted.
/// - `false`: The key is unencrypted, unreadable, or in an unrecognized format.
pub(super) fn private_key_is_encrypted(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(Zeroizing::new)
        .is_ok_and(|contents| key_text_is_encrypted(&contents))
}

/// Report whether PEM-armored private key text is protected by a passphrase.
///
/// OpenSSH-format keys record their cipher in the key body. Legacy PEM keys mark
/// encryption with a `Proc-Type: 4,ENCRYPTED` header or an `ENCRYPTED PRIVATE KEY` label.
///
/// ### Arguments
/// - `contents`: Private key file contents.
///
/// ### Returns
/// - `true`: The key is encrypted.
/// - `false`: The key is unencrypted or could not be parsed.
fn key_text_is_encrypted(contents: &str) -> bool {
    if contents.contains("BEGIN OPENSSH PRIVATE KEY") {
        return ssh_key::PrivateKey::from_openssh(contents).is_ok_and(|key| key.is_encrypted());
    }
    contents.contains("ENCRYPTED")
}

#[cfg(test)]
mod tests {
    use super::{expand_tilde, key_text_is_encrypted, public_key_blob};
    use std::path::{Path, PathBuf};

    const ENCRYPTED_OPENSSH_KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAACmFlczI1Ni1jdHIAAAAGYmNyeXB0AAAAGAAAABCaCn5DD/
jdz/LNFPwLZH1wAAAAGAAAAAEAAAAzAAAAC3NzaC1lZDI1NTE5AAAAIESLVi4AN2cqqCeh
vSyo8/l6WrZKcUnuaVCGJHDBWNMvAAAAoBx9etaNsE5hQ/TH3rw7AUg+O1FKxdKyoDmyeN
cs9qx35E2N3OE25WQt9fy+N3u6g7ANzUN605ZA3xOQKraGy6sCFusrZBrYocKgZ1ufURG+
NtNf0ShqUB/hQm8O69jz/juSkexnV5ptnKV82eEDn5wGKjiIKm12VoSkV7kSTBBpeQoyyi
z28Do0RL4Otgv+OWFUzLoWuHiROotpETe5v6k=
-----END OPENSSH PRIVATE KEY-----
";

    const PLAIN_OPENSSH_KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACDwEwEHCKeFF4b882w1lLMGGiIpINtpxM85xdoLCCh6ZwAAAJD2YAgV9mAI
FQAAAAtzc2gtZWQyNTUxOQAAACDwEwEHCKeFF4b882w1lLMGGiIpINtpxM85xdoLCCh6Zw
AAAEC8/RthVFFpU332IsoUk76F+UiciZIdFEySz8Rdf8ya+fATAQcIp4UXhvzzbDWUswYa
Iikg22nEzznF2gsIKHpnAAAADWZpeHR1cmUtcGxhaW4=
-----END OPENSSH PRIVATE KEY-----
";

    fn home() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\Users\alice")
        } else {
            PathBuf::from("/home/alice")
        }
    }

    #[test]
    fn key_text_is_encrypted_detects_openssh_cipher() {
        assert!(key_text_is_encrypted(ENCRYPTED_OPENSSH_KEY));
        assert!(!key_text_is_encrypted(PLAIN_OPENSSH_KEY));
    }

    #[test]
    fn key_text_is_encrypted_detects_legacy_pem_headers() {
        let legacy = "-----BEGIN RSA PRIVATE KEY-----\nProc-Type: 4,ENCRYPTED\n\
                      DEK-Info: AES-128-CBC,00\n\nAAAA\n-----END RSA PRIVATE KEY-----\n";
        let pkcs8 =
            "-----BEGIN ENCRYPTED PRIVATE KEY-----\nAAAA\n-----END ENCRYPTED PRIVATE KEY-----\n";
        let plain = "-----BEGIN RSA PRIVATE KEY-----\nAAAA\n-----END RSA PRIVATE KEY-----\n";
        assert!(key_text_is_encrypted(legacy));
        assert!(key_text_is_encrypted(pkcs8));
        assert!(!key_text_is_encrypted(plain));
    }

    #[test]
    fn expand_tilde_only_rewrites_leading_tilde() {
        let home = home();
        assert_eq!(expand_tilde("~", &home), home);
        assert_eq!(expand_tilde("~/.ssh/id", &home), home.join(".ssh/id"));
        assert_eq!(expand_tilde("/etc/key~", &home), Path::new("/etc/key~"));
    }

    #[test]
    fn public_key_blob_reads_encrypted_openssh_key_without_passphrase() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let encrypted = dir.path().join("id_encrypted");
        let plain = dir.path().join("id_plain");
        std::fs::write(&encrypted, ENCRYPTED_OPENSSH_KEY).expect("failed to write key");
        std::fs::write(&plain, PLAIN_OPENSSH_KEY).expect("failed to write key");

        let encrypted_blob = public_key_blob(&encrypted).expect("public half is readable");
        let plain_blob = public_key_blob(&plain).expect("public half is readable");

        assert_ne!(encrypted_blob, plain_blob);
        assert!(public_key_blob(&dir.path().join("missing")).is_none());
    }

    #[test]
    fn public_key_blob_prefers_pub_file() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let key = dir.path().join("id_plain");
        std::fs::write(&key, "not a key").expect("failed to write key");
        std::fs::write(
            dir.path().join("id_plain.pub"),
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPATAQcIp4UXhvzzbDWUswYaIikg22nEzznF2gsIKHpn fixture-plain\n",
        )
        .expect("failed to write public key");

        let from_pub = public_key_blob(&key).expect("pub file is readable");
        std::fs::write(&key, PLAIN_OPENSSH_KEY).expect("failed to write key");
        std::fs::remove_file(dir.path().join("id_plain.pub")).expect("failed to remove pub");
        let from_private = public_key_blob(&key).expect("private key is readable");

        assert_eq!(from_pub, from_private);
    }
}
