//! Parsing of the Fulgurant server version header and the share-flow feature
//! gates derived from it.
//!
//! Fulgurant advertises its version through the `x-fulgurant-version` response
//! header. Both the SSE worker and the begin flow use this to decide which
//! share retrieval scheme to use, so the logic lives here, shared between them.

/// HTTP header advertised by Fulgurant 0.7.0+ carrying the server version.
pub const FULGURANT_VERSION_HEADER: &str = "x-fulgurant-version";

/// Minimum Fulgurant `(major, minor)` version that supports fetching a single
/// share by id (`GET /api/shares/:id`) and advertises `x-fulgurant-version`.
const MIN_PER_ID_FETCH_VERSION: (u64, u64) = (0, 7);

/// Minimum Fulgurant `(major, minor)` version that supports the v2 read/ack
/// share flow (`GET /api/v2/shares/:id` + `POST /api/v2/shares/:id/successful`).
const MIN_V2_SHARE_FLOW_VERSION: (u64, u64) = (0, 8);

/// Parse the advertised Fulgurant version header into a `(major, minor)` pair.
///
/// ### Arguments
/// - `version_header`: The raw `x-fulgurant-version` value, if present.
///
/// ### Returns
/// - `Some((major, minor))`: The parsed version.
/// - `None`: The header is absent or unparseable.
fn parse_fulgurant_version(version_header: Option<&str>) -> Option<(u64, u64)> {
    let raw = version_header?;
    let trimmed = raw.trim().trim_start_matches('v');
    match semver::Version::parse(trimmed) {
        Ok(version) => Some((version.major, version.minor)),
        Err(e) => {
            log::warn!("Unparseable {FULGURANT_VERSION_HEADER} header '{raw}': {e}");
            None
        }
    }
}

/// Decide whether the server supports per-id share fetch from its advertised version.
///
/// ### Arguments
/// - `version_header`: The raw `x-fulgurant-version` value, if present.
///
/// ### Returns
/// - `true`: The server is recent enough to fetch shares by id.
/// - `false`: The header is absent, unparseable, or older than 0.7.0.
#[must_use]
pub fn version_supports_per_id_fetch(version_header: Option<&str>) -> bool {
    parse_fulgurant_version(version_header).is_some_and(|v| v >= MIN_PER_ID_FETCH_VERSION)
}

/// Decide whether the server supports the v2 read/ack share flow from its advertised version.
///
/// ### Arguments
/// - `version_header`: The raw `x-fulgurant-version` value, if present.
///
/// ### Returns
/// - `true`: The server is 0.8.0 or newer and supports the read/ack flow.
/// - `false`: The header is absent, unparseable, or older than 0.8.0.
#[must_use]
pub fn version_supports_v2_share_flow(version_header: Option<&str>) -> bool {
    parse_fulgurant_version(version_header).is_some_and(|v| v >= MIN_V2_SHARE_FLOW_VERSION)
}

#[cfg(test)]
mod tests {
    use super::{version_supports_per_id_fetch, version_supports_v2_share_flow};

    #[test]
    fn absent_header_falls_back_to_bulk() {
        assert!(!version_supports_per_id_fetch(None));
    }

    #[test]
    fn unparseable_header_falls_back_to_bulk() {
        assert!(!version_supports_per_id_fetch(Some("not-a-version")));
    }

    #[test]
    fn exact_minimum_version_is_supported() {
        assert!(version_supports_per_id_fetch(Some("0.7.0")));
    }

    #[test]
    fn older_version_is_not_supported() {
        assert!(!version_supports_per_id_fetch(Some("0.6.9")));
    }

    #[test]
    fn newer_minor_and_major_are_supported() {
        assert!(version_supports_per_id_fetch(Some("0.8.1")));
        assert!(version_supports_per_id_fetch(Some("1.0.0")));
    }

    #[test]
    fn leading_v_and_whitespace_are_tolerated() {
        assert!(version_supports_per_id_fetch(Some("  v0.7.0  ")));
    }

    #[test]
    fn v2_flow_requires_at_least_0_8_0() {
        assert!(!version_supports_v2_share_flow(None));
        assert!(!version_supports_v2_share_flow(Some("not-a-version")));
        assert!(!version_supports_v2_share_flow(Some("0.7.9")));
        assert!(version_supports_v2_share_flow(Some("0.8.0")));
        assert!(version_supports_v2_share_flow(Some("  v0.8.1  ")));
        assert!(version_supports_v2_share_flow(Some("1.0.0")));
    }

    #[test]
    fn version_0_7_x_supports_per_id_fetch_but_not_v2_flow() {
        assert!(version_supports_per_id_fetch(Some("0.7.5")));
        assert!(!version_supports_v2_share_flow(Some("0.7.5")));
    }
}
