use std::time::Duration;

use gpui::{
    App, ClipboardItem, ElementId, IntoElement, RenderOnce, SharedString, Styled, Window,
    prelude::FluentBuilder,
};
use gpui_component::{
    Sizable, StyledExt,
    button::{Button, ButtonVariants},
};

use crate::fulgur::ui::{
    components_utils::{CORNERS_SIZE, SEARCH_BAR_HEIGHT},
    icons::CustomIcon,
};

/// A copy-to-clipboard button styled to match Fulgur's bar buttons.
///
/// Clicking copies the configured value to the clipboard and briefly shows
/// a confirmation icon for 2 seconds before reverting to the copy icon.
#[derive(IntoElement)]
pub struct CopyButton {
    id: ElementId,
    value: SharedString,
}

impl CopyButton {
    /// Create a new CopyButton with the given element ID.
    ///
    /// ### Arguments
    /// - `id`: A unique element ID for this button
    ///
    /// ### Returns
    /// - `Self`: A new CopyButton with an empty clipboard value
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            value: SharedString::default(),
        }
    }

    /// Set the value to be written to the clipboard on click.
    ///
    /// ### Arguments
    /// - `value`: The string to copy
    ///
    /// ### Returns
    /// - `Self`: The button with the clipboard value set
    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.value = value.into();
        self
    }
}

#[derive(Default)]
struct CopyButtonState {
    copied: bool,
}

impl RenderOnce for CopyButton {
    /// Render the copy button as a styled ghost bar button.
    ///
    /// Shows a Copy icon normally and a CircleCheck icon for 2 seconds after
    /// the value has been written to the clipboard.
    ///
    /// ### Arguments
    /// - `window`: The window context, used to access per-element keyed state
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered button element
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| CopyButtonState::default());
        let copied = state.read(cx).copied;
        let value = self.value.clone();
        let button_id = self.id.clone();
        let icon = if copied {
            CustomIcon::CircleCheck
        } else {
            CustomIcon::Copy
        };
        Button::new(button_id)
            .icon(icon)
            .text()
            .small()
            .tooltip("Copy")
            .ghost()
            .p_0()
            .m_0()
            .border_0()
            .cursor_pointer()
            .corner_radii(CORNERS_SIZE)
            .h(SEARCH_BAR_HEIGHT)
            .w(SEARCH_BAR_HEIGHT)
            .when(!copied, |this| {
                this.on_click(move |_, _window, cx| {
                    cx.stop_propagation();
                    cx.write_to_clipboard(ClipboardItem::new_string(value.to_string()));
                    state.update(cx, |state, cx| {
                        state.copied = true;
                        cx.notify();
                    });
                    let state = state.clone();
                    cx.spawn(async move |cx| {
                        cx.background_executor().timer(Duration::from_secs(2)).await;
                        state.update(cx, |state, cx| {
                            state.copied = false;
                            cx.notify();
                        });
                    })
                    .detach();
                })
            })
    }
}
