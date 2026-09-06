use super::error::{SynchronizationError, handle_ureq_error};
use crate::fulgur::settings::ServerProfile;
use crate::fulgur::shared_state::AppNotification;
use crate::fulgur::sync::access_token::get_valid_token;
use crate::fulgur::ui::notifications::progress::spawn_with_progress;
use fulgur_common::api::sync::PingResponse;
use gpui::{App, Window};
use gpui_component::notification::NotificationType;
use std::sync::Arc;

/// Ping an authenticated Fulgurant server endpoint to test connectivity and credentials.
///
/// ### Arguments
/// - `server_url`: The base URL of the server (e.g. `https://example.com`).
/// - `token`: A valid JWT Bearer token.
/// - `http_agent`: Shared HTTP agent.
///
/// ### Errors
/// Returns a `SynchronizationError` if the server is unreachable, the token is
/// rejected, or the response cannot be parsed as the expected `ok: true` body.
///
/// ### Returns
/// - `Ok(())`: Server responded with `ok: true`.
/// - `Err(SynchronizationError)`: Server is unreachable, auth failed, or returned an unexpected response.
pub fn ping_server(
    server_url: &str,
    token: &str,
    http_agent: &ureq::Agent,
) -> Result<(), SynchronizationError> {
    let ping_url = format!("{server_url}/api/ping");
    let mut response = http_agent
        .get(&ping_url)
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| handle_ureq_error(e, "Ping failed"))?;
    let body = response
        .body_mut()
        .with_config()
        .limit(super::limits::MAX_HTTP_SMALL_RESPONSE_BYTES)
        .read_to_string()
        .map_err(|e| SynchronizationError::Other(e.to_string()))?;
    let ping_response: PingResponse = serde_json::from_str(&body)
        .map_err(|e| SynchronizationError::InvalidResponse(e.to_string()))?;
    if ping_response.ok {
        Ok(())
    } else {
        Err(SynchronizationError::Other(
            "Server returned ok: false".to_string(),
        ))
    }
}

/// Ping a Fulgurant server with a progress indicator and a result notification.
///
/// ### Arguments
/// - `profile`: The server profile to authenticate and ping.
/// - `display_name`: Human-readable label shown in the progress/result notifications.
/// - `window`: The window to attach the progress indicator to.
/// - `cx`: The application context.
pub fn perform_ping_with_progress(
    profile: ServerProfile,
    display_name: String,
    window: &mut Window,
    cx: &mut App,
) {
    let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
    let sync_state = shared.sync_state_for(&profile.id);
    let notification_tx = sync_state.notification_tx.clone();
    let token_state = Arc::clone(&sync_state.token_state);
    let http_agent = Arc::clone(&shared.http_agent);

    spawn_with_progress(
        window,
        cx,
        format!("Testing connection to {display_name}...").into(),
        None,
        move |_cancel_flag| {
            let result =
                get_valid_token(&profile, &token_state, &http_agent).and_then(
                    |token| match profile.server_url.as_deref() {
                        Some(url) => ping_server(url, &token, &http_agent),
                        None => Err(SynchronizationError::ServerUrlMissing),
                    },
                );
            let notification = match result {
                Ok(()) => AppNotification::background(
                    NotificationType::Success,
                    format!("{display_name}: Server is reachable"),
                ),
                Err(e) => AppNotification::background(
                    NotificationType::Error,
                    format!("{display_name}: Ping failed: {e}"),
                ),
            };
            let _ = notification_tx.unbounded_send(notification);
        },
    );
}
