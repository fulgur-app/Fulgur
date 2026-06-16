use crate::fulgur::sync::share;

/// Maximum number of bytes accepted from small JSON HTTP responses (token, ping).
pub const MAX_HTTP_SMALL_RESPONSE_BYTES: u64 = 64 * 1024;

/// Maximum number of bytes accepted from the devices listing response
/// (`GET /api/devices`).
pub const MAX_HTTP_DEVICES_RESPONSE_BYTES: u64 = 1024 * 1024;

/// Top-level JSON framing overhead allowance for a single-share response
/// (object braces, sibling fields like `id`, `file_name`, timestamps).
const SHARE_RESPONSE_FRAMING_BYTES: u64 = 4 * 1024;

/// Maximum number of bytes accepted from a single-share HTTP response
/// (`GET /api/shares/:id`) when the server advertises no size limit.
pub const MAX_HTTP_SINGLE_SHARE_RESPONSE_BYTES: u64 = (share::MAX_SYNC_SHARE_PAYLOAD_BYTES as u64)
    * 2
    + (share::JSON_OVERHEAD_PER_SHARE_BYTES as u64)
    + SHARE_RESPONSE_FRAMING_BYTES;

/// Maximum number of bytes accepted from the legacy `POST /api/begin` response. Sized to fit the worst-case bundle.
pub(super) const MAX_HTTP_V1_BEGIN_RESPONSE_BYTES: u64 = (share::MAX_PENDING_SHARES_PER_RESPONSE
    as u64)
    * ((share::MAX_SYNC_SHARE_PAYLOAD_BYTES as u64)
        + (share::JSON_OVERHEAD_PER_SHARE_BYTES as u64))
    + SHARE_RESPONSE_FRAMING_BYTES;

/// Compute the wire-size cap for a bulk `GET /api/shares` drain, derived from
/// the server's advertised maximum file size.
///
/// ### Arguments
/// - `server_max_file_size`: The server-advertised max file size in bytes, or
///   `u64::MAX` when the server reports no limit.
///
/// ### Returns
/// - `u64`: The maximum number of bytes to accept from the bulk drain response.
pub fn max_http_bulk_shares_response_bytes(server_max_file_size: u64) -> u64 {
    if server_max_file_size == u64::MAX {
        return MAX_HTTP_V1_BEGIN_RESPONSE_BYTES;
    }
    let per_share_wire = server_max_file_size
        .saturating_mul(2)
        .saturating_add(share::JSON_OVERHEAD_PER_SHARE_BYTES as u64);
    (share::MAX_PENDING_SHARES_PER_RESPONSE as u64)
        .saturating_mul(per_share_wire)
        .saturating_add(SHARE_RESPONSE_FRAMING_BYTES)
}

/// Compute the wire-size cap for a single `GET /api/shares/:id` fetch, derived from the server's advertised maximum file size.
///
/// ### Arguments
/// - `server_max_file_size`: The server-advertised max file size in bytes, or
///   `u64::MAX` when the server reports no limit.
///
/// ### Returns
/// - `u64`: The maximum number of bytes to accept from the single-share response.
pub fn max_http_single_share_response_bytes(server_max_file_size: u64) -> u64 {
    if server_max_file_size == u64::MAX {
        return MAX_HTTP_SINGLE_SHARE_RESPONSE_BYTES;
    }
    server_max_file_size
        .saturating_mul(2)
        .saturating_add(share::JSON_OVERHEAD_PER_SHARE_BYTES as u64)
        .saturating_add(SHARE_RESPONSE_FRAMING_BYTES)
}

/// Resolve the server-advertised `max_file_size_bytes` into a concrete cap.
///
/// ### Arguments
/// - `advertised`: The `Option<u64>` received from the server's response
///
/// ### Returns
/// - `u64`: The resolved cap in bytes, or `u64::MAX` when unlimited
pub fn resolve_server_max_file_size(advertised: Option<u64>) -> u64 {
    match advertised {
        None => u64::MAX,
        Some(0) => share::MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
        Some(n) => n,
    }
}

/// Validate and persist the server-advertised `max_file_size_bytes`.
///
/// ### Arguments
/// - `atomic`: The shared atomic holding the current cap
/// - `advertised`: The `Option<u64>` received from the server's response
pub fn store_server_max_file_size(atomic: &std::sync::atomic::AtomicU64, advertised: Option<u64>) {
    match advertised {
        None => log::info!("Server max file size: no limit"),
        Some(0) => log::warn!(
            "Server advertised max_file_size_bytes = 0 (would disable sharing); falling back to {} bytes",
            share::MAX_SYNC_SHARE_PAYLOAD_BYTES
        ),
        Some(n) => log::info!("Server max file size: {n} bytes"),
    }
    let value = resolve_server_max_file_size(advertised);
    atomic.store(value, std::sync::atomic::Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_HTTP_SINGLE_SHARE_RESPONSE_BYTES, MAX_HTTP_V1_BEGIN_RESPONSE_BYTES,
        max_http_bulk_shares_response_bytes, max_http_single_share_response_bytes,
        resolve_server_max_file_size, store_server_max_file_size,
    };
    use crate::fulgur::sync::share;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn bulk_cap_falls_back_to_static_bound_when_unlimited() {
        assert_eq!(
            max_http_bulk_shares_response_bytes(u64::MAX),
            MAX_HTTP_V1_BEGIN_RESPONSE_BYTES
        );
    }

    #[test]
    fn bulk_cap_derives_from_server_limit() {
        let server_max = 2 * 1024 * 1024;
        let per_share = server_max * 2 + share::JSON_OVERHEAD_PER_SHARE_BYTES as u64;
        let expected = (share::MAX_PENDING_SHARES_PER_RESPONSE as u64) * per_share + 4 * 1024;
        assert_eq!(max_http_bulk_shares_response_bytes(server_max), expected);
    }

    #[test]
    fn bulk_cap_saturates_instead_of_overflowing() {
        assert_eq!(max_http_bulk_shares_response_bytes(u64::MAX - 1), u64::MAX);
    }

    #[test]
    fn single_share_cap_falls_back_to_static_bound_when_unlimited() {
        assert_eq!(
            max_http_single_share_response_bytes(u64::MAX),
            MAX_HTTP_SINGLE_SHARE_RESPONSE_BYTES
        );
    }

    #[test]
    fn single_share_cap_derives_from_server_limit() {
        let server_max = 5 * 1024 * 1024;
        let expected = server_max * 2 + share::JSON_OVERHEAD_PER_SHARE_BYTES as u64 + 4 * 1024;
        assert_eq!(max_http_single_share_response_bytes(server_max), expected);
    }

    #[test]
    fn single_share_cap_covers_incompressible_default_file() {
        let default_plaintext = share::MAX_SYNC_SHARE_PAYLOAD_BYTES as u64;
        let worst_case_wire = default_plaintext * 137 / 100;
        assert!(max_http_single_share_response_bytes(u64::MAX) >= worst_case_wire);
        assert!(max_http_single_share_response_bytes(default_plaintext) >= worst_case_wire);
    }

    #[test]
    fn single_share_cap_saturates_instead_of_overflowing() {
        assert_eq!(max_http_single_share_response_bytes(u64::MAX - 1), u64::MAX);
    }

    #[test]
    fn resolve_maps_none_to_unlimited() {
        assert_eq!(resolve_server_max_file_size(None), u64::MAX);
    }

    #[test]
    fn resolve_maps_zero_to_safe_default() {
        assert_eq!(
            resolve_server_max_file_size(Some(0)),
            share::MAX_SYNC_SHARE_PAYLOAD_BYTES as u64
        );
    }

    #[test]
    fn resolve_trusts_positive_values() {
        assert_eq!(resolve_server_max_file_size(Some(7 * 1024)), 7 * 1024);
    }

    #[test]
    fn none_is_stored_as_unlimited() {
        let atomic = AtomicU64::new(0);
        store_server_max_file_size(&atomic, None);
        assert_eq!(atomic.load(Ordering::Acquire), u64::MAX);
    }

    #[test]
    fn zero_is_replaced_with_safe_default() {
        let atomic = AtomicU64::new(0);
        store_server_max_file_size(&atomic, Some(0));
        assert_eq!(
            atomic.load(Ordering::Acquire),
            share::MAX_SYNC_SHARE_PAYLOAD_BYTES as u64
        );
    }

    #[test]
    fn positive_values_are_accepted_verbatim() {
        let atomic = AtomicU64::new(0);
        store_server_max_file_size(&atomic, Some(5 * 1024 * 1024));
        assert_eq!(atomic.load(Ordering::Acquire), 5 * 1024 * 1024);
    }

    #[test]
    fn very_large_values_are_trusted_as_user_choice() {
        let atomic = AtomicU64::new(0);
        store_server_max_file_size(&atomic, Some(u64::MAX - 1));
        assert_eq!(atomic.load(Ordering::Acquire), u64::MAX - 1);
    }
}
