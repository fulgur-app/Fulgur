use semver::Version;
use serde::Deserialize;
use url::Url;

const GITHUB_API_URL: &str = "https://api.github.com/repos/fulgur-app/Fulgur/releases/latest";
const RELEASE_PAGE_HOST: &str = "github.com";
const RELEASE_PAGE_PATH_PREFIX: &str = "/fulgur-app/Fulgur/releases/tag/";

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: String,
    pub body: Option<String>,
    pub html_url: String,
    pub published_at: String,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub is_newer: bool,
    pub download_url: String,
    pub release_notes: String,
}

/// Parse a version string into a Version struct
///
/// ### Arguments
/// - `version_str`: The version string to parse
///
/// ### Returns
/// - `Ok(Version)`: The parsed version
/// - `Err(anyhow::Error)`: If the version string could not be parsed
fn parse_version(version_str: &str) -> anyhow::Result<Version> {
    let cleaned = version_str.trim_start_matches('v');
    Version::parse(cleaned).map_err(|e| e.into())
}

/// Validate that a URL points to the canonical Fulgur release page on GitHub
///
/// ### Arguments
/// - `url`: The URL to validate
///
/// ### Returns
/// - `true`: If the URL is a well-formed canonical Fulgur release page URL
/// - `false`: Otherwise
pub fn is_valid_release_page_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "https" {
        return false;
    }
    if parsed.host_str() != Some(RELEASE_PAGE_HOST) {
        return false;
    }
    let Some(tag) = parsed.path().strip_prefix(RELEASE_PAGE_PATH_PREFIX) else {
        return false;
    };
    let tag = tag.trim_end_matches('/');
    if tag.is_empty() || tag.contains('/') {
        return false;
    }
    let Some(rest) = tag.strip_prefix('v') else {
        return false;
    };
    Version::parse(rest).is_ok()
}

/// Check for updates
///
/// ### Arguments
/// - `current_version`: The current version of the application
///
/// ### Returns
/// - `Ok(Some(UpdateInfo))`: The update information if an update is available
/// - `Ok(None)`: If no update is available
/// - `Err(anyhow::Error)`: If the update check could not be performed
pub fn check_for_updates(current_version: &str) -> anyhow::Result<Option<UpdateInfo>> {
    let agent = ureq::Agent::new_with_config(
        ureq::config::Config::builder()
            .timeout_connect(Some(std::time::Duration::from_secs(5)))
            .timeout_global(Some(std::time::Duration::from_secs(10)))
            .build(),
    );
    let mut response = agent
        .get(GITHUB_API_URL)
        .header("User-Agent", "Fulgur")
        .call()?;
    if response.status() != 200 {
        return Err(anyhow::anyhow!("GitHub API error: {}", response.status()));
    }
    let release: GitHubRelease = response.body_mut().read_json()?;
    let current = parse_version(current_version)?;
    let latest = parse_version(&release.tag_name)?;
    if latest > current {
        log::info!("New version available: {current} -> {latest}");
        let download_url = release_page_url(&release)?;
        Ok(Some(UpdateInfo {
            current_version: current.to_string(),
            latest_version: latest.to_string(),
            is_newer: true,
            download_url,
            release_notes: release.body.unwrap_or_default(),
        }))
    } else {
        log::info!("Already on latest version: {current}");
        Ok(None)
    }
}

/// Extract and validate the GitHub release page URL for a release
///
/// ### Arguments
/// - `release`: The release information returned by the GitHub API
///
/// ### Returns
/// - `Ok(String)`: The validated release page URL
/// - `Err(anyhow::Error)`: If the URL fails the canonical-form check
fn release_page_url(release: &GitHubRelease) -> anyhow::Result<String> {
    if !is_valid_release_page_url(&release.html_url) {
        return Err(anyhow::anyhow!(
            "Release html_url is not a canonical Fulgur release page: {}",
            release.html_url
        ));
    }
    Ok(release.html_url.clone())
}

#[cfg(test)]
mod tests {
    use super::{is_valid_release_page_url, parse_version};

    #[test]
    fn test_parse_version_with_v_prefix() {
        let result = parse_version("v1.2.3");
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 2);
        assert_eq!(version.patch, 3);
    }

    #[test]
    fn test_parse_version_without_v_prefix() {
        let result = parse_version("1.2.3");
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 2);
        assert_eq!(version.patch, 3);
    }

    #[test]
    fn test_parse_version_with_prelease() {
        let result = parse_version("v1.2.3-alpha.1");
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 2);
        assert_eq!(version.patch, 3);
    }

    #[test]
    fn test_parse_version_with_build_metadata() {
        let result = parse_version("v1.2.3+20240101");
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 2);
        assert_eq!(version.patch, 3);
    }

    #[test]
    fn test_parse_version_invalid_format() {
        let result = parse_version("not-a-version");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_version_empty_string() {
        let result = parse_version("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_version_multiple_v_prefixes() {
        let result = parse_version("vv1.2.3");
        assert!(result.is_ok());
        let version = result.unwrap();
        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 2);
        assert_eq!(version.patch, 3);
    }

    #[test]
    fn test_validate_release_page_url_canonical() {
        assert!(is_valid_release_page_url(
            "https://github.com/fulgur-app/Fulgur/releases/tag/v0.6.0"
        ));
    }

    #[test]
    fn test_validate_release_page_url_canonical_with_prerelease_tag() {
        assert!(is_valid_release_page_url(
            "https://github.com/fulgur-app/Fulgur/releases/tag/v1.2.3-rc.1"
        ));
    }

    #[test]
    fn test_validate_release_page_url_canonical_with_nightly_tag() {
        assert!(is_valid_release_page_url(
            "https://github.com/fulgur-app/Fulgur/releases/tag/v1.2.3-nightly"
        ));
    }

    #[test]
    fn test_validate_release_page_url_canonical_with_trailing_slash() {
        assert!(is_valid_release_page_url(
            "https://github.com/fulgur-app/Fulgur/releases/tag/v1.2.3/"
        ));
    }

    #[test]
    fn test_validate_release_page_url_rejects_non_semver_tag() {
        assert!(!is_valid_release_page_url(
            "https://github.com/fulgur-app/Fulgur/releases/tag/vEVIL"
        ));
    }

    #[test]
    fn test_validate_release_page_url_rejects_tag_missing_v_prefix() {
        assert!(!is_valid_release_page_url(
            "https://github.com/fulgur-app/Fulgur/releases/tag/1.2.3"
        ));
    }

    #[test]
    fn test_validate_release_page_url_rejects_tag_with_double_v() {
        assert!(!is_valid_release_page_url(
            "https://github.com/fulgur-app/Fulgur/releases/tag/vv1.2.3"
        ));
    }

    #[test]
    fn test_validate_release_page_url_rejects_tag_with_extra_path() {
        assert!(!is_valid_release_page_url(
            "https://github.com/fulgur-app/Fulgur/releases/tag/v1.2.3/../malware"
        ));
    }

    #[test]
    fn test_validate_release_page_url_rejects_empty_tag() {
        assert!(!is_valid_release_page_url(
            "https://github.com/fulgur-app/Fulgur/releases/tag/"
        ));
    }

    #[test]
    fn test_validate_release_page_url_rejects_partial_version_tag() {
        assert!(!is_valid_release_page_url(
            "https://github.com/fulgur-app/Fulgur/releases/tag/v1.2"
        ));
    }

    #[test]
    fn test_validate_release_page_url_rejects_http_scheme() {
        assert!(!is_valid_release_page_url(
            "http://github.com/fulgur-app/Fulgur/releases/tag/v0.6.0"
        ));
    }

    #[test]
    fn test_validate_release_page_url_rejects_subdomain_lookalike() {
        assert!(!is_valid_release_page_url(
            "https://github.com.evil.com/fulgur-app/Fulgur/releases/tag/v0.6.0"
        ));
    }

    #[test]
    fn test_validate_release_page_url_rejects_repo_lookalike() {
        assert!(!is_valid_release_page_url(
            "https://github.com/fulgur-app/Fulgur.evil.com/releases/tag/v0.6.0"
        ));
    }

    #[test]
    fn test_validate_release_page_url_rejects_wrong_owner() {
        assert!(!is_valid_release_page_url(
            "https://github.com/attacker/Fulgur/releases/tag/v0.6.0"
        ));
    }

    #[test]
    fn test_validate_release_page_url_rejects_releases_download_path() {
        assert!(!is_valid_release_page_url(
            "https://github.com/fulgur-app/Fulgur/releases/download/v0.6.0/Fulgur-macos-aarch64.dmg"
        ));
    }

    #[test]
    fn test_validate_release_page_url_rejects_random_path() {
        assert!(!is_valid_release_page_url(
            "https://github.com/fulgur-app/Fulgur/issues/1"
        ));
    }

    #[test]
    fn test_validate_release_page_url_rejects_malformed_url() {
        assert!(!is_valid_release_page_url("not a url"));
    }

    #[test]
    fn test_validate_release_page_url_rejects_empty_string() {
        assert!(!is_valid_release_page_url(""));
    }
}
