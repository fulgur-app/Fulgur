use crate::fulgur::{Fulgur, menus::build_menus};
use gpui::*;
use gpui_component::{
    WindowExt,
    button::{Button, ButtonVariants},
    notification::{Notification, NotificationType},
};
use semver::Version;
use serde::Deserialize;

const GITHUB_API_URL: &str = "https://api.github.com/repos/PRRPCHT/Fulgur/releases/latest";

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
    pub release_page: String,
}

fn parse_version(version_str: &str) -> anyhow::Result<Version> {
    let cleaned = version_str.trim_start_matches('v');
    Version::parse(cleaned).map_err(|e| e.into())
}

pub fn check_for_updates(current_version: String) -> anyhow::Result<Option<UpdateInfo>> {
    let mut response = ureq::get(GITHUB_API_URL)
        .header("User-Agent", "Fulgur")
        .call()?;
    if response.status() != 200 {
        return Err(anyhow::anyhow!("GitHub API error: {}", response.status()));
    }
    let release: GitHubRelease = response.body_mut().read_json()?;
    let current = parse_version(current_version.as_str())?;
    let latest = parse_version(&release.tag_name)?;
    if latest > current {
        log::info!("New version available: {} -> {}", current, latest);
        let download_url = get_platform_download_url(&release)?;
        Ok(Some(UpdateInfo {
            current_version: current.to_string(),
            latest_version: latest.to_string(),
            is_newer: true,
            download_url,
            release_notes: release.body.unwrap_or_default(),
            release_page: release.html_url,
        }))
    } else {
        log::info!("Already on latest version: {}", current);
        Ok(None)
    }
}

fn get_platform_download_url(release: &GitHubRelease) -> anyhow::Result<String> {
    let platform = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let pattern = match (platform, arch) {
        ("macos", "aarch64") => "macos-aarch64", // Apple Silicon
        ("macos", "x86_64") => "macos-x86_64",   // Intel Mac
        ("macos", _) => "macos-universal",       // Universal binary
        ("windows", "x86_64") => "windows-x86_64",
        ("windows", "aarch64") => "windows-aarch64",
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        _ => {
            return Err(anyhow::anyhow!(
                "Unsupported platform: {}-{}",
                platform,
                arch
            ));
        }
    };
    for asset in &release.assets {
        if asset.name.contains(pattern) {
            return Ok(asset.browser_download_url.clone());
        }
    }
    Ok(release.html_url.clone())
}

impl Fulgur {
    // Check for updates, open the download page in the browser if an update is available + update the menus to show the update available action
    // @param window: The window context
    // @param cx: The application context
    pub fn check_for_updates(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(update_link) = self.update_link.as_ref() {
            match open::that(update_link) {
                Ok(_) => {
                    log::debug!("Successfully opened browser");
                }
                Err(e) => {
                    log::error!("Failed to open browser: {}", e);
                }
            }
            return;
        }
        let bg = cx.background_executor().clone();
        cx.spawn_in(window, async move |view, window| {
            log::debug!("Checking for updates");
            let current_version = env!("CARGO_PKG_VERSION");
            log::debug!("Current version: {}", current_version);
            let update_info = bg
                .spawn(async move {
                    check_for_updates(current_version.to_string())
                        .ok()
                        .flatten()
                })
                .await;
            window
                .update(|window, cx| {
                    if let Some(update_info) = update_info {
                        let _ = view.update(cx, |this, cx| {
                            this.update_link = Some(update_info.download_url.clone());
                            let menus = build_menus(
                                cx,
                                &this.settings.recent_files.get_files(),
                                this.update_link.clone(),
                            );
                            cx.set_menus(menus);
                            cx.notify();
                        });
                        let notification_text = SharedString::from(format!(
                            "Update found! {} -> {}",
                            update_info.current_version, update_info.latest_version
                        ));
                        let update_info_clone = update_info.clone();
                        let notification = Notification::new().message(notification_text).action(
                            move |_, _, cx| {
                                let _download_url = update_info_clone.download_url.clone();
                                Button::new("download")
                                    .primary()
                                    .label("Download")
                                    .mr_2()
                                    .on_click(cx.listener({
                                        let url = update_info.download_url.clone();
                                        move |this, _, window, cx| {
                                            match open::that(&url) {
                                                Ok(_) => {
                                                    log::debug!("Successfully opened browser");
                                                }
                                                Err(e) => {
                                                    log::error!("Failed to open browser: {}", e);
                                                }
                                            }
                                            this.dismiss(window, cx);
                                        }
                                    }))
                            },
                        );
                        window.push_notification(notification, cx);
                    } else {
                        let notification = SharedString::from("No update found");
                        window.push_notification((NotificationType::Info, notification), cx);
                    }
                })
                .ok();
        })
        .detach();
    }
}
