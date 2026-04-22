use super::error::SshError;
use super::url::RemoteSpec;
use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use ssh_key::{
    PublicKey,
    known_hosts::{Entry as KnownHostEntry, HostPatterns, KnownHosts},
};
use ssh2::Session;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;
use zeroize::Zeroizing;

const CONNECT_TIMEOUT_SECS: u64 = 10;
const SESSION_TIMEOUT_MS: u32 = 30_000;

/// An established SSH session with an open SFTP subsystem.
///
/// `Session` and `Sftp` are not `Send`, so this struct must stay within the thread that created it.
pub struct SshSession {
    pub session: Session,
    pub sftp: ssh2::Sftp,
}

/// Decision returned by the host-key callback when a server is not yet in `known_hosts`.
pub enum HostKeyDecision {
    Accept,
    Reject,
}

/// A request posted by the SSH background thread when it encounters a host key not in `known_hosts`.
///
/// The SSH thread places this into `Fulgur::pending_host_key_request` and then blocks on
/// `decision_rx.recv()`. The GPUI monitoring task picks up the request, shows the fingerprint
/// dialog, and sends the user's decision back via `decision_tx`.
pub struct HostKeyRequest {
    /// SHA-256 fingerprint of the server's host key, formatted as colon-separated hex pairs.
    pub fingerprint: String,
    /// Hostname of the remote server.
    pub host: String,
    /// SSH port of the remote server.
    pub port: u16,
    /// Channel sender to unblock the SSH thread once the user has decided.
    pub decision_tx: std::sync::mpsc::Sender<HostKeyDecision>,
}

/// Connect to a remote host over SSH and open an SFTP subsystem.
///
/// ### Description
/// Performs TCP connect (10 s timeout), SSH handshake, host-key verification against
/// `~/.ssh/known_hosts` (TOFU policy), password authentication, and SFTP subsystem init.
/// `host_key_cb` is called synchronously when the server's key is not yet in `known_hosts`;
/// on `Accept` the key is appended to the file.
///
/// ### Arguments
/// - `spec`: Parsed remote specification supplying host and port.
/// - `user`: Resolved username; must not be empty.
/// - `password`: Session-scoped password, zeroed on drop by the `Zeroizing` wrapper.
/// - `host_key_cb`: Called with `(fingerprint_sha256_hex, host, port)` when the host key is unknown.
///
/// ### Returns
/// - `Ok(SshSession)`: Ready session with an open SFTP subsystem.
/// - `Err(SshError)`: Any failure during TCP connect, handshake, host-key check, auth, or SFTP init.
pub fn connect(
    spec: &RemoteSpec,
    user: &str,
    password: &Zeroizing<String>,
    host_key_cb: impl FnOnce(&str, &str, u16) -> HostKeyDecision,
) -> Result<SshSession, SshError> {
    let addr_str = format!("{}:{}", spec.host, spec.port);
    let addr = addr_str
        .to_socket_addrs()
        .map_err(|e| SshError::ConnectionFailed(format!("Cannot resolve {}: {}", addr_str, e)))?
        .next()
        .ok_or_else(|| {
            SshError::ConnectionFailed(format!("No addresses found for {}", addr_str))
        })?;

    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .map_err(|e| SshError::ConnectionFailed(e.to_string()))?;

    let mut session = Session::new().map_err(|e| SshError::ConnectionFailed(e.to_string()))?;
    if let Some(hostkey_prefs) = hostkey_method_preferences_from_known_hosts(&spec.host, spec.port)
    {
        let _ = session.method_pref(ssh2::MethodType::HostKey, &hostkey_prefs);
    }
    session.set_timeout(SESSION_TIMEOUT_MS);
    session.set_tcp_stream(tcp);
    session
        .handshake()
        .map_err(|e| SshError::ConnectionFailed(e.to_string()))?;

    check_host_key(&session, &spec.host, spec.port, host_key_cb)?;

    session
        .userauth_password(user, password.as_str())
        .map_err(|_| SshError::AuthFailed)?;

    if !session.authenticated() {
        return Err(SshError::AuthFailed);
    }

    let sftp = session
        .sftp()
        .map_err(|e| SshError::SftpError(e.to_string()))?;

    Ok(SshSession { session, sftp })
}

/// Verify the server's host key against `~/.ssh/known_hosts`.
///
/// ### Description
/// Match → proceed silently. Mismatch → hard error (user must edit `known_hosts` manually).
/// NotFound/Failure → call `host_key_cb`; on `Accept` append the entry to the file.
///
/// ### Arguments
/// - `session`: Active SSH session after handshake, used to retrieve the server's host key.
/// - `host`: Hostname string, used for `known_hosts` lookup and callback.
/// - `port`: SSH port, used for `known_hosts` lookup and callback.
/// - `host_key_cb`: Called with `(fingerprint_sha256_hex, host, port)` when the key is not found.
///
/// ### Returns
/// - `Ok(())`: Host key verified or accepted by the user.
/// - `Err(SshError::HostKeyMismatch)`: Key in `known_hosts` does not match the server.
/// - `Err(SshError::ConnectionFailed)`: Key rejected by the user or I/O error on `known_hosts`.
fn check_host_key(
    session: &Session,
    host: &str,
    port: u16,
    host_key_cb: impl FnOnce(&str, &str, u16) -> HostKeyDecision,
) -> Result<(), SshError> {
    let kh_path = known_hosts_path();
    let mut known_hosts = session
        .known_hosts()
        .map_err(|e| SshError::ConnectionFailed(e.to_string()))?;

    if kh_path.exists() {
        known_hosts
            .read_file(&kh_path, ssh2::KnownHostFileKind::OpenSSH)
            .map_err(|e| SshError::ConnectionFailed(format!("Failed to read known_hosts: {e}")))?;
    }

    let (key, key_type) = session
        .host_key()
        .ok_or_else(|| SshError::ConnectionFailed("Server provided no host key".to_string()))?;

    match resolve_known_host_check_result_with_known_hosts_fallback(&known_hosts, host, port, key) {
        ssh2::CheckResult::Match => Ok(()),
        ssh2::CheckResult::Mismatch => Err(SshError::HostKeyMismatch {
            host: host.to_string(),
            port,
        }),
        ssh2::CheckResult::NotFound | ssh2::CheckResult::Failure => {
            let fingerprint = sha256_fingerprint(key);
            match host_key_cb(&fingerprint, host, port) {
                HostKeyDecision::Reject => Err(SshError::ConnectionFailed(format!(
                    "Host key rejected for {host}:{port}"
                ))),
                HostKeyDecision::Accept => {
                    ensure_ssh_dir()?;
                    let known_host = known_hosts_entry_host(host, port);
                    known_hosts
                        .add(&known_host, key, "", host_key_type_to_format(key_type))
                        .map_err(|e| {
                            SshError::ConnectionFailed(format!("Failed to add host key: {e}"))
                        })?;
                    known_hosts
                        .write_file(&kh_path, ssh2::KnownHostFileKind::OpenSSH)
                        .map_err(|e| {
                            SshError::ConnectionFailed(format!("Failed to write known_hosts: {e}"))
                        })?;
                    set_file_permissions_600(&kh_path);
                    Ok(())
                }
            }
        }
    }
}

/// Resolve host-key check result with a pure-Rust `known_hosts` fallback.
///
/// ### Description
/// The primary check uses `libssh2` APIs. On some real-world `known_hosts` layouts, those APIs can
/// return non-match states even when OpenSSH would accept the same host key. For non-match results,
/// this method parses `known_hosts` with `ssh-key` and refines the decision when it can.
///
/// ### Arguments
/// - `known_hosts`: Loaded known-hosts collection.
/// - `host`: Hostname or IP used for the SSH connection.
/// - `port`: SSH port used for the SSH connection.
/// - `key`: Raw server host key returned by libssh2.
///
/// ### Returns
/// - `ssh2::CheckResult`: Resolved check result after optional fallback refinement.
fn resolve_known_host_check_result_with_known_hosts_fallback(
    known_hosts: &ssh2::KnownHosts,
    host: &str,
    port: u16,
    key: &[u8],
) -> ssh2::CheckResult {
    let primary = resolve_known_host_check_result(known_hosts, host, port, key);
    if matches!(primary, ssh2::CheckResult::Match) {
        return primary;
    }

    let fallback = check_known_hosts_with_parser(host, port, key, &known_hosts_path());
    match fallback {
        Some(ssh2::CheckResult::Match) => ssh2::CheckResult::Match,
        Some(ssh2::CheckResult::Mismatch) => ssh2::CheckResult::Mismatch,
        Some(ssh2::CheckResult::NotFound) => {
            if matches!(primary, ssh2::CheckResult::Failure) {
                ssh2::CheckResult::NotFound
            } else {
                primary
            }
        }
        Some(ssh2::CheckResult::Failure) | None => primary,
    }
}

/// Resolve host-key check result across host representations used by OpenSSH.
///
/// ### Description
/// `known_hosts` entries may exist as plain hostnames/IPs, bracketed host:port forms,
/// or be interpreted differently by `check` vs `check_port`. To stay compatible with
/// OpenSSH behavior, this method checks all relevant forms and accepts on any match.
///
/// ### Arguments
/// - `known_hosts`: Loaded known-hosts collection.
/// - `host`: Hostname or IP used for the SSH connection.
/// - `port`: SSH port used for the SSH connection.
/// - `key`: Raw server host key returned by libssh2.
///
/// ### Returns
/// - `ssh2::CheckResult::Match`: Any representation matched.
/// - `ssh2::CheckResult::Mismatch`: No matches and at least one representation mismatched.
/// - `ssh2::CheckResult::NotFound`: No matches/mismatches and at least one representation was missing.
/// - `ssh2::CheckResult::Failure`: All checks failed unexpectedly.
fn resolve_known_host_check_result(
    known_hosts: &ssh2::KnownHosts,
    host: &str,
    port: u16,
    key: &[u8],
) -> ssh2::CheckResult {
    let bracket_host = format!("[{host}]:{port}");
    if port == 22 {
        aggregate_check_results([
            known_hosts.check_port(host, port, key),
            known_hosts.check(host, key),
            known_hosts.check(&bracket_host, key),
        ])
    } else {
        aggregate_check_results([
            known_hosts.check_port(host, port, key),
            known_hosts.check(&bracket_host, key),
        ])
    }
}

/// Parse `known_hosts` with `ssh-key` and compare keys with the server key.
///
/// ### Arguments
/// - `host`: Hostname or IP used for the SSH connection.
/// - `port`: SSH port used for the SSH connection.
/// - `key`: Raw server host key returned by libssh2.
/// - `known_hosts_path`: Path to the `known_hosts` file.
///
/// ### Returns
/// - `Some(ssh2::CheckResult)`: Parsed result from known-host entries.
/// - `None`: The file could not be parsed or the key format is unsupported.
fn check_known_hosts_with_parser(
    host: &str,
    port: u16,
    key: &[u8],
    known_hosts_path: &Path,
) -> Option<ssh2::CheckResult> {
    let entries = KnownHosts::read_file(known_hosts_path).ok()?;
    let server_key = PublicKey::from_bytes(key).ok()?;
    Some(resolve_known_host_check_result_from_entries(
        &entries,
        host,
        port,
        &server_key,
    ))
}

/// Resolve host-key check result from parsed `known_hosts` entries.
///
/// ### Arguments
/// - `entries`: Parsed `known_hosts` entries.
/// - `host`: Hostname or IP used for the SSH connection.
/// - `port`: SSH port used for the SSH connection.
/// - `server_key`: Server key parsed from libssh2 raw bytes.
///
/// ### Returns
/// - `ssh2::CheckResult::Match`: A matching host entry with an identical key was found.
/// - `ssh2::CheckResult::Mismatch`: Host entry exists but key differs.
/// - `ssh2::CheckResult::NotFound`: No host entry matched.
fn resolve_known_host_check_result_from_entries(
    entries: &[KnownHostEntry],
    host: &str,
    port: u16,
    server_key: &PublicKey,
) -> ssh2::CheckResult {
    let mut saw_host_entry = false;
    for entry in entries {
        if !known_host_entry_matches_target(entry, host, port) {
            continue;
        }

        saw_host_entry = true;
        if entry.public_key().key_data() == server_key.key_data() {
            return ssh2::CheckResult::Match;
        }
    }

    if saw_host_entry {
        ssh2::CheckResult::Mismatch
    } else {
        ssh2::CheckResult::NotFound
    }
}

/// Build an SSH host-key algorithm preference from existing `known_hosts` entries.
///
/// ### Description
/// Some servers offer multiple host-key algorithms. OpenSSH tends to prefer the algorithm already
/// present in `known_hosts`, while libssh2 may negotiate a different one and then fail host-key
/// validation. This function extracts known key types for the target host and builds a preference
/// list that places those algorithms first.
///
/// ### Arguments
/// - `host`: Hostname or IP used for the SSH connection.
/// - `port`: SSH port used for the SSH connection.
///
/// ### Returns
/// - `Some(String)`: Comma-separated `MethodType::HostKey` preference string.
/// - `None`: No known key types were found or `known_hosts` could not be parsed.
fn hostkey_method_preferences_from_known_hosts(host: &str, port: u16) -> Option<String> {
    let known_types = known_host_algorithms_for_target(host, port, &known_hosts_path())?;
    if known_types.is_empty() {
        return None;
    }

    let mut merged: Vec<String> = Vec::new();
    for known in known_types {
        push_unique_hostkey_alg(&mut merged, &known);
    }
    for alg in default_hostkey_algorithms() {
        push_unique_hostkey_alg(&mut merged, alg);
    }
    Some(merged.join(","))
}

/// Extract matching host-key algorithms from parsed `known_hosts` entries.
///
/// ### Arguments
/// - `host`: Hostname or IP used for lookup.
/// - `port`: SSH port used for lookup.
/// - `known_hosts_path`: Path to the `known_hosts` file.
///
/// ### Returns
/// - `Some(Vec<String>)`: Distinct algorithm names in first-seen order.
/// - `None`: The file could not be parsed.
fn known_host_algorithms_for_target(
    host: &str,
    port: u16,
    known_hosts_path: &Path,
) -> Option<Vec<String>> {
    let entries = KnownHosts::read_file(known_hosts_path).ok()?;
    let mut key_types: Vec<String> = Vec::new();
    for entry in entries {
        if known_host_entry_matches_target(&entry, host, port) {
            push_unique_hostkey_alg(&mut key_types, entry.public_key().algorithm().as_ref());
        }
    }
    Some(key_types)
}

/// Check whether a `known_hosts` entry matches a target host and port.
///
/// ### Arguments
/// - `entry`: Parsed `known_hosts` entry.
/// - `host`: Hostname or IP used for the SSH connection.
/// - `port`: SSH port used for the SSH connection.
///
/// ### Returns
/// - `true`: The entry host pattern matches the target.
/// - `false`: The entry does not apply to the target.
fn known_host_entry_matches_target(entry: &KnownHostEntry, host: &str, port: u16) -> bool {
    known_host_patterns_match(entry.host_patterns(), host, port)
}

/// Evaluate known-host host-pattern matching for a target host and port.
///
/// ### Arguments
/// - `patterns`: Parsed host pattern set for one known-host entry.
/// - `host`: Hostname or IP used for the SSH connection.
/// - `port`: SSH port used for the SSH connection.
///
/// ### Returns
/// - `true`: Pattern set matches this target host.
/// - `false`: Pattern set does not match.
fn known_host_patterns_match(patterns: &HostPatterns, host: &str, port: u16) -> bool {
    let candidates = host_match_candidates(host, port);
    match patterns {
        HostPatterns::Patterns(patterns) => match_known_host_patterns(patterns, &candidates),
        HostPatterns::HashedName { salt, hash } => candidates
            .iter()
            .any(|candidate| hashed_host_matches(salt, hash, candidate)),
    }
}

/// Build candidate host strings used by OpenSSH known-host matching.
///
/// ### Arguments
/// - `host`: Hostname or IP used for the SSH connection.
/// - `port`: SSH port used for the SSH connection.
///
/// ### Returns
/// - `Vec<String>`: Candidate values to test against one pattern entry.
fn host_match_candidates(host: &str, port: u16) -> Vec<String> {
    if port == 22 {
        vec![host.to_string(), format!("[{host}]:22")]
    } else {
        vec![format!("[{host}]:{port}")]
    }
}

/// Match known-host pattern list with OpenSSH negation semantics.
///
/// ### Arguments
/// - `patterns`: Comma-separated host patterns from one known-host entry.
/// - `candidates`: Candidate host representations for the current connection.
///
/// ### Returns
/// - `true`: At least one positive pattern matched and no negated match rejected it.
/// - `false`: No positive match, or a negated pattern matched.
fn match_known_host_patterns(patterns: &[String], candidates: &[String]) -> bool {
    let mut matched = false;
    for pattern in patterns {
        let (negated, token) = if let Some(stripped) = pattern.strip_prefix('!') {
            (true, stripped)
        } else {
            (false, pattern.as_str())
        };

        if candidates
            .iter()
            .any(|candidate| wildcard_pattern_matches(token, candidate))
        {
            if negated {
                return false;
            }
            matched = true;
        }
    }
    matched
}

/// Match a target string against a known-host wildcard pattern.
///
/// ### Arguments
/// - `pattern`: Wildcard pattern using `*` and `?`.
/// - `target`: Candidate host string.
///
/// ### Returns
/// - `true`: Pattern matches the target.
/// - `false`: Pattern does not match.
fn wildcard_pattern_matches(pattern: &str, target: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let target_chars: Vec<char> = target.chars().collect();
    let mut pattern_index = 0usize;
    let mut target_index = 0usize;
    let mut star_pattern_index = None;
    let mut star_target_index = 0usize;

    while target_index < target_chars.len() {
        if pattern_index < pattern_chars.len()
            && (pattern_chars[pattern_index] == '?'
                || pattern_chars[pattern_index] == target_chars[target_index])
        {
            pattern_index += 1;
            target_index += 1;
        } else if pattern_index < pattern_chars.len() && pattern_chars[pattern_index] == '*' {
            star_pattern_index = Some(pattern_index);
            pattern_index += 1;
            star_target_index = target_index;
        } else if let Some(star_idx) = star_pattern_index {
            pattern_index = star_idx + 1;
            star_target_index += 1;
            target_index = star_target_index;
        } else {
            return false;
        }
    }

    while pattern_index < pattern_chars.len() && pattern_chars[pattern_index] == '*' {
        pattern_index += 1;
    }

    pattern_index == pattern_chars.len()
}

/// Validate a hashed known-host entry against a candidate host string.
///
/// ### Arguments
/// - `salt`: HMAC salt from the known-hosts hashed entry.
/// - `expected_hash`: SHA-1 HMAC output from the known-hosts hashed entry.
/// - `candidate`: Candidate host representation to verify.
///
/// ### Returns
/// - `true`: Candidate matches the hashed entry.
/// - `false`: Candidate does not match.
fn hashed_host_matches(salt: &[u8], expected_hash: &[u8; 20], candidate: &str) -> bool {
    type HmacSha1 = Hmac<Sha1>;
    let Ok(mut mac) = HmacSha1::new_from_slice(salt) else {
        return false;
    };
    mac.update(candidate.as_bytes());
    mac.verify_slice(expected_hash).is_ok()
}

/// Append a host-key algorithm to a list if it is not already present.
///
/// ### Arguments
/// - `list`: Ordered host-key algorithm list to mutate.
/// - `alg`: Candidate algorithm name.
fn push_unique_hostkey_alg(list: &mut Vec<String>, alg: &str) {
    if !alg.is_empty() && !list.iter().any(|existing| existing == alg) {
        list.push(alg.to_string());
    }
}

/// Return default host-key algorithms in preferred order.
///
/// ### Returns
/// - `&'static [&'static str]`: Built-in fallback algorithms.
fn default_hostkey_algorithms() -> &'static [&'static str] {
    &[
        "ssh-ed25519",
        "ecdsa-sha2-nistp256",
        "ecdsa-sha2-nistp384",
        "ecdsa-sha2-nistp521",
        "rsa-sha2-512",
        "rsa-sha2-256",
        "ssh-rsa",
    ]
}

/// Aggregate several `CheckResult` values into a single decision.
///
/// ### Description
/// Priority order is `Match` > `Mismatch` > `NotFound` > `Failure`.
/// This ensures we accept if any representation is an exact match, while still treating
/// genuine mismatches as errors when no representation matches.
///
/// ### Arguments
/// - `results`: Check results from different host representations.
///
/// ### Returns
/// - `ssh2::CheckResult`: Combined result following the documented priority order.
fn aggregate_check_results(
    results: impl IntoIterator<Item = ssh2::CheckResult>,
) -> ssh2::CheckResult {
    let mut saw_mismatch = false;
    let mut saw_not_found = false;
    for result in results {
        match result {
            ssh2::CheckResult::Match => return ssh2::CheckResult::Match,
            ssh2::CheckResult::Mismatch => saw_mismatch = true,
            ssh2::CheckResult::NotFound => saw_not_found = true,
            ssh2::CheckResult::Failure => {}
        }
    }
    if saw_mismatch {
        ssh2::CheckResult::Mismatch
    } else if saw_not_found {
        ssh2::CheckResult::NotFound
    } else {
        ssh2::CheckResult::Failure
    }
}

/// Build the host string format used when storing entries in `known_hosts`.
///
/// ### Arguments
/// - `host`: Hostname or IP.
/// - `port`: SSH port.
///
/// ### Returns
/// - `String`: Plain host for port 22, or bracketed `[host]:port` for non-default ports.
fn known_hosts_entry_host(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

/// Compute a colon-separated SHA-256 hex fingerprint from raw host-key bytes.
///
/// ### Arguments
/// - `key`: Raw bytes of the server's host key.
///
/// ### Returns
/// - `String`: Hex pairs joined by colons, e.g. `"ab:cd:ef:…"`.
fn sha256_fingerprint(key: &[u8]) -> String {
    let hash = Sha256::digest(key);
    hash.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// Map a `ssh2::HostKeyType` to the `KnownHostKeyFormat` required by `known_hosts.add`.
///
/// ### Arguments
/// - `key_type`: Key type reported by libssh2 after handshake.
///
/// ### Returns
/// - `ssh2::KnownHostKeyFormat`: Corresponding format constant; `Unknown` falls back to `SshRsa`.
fn host_key_type_to_format(key_type: ssh2::HostKeyType) -> ssh2::KnownHostKeyFormat {
    match key_type {
        ssh2::HostKeyType::Rsa => ssh2::KnownHostKeyFormat::SshRsa,
        ssh2::HostKeyType::Dss => ssh2::KnownHostKeyFormat::SshDss,
        ssh2::HostKeyType::Ecdsa256 => ssh2::KnownHostKeyFormat::Ecdsa256,
        ssh2::HostKeyType::Ecdsa384 => ssh2::KnownHostKeyFormat::Ecdsa384,
        ssh2::HostKeyType::Ecdsa521 => ssh2::KnownHostKeyFormat::Ecdsa521,
        ssh2::HostKeyType::Ed25519 => ssh2::KnownHostKeyFormat::Ed25519,
        ssh2::HostKeyType::Unknown => ssh2::KnownHostKeyFormat::SshRsa,
    }
}

/// Return the platform-appropriate path to `~/.ssh/known_hosts`.
///
/// ### Returns
/// - `PathBuf`: Absolute path derived from `home_dir()`.
fn known_hosts_path() -> PathBuf {
    home_dir().join(".ssh").join("known_hosts")
}

/// Create `~/.ssh` with mode `0700` on Unix if it does not already exist.
///
/// ### Returns
/// - `Ok(())`: Directory exists or was created successfully.
/// - `Err(SshError::IoError)`: Directory could not be created.
fn ensure_ssh_dir() -> Result<(), SshError> {
    let ssh_dir = home_dir().join(".ssh");
    if !ssh_dir.exists() {
        std::fs::create_dir_all(&ssh_dir)
            .map_err(|e| SshError::IoError(format!("Failed to create ~/.ssh: {e}")))?;
        set_dir_permissions_700(&ssh_dir);
    }
    Ok(())
}

/// Return the current user's home directory from platform-specific environment variables.
///
/// ### Returns
/// - `PathBuf`: Home directory path; falls back to `/tmp` on Unix or `C:\Users\User` on Windows
///   if the environment variable is missing.
pub fn home_dir() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE")
            .or_else(|_| {
                std::env::var("HOMEDRIVE")
                    .and_then(|d| std::env::var("HOMEPATH").map(|p| format!("{d}{p}")))
            })
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(r"C:\Users\User"))
    }
    #[cfg(not(windows))]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"))
    }
}

/// Set file permissions to `0600` on Unix; no-op on Windows.
///
/// ### Arguments
/// - `path`: Path to the file whose permissions are to be set.
fn set_file_permissions_600(path: &Path) {
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
    use super::{
        aggregate_check_results, default_hostkey_algorithms, known_host_algorithms_for_target,
        known_hosts_entry_host, push_unique_hostkey_alg,
        resolve_known_host_check_result_from_entries, wildcard_pattern_matches,
    };
    use ssh_key::{PublicKey, known_hosts::KnownHosts};

    fn parse_known_host_entries(input: &str) -> Vec<ssh_key::known_hosts::Entry> {
        KnownHosts::new(input)
            .collect::<ssh_key::Result<Vec<_>>>()
            .expect("failed to parse known_hosts entries")
    }

    #[test]
    fn aggregate_prefers_match_over_mismatch() {
        let result = aggregate_check_results([
            ssh2::CheckResult::Mismatch,
            ssh2::CheckResult::Match,
            ssh2::CheckResult::NotFound,
        ]);
        assert!(matches!(result, ssh2::CheckResult::Match));
    }

    #[test]
    fn aggregate_returns_mismatch_when_no_match() {
        let result = aggregate_check_results([
            ssh2::CheckResult::Failure,
            ssh2::CheckResult::Mismatch,
            ssh2::CheckResult::NotFound,
        ]);
        assert!(matches!(result, ssh2::CheckResult::Mismatch));
    }

    #[test]
    fn aggregate_returns_not_found_before_failure() {
        let result =
            aggregate_check_results([ssh2::CheckResult::Failure, ssh2::CheckResult::NotFound]);
        assert!(matches!(result, ssh2::CheckResult::NotFound));
    }

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
    fn wildcard_pattern_matches_supports_globs() {
        assert!(wildcard_pattern_matches("*.example.com", "dev.example.com"));
        assert!(wildcard_pattern_matches("srv-??", "srv-01"));
        assert!(!wildcard_pattern_matches("srv-??", "srv-001"));
    }

    #[test]
    fn parser_fallback_returns_match_for_plain_entry() {
        let entries = parse_known_host_entries(
            "example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ",
        );
        let server_key = PublicKey::from_openssh(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ server",
        )
        .expect("failed to parse server key");

        let result =
            resolve_known_host_check_result_from_entries(&entries, "example.com", 22, &server_key);
        assert!(matches!(result, ssh2::CheckResult::Match));
    }

    #[test]
    fn parser_fallback_returns_mismatch_for_different_key() {
        let entries = parse_known_host_entries(
            "example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ",
        );
        let server_key = PublicKey::from_openssh(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G1sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X server",
        )
        .expect("failed to parse server key");

        let result =
            resolve_known_host_check_result_from_entries(&entries, "example.com", 22, &server_key);
        assert!(matches!(result, ssh2::CheckResult::Mismatch));
    }

    #[test]
    fn parser_fallback_matches_hashed_entry() {
        let entries = parse_known_host_entries(
            "|1|O33ESRMWPVkMYIwJ1Uw+n877jTo=|nuuC5vEqXlEZ/8BXQR7m619W6Ak= ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILIG2T/B0l0gaqj3puu510tu9N1OkQ4znY3LYuEm5zCF",
        );
        let server_key = PublicKey::from_openssh(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILIG2T/B0l0gaqj3puu510tu9N1OkQ4znY3LYuEm5zCF server",
        )
        .expect("failed to parse server key");

        let result =
            resolve_known_host_check_result_from_entries(&entries, "example.com", 22, &server_key);
        assert!(matches!(result, ssh2::CheckResult::Match));
    }

    #[test]
    fn known_host_algorithms_for_target_deduplicates_preserving_order() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let known_hosts_path = dir.path().join("known_hosts");
        std::fs::write(
            &known_hosts_path,
            "\
example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ
example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G1sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X",
        )
        .expect("failed to write known_hosts");

        let key_types =
            known_host_algorithms_for_target("example.com", 22, &known_hosts_path).unwrap();
        assert_eq!(key_types, vec!["ssh-ed25519".to_string()]);
    }

    #[test]
    fn push_unique_hostkey_alg_deduplicates() {
        let mut list = vec!["ssh-ed25519".to_string()];
        push_unique_hostkey_alg(&mut list, "ssh-ed25519");
        push_unique_hostkey_alg(&mut list, "rsa-sha2-256");
        assert_eq!(
            list,
            vec!["ssh-ed25519".to_string(), "rsa-sha2-256".to_string()]
        );
    }

    #[test]
    fn default_hostkey_algorithms_is_not_empty() {
        assert!(!default_hostkey_algorithms().is_empty());
    }
}
