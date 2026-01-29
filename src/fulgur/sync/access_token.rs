use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::fulgur::settings::SynchronizationSettings;
use crate::fulgur::sync::sync::SynchronizationError;
use crate::fulgur::utils::crypto_helper::load_device_api_key_from_keychain;
use fulgur_common::api::sync::AccessTokenResponse;
use parking_lot::Mutex;
use time::OffsetDateTime;

/// JWT access token state for thread-safe token management
///
/// ### Fields
/// - `access_token`: The current JWT access token (None if not yet obtained)
/// - `token_expires_at`: When the current token expires (None if no token)
/// - `is_refreshing_token`: Lock flag to prevent concurrent token refreshes
pub struct TokenState {
    pub access_token: Option<String>,
    pub token_expires_at: Option<OffsetDateTime>,
    pub is_refreshing_token: bool,
}

impl TokenState {
    /// Create a new empty TokenState
    pub fn new() -> Self {
        Self {
            access_token: None,
            token_expires_at: None,
            is_refreshing_token: false,
        }
    }
}

/// Request a JWT access token from the server using the device key
///
/// ### Arguments
/// - `synchronization_settings`: The synchronization settings containing device key
///
/// ### Returns
/// - `Ok(AccessTokenResponse)`: The JWT access token and expiration info
/// - `Err(SynchronizationError)`: If the token request failed
fn request_access_token(
    synchronization_settings: &SynchronizationSettings,
) -> Result<AccessTokenResponse, SynchronizationError> {
    let server_url = synchronization_settings.server_url.clone();
    let email = synchronization_settings.email.clone();
    let device_api_key = match load_device_api_key_from_keychain() {
        Ok(value) => value,
        Err(_) => return Err(SynchronizationError::DeviceKeyMissing),
    };
    if server_url.is_none() {
        return Err(SynchronizationError::ServerUrlMissing);
    }
    if email.is_none() {
        return Err(SynchronizationError::EmailMissing);
    }
    if device_api_key.is_none() {
        return Err(SynchronizationError::DeviceKeyMissing);
    }
    let token_url = format!("{}/api/token", server_url.unwrap());
    log::debug!("Requesting JWT access token from server");
    let mut response = match ureq::post(&token_url)
        .header("Authorization", &format!("Bearer {}", device_api_key.unwrap()))
        .header("X-User-Email", email.unwrap())
        .send("")
    {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(code)) => {
            log::error!("Failed to obtain access token: HTTP status {}", code);
            if code == 401 || code == 403 {
                return Err(SynchronizationError::AuthenticationFailed);
            } else {
                return Err(SynchronizationError::ServerError(code));
            }
        }
        Err(ureq::Error::Io(io_error)) => {
            log::error!("Failed to obtain access token (IO): {}", io_error);
            return match io_error.kind() {
                std::io::ErrorKind::ConnectionRefused => {
                    Err(SynchronizationError::ConnectionFailed)
                }
                std::io::ErrorKind::TimedOut => Err(SynchronizationError::ConnectionFailed),
                _ => Err(SynchronizationError::Other(io_error.to_string())),
            };
        }
        Err(ureq::Error::ConnectionFailed) => {
            log::error!("Failed to obtain access token: Connection failed");
            return Err(SynchronizationError::ConnectionFailed);
        }
        Err(ureq::Error::HostNotFound) => {
            log::error!("Failed to obtain access token: Host not found");
            return Err(SynchronizationError::HostNotFound);
        }
        Err(ureq::Error::Timeout(timeout)) => {
            log::error!("Failed to obtain access token: Timeout ({})", timeout);
            return Err(SynchronizationError::ConnectionFailed);
        }
        Err(e) => {
            log::error!("Failed to obtain access token: {}", e);
            return Err(SynchronizationError::Other(e.to_string()));
        }
    };
    let body = match response.body_mut().read_to_string() {
        Ok(body) => body,
        Err(e) => {
            log::error!("Failed to read access token response body: {}", e);
            return Err(SynchronizationError::Other(e.to_string()));
        }
    };
    let token_response: AccessTokenResponse = match serde_json::from_str(&body) {
        Ok(response) => response,
        Err(e) => {
            log::error!("Failed to parse access token response: {}", e);
            return Err(SynchronizationError::Other(e.to_string()));
        }
    };
    log::info!(
        "Access token obtained successfully (expires in {} seconds)",
        token_response.expires_in
    );
    Ok(token_response)
}

/// Check if the access token is still valid (with 5-minute buffer for proactive refresh)
///
/// ### Arguments
/// - `expires_at`: The token expiration time
///
/// ### Returns
/// - `true` if the token is still valid (has >5 minutes remaining)
/// - `false` if the token is expired or will expire in <5 minutes
fn is_token_valid(expires_at: &OffsetDateTime) -> bool {
    let now = OffsetDateTime::now_utc();
    let buffer = time::Duration::minutes(5);
    *expires_at > now + buffer
}

/// Get a valid JWT access token, refreshing if necessary
///
/// ### Arguments
/// - `synchronization_settings`: The synchronization settings
/// - `token_state`: Arc to the token state (thread-safe)
///
/// ### Returns
/// - `Ok(String)`: A valid JWT access token
/// - `Err(SynchronizationError)`: If token refresh failed
pub fn get_valid_token(
    synchronization_settings: &SynchronizationSettings,
    token_state: Arc<Mutex<TokenState>>,
) -> Result<String, SynchronizationError> {
    {
        let state = token_state.lock();
        if let (Some(token_str), Some(exp_time)) = (&state.access_token, &state.token_expires_at) {
            if is_token_valid(exp_time) {
                return Ok(token_str.clone());
            }
        }
    }
    {
        let mut state = token_state.lock();
        if let (Some(token_str), Some(exp_time)) = (&state.access_token, &state.token_expires_at) {
            if is_token_valid(exp_time) && !state.is_refreshing_token {
                return Ok(token_str.clone());
            }
        }
        if state.is_refreshing_token {
            drop(state);
            thread::sleep(Duration::from_millis(100));
            let state = token_state.lock();
            if let (Some(token_str), Some(exp_time)) =
                (&state.access_token, &state.token_expires_at)
            {
                if is_token_valid(exp_time) {
                    return Ok(token_str.clone());
                }
            }
        } else {
            state.is_refreshing_token = true;
        }
    }
    log::debug!("Access token expired or missing, requesting new token");
    let token_response = request_access_token(synchronization_settings)?;
    let expires_at = OffsetDateTime::parse(
        &token_response.expires_at,
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|e| {
        log::error!("Failed to parse token expiration time: {}", e);
        SynchronizationError::Other(e.to_string())
    })?;
    let mut state = token_state.lock();
    state.access_token = Some(token_response.access_token.clone());
    state.token_expires_at = Some(expires_at);
    state.is_refreshing_token = false;
    log::debug!("Access token refreshed successfully");
    Ok(token_response.access_token)
}
