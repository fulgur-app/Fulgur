//! Parsing of the Fulgurant server version header and the minimum-version gate derived from it.

/// HTTP header advertised by Fulgurant 0.7.0+ carrying the server version.
pub const FULGURANT_VERSION_HEADER: &str = "x-fulgurant-version";

/// Minimum Fulgurant `(major, minor)` version Fulgur can synchronize with.
const MIN_SUPPORTED_FULGURANT_VERSION: (u64, u64) = (0, 8);

/// Human-readable form of `MIN_SUPPORTED_FULGURANT_VERSION` for error messages.
pub const MIN_SUPPORTED_FULGURANT_VERSION_DISPLAY: &str = "0.8.0";

/// Minimum Fulgurant version this build of Fulgur is best paired with.
pub const RECOMMENDED_FULGURANT_VERSION: &str = "0.8.1";

/// Version assumed for a Fulgurant server that does not advertise the
/// `x-fulgurant-version` header.
pub const FULGURANT_VERSION_WITHOUT_HEADER: &str = "0.6.0";

/// Compatibility verdict between a running component version and a minimum
/// version required by its counterpart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionCompatibility {
    /// The running version satisfies the required minimum.
    Compatible,
    /// The running version is behind the required minimum but still within the
    /// supported window: a soft "update recommended" hint is enough.
    UpdateRecommended,
    /// The running version is at least three minor versions or one major
    /// version behind the required minimum: an update is required.
    UpdateRequired,
}

/// Compare a running version against a required minimum version.
///
/// ### Arguments
/// - `current`: The running version (e.g. `0.8.0`); a leading `v` is tolerated.
/// - `required`: The minimum required version advertised by the counterpart.
///
/// ### Returns
/// - `VersionCompatibility::Compatible`: `current` satisfies `required`, or
///   either value is unparseable (the gap cannot be judged, so stay lenient).
/// - `VersionCompatibility::UpdateRecommended`: `current` is behind `required`
///   but within the supported window.
/// - `VersionCompatibility::UpdateRequired`: `current` is at least three minor
///   versions or one major version behind `required`.
#[must_use]
pub fn compare_required_version(current: &str, required: &str) -> VersionCompatibility {
    let parse = |raw: &str| semver::Version::parse(raw.trim().trim_start_matches(['v', 'V']));
    let (Ok(current), Ok(required)) = (parse(current), parse(required)) else {
        return VersionCompatibility::Compatible;
    };
    if required <= current {
        return VersionCompatibility::Compatible;
    }
    // At this point `required > current`, so a lower required major already
    // resolved to `Compatible`: the minor gap below is always within one major.
    if required.major > current.major || required.minor >= current.minor.saturating_add(3) {
        return VersionCompatibility::UpdateRequired;
    }
    VersionCompatibility::UpdateRecommended
}

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

/// Decide whether the server is recent enough for Fulgur to synchronize with.
///
/// ### Arguments
/// - `version_header`: The raw `x-fulgurant-version` value, if present.
///
/// ### Returns
/// - `true`: The server is 0.8.0 or newer and speaks the read/ack share flow.
/// - `false`: The header is absent, unparseable, or older than 0.8.0.
#[must_use]
pub fn server_meets_minimum_version(version_header: Option<&str>) -> bool {
    parse_fulgurant_version(version_header).is_some_and(|v| v >= MIN_SUPPORTED_FULGURANT_VERSION)
}

#[cfg(test)]
mod tests {
    use super::{
        MIN_SUPPORTED_FULGURANT_VERSION_DISPLAY, RECOMMENDED_FULGURANT_VERSION,
        VersionCompatibility, compare_required_version, server_meets_minimum_version,
    };

    #[test]
    fn recommended_fulgurant_version_is_valid_semver() {
        assert!(semver::Version::parse(RECOMMENDED_FULGURANT_VERSION).is_ok());
    }

    #[test]
    fn equal_or_newer_current_is_compatible() {
        assert_eq!(
            compare_required_version("0.8.0", "0.8.0"),
            VersionCompatibility::Compatible
        );
        assert_eq!(
            compare_required_version("0.9.0", "0.8.0"),
            VersionCompatibility::Compatible
        );
        assert_eq!(
            compare_required_version("1.0.0", "0.8.0"),
            VersionCompatibility::Compatible
        );
    }

    #[test]
    fn small_gap_recommends_update() {
        assert_eq!(
            compare_required_version("0.8.0", "0.9.0"),
            VersionCompatibility::UpdateRecommended
        );
        assert_eq!(
            compare_required_version("0.8.0", "0.10.0"),
            VersionCompatibility::UpdateRecommended
        );
        assert_eq!(
            compare_required_version("0.8.0", "0.8.1"),
            VersionCompatibility::UpdateRecommended
        );
    }

    #[test]
    fn three_minor_or_one_major_gap_requires_update() {
        assert_eq!(
            compare_required_version("0.8.0", "0.11.0"),
            VersionCompatibility::UpdateRequired
        );
        assert_eq!(
            compare_required_version("0.8.0", "1.0.0"),
            VersionCompatibility::UpdateRequired
        );
        assert_eq!(
            compare_required_version("1.0.0", "2.0.0"),
            VersionCompatibility::UpdateRequired
        );
    }

    #[test]
    fn unparseable_versions_are_treated_as_compatible() {
        assert_eq!(
            compare_required_version("not-a-version", "0.9.0"),
            VersionCompatibility::Compatible
        );
        assert_eq!(
            compare_required_version("0.8.0", "garbage"),
            VersionCompatibility::Compatible
        );
    }

    #[test]
    fn leading_v_is_tolerated_in_comparison() {
        assert_eq!(
            compare_required_version("v0.8.0", "v0.11.0"),
            VersionCompatibility::UpdateRequired
        );
    }

    #[test]
    fn absent_header_is_rejected() {
        assert!(!server_meets_minimum_version(None));
    }

    #[test]
    fn unparseable_header_is_rejected() {
        assert!(!server_meets_minimum_version(Some("not-a-version")));
    }

    #[test]
    fn exact_minimum_version_is_supported() {
        assert!(server_meets_minimum_version(Some("0.8.0")));
    }

    #[test]
    fn versions_below_the_minimum_are_rejected() {
        assert!(!server_meets_minimum_version(Some("0.6.9")));
        assert!(!server_meets_minimum_version(Some("0.7.9")));
    }

    #[test]
    fn newer_minor_and_major_are_supported() {
        assert!(server_meets_minimum_version(Some("0.8.1")));
        assert!(server_meets_minimum_version(Some("1.0.0")));
    }

    #[test]
    fn leading_v_and_whitespace_are_tolerated() {
        assert!(server_meets_minimum_version(Some("  v0.8.0  ")));
    }

    #[test]
    fn displayed_minimum_version_matches_the_gate() {
        assert!(server_meets_minimum_version(Some(
            MIN_SUPPORTED_FULGURANT_VERSION_DISPLAY
        )));
        assert!(semver::Version::parse(MIN_SUPPORTED_FULGURANT_VERSION_DISPLAY).is_ok());
    }
}
