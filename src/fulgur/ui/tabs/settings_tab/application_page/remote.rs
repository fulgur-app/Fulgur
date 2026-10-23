use crate::fulgur::{Fulgur, state::MAX_REMOTE_HISTORY_ENTRIES};
use gpui_kit::component::{
    Disableable, Sizable,
    button::Button,
    setting::{SettingField, SettingItem},
};
use gpui_kit::{App, IntoElement};

/// Build the setting row that clears the Open Remote File dialog history.
///
/// ### Returns
/// - `SettingItem`: The row with its clear button, disabled while the history is empty
pub(super) fn remote_history_item() -> SettingItem {
    SettingItem::new(
        "Remote URL History",
        SettingField::render(|_options, _window, cx: &mut App| {
            render_clear_remote_history_button(cx)
        }),
    )
    .description(format!(
        "The last {MAX_REMOTE_HISTORY_ENTRIES} URLs opened from the Open Remote File dialog."
    ))
}

/// Render the button that forgets every remote history URL.
///
/// ### Arguments
/// - `cx`: The application context, read for the remote history
///
/// ### Returns
/// - `impl IntoElement`: The clear button, disabled while the history is empty
fn render_clear_remote_history_button(cx: &App) -> impl IntoElement + use<> {
    Button::new("clear-remote-history")
        .label("Clear History")
        .outline()
        .small()
        .disabled(Fulgur::shared_state(cx).remote_history.is_empty())
        .on_click(|_, _window, cx| Fulgur::clear_remote_history(cx))
}
