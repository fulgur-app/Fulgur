use crate::fulgur::ui::{
    components_utils::{CORNERS_SIZE, SEARCH_BAR_HEIGHT},
    icons::CustomIcon,
};
use gpui::{
    App, ClickEvent, ElementId, IntoElement, RenderOnce, Styled, Window, prelude::FluentBuilder,
};
use gpui_component::{
    Sizable, StyledExt,
    button::{Button, ButtonVariants},
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// An insert-at-cursor button styled to match Fulgur's bar buttons.
///
/// Clicking inserts the associated value at the cursor position in the active
/// editor, replacing any current selection. Has no internal state; behavior is
/// provided entirely via the [`on_click`](InsertButton::on_click) handler.
#[derive(IntoElement)]
pub struct InsertButton {
    id: ElementId,
    on_click: Option<ClickHandler>,
}

impl InsertButton {
    /// Create a new InsertButton with the given element ID.
    ///
    /// ### Arguments
    /// - `id`: A unique element ID for this button
    ///
    /// ### Returns
    /// - `Self`: A new InsertButton with no click handler set
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            on_click: None,
        }
    }

    /// Set the click handler called when the button is activated.
    ///
    /// ### Arguments
    /// - `handler`: A closure invoked with the click event, window, and app context
    ///
    /// ### Returns
    /// - `Self`: The button with the click handler set
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for InsertButton {
    /// Render the insert button as a styled ghost bar button with a Plus icon.
    ///
    /// ### Arguments
    /// - `_window`: The window context (unused)
    /// - `_cx`: The application context (unused)
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered button element
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        Button::new(self.id)
            .icon(CustomIcon::InserHorizontal)
            .text()
            .small()
            .tooltip("Insert at cursor")
            .ghost()
            .p_0()
            .m_0()
            .border_0()
            .cursor_pointer()
            .corner_radii(CORNERS_SIZE)
            .h(SEARCH_BAR_HEIGHT)
            .w(SEARCH_BAR_HEIGHT)
            .when_some(self.on_click, |this, handler| this.on_click(handler))
    }
}
