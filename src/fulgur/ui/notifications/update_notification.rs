use crate::fulgur::utils::updater::{UpdateInfo, is_valid_release_page_url};
use gpui::{SharedString, Styled};
use gpui_component::{button::ButtonVariants, notification::Notification};

/// Create an update notification with a download button
///
/// ### Arguments
/// - `update_info`: Information about the available update
///
/// ### Returns
/// - `Notification`: A notification with update information and download action
pub fn make_update_notification(update_info: &UpdateInfo) -> Notification {
    let message = SharedString::from(format!(
        "A new version of Fulgur is available: {}",
        update_info.latest_version
    ));
    let download_url = update_info.download_url.clone();
    gpui_component::notification::Notification::new()
        .message(message)
        .action(move |_, _, cx| {
            let url = download_url.clone();
            gpui_component::button::Button::new("download")
                .primary()
                .label("Download")
                .mr_2()
                .on_click(cx.listener(move |this, _, window, cx| {
                    if is_valid_release_page_url(&url) {
                        match open::that(&url) {
                            Ok(()) => {
                                log::debug!("Successfully opened browser for update");
                            }
                            Err(e) => {
                                log::error!("Failed to open browser: {e}");
                            }
                        }
                    } else {
                        log::error!("Refusing to open non-canonical update URL: {url}");
                    }
                    this.dismiss(window, cx);
                }))
        })
}
