use crate::fulgur::utils::{crypto_helper::SECRETS_FILE_NAME, portable};
use gpui_kit::component::{
    ActiveTheme,
    label::Label,
    setting::{SettingField, SettingItem},
};
use gpui_kit::{App, SharedString, Styled};

/// Build the read-only row telling where a portable Fulgur keeps its data.
///
/// ### Returns
/// - `Some(SettingItem)`: The row, when the process runs in portable mode
/// - `None`: The process uses the regular user profile layout
pub(super) fn portable_mode_item() -> Option<SettingItem> {
    let data_dir = SharedString::from(portable::layout()?.data_dir().display().to_string());
    Some(
        SettingItem::new(
            "Portable Mode",
            SettingField::render(move |_options, _window, cx: &mut App| {
                Label::new(data_dir.clone())
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
            }),
        )
        .description(format!(
            "Settings, session and sync secrets are stored in this folder. Sync secrets are \
             not encrypted ({SECRETS_FILE_NAME}): keep the drive safe, and revoke this device \
             from another one if the drive is lost."
        )),
    )
}
