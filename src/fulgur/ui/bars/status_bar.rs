use crate::fulgur::{
    Fulgur,
    languages::supported_languages::{SupportedLanguage, pretty_name},
    settings::MarkdownPreviewMode,
    sync::synchronization::SynchronizationStatus,
    tab::Tab,
    ui::{
        components_utils::{EMPTY, UTF_8},
        icons::CustomIcon,
    },
};
use gpui::{prelude::FluentBuilder, *};
use gpui_component::{ActiveTheme, Icon, h_flex, input::Position};
use std::f32::consts::PI;
use std::time::Duration;

/// Create a status bar item
///
/// ### Arguments
/// - `content`: The content of the status bar item
/// - `border_color`: The color of the border
///
/// ### Returns
/// - `Div`: A status bar item
pub fn status_bar_item_factory(content: impl IntoElement, border_color: Hsla) -> Div {
    div()
        .text_xs()
        .px_2()
        .py_1()
        .border_color(border_color)
        .child(content)
}

/// Create a status bar button
///
/// ### Arguments
/// - `content`: The content of the status bar button
/// - `border_color`: The color of the border
/// - `accent_color`: The color of the accent
///
/// ### Returns
/// - `Div`: A status bar button
pub fn status_bar_button_factory(
    content: impl IntoElement,
    border_color: Hsla,
    accent_color: Hsla,
) -> Div {
    status_bar_item_factory(content, border_color)
        .hover(|this| this.bg(accent_color))
        .cursor_pointer()
}

/// Create a status bar item, right hand side
///
/// ### Arguments
/// - `content`: The content of the status bar right item
/// - `border_color`: The color of the border
///
/// ### Returns
/// - `impl IntoElement`: A status bar right item
pub fn status_bar_right_item_factory(content: String, border_color: Hsla) -> impl IntoElement {
    status_bar_item_factory(content, border_color) //.border_l_1()
}

/// Create a status bar toggle button
///
/// ### Arguments
/// - `content`: The content of the status bar toggle button
/// - `border_color`: The color of the border
/// - `accent_color`: The color of the accent
/// - `checked`: Whether the toggle is checked
///
/// ### Returns
/// - `Div`: A status bar toggle button
pub fn status_bar_toggle_button_factory(
    content: impl IntoElement,
    border_color: Hsla,
    accent_color: Hsla,
    checked: bool,
) -> Div {
    let mut button = status_bar_button_factory(content, border_color, accent_color);
    if checked {
        button = button.bg(accent_color);
    }
    button
}

/// Parameters for the sync button styling
pub struct SyncButtonStyle {
    pub connected_icon: Icon,
    pub disconnected_icon: Icon,
    pub border_color: Hsla,
    pub connected_color: Hsla,
    pub connected_foreground_color: Hsla,
    pub connected_hover_color: Hsla,
    pub disconnected_color: Hsla,
    pub disconnected_foreground_color: Hsla,
    pub disconnected_hover_color: Hsla,
    pub connecting_color: Hsla,
    pub connecting_foreground_color: Hsla,
}

/// The visual state of the sync button
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncButtonState {
    Connected,
    Connecting,
    Disconnected,
}

/// Delay before showing the connecting spinner (to avoid flickering on fast connections)
const CONNECTING_SPINNER_DELAY: Duration = Duration::from_millis(500);

/// Create a status bar sync button
///
/// ### Arguments
/// - `style`: The styling parameters for the sync button
/// - `state`: The current sync button state
/// - `show_spinner`: Whether to show the spinning animation (only after delay)
///
/// ### Returns
/// - `Div`: A status bar sync button
pub fn status_bar_sync_button(
    style: SyncButtonStyle,
    state: SyncButtonState,
    show_spinner: bool,
) -> Div {
    let mut button = div()
        .text_sm()
        .flex()
        .items_center()
        .justify_center()
        .px_4()
        .py_1()
        .border_color(style.border_color);
    match state {
        SyncButtonState::Connected => {
            button = button
                .child(style.connected_icon)
                .bg(style.connected_color)
                .text_color(style.connected_foreground_color)
                .hover(|this| this.bg(style.connected_hover_color))
                .cursor_pointer();
        }
        SyncButtonState::Connecting => {
            if show_spinner {
                let spinning_icon = Icon::new(CustomIcon::Zap).with_animation(
                    "sync-spinner",
                    Animation::new(Duration::from_secs(1)).repeat(),
                    |icon, delta| icon.rotate(radians(delta * 2.0 * PI)),
                );
                button = button
                    .child(spinning_icon)
                    .bg(style.connecting_color)
                    .text_color(style.connecting_foreground_color);
            } else {
                button = button
                    .child(style.connected_icon)
                    .bg(style.connecting_color)
                    .text_color(style.connecting_foreground_color);
            }
        }
        SyncButtonState::Disconnected => {
            button = button
                .child(style.disconnected_icon)
                .bg(style.disconnected_color)
                .text_color(style.disconnected_foreground_color)
                .hover(|this| this.bg(style.disconnected_hover_color))
                .cursor_pointer();
        }
    }
    button
}

impl Fulgur {
    /// Resolve the state reflected by the status bar for cursor position, language and encoding.
    ///
    /// ### Parameters:
    /// - `cx`: The application context.
    ///
    /// ### Returns:
    /// - `(Position, String, String)`: Cursor position, display language label and encoding label.
    fn status_bar_reflection_values(&self, cx: &Context<Self>) -> (Position, String, String) {
        let (cursor_pos, language) = match self.active_tab_index {
            Some(index) => {
                if let Some(editor_tab) = self.tabs[index].as_editor() {
                    (
                        editor_tab.content.read(cx).cursor_position(),
                        Some(editor_tab.language),
                    )
                } else {
                    (Position::default(), Some(SupportedLanguage::Plain))
                }
            }
            None => (Position::default(), None),
        };
        let language = match language {
            Some(language) => pretty_name(&language),
            None => EMPTY.to_string(),
        };
        let encoding = match self.active_tab_index {
            Some(index) => {
                if let Some(editor_tab) = self.tabs[index].as_editor() {
                    editor_tab.encoding.clone()
                } else {
                    EMPTY.to_string()
                }
            }
            None => UTF_8.to_string(),
        };

        (cursor_pos, language, encoding)
    }

    /// Resolve the sync indicator visual state reflected by the status bar.
    ///
    /// ### Parameters:
    /// - `cx`: The application context.
    ///
    /// ### Returns:
    /// - `(SyncButtonState, bool)`: The sync button state and whether the spinner should be shown.
    fn status_bar_sync_button_state(&self, cx: &Context<Self>) -> (SyncButtonState, bool) {
        let sync_status = *self.shared_state(cx).sync_state.connection_status.lock();
        match sync_status {
            SynchronizationStatus::Connected => (SyncButtonState::Connected, false),
            SynchronizationStatus::Connecting => {
                let show = self
                    .shared_state(cx)
                    .sync_state
                    .connecting_since
                    .lock()
                    .map(|since| since.elapsed() >= CONNECTING_SPINNER_DELAY)
                    .unwrap_or(false);
                (SyncButtonState::Connecting, show)
            }
            _ => (SyncButtonState::Disconnected, false),
        }
    }

    /// Render the status bar
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered status bar element
    pub fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (cursor_pos, language, encoding) = self.status_bar_reflection_values(cx);
        let jump_to_line_button_content = format!(
            "Ln {}, Col {}",
            cursor_pos.line + 1,
            cursor_pos.character + 1
        );
        let jump_to_line_button = status_bar_button_factory(
            jump_to_line_button_content,
            cx.theme().border,
            cx.theme().muted,
        );
        let jump_to_line_button = jump_to_line_button.on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                this.show_jump_to_line_dialog(window, cx);
            }),
        );
        let language_button =
            status_bar_button_factory(language, cx.theme().border, cx.theme().muted).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    //set_language(this, window, cx, language_shared.clone());
                    this.render_select_language_sheet(window, cx);
                }),
            );
        let (preview_button, toolbar_button) = match self.get_active_editor_tab() {
            None => (div(), div()),
            Some(active_editor_tab) => {
                let editor_id = active_editor_tab.id;
                let preview_active = match self
                    .settings
                    .editor_settings
                    .markdown_settings
                    .preview_mode
                {
                    MarkdownPreviewMode::DedicatedTab => self.tabs.iter().any(
                        |t| matches!(t, Tab::MarkdownPreview(p) if p.source_tab_id == editor_id),
                    ),
                    MarkdownPreviewMode::Panel => active_editor_tab.show_markdown_preview,
                };
                let preview_button = status_bar_toggle_button_factory(
                    "Preview".to_string(),
                    cx.theme().border,
                    cx.theme().muted,
                    preview_active,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                        if this.settings.editor_settings.markdown_settings.preview_mode
                            == MarkdownPreviewMode::DedicatedTab
                        {
                            this.open_markdown_preview_tab(window, cx);
                        } else {
                            if let Some(active_editor_tab) = this.get_active_editor_tab_mut() {
                                active_editor_tab.show_markdown_preview =
                                    !active_editor_tab.show_markdown_preview;
                            }
                            cx.notify();
                        }
                    }),
                );
                let show_markdown_toolbar = active_editor_tab.show_markdown_toolbar;
                let toolbar_button = status_bar_toggle_button_factory(
                    "Toolbar".to_string(),
                    cx.theme().border,
                    cx.theme().muted,
                    show_markdown_toolbar,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                        let active_editor_tab = this.get_active_editor_tab_mut();
                        if let Some(active_editor_tab) = active_editor_tab {
                            active_editor_tab.show_markdown_toolbar =
                                !active_editor_tab.show_markdown_toolbar;
                        }
                        cx.notify();
                    }),
                );
                (preview_button, toolbar_button)
            }
        };
        let is_markdown = self.is_markdown();
        let (sync_button_state, show_spinner) = self.status_bar_sync_button_state(cx);
        let sync_button = status_bar_sync_button(
            SyncButtonStyle {
                connected_icon: Icon::new(CustomIcon::Zap),
                disconnected_icon: Icon::new(CustomIcon::ZapOff),
                border_color: cx.theme().border,
                connected_color: cx.theme().primary,
                connected_foreground_color: cx.theme().primary_foreground,
                connected_hover_color: cx.theme().primary_hover,
                disconnected_color: cx.theme().danger,
                disconnected_foreground_color: cx.theme().danger_foreground,
                disconnected_hover_color: cx.theme().danger_hover,
                connecting_color: cx.theme().warning,
                connecting_foreground_color: cx.theme().warning_foreground,
            },
            sync_button_state,
            show_spinner,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, window, cx| {
                this.open_share_file_sheet(window, cx);
            }),
        );
        h_flex()
            .justify_between()
            .bg(cx.theme().tab_bar)
            .py_0()
            .my_0()
            .border_t_1()
            .border_color(cx.theme().border)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .flex()
                    .justify_start()
                    .when(
                        self.settings
                            .app_settings
                            .synchronization_settings
                            .is_synchronization_activated,
                        |this| this.child(sync_button),
                    )
                    .child(language_button)
                    .when(is_markdown, |this| this.child(preview_button))
                    .when(is_markdown, |this| this.child(toolbar_button)),
            )
            .child({
                let color_picker_active = self.color_picker_bar_state.show_color_picker;
                let color_button = status_bar_toggle_button_factory(
                    "Color".to_string(),
                    cx.theme().border,
                    cx.theme().muted,
                    color_picker_active,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseDownEvent, _window, cx| {
                        this.color_picker_bar_state.show_color_picker =
                            !this.color_picker_bar_state.show_color_picker;
                        cx.notify();
                    }),
                );
                div()
                    .flex()
                    .justify_end()
                    .child(color_button)
                    .child(jump_to_line_button)
                    .child(status_bar_right_item_factory(encoding, cx.theme().border))
            })
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::{Fulgur, SyncButtonState};
    use crate::fulgur::{
        languages::supported_languages::SupportedLanguage, settings::Settings,
        shared_state::SharedAppState, sync::synchronization::SynchronizationStatus,
        ui::components_utils::UTF_8, window_manager::WindowManager,
    };
    use gpui::{
        AppContext, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext,
        Window, div,
    };
    use gpui_component::input::Position;
    use parking_lot::Mutex;
    use std::{
        cell::RefCell,
        path::PathBuf,
        sync::Arc,
        time::{Duration, Instant},
    };

    struct EmptyView;

    impl Render for EmptyView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let mut settings = Settings::new();
            settings.editor_settings.watch_files = false;
            let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
            cx.set_global(SharedAppState::new(settings, pending_files));
            cx.set_global(WindowManager::new());
        });

        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        let window = cx
            .update(|cx| {
                cx.open_window(Default::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                    *fulgur_slot.borrow_mut() = Some(fulgur);
                    cx.new(|_| EmptyView)
                })
            })
            .expect("failed to open test window");

        let visual_cx = VisualTestContext::from_window(window.into(), cx);
        visual_cx.run_until_parked();
        let fulgur = fulgur_slot
            .into_inner()
            .expect("failed to capture Fulgur entity");
        (fulgur, visual_cx)
    }

    #[gpui::test]
    fn test_status_bar_reflects_active_editor_cursor_language_and_encoding(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let editor = this
                    .get_active_editor_tab_mut()
                    .expect("expected active editor tab");
                editor.language = SupportedLanguage::Rust;
                editor.encoding = "ISO-8859-1".to_string();
                editor.content.update(cx, |content, cx| {
                    content.set_value("first line\nsecond line", window, cx);
                    content.set_cursor_position(
                        Position {
                            line: 1,
                            character: 4,
                        },
                        window,
                        cx,
                    );
                });

                let (cursor, language, encoding) = this.status_bar_reflection_values(cx);
                assert_eq!(cursor.line, 1);
                assert_eq!(cursor.character, 4);
                assert_eq!(language, "Rust");
                assert_eq!(encoding, "ISO-8859-1");
            });
        });
    }

    #[gpui::test]
    fn test_status_bar_reflection_uses_defaults_without_active_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                this.active_tab_index = None;

                let (cursor, language, encoding) = this.status_bar_reflection_values(cx);
                assert_eq!(cursor.line, 0);
                assert_eq!(cursor.character, 0);
                assert!(language.is_empty());
                assert_eq!(encoding, UTF_8);
            });
        });
    }

    #[gpui::test]
    fn test_status_bar_sync_indicator_connected(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                *this.shared_state(cx).sync_state.connection_status.lock() =
                    SynchronizationStatus::Connected;
                *this.shared_state(cx).sync_state.connecting_since.lock() = None;

                let (state, show_spinner) = this.status_bar_sync_button_state(cx);
                assert_eq!(state, SyncButtonState::Connected);
                assert!(!show_spinner);
            });
        });
    }

    #[gpui::test]
    fn test_status_bar_sync_indicator_connecting_with_elapsed_delay(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                *this.shared_state(cx).sync_state.connection_status.lock() =
                    SynchronizationStatus::Connecting;
                *this.shared_state(cx).sync_state.connecting_since.lock() =
                    Some(Instant::now() - Duration::from_millis(600));

                let (state, show_spinner) = this.status_bar_sync_button_state(cx);
                assert_eq!(state, SyncButtonState::Connecting);
                assert!(show_spinner);
            });
        });
    }

    #[gpui::test]
    fn test_status_bar_sync_indicator_connecting_before_delay(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                *this.shared_state(cx).sync_state.connection_status.lock() =
                    SynchronizationStatus::Connecting;
                *this.shared_state(cx).sync_state.connecting_since.lock() =
                    Some(Instant::now() - Duration::from_millis(100));

                let (state, show_spinner) = this.status_bar_sync_button_state(cx);
                assert_eq!(state, SyncButtonState::Connecting);
                assert!(!show_spinner);
            });
        });
    }

    #[gpui::test]
    fn test_status_bar_sync_indicator_non_connected_maps_to_disconnected(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                *this.shared_state(cx).sync_state.connection_status.lock() =
                    SynchronizationStatus::AuthenticationFailed;
                *this.shared_state(cx).sync_state.connecting_since.lock() =
                    Some(Instant::now() - Duration::from_secs(2));

                let (state, show_spinner) = this.status_bar_sync_button_state(cx);
                assert_eq!(state, SyncButtonState::Disconnected);
                assert!(!show_spinner);
            });
        });
    }
}
