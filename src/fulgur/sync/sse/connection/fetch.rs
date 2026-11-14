use crate::fulgur::{
    settings::ServerProfile,
    sync::{
        access_token::TokenStateManager,
        share::{
            ClaimedFetch, fetch_share_by_id, fetch_unclaimed_share, rejected_share_notification,
        },
        synchronization::list_pending_share_ids,
    },
};
use std::sync::{Arc, atomic::Ordering};

use super::SseShareState;

/// Decide whether a fetched share's encrypted payload is too large to queue,
/// based on the server-advertised max file size.
///
/// ### Arguments
/// - `content_len`: The encrypted payload length of the fetched share.
/// - `server_max_file_size`: The server-advertised max file size, or `u64::MAX`.
///
/// ### Returns
/// - `true`: The payload exceeds twice the server limit and should be dropped.
/// - `false`: The payload is within bounds, or the server advertises no limit.
fn share_payload_exceeds_limit(content_len: usize, server_max_file_size: u64) -> bool {
    server_max_file_size != u64::MAX && content_len as u64 > server_max_file_size.saturating_mul(2)
}

/// Fetch a single share by id via the read/ack flow into the shared queue.
///
/// ### Arguments
/// - `profile`: The server profile to fetch from
/// - `token_state`: Arc to the per-profile token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `share_state`: Per-profile queue, ack set, and server-advertised max file size
/// - `share_id`: The id announced by the doorbell event
pub(super) fn fetch_single_share_into(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &Arc<ureq::Agent>,
    share_state: &SseShareState,
    share_id: &str,
) {
    let server_max_file_size = share_state.max_file_size_bytes.load(Ordering::Acquire);
    match fetch_unclaimed_share(&share_state.pending_ack_share_ids, share_id, || {
        fetch_share_by_id(
            profile,
            token_state,
            http_agent,
            share_id,
            server_max_file_size,
        )
    }) {
        ClaimedFetch::AlreadyClaimed => {
            log::debug!("Fetch (doorbell): share id {share_id} already claimed, skipping");
        }
        ClaimedFetch::Fetched(share) => {
            if share_payload_exceeds_limit(share.content.len(), server_max_file_size) {
                log::warn!(
                    "Dropping shared file '{}' from device {}: encrypted payload ({} bytes) exceeds 2x the server max ({} bytes)",
                    share.file_name,
                    share.source_device_id,
                    share.content.len(),
                    server_max_file_size
                );
                return;
            }
            share_state.pending_shared_files.lock().push(share);
            log::info!("Fetch (doorbell): queued share id {share_id}, pending ack");
        }
        ClaimedFetch::Failed(e) => {
            log::warn!("Fetch (doorbell) for id {share_id} failed: {e}");
        }
        ClaimedFetch::Rejected(e) => {
            log::warn!("Fetch (doorbell) for id {share_id} rejected, not retrying: {e}");
            let notification = rejected_share_notification(&profile.name, &e);
            if share_state
                .notification_tx
                .unbounded_send(notification)
                .is_err()
            {
                log::warn!("Notification channel closed, dropping rejected share notification");
            }
        }
    }
}

/// Catch up on pending shares after an SSE reconnect.
///
/// ### Description
/// The server does not replay doorbell events for shares that arrived while the
/// connection was down, so the catch-up enumerates the device's pending share
/// ids with the non-consuming `POST /api/v2/begin` (Fulgurant exposes no
/// standalone listing endpoint) and routes each through
/// `fetch_single_share_into`, which claims each id before downloading it so
/// shares already queued, awaiting acknowledgement, or quarantined are skipped.
///
/// ### Arguments
/// - `profile`: The server profile to fetch from
/// - `token_state`: Arc to the per-profile token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `share_state`: Per-profile queue, ack set, and server-advertised max file size
pub(super) fn fetch_pending_shares_into(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &Arc<ureq::Agent>,
    share_state: &SseShareState,
) {
    match list_pending_share_ids(profile, token_state, http_agent) {
        Ok(share_ids) => {
            if share_ids.is_empty() {
                log::debug!("Fetch (reconnect): no pending shares");
                return;
            }
            let count = share_ids.len();
            for share_id in &share_ids {
                fetch_single_share_into(profile, token_state, http_agent, share_state, share_id);
            }
            log::info!("Fetch (reconnect): processed {count} pending share id(s)");
        }
        Err(e) => {
            log::warn!("Fetch (reconnect) failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::share_payload_exceeds_limit;

    #[test]
    fn unlimited_server_never_drops() {
        assert!(!share_payload_exceeds_limit(usize::MAX, u64::MAX));
    }

    #[test]
    fn payload_within_twice_the_limit_is_kept() {
        let server_max = 1024;
        assert!(!share_payload_exceeds_limit(2 * 1024, server_max));
    }

    #[test]
    fn payload_above_twice_the_limit_is_dropped() {
        let server_max = 1024;
        assert!(share_payload_exceeds_limit(2 * 1024 + 1, server_max));
    }
}
