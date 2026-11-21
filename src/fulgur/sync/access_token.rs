use crate::fulgur::settings::ServerProfile;
use crate::fulgur::sync::synchronization::{SynchronizationError, handle_ureq_error};
use crate::fulgur::utils::crypto_helper::load_device_api_key_from_keychain;
use fulgur_common::api::sync::AccessTokenResponse;
use parking_lot::{Condvar, Mutex};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Maximum time a thread waits for another thread's in-flight token refresh to complete before giving up.
const REFRESH_WAIT_TIMEOUT: Duration = Duration::from_secs(30);

/// Longest margin before expiry at which a token is proactively refreshed.
const MAX_REFRESH_BUFFER: Duration = Duration::from_mins(5);

/// JWT access token state for thread-safe token management
///
/// ### Fields
/// - `access_token`: The current JWT access token (None if not yet obtained)
/// - `token_refresh_at`: Local monotonic instant from which the token must be
///   refreshed, a safety buffer ahead of its expiry (None if no token)
/// - `is_refreshing_token`: Lock flag to prevent concurrent token refreshes
pub struct TokenState {
    pub access_token: Option<String>,
    pub token_refresh_at: Option<Instant>,
    pub is_refreshing_token: bool,
}

/// Where `acquire_token` got the token it returns
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TokenSource {
    /// Served from the cache, possibly revoked server-side since it was minted
    Cached,
    /// Minted by this call, so a rejection is not caused by a stale cache
    Minted,
}

/// Thread-safe token manager with proper wait/notify synchronization
///
/// Bundles the token state with a condition variable to efficiently coordinate
/// concurrent token refresh requests. When multiple threads need a token simultaneously,
/// only one performs the refresh while others wait efficiently (no busy-wait polling).
///
/// ### Fields
/// - `state`: The mutex-protected token state
/// - `refresh_notify`: Condition variable to signal when token refresh completes
pub struct TokenStateManager {
    state: Mutex<TokenState>,
    refresh_notify: Condvar,
}

impl TokenStateManager {
    /// Create a new token state manager with empty state
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(TokenState::new()),
            refresh_notify: Condvar::new(),
        }
    }

    /// Clear the cached token, forcing a refresh on next access
    ///
    /// This is useful when:
    /// - The device API key has changed
    /// - You want to force re-authentication
    /// - Switching to a different server/account
    pub fn clear_token(&self) {
        let mut state = self.state.lock();
        state.access_token = None;
        state.token_refresh_at = None;
        log::debug!("Cleared cached access token");
    }

    /// Drop the cached token if it is the one the server just rejected
    ///
    /// ### Arguments
    /// - `rejected_token`: The token the server answered 401 or 403 to
    pub fn invalidate_token(&self, rejected_token: &str) {
        let mut state = self.state.lock();
        if state.access_token.as_deref() == Some(rejected_token) {
            state.access_token = None;
            state.token_refresh_at = None;
            log::debug!("Invalidated access token rejected by the server");
        }
    }

    /// Return the cached token if it has not reached its refresh deadline
    ///
    /// ### Arguments
    /// - `state`: The locked token state
    ///
    /// ### Returns
    /// - `Some(String)`: The cached token, still valid
    /// - `None`: No token is cached, or it is due for a refresh
    fn cached_token(state: &TokenState) -> Option<String> {
        match (&state.access_token, state.token_refresh_at) {
            (Some(token), Some(refresh_at)) if is_token_valid(refresh_at, Instant::now()) => {
                Some(token.clone())
            }
            _ => None,
        }
    }
}

impl Default for TokenStateManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a new empty `TokenState`
///
/// ### Returns
/// - `TokenState`: A new `TokenState` with all fields initialized to default/empty values
impl Default for TokenState {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenState {
    /// Create a new empty `TokenState`
    #[must_use]
    pub fn new() -> Self {
        Self {
            access_token: None,
            token_refresh_at: None,
            is_refreshing_token: false,
        }
    }
}

/// Request a JWT access token from the server using the device key
///
/// ### Arguments
/// - `profile`: The server profile to authenticate against (URL, email, id used to look up the device key)
/// - `http_agent`: Shared HTTP agent for connection pooling
///
/// ### Returns
/// - `Ok(AccessTokenResponse)`: The JWT access token and expiration info
/// - `Err(SynchronizationError)`: If the token request failed
fn request_access_token(
    profile: &ServerProfile,
    http_agent: &ureq::Agent,
) -> Result<AccessTokenResponse, SynchronizationError> {
    let Some(server_url) = profile.server_url.clone() else {
        return Err(SynchronizationError::ServerUrlMissing);
    };
    let Some(email) = profile.email.clone() else {
        return Err(SynchronizationError::EmailMissing);
    };
    let Some(device_api_key) = (match load_device_api_key_from_keychain(&profile.id) {
        Ok(value) => value,
        Err(_) => return Err(SynchronizationError::DeviceKeyMissing),
    }) else {
        return Err(SynchronizationError::DeviceKeyMissing);
    };
    let token_url = format!("{server_url}/api/token");
    log::debug!("Requesting JWT access token from server");
    let mut response = http_agent
        .post(&token_url)
        .header("Authorization", &format!("Bearer {device_api_key}"))
        .header("X-User-Email", email)
        .send("")
        .map_err(|e| handle_ureq_error(e, "Failed to obtain access token"))?;
    let body = match response
        .body_mut()
        .with_config()
        .limit(crate::fulgur::sync::synchronization::MAX_HTTP_SMALL_RESPONSE_BYTES)
        .read_to_string()
    {
        Ok(body) => body,
        Err(e) => {
            log::error!("Failed to read access token response body: {e}");
            return Err(SynchronizationError::Other(e.to_string()));
        }
    };
    let token_response: AccessTokenResponse = match serde_json::from_str(&body) {
        Ok(response) => response,
        Err(e) => {
            log::error!("Failed to parse access token response: {e}");
            return Err(SynchronizationError::Other(e.to_string()));
        }
    };
    log::info!(
        "Access token obtained successfully (expires in {} seconds)",
        token_response.expires_in
    );
    Ok(token_response)
}

/// Compute when a freshly minted token must be refreshed
///
/// ### Arguments
/// - `minted_at`: Local instant at which the token response was received
/// - `lifetime`: Token lifetime reported by the server (`expires_in`)
///
/// ### Returns
/// - `Instant`: The local instant from which the token is considered stale
fn refresh_deadline(minted_at: Instant, lifetime: Duration) -> Instant {
    let buffer = (lifetime / 4).min(MAX_REFRESH_BUFFER);
    minted_at + lifetime.saturating_sub(buffer)
}

/// Check if the access token has not yet reached its refresh deadline
///
/// ### Arguments
/// - `refresh_at`: The token refresh deadline, from `refresh_deadline`
/// - `now`: The current local instant
///
/// ### Returns
/// - `true` if the token can still be used
/// - `false` if the token must be refreshed
fn is_token_valid(refresh_at: Instant, now: Instant) -> bool {
    now < refresh_at
}

/// Get a valid JWT access token, refreshing if necessary
///
/// Uses proper condition variable synchronization to avoid busy-waiting.
/// When multiple threads need a token simultaneously, only one performs
/// the refresh while others wait efficiently on the condition variable.
///
/// ### Arguments
/// - `profile`: The server profile to authenticate against
/// - `token_manager`: Arc to the token state manager (thread-safe)
/// - `http_agent`: Shared HTTP agent for connection pooling
///
/// ### Errors
/// Returns a `SynchronizationError` if the token refresh request fails, the
/// server rejects the credentials, or the response cannot be parsed.
///
/// ### Returns
/// - `Ok(String)`: A valid JWT access token
/// - `Err(SynchronizationError)`: If token refresh failed
pub fn get_valid_token(
    profile: &ServerProfile,
    token_manager: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
) -> Result<String, SynchronizationError> {
    acquire_token(profile, token_manager, http_agent).map(|(token, _)| token)
}

/// Run an authenticated request, retrying once if a cached token is rejected
///
/// ### Arguments
/// - `profile`: The server profile to authenticate against
/// - `token_manager`: Arc to the token state manager (thread-safe)
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `request`: The request to run, given the bearer token
///
/// ### Errors
/// - Returns a `SynchronizationError` if no token can be obtained, or the error
///   of the last `request` attempt.
///
/// ### Returns
/// - `Ok(T)`: The result of a successful `request`
/// - `Err(SynchronizationError)`: The token could not be obtained, or `request` failed
pub fn with_valid_token<T>(
    profile: &ServerProfile,
    token_manager: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
    mut request: impl FnMut(&str) -> Result<T, SynchronizationError>,
) -> Result<T, SynchronizationError> {
    let (token, source) = acquire_token(profile, token_manager, http_agent)?;
    match request(&token) {
        Err(SynchronizationError::AuthenticationFailed) if source == TokenSource::Cached => {
            log::warn!("Server rejected the cached access token, retrying with a new one");
            token_manager.invalidate_token(&token);
            let (token, _) = acquire_token(profile, token_manager, http_agent)?;
            request(&token)
        }
        result => result,
    }
}

/// Get a valid JWT access token and tell whether it came from the cache
///
/// ### Arguments
/// - `profile`: The server profile to authenticate against
/// - `token_manager`: Arc to the token state manager (thread-safe)
/// - `http_agent`: Shared HTTP agent for connection pooling
///
/// ### Errors
/// - Returns a `SynchronizationError` if the token refresh request fails, the
///   server rejects the credentials, or another thread's refresh times out.
///
/// ### Returns
/// - `Ok((String, TokenSource))`: A valid token and where it came from
/// - `Err(SynchronizationError)`: If token refresh failed
fn acquire_token(
    profile: &ServerProfile,
    token_manager: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
) -> Result<(String, TokenSource), SynchronizationError> {
    let mut state = token_manager.state.lock();
    if let Some(token) = TokenStateManager::cached_token(&state) {
        return Ok((token, TokenSource::Cached));
    }

    // If another thread is already refreshing, wait for it to complete
    while state.is_refreshing_token {
        let wait_result = token_manager
            .refresh_notify
            .wait_for(&mut state, REFRESH_WAIT_TIMEOUT);

        // The refresher may have completed (and notified) before or at the moment we woke up => return the fresh token
        if let Some(token) = TokenStateManager::cached_token(&state) {
            return Ok((token, TokenSource::Minted));
        }

        if wait_result.timed_out() && state.is_refreshing_token {
            log::warn!("Timed out waiting for a token refresh to complete");
            return Err(SynchronizationError::Timeout(
                "Timed out waiting for token refresh".to_string(),
            ));
        }
    }

    // We're the thread responsible for refreshing the token
    state.is_refreshing_token = true;

    // Release lock while performing the network request
    drop(state);

    log::debug!("Access token expired or missing, requesting new token");
    let token_response = request_access_token(profile, http_agent);
    let mut state = token_manager.state.lock();
    state.is_refreshing_token = false;
    let token_response = match token_response {
        Ok(response) => response,
        Err(e) => {
            token_manager.refresh_notify.notify_all();
            return Err(e);
        }
    };
    let lifetime = Duration::from_secs(u64::try_from(token_response.expires_in).unwrap_or(0));
    state.access_token = Some(token_response.access_token.clone());
    state.token_refresh_at = Some(refresh_deadline(Instant::now(), lifetime));
    token_manager.refresh_notify.notify_all();

    log::debug!("Access token refreshed successfully");
    Ok((token_response.access_token, TokenSource::Minted))
}

impl TokenStateManager {
    /// Inject a pre-built token directly into the manager
    ///
    /// Bypasses network calls so tests can verify the fast path of `get_valid_token`
    /// and the effect of `clear_token` on the cached state.
    ///
    /// This method is intended for use in tests only.
    ///
    /// ### Arguments
    /// - `token`: The token to cache
    /// - `lifetime`: The lifetime the server would report (`expires_in`),
    ///   counted from now and subject to the usual refresh buffer
    pub fn inject_token_for_test(&self, token: String, lifetime: Duration) {
        let mut state = self.state.lock();
        state.access_token = Some(token);
        state.token_refresh_at = Some(refresh_deadline(Instant::now(), lifetime));
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_REFRESH_BUFFER, is_token_valid, refresh_deadline};
    use std::time::{Duration, Instant};

    const ONE_HOUR: Duration = Duration::from_hours(1);

    #[test]
    fn a_long_lived_token_is_refreshed_five_minutes_before_expiry() {
        let minted_at = Instant::now();
        let deadline = refresh_deadline(minted_at, ONE_HOUR);
        assert_eq!(deadline + MAX_REFRESH_BUFFER, minted_at + ONE_HOUR);
    }

    #[test]
    fn a_short_lived_token_keeps_three_quarters_of_its_lifetime() {
        let minted_at = Instant::now();
        let lifetime = Duration::from_secs(60);
        let deadline = refresh_deadline(minted_at, lifetime);
        assert_eq!(deadline, minted_at + Duration::from_secs(45));
        assert!(is_token_valid(
            deadline,
            minted_at + Duration::from_secs(44)
        ));
        assert!(!is_token_valid(
            deadline,
            minted_at + Duration::from_secs(45)
        ));
    }

    #[test]
    fn a_token_at_the_five_minute_boundary_is_still_reused() {
        let minted_at = Instant::now();
        let lifetime = MAX_REFRESH_BUFFER;
        let deadline = refresh_deadline(minted_at, lifetime);
        assert!(
            is_token_valid(deadline, minted_at),
            "A 5-minute token must not be stale as soon as it is minted"
        );
    }

    #[test]
    fn a_token_with_no_lifetime_is_immediately_stale() {
        let minted_at = Instant::now();
        let deadline = refresh_deadline(minted_at, Duration::ZERO);
        assert!(!is_token_valid(deadline, minted_at));
    }

    #[test]
    fn a_token_is_valid_until_its_deadline_and_stale_from_it() {
        let minted_at = Instant::now();
        let deadline = refresh_deadline(minted_at, ONE_HOUR);
        assert!(is_token_valid(deadline, minted_at));
        assert!(is_token_valid(
            deadline,
            minted_at + Duration::from_secs(55 * 60 - 1)
        ));
        assert!(!is_token_valid(deadline, deadline));
        assert!(!is_token_valid(deadline, deadline + Duration::from_secs(1)));
    }
}
