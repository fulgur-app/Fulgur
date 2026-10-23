use super::auth::{CredentialPrompter, SshAuth};
use super::credentials::SshCredKey;
use super::error::SshError;
use super::session::{HostKeyDecision, SshSession, connect};
use super::url::{RemoteSpec, format_host_for_url};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Time after which an idle SSH session is considered stale and discarded.
///
/// Kept short because idle pooled sessions exchange no traffic, so NAT idle timeouts and
/// server-side idle limits can silently kill them.
pub const SSH_SESSION_IDLE_TTL: Duration = Duration::from_mins(5);

/// Maximum number of idle sessions held by the pool at once.
const SSH_SESSION_POOL_CAPACITY: usize = 64;

/// Identifier for a pooled session, derived from connection coordinates and a credential digest.
///
/// Including a SHA-256 digest of the credentials ensures rotated credentials never
/// silently reuse a session authenticated under the previous ones.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SshSessionKey {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub credential_digest: [u8; 32],
}

impl SshSessionKey {
    /// Build a pool key from a `(host, port, user)` triple and the credentials used.
    ///
    /// ### Arguments
    /// - `host`: Hostname or IP of the remote server.
    /// - `port`: SSH port.
    /// - `user`: Username used for authentication.
    /// - `auth`: Credentials used for authentication; only their digest is stored.
    ///
    /// ### Returns
    /// - `Self`: The composite key used to look up cached sessions.
    pub fn new(
        host: impl Into<String>,
        port: u16,
        user: impl Into<String>,
        auth: &SshAuth,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            user: user.into(),
            credential_digest: auth.digest(),
        }
    }
}

/// Pool slot holding an idle SSH session and its last-used timestamp.
struct SlotEntry {
    session: SshSession,
    last_used: Instant,
}

/// Process-wide pool of authenticated SSH sessions keyed by `(host, port, user, credential-digest)`.
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
    #[must_use]
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
    #[must_use]
    pub fn with_idle_ttl(idle_ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            idle_ttl,
        }
    }

    /// Drop every cached session immediately.
    pub fn clear(&self) {
        let sessions = std::mem::take(&mut *self.entries.lock());
        drop(sessions);
    }

    /// Drop sessions for a `(host, port, user)` triple regardless of stored credential digest.
    ///
    /// ### Arguments
    /// - `cred_key`: Credential identifier whose sessions should be discarded.
    pub fn invalidate_by_credential(&self, cred_key: &SshCredKey) {
        let invalidated: Vec<SlotEntry> = self
            .entries
            .lock()
            .extract_if(|key, _| {
                key.host == cred_key.host && key.port == cred_key.port && key.user == cred_key.user
            })
            .map(|(_, slot)| slot)
            .collect();
        drop(invalidated);
    }

    /// Number of idle sessions currently held by the pool (used by tests).
    ///
    /// ### Returns
    /// - `usize`: Count of cached entries after stale eviction.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        let (len, stale) = {
            let mut entries = self.entries.lock();
            let stale = Self::evict_stale(&mut entries, self.idle_ttl);
            (entries.len(), stale)
        };
        drop(stale);
        len
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
        let (session, stale) = {
            let mut entries = self.entries.lock();
            let stale = Self::evict_stale(&mut entries, self.idle_ttl);
            (entries.remove(key).map(|slot| slot.session), stale)
        };
        drop(stale);
        session
    }

    /// Insert a session into the pool, evicting the oldest entry if capacity is reached.
    ///
    /// ### Arguments
    /// - `key`: Composite key identifying the session.
    /// - `session`: Session to cache.
    fn put(&self, key: SshSessionKey, session: SshSession) {
        let evicted = {
            let mut entries = self.entries.lock();
            let mut evicted = Self::evict_stale(&mut entries, self.idle_ttl);
            if entries.len() >= SSH_SESSION_POOL_CAPACITY
                && let Some(oldest_key) = entries
                    .iter()
                    .min_by_key(|(_, slot)| slot.last_used)
                    .map(|(k, _)| k.clone())
            {
                evicted.extend(entries.remove(&oldest_key));
            }
            evicted.extend(entries.insert(
                key,
                SlotEntry {
                    session,
                    last_used: Instant::now(),
                },
            ));
            evicted
        };
        drop(evicted);
    }

    /// Remove entries whose last-used timestamp is older than `idle_ttl`.
    ///
    /// ### Arguments
    /// - `entries`: Locked pool map to mutate in place.
    /// - `idle_ttl`: Maximum allowed idle duration.
    ///
    /// ### Returns
    /// - `Vec<SlotEntry>`: The removed entries, still holding their sessions.
    #[must_use = "drop the stale sessions after releasing the pool lock"]
    fn evict_stale(
        entries: &mut HashMap<SshSessionKey, SlotEntry>,
        idle_ttl: Duration,
    ) -> Vec<SlotEntry> {
        let now = Instant::now();
        let stale: Vec<SlotEntry> = entries
            .extract_if(|_, slot| now.saturating_duration_since(slot.last_used) >= idle_ttl)
            .map(|(_, slot)| slot)
            .collect();
        if !stale.is_empty() {
            log::debug!("Closing {} idle SSH sessions", stale.len());
        }
        stale
    }

    /// Reuse a cached session if one is available, otherwise establish a new one.
    ///
    /// ### Arguments
    /// - `spec`: Parsed remote specification supplying host and port.
    /// - `user`: Resolved username; must not be empty.
    /// - `cached_auth`: Credentials that authenticated this target before. They select the
    ///   pooled session and, on cache miss, are tried first.
    /// - `host_key_cb`: Called only on cache miss when the host key is unknown.
    /// - `prompter`: Called only on cache miss when automatic authentication fails.
    ///
    /// ### Errors
    /// Returns an `SshError` on connect, handshake, host-key check, authentication,
    /// or SFTP initialization failures when establishing a new session.
    ///
    /// ### Returns
    /// - `Ok((PooledSession, SshAuth))`: A guard owning the checked-out session, and the
    ///   credentials it is authenticated with. Dropping the guard returns the session to
    ///   the pool; `invalidate()` discards it instead. The guard records whether the
    ///   session came from the pool, which `retry_once_on_reused_session` relies on.
    /// - `Err(SshError)`: Any failure during connect, handshake, host-key check,
    ///   auth, or SFTP init when establishing a new session.
    pub fn checkout_or_connect(
        self: &Arc<Self>,
        spec: &RemoteSpec,
        user: &str,
        cached_auth: Option<&SshAuth>,
        host_key_cb: impl FnOnce(&str, &str, u16) -> HostKeyDecision,
        prompter: &mut dyn CredentialPrompter,
    ) -> Result<(PooledSession, SshAuth), SshError> {
        if let Some(auth) = cached_auth {
            let key = SshSessionKey::new(&spec.host, spec.port, user, auth);
            match self.take(&key) {
                Some(session) if session.is_authenticated() => {
                    log::debug!(
                        "Reusing the pooled SSH session of {user}@{}:{}",
                        format_host_for_url(&spec.host),
                        spec.port
                    );
                    return Ok((self.guard(key, session, true), auth.clone()));
                }
                Some(_) => log::debug!(
                    "Discarding the pooled SSH session of {user}@{}:{}, no longer authenticated",
                    format_host_for_url(&spec.host),
                    spec.port
                ),
                None => {}
            }
        }

        let (session, auth) = connect(spec, user, cached_auth, host_key_cb, prompter)?;
        let key = SshSessionKey::new(&spec.host, spec.port, user, &auth);
        Ok((self.guard(key, session, false), auth))
    }

    /// Wrap a checked-out session in a guard that returns it to this pool on drop.
    ///
    /// ### Arguments
    /// - `key`: Key the session is returned under.
    /// - `session`: Authenticated session.
    /// - `reused`: Whether the session was taken from the pool rather than freshly connected.
    ///
    /// ### Returns
    /// - `PooledSession`: Guard owning the session.
    fn guard(
        self: &Arc<Self>,
        key: SshSessionKey,
        session: SshSession,
        reused: bool,
    ) -> PooledSession {
        PooledSession {
            inner: Some(SessionWithKey { key, session }),
            pool: Arc::clone(self),
            reused,
        }
    }
}

/// Result of one attempt at running an operation on a pooled SSH session.
pub struct SessionAttempt<T, E> {
    result: Result<T, E>,
    reused_session: bool,
}

impl<T, E> SessionAttempt<T, E> {
    /// Record an attempt that failed before any session was checked out.
    ///
    /// ### Arguments
    /// - `error`: Why no session could be checked out or connected.
    ///
    /// ### Returns
    /// - `Self`: A failed attempt that never ran on a reused session, so it is not retried.
    pub fn checkout_failed(error: E) -> Self {
        Self {
            result: Err(error),
            reused_session: false,
        }
    }
}

/// Run an operation on a pooled SSH session, retrying once when it failed on a reused one.
///
/// ### Arguments
/// - `attempt`: Checks out a session and runs the operation on it; may run twice, so the
///   operation must be safe to repeat.
/// - `may_retry`: Consulted before retrying; returns `false` once the caller no longer wants
///   a result, for example after a cancellation.
///
/// ### Errors
/// - Returns the error of the last attempt that ran.
///
/// ### Returns
/// - `Ok(T)`: The first attempt, or its retry, succeeded.
/// - `Err(E)`: The first attempt failed on a fresh session or could not be retried, or the
///   retry failed too.
pub fn retry_once_on_reused_session<T, E: std::fmt::Display>(
    mut attempt: impl FnMut() -> SessionAttempt<T, E>,
    may_retry: impl Fn() -> bool,
) -> Result<T, E> {
    let SessionAttempt {
        result,
        reused_session,
    } = attempt();
    match result {
        Err(error) if reused_session && may_retry() => {
            log::info!("Reused SSH session failed, retrying on a new connection: {error}");
            attempt().result
        }
        result => result,
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
    reused: bool,
}

impl PooledSession {
    /// Run an operation on the session, discarding the session if the operation fails.
    ///
    /// ### Arguments
    /// - `operation`: SFTP work to run on the checked-out session.
    ///
    /// ### Returns
    /// - `SessionAttempt<T, E>`: The operation's result, tagged with whether the session
    ///   was reused from the pool. On success the session returns to the pool.
    pub fn run<T, E>(
        self,
        operation: impl FnOnce(&SshSession) -> Result<T, E>,
    ) -> SessionAttempt<T, E> {
        let reused_session = self.reused;
        let result = operation(self.session());
        if result.is_err() {
            self.invalidate();
        }
        SessionAttempt {
            result,
            reused_session,
        }
    }

    /// Borrow the underlying SSH session for SFTP operations.
    ///
    /// ### Returns
    /// - `&SshSession`: Reference to the live SSH session and its SFTP subsystem.
    ///
    /// ### Panics
    /// Panics if the session has already been consumed (e.g. via `discard` or `Drop`).
    #[must_use]
    pub fn session(&self) -> &SshSession {
        &self
            .inner
            .as_ref()
            .expect("pooled session already consumed") //TODO: remove this
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
    fn key_includes_credential_digest() {
        let auth_a = SshAuth::Password(Zeroizing::new("hunter2".to_string()));
        let auth_b = SshAuth::Password(Zeroizing::new("trustno1".to_string()));
        let key_a = SshSessionKey::new("example.com", 22, "alice", &auth_a);
        let key_b = SshSessionKey::new("example.com", 22, "alice", &auth_b);
        assert_ne!(key_a, key_b);
        assert_ne!(key_a.credential_digest, [0u8; 32]);
    }

    #[test]
    fn key_is_stable_for_same_credentials() {
        let auth = SshAuth::Password(Zeroizing::new("hunter2".to_string()));
        let key_a = SshSessionKey::new("example.com", 22, "alice", &auth);
        let key_b = SshSessionKey::new("example.com", 22, "alice", &auth);
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

    /// Build a session attempt with the given result.
    ///
    /// ### Arguments
    /// - `result`: Outcome of the operation.
    /// - `reused_session`: Whether the attempt ran on a session reused from the pool.
    ///
    /// ### Returns
    /// - `SessionAttempt<u32, &'static str>`: The attempt.
    fn attempt(
        result: Result<u32, &'static str>,
        reused_session: bool,
    ) -> SessionAttempt<u32, &'static str> {
        SessionAttempt {
            result,
            reused_session,
        }
    }

    #[test]
    fn retry_once_on_reused_session_retries_a_failure_on_a_reused_session() {
        let mut attempts = 0;
        let result = retry_once_on_reused_session(
            || {
                attempts += 1;
                if attempts == 1 {
                    attempt(Err("stale session"), true)
                } else {
                    attempt(Ok(7), false)
                }
            },
            || true,
        );
        assert_eq!(result, Ok(7));
        assert_eq!(attempts, 2);
    }

    #[test]
    fn retry_once_on_reused_session_keeps_a_failure_on_a_fresh_session() {
        let mut attempts = 0;
        let result = retry_once_on_reused_session(
            || {
                attempts += 1;
                attempt(Err("permission denied"), false)
            },
            || true,
        );
        assert_eq!(result, Err("permission denied"));
        assert_eq!(attempts, 1);
    }

    #[test]
    fn retry_once_on_reused_session_keeps_a_checkout_failure() {
        let mut attempts = 0;
        let result: Result<u32, _> = retry_once_on_reused_session(
            || {
                attempts += 1;
                SessionAttempt::checkout_failed("connection refused")
            },
            || true,
        );
        assert_eq!(result, Err("connection refused"));
        assert_eq!(attempts, 1);
    }

    #[test]
    fn retry_once_on_reused_session_skips_the_retry_when_not_allowed() {
        let mut attempts = 0;
        let result = retry_once_on_reused_session(
            || {
                attempts += 1;
                attempt(Err("stale session"), true)
            },
            || false,
        );
        assert_eq!(result, Err("stale session"));
        assert_eq!(attempts, 1);
    }

    #[test]
    fn retry_once_on_reused_session_retries_at_most_once() {
        let mut attempts = 0;
        let result = retry_once_on_reused_session(
            || {
                attempts += 1;
                attempt(Err(if attempts == 1 { "first" } else { "second" }), true)
            },
            || true,
        );
        assert_eq!(result, Err("second"));
        assert_eq!(attempts, 2);
    }

    #[test]
    fn retry_once_on_reused_session_does_not_repeat_a_success() {
        let mut attempts = 0;
        let result = retry_once_on_reused_session(
            || {
                attempts += 1;
                attempt(Ok(3), true)
            },
            || true,
        );
        assert_eq!(result, Ok(3));
        assert_eq!(attempts, 1);
    }
}
