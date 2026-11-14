use crate::fulgur::{Fulgur, tab::Tab, ui};
use gpui_kit::component::{menu::PopupMenu, text::TextViewState};
use gpui_kit::{
    AnyElement, App, Context, DismissEvent, Entity, Focusable, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, ParentElement, Styled, Window, div,
};

impl Fulgur {
    /// Handle a right-click in the editor area to show a custom context menu.
    ///
    /// Called during the capture phase so propagation can be stopped before
    /// the editor's built-in context menu fires.
    ///
    /// ### Arguments
    /// - `event`: The mouse-down event
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(super) fn on_editor_right_click(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Right {
            return;
        }
        cx.stop_propagation();

        let Some(Tab::Editor(editor_tab)) = self.active_tab(cx) else {
            return;
        };
        let active_tab_id = editor_tab.id;
        let editor_focus = editor_tab.content.focus_handle(cx);
        let has_file = editor_tab.file_path().is_some();
        let position = event.position;

        let menu = PopupMenu::build(window, cx, {
            let editor_focus = editor_focus.clone();
            move |mut menu, _window, _cx| {
                menu = menu.action_context(editor_focus);
                if has_file {
                    menu = menu
                        .menu(
                            crate::fulgur::ui::components_utils::reveal_in_file_manager_label(),
                            Box::new(ui::tabs::tab_bar::ShowInFileManager(active_tab_id)),
                        )
                        .separator();
                }
                menu.menu("Cut", Box::new(gpui_kit::component::input::Cut))
                    .menu("Copy", Box::new(gpui_kit::component::input::Copy))
                    .menu("Paste", Box::new(gpui_kit::component::input::Paste))
                    .separator()
                    .menu(
                        "Select All",
                        Box::new(gpui_kit::component::input::SelectAll),
                    )
            }
        });

        let subscription = cx.subscribe_in(
            &menu,
            window,
            |this: &mut Self, _, _: &DismissEvent, _, cx| {
                this.editor_context_menu = None;
                this.editor_context_menu_subscription = None;
                cx.notify();
            },
        );

        self.editor_context_menu = Some((position, menu));
        self.editor_context_menu_subscription = Some(subscription);
        cx.notify();
    }

    /// Handle a right-click on the markdown preview to show its context menu.
    ///
    /// ### Arguments
    /// - `event`: The mouse-down event
    /// - `window`: The window context
    /// - `cx`: The application context
    fn on_preview_right_click(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Right {
            return;
        }
        cx.stop_propagation();

        let position = event.position;
        let preview_focus = self.markdown_preview_focus.clone();

        let selection = gpui_kit::base::TextSelection::selected_text(window, cx)
            .trim()
            .to_string();
        let has_selection = !selection.is_empty();
        self.markdown_preview_pending_copy = has_selection.then_some(selection);

        let menu = PopupMenu::build(window, cx, move |menu, _window, _cx| {
            menu.action_context(preview_focus)
                .menu_with_enable(
                    "Copy",
                    Box::new(gpui_kit::component::input::Copy),
                    has_selection,
                )
                .menu(
                    "Select All",
                    Box::new(gpui_kit::component::input::SelectAll),
                )
        });

        let subscription = cx.subscribe_in(
            &menu,
            window,
            |this: &mut Self, _, _: &DismissEvent, _, cx| {
                this.editor_context_menu = None;
                this.editor_context_menu_subscription = None;
                cx.notify();
            },
        );

        self.editor_context_menu = Some((position, menu));
        self.editor_context_menu_subscription = Some(subscription);
        cx.notify();
    }

    /// Resolve the text view state backing the currently visible markdown preview.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(Entity<TextViewState>)`: The state of the active preview, either a
    ///   dedicated preview tab or the inline panel of a markdown editor tab.
    /// - `None`: If no markdown preview is currently displayed.
    fn active_markdown_preview_state(&self, cx: &App) -> Option<Entity<TextViewState>> {
        match self.active_tab(cx)? {
            Tab::MarkdownPreview(preview_tab) => Some(preview_tab.preview.read(cx).view_state()),
            Tab::Editor(_) => self
                .markdown_panel_preview
                .as_ref()
                .map(|preview| preview.read(cx).view_state()),
            Tab::Settings(_) => None,
        }
    }

    /// Wrap a markdown preview element with its context-menu affordances.
    ///
    /// ### Arguments
    /// - `child`: The preview element to wrap
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The wrapped, right-clickable preview element.
    pub(super) fn wrap_markdown_preview(
        &self,
        child: AnyElement,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let right_click = cx.listener(|this, event: &MouseDownEvent, window, cx| {
            this.on_preview_right_click(event, window, cx);
        });
        let copy = cx.listener(|this, _: &gpui_kit::component::input::Copy, _window, cx| {
            if let Some(text) = this.markdown_preview_pending_copy.take() {
                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(text));
            }
        });
        let select_all = cx.listener(
            |this, _: &gpui_kit::component::input::SelectAll, _window, cx| {
                if let Some(state) = this.active_markdown_preview_state(cx) {
                    state.update(cx, TextViewState::select_all);
                }
            },
        );

        div()
            .id("markdown-preview-context")
            .track_focus(&self.markdown_preview_focus)
            .size_full()
            .capture_any_mouse_down(right_click)
            .on_action(copy)
            .on_action(select_all)
            .child(child)
    }
}
