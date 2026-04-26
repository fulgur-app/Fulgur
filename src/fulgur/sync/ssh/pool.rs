use super::credentials::SshCredKey;
use super::error::SshError;
use super::session::{HostKeyDecision, SshSession, connect};
use super::url::RemoteSpec;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

/// Time after which an idle SSH session is considered stale and discarded.
pub const SSH_SESSION_IDLE_TTL: Duration = Duration::from_secs(60 * 60);

/// Maximum number of idle sessions held by the pool at once.
const SSH_SESSION_POOL_CAPACITY: usize = 64;

/// Identifier for a pooled session, derived from connection coordinates and password digest.
///
/// Including a SHA-256 digest of the password ensures rotated credentials never
/// silently reuse a session authenticated under the previous password.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SshSessionKey {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password_hash: [u8; 32],
}

impl SshSessionKey {
    /// Build a pool key from a `(host, port, user)` triple and a password.
    ///
    /// ### Arguments
    /// - `host`: Hostname or IP of the remote server.
    /// - `port`: SSH port.
    /// - `user`: Username used for authentication.
    /// - `password`: Password used for authentication; its SHA-256 digest is
    ///   stored in the key.
    ///
    /// ### Returns
    /// - `Self`: The composite key used to look up cached sessions.
    pub fn new(
        host: impl Into<String>,
        port: u16,
        user: impl Into<String>,
        password: &Zeroizing<String>,
    ) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(password.as_bytes());
        let mut password_hash = [0u8; 32];
        password_hash.copy_from_slice(&hasher.finalize());
        Self {
            host: host.into(),
            port,
            user: user.into(),
            password_hash,
        }
    }
}

/// Pool slot holding an idle SSH session and its last-used timestamp.
struct SlotEntry {
    session: SshSession,
    last_used: Instant,
}

/// Process-wide pool of authenticated SSH sessions keyed by `(host, port, user, password-hash)`.
///
/// Idle sessions are checked out on demand and returned on guard drop. Stale entries are
/// evicted lazily on every `take`/`put` based on a configurable idle TTL.
pub struct SshSessionPool {
    entries: Mutex<HashMap<SshSessionKey, SlotEntry>>,
    idle_ttl: Duration,
}

impl SshSessionPool {
    /// Create an empty pool with the default idle TTL.
    ///
    /// ### Returns
    /// - `Self`: Empty pool ready to accept sessions.
    pub fn new() -> Self {
        Self::with_idle_ttl(SSH_SESSION_IDLE_TTL)
    }

    /// Create an empty pool with a custom idle TTL (used by tests).
    ///
    /// ### Arguments
    /// - `idle_ttl`: Maximum time an idle session may remain in the pool.
    ///
    /// ### Returns
    /// - `Self`: Empty pool with the supplied TTL.
    pub fn with_idle_ttl(idle_ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            idle_ttl,
        }
    }

    /// Drop every cached session immediately.
    pub fn clear(&self) {
        self.entries.lock().clear();
    }

    /// Drop sessions for a `(host, port, user)` triple regardless of stored password digest.
    ///
    /// ### Arguments
    /// - `cred_key`: Credential identifier whose sessions should be discarded.
    pub fn invalidate_by_credential(&self, cred_key: &SshCredKey) {
        let mut entries = self.entries.lock();
        entries.retain(|key, _| {
            !(key.host == cred_key.host && key.port == cred_key.port && key.user == cred_key.user)
        });
    }

    /// Number of idle sessions currently held by the pool (used by tests).
    ///
    /// ### Returns
    /// - `usize`: Count of cached entries after stale eviction.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        let mut entries = self.entries.lock();
        Self::evict_stale(&mut entries, self.idle_ttl);
        entries.len()
    }

    /// Whether the pool currently holds no idle sessions (used by tests).
    ///
    /// ### Returns
    /// - `true`: No cached entries remain after stale eviction.
    /// - `false`: At least one session is cached.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Try to remove and return a session for the given key, evicting stale entries first.
    ///
    /// ### Arguments
    /// - `key`: Composite key identifying the session.
    ///
    /// ### Returns
    /// - `Some(SshSession)`: A fresh enough idle session was available.
    /// - `None`: No session is cached for this key, or the cached one had expired.
    fn take(&self, key: &SshSessionKey) -> Option<SshSession> {
        let mut entries = self.entries.lock();
        Self::evict_stale(&mut entries, self.idle_ttl);
        entries.remove(key).map(|slot| slot.session)
    }

    /// Insert a session into the pool, evicting the oldest entry if capacity is reached.
    ///
    /// ### Arguments
    /// - `key`: Composite key identifying the session.
    /// - `session`: Session to cache.
    fn put(&self, key: SshSessionKey, session: SshSession) {
        let mut entries = self.entries.lock();
        Self::evict_stale(&mut entries, self.idle_ttl);
        if entries.len() >= SSH_SESSION_POOL_CAPACITY
            && let Some(oldest_key) = entries
                .iter()
                .min_by_key(|(_, slot)| slot.last_used)
                .map(|(k, _)| k.clone())
        {
            entries.remove(&oldest_key);
        }
        entries.insert(
            key,
            SlotEntry {
                session,
                last_used: Instant::now(),
            },
        );
    }

    /// Drop entries whose last-used timestamp is older than `idle_ttl`.
    ///
    /// ### Arguments
    /// - `entries`: Locked pool map to mutate in place.
    /// - `idle_ttl`: Maximum allowed idle duration.
    fn evict_stale(entries: &mut HashMap<SshSessionKey, SlotEntry>, idle_ttl: Duration) {
        let now = Instant::now();
        entries.retain(|_, slot| now.saturating_duration_since(slot.last_used) < idle_ttl);
    }

    /// Reuse a cached session if one is available, otherwise establish a new one.
    /// ### Arguments
    /// - `spec`: Parsed remote specification supplying host and port.
    /// - `user`: Resolved username; must not be empty.
    /// - `password`: Session-scoped password used to compute the pool key and, on
    ///   cache miss, to authenticate.
    /// - `host_key_cb`: Called only on cache miss when the host key is unknown.
    ///
    /// ### Returns
    /// - `Ok(PooledSession)`: A guard owning the checked-out session. Drop returns
    ///   it to the pool; `invalidate()` discards it instead.
    /// - `Err(SshError)`: Any failure during connect, handshake, host-key check,
    ///   auth, or SFTP init when establishing a new session.
    pub fn checkout_or_connect(
        self: &Arc<Self>,
        spec: &RemoteSpec,
        user: &str,
        password: &Zeroizing<String>,
        host_key_cb: impl FnOnce(&str, &str, u16) -> HostKeyDecision,
    ) -> Result<PooledSession, SshError> {
        let key = SshSessionKey::new(&spec.host, spec.port, user, password);

        if let Some(session) = self.take(&key)
            && session.is_authenticated()
        {
            return Ok(PooledSession {
                inner: Some(SessionWithKey { key, session }),
                pool: Arc::clone(self),
            });
        }

        let session = connect(spec, user, password, host_key_cb)?;
        Ok(PooledSession {
            inner: Some(SessionWithKey { key, session }),
            pool: Arc::clone(self),
        })
    }
}

impl Default for SshSessionPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal pairing of a checked-out session with the key it should return to.
struct SessionWithKey {
    key: SshSessionKey,
    session: SshSession,
}

/// RAII guard owning an SSH session checked out from an `SshSessionPool`.
///
/// Drop returns the session to the pool. Call `invalidate()` after a transport
/// failure to discard the session instead of recycling a broken connection.
pub struct PooledSession {
    inner: Option<SessionWithKey>,
    pool: Arc<SshSessionPool>,
}

impl PooledSession {
    /// Borrow the underlying SSH session for SFTP operations.
    ///
    /// ### Returns
    /// - `&SshSession`: Reference to the live SSH session and its SFTP subsystem.
    pub fn session(&self) -> &SshSession {
        &self
            .inner
            .as_ref()
            .expect("pooled session already consumed")
            .session
    }

    /// Discard the session without returning it to the pool.
    ///
    /// Use this after any transport-level error so a broken session is never
    /// served again.
    pub fn invalidate(mut self) {
        self.inner = None;
    }
}

impl Drop for PooledSession {
    fn drop(&mut self) {
        if let Some(SessionWithKey { key, session }) = self.inner.take() {
            self.pool.put(key, session);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroize::Zeroizing;

    #[test]
    fn key_includes_password_digest() {
        let pwd_a = Zeroizing::new("hunter2".to_string());
        let pwd_b = Zeroizing::new("trustno1".to_string());
        let key_a = SshSessionKey::new("example.com", 22, "alice", &pwd_a);
        let key_b = SshSessionKey::new("example.com", 22, "alice", &pwd_b);
        assert_ne!(key_a, key_b);
        assert_ne!(key_a.password_hash, [0u8; 32]);
    }

    #[test]
    fn key_is_stable_for_same_password() {
        let pwd = Zeroizing::new("hunter2".to_string());
        let key_a = SshSessionKey::new("example.com", 22, "alice", &pwd);
        let key_b = SshSessionKey::new("example.com", 22, "alice", &pwd);
        assert_eq!(key_a, key_b);
    }

    #[test]
    fn invalidate_by_credential_drops_matching_entries() {
        let pool = Arc::new(SshSessionPool::new());
        let cred = SshCredKey::new("example.com", 22, "alice");
        // Without an actual session we cannot exercise put/take here; just confirm
        // the API is callable on an empty pool without panicking.
        pool.invalidate_by_credential(&cred);
        assert_eq!(pool.len(), 0);
    }
}
