use gpui::*;
use gpui_component::ActiveTheme;

/// Data carried during a tab drag operation.
///
/// Also implements `Render` so it can serve as the ghost view shown under
/// the cursor while dragging.
#[derive(Clone)]
pub struct DraggedTab {
    /// Index of the tab in the source window at the time the drag started
    pub tab_index: usize,
    pub title: SharedString,
    pub is_modified: bool,
}

impl Render for DraggedTab {
    /// Renders the floating ghost tab shown under the cursor while dragging
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modified_indicator = if self.is_modified { " •" } else { "" };
        div()
            .px_3()
            .py_1()
            .rounded_md()
            .shadow_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().tab_active)
            .text_color(cx.theme().tab_active_foreground)
            .text_sm()
            .child(format!("{}{}", self.title, modified_indicator))
    }
}
