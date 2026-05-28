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
use gpui::{
    Animation, AnimationExt, Context, Div, Hsla, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, StatefulInteractiveElement, Styled, div, prelude::FluentBuilder,
};
use gpui_component::{
    ActiveTheme, Icon, StyledExt, h_flex, input::Position, tooltip::Tooltip, v_flex,
};
use std::f32::consts::PI;
use std::time::{Duration, Instant};

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
                    |icon, delta| icon.rotate(gpui::radians(delta * 2.0 * PI)),
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

/// Cached status bar label strings
pub(crate) struct StatusBarCache {
    active_tab_index: Option<usize>,
    cursor_line: u32,
    cursor_character: u32,
    language: Option<SupportedLanguage>,
    encoding: String,
    line_col: String,
    language_label: String,
    encoding_label: String,
}

impl Default for StatusBarCache {
    fn default() -> Self {
        Self {
            active_tab_index: Some(usize::MAX),
            cursor_line: 0,
            cursor_character: 0,
            language: None,
            encoding: String::new(),
            line_col: String::new(),
            language_label: String::new(),
            encoding_label: String::new(),
        }
    }
}

impl Fulgur {
    /// Refresh the cached status-bar label strings when the active tab's cursor, language, or
    /// encoding has changed since the last render.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub(crate) fn refresh_status_bar_labels(&mut self, cx: &Context<Self>) {
        let (cursor_pos, language, encoding) = match self.active_tab_index {
            Some(index) => {
                if let Some(editor_tab) = self.tabs[index].as_editor() {
                    let cursor = editor_tab.content.read(cx).cursor_position();
                    let enc = editor_tab.encoding.clone();
                    (cursor, Some(editor_tab.language), enc)
                } else {
                    (
                        Position::default(),
                        Some(SupportedLanguage::Plain),
                        EMPTY.to_string(),
                    )
                }
            }
            None => (Position::default(), None, String::new()),
        };

        // Return early when the inputs match the previously cached values.
        if self.status_bar_cache.active_tab_index == self.active_tab_index
            && self.status_bar_cache.cursor_line == cursor_pos.line
            && self.status_bar_cache.cursor_character == cursor_pos.character
            && self.status_bar_cache.language == language
            && self.status_bar_cache.encoding == encoding
        {
            return;
        }

        let language_label = match &language {
            Some(lang) => pretty_name(lang),
            None => EMPTY.to_string(),
        };
        let encoding_label = match self.active_tab_index {
            Some(_) => encoding.clone(),
            None => UTF_8.to_string(),
        };

        let cache = &mut self.status_bar_cache;
        cache.active_tab_index = self.active_tab_index;
        cache.cursor_line = cursor_pos.line;
        cache.cursor_character = cursor_pos.character;
        cache.language = language;
        cache.encoding = encoding;
        cache.line_col = format!(
            "Ln {}, Col {}",
            cursor_pos.line + 1,
            cursor_pos.character + 1
        );
        cache.language_label = language_label;
        cache.encoding_label = encoding_label;
    }

    /// Aggregate sync button state across all active profiles.
    ///
    /// Priority order: Connected beats Connecting beats Disconnected. The spinner is
    /// shown once the earliest connecting-since timestamp has exceeded
    /// `CONNECTING_SPINNER_DELAY`.
    ///
    /// ### Parameters:
    /// - `cx`: The application context.
    ///
    /// ### Returns:
    /// - `(SyncButtonState, bool)`: The aggregated state and whether to show the spinner.
    fn status_bar_sync_button_state(&self, cx: &Context<Self>) -> (SyncButtonState, bool) {
        let profiles = &self.settings.app_settings.synchronization_settings.profiles;
        let shared = Fulgur::shared_state(cx);
        let sync_states = shared.sync_states.read();

        let mut any_connected = false;
        let mut any_connecting = false;
        let mut earliest_connecting_since: Option<Instant> = None;

        for profile in profiles.iter().filter(|p| p.is_active) {
            let Some(state) = sync_states.get(&profile.id) else {
                continue;
            };
            match *state.connection_status.lock() {
                SynchronizationStatus::Connected => any_connected = true,
                SynchronizationStatus::Connecting => {
                    any_connecting = true;
                    if let Some(since) = *state.connecting_since.lock() {
                        earliest_connecting_since = Some(match earliest_connecting_since {
                            None => since,
                            Some(existing) if since < existing => since,
                            Some(existing) => existing,
                        });
                    }
                }
                _ => {}
            }
        }

        if any_connected {
            (SyncButtonState::Connected, false)
        } else if any_connecting {
            let show = earliest_connecting_since
                .is_some_and(|since| since.elapsed() >= CONNECTING_SPINNER_DELAY);
            (SyncButtonState::Connecting, show)
        } else {
            (SyncButtonState::Disconnected, false)
        }
    }

    /// Collect per-profile tooltip data for all active profiles.
    ///
    /// Returns one `(name, label)` pair per profile whose `is_active` flag is set.
    /// An empty vec is returned when there are no active profiles.
    ///
    /// ### Parameters:
    /// - `cx`: The application context.
    ///
    /// ### Returns:
    /// - `Vec<(String, String)>`: Profile name and its human-readable status label.
    fn sync_profiles_tooltip_data(&self, cx: &Context<Self>) -> Vec<(String, String)> {
        let profiles = &self.settings.app_settings.synchronization_settings.profiles;
        let shared = Fulgur::shared_state(cx);
        let sync_states = shared.sync_states.read();

        profiles
            .iter()
            .filter(|p| p.is_active)
            .map(|profile| {
                let state = sync_states.get(&profile.id);
                let label = state.map_or("Inactive", |s| s.connection_status.lock().label());
                let device_name = state.and_then(|s| s.device_name.lock().clone());
                let name = match device_name {
                    Some(device) if !device.is_empty() => {
                        format!("{} @ {}", device, profile.name)
                    }
                    _ => profile.name.clone(),
                };
                (name, label.to_string())
            })
            .collect()
    }

    /// Render the status bar
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered status bar element
    pub fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let line_col = self.status_bar_cache.line_col.clone();
        let language = self.status_bar_cache.language_label.clone();
        let encoding = self.status_bar_cache.encoding_label.clone();
        let jump_to_line_button =
            status_bar_button_factory(line_col, cx.theme().border, cx.theme().muted);
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
        let profile_statuses = self.sync_profiles_tooltip_data(cx);
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
        .id("sync-status-button")
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, window, cx| {
                this.open_share_file_sheet(window, cx);
            }),
        )
        .when(!profile_statuses.is_empty(), move |this| {
            this.tooltip(move |window, cx| {
                let rows = profile_statuses.clone();
                Tooltip::element(move |_, cx| {
                    let mut container = v_flex().gap_1().py_1().px_1();
                    for (name, label) in &rows {
                        container = container.child(
                            h_flex()
                                .gap_2()
                                .child(div().text_sm().font_semibold().child(format!("{name}:")))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(label.clone()),
                                ),
                        );
                    }
                    container
                })
                .build(window, cx)
            })
        });
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
        languages::supported_languages::SupportedLanguage,
        settings::{ServerProfile, Settings},
        shared_state::SharedAppState,
        sync::synchronization::SynchronizationStatus,
        ui::components_utils::UTF_8,
        window_manager::WindowManager,
    };
    use gpui::{
        AppContext, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext,
        Window, WindowOptions, div,
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
                cx.open_window(WindowOptions::default(), |window, cx| {
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

    /// Set up a `Fulgur` instance with one active sync profile seeded into settings.
    ///
    /// Returns the entity, the visual context, and the profile id so tests can
    /// address the correct per-profile `SyncState` via `sync_state_for`.
    fn setup_fulgur_with_active_profile(
        cx: &mut TestAppContext,
    ) -> (Entity<Fulgur>, VisualTestContext, String) {
        let mut profile = ServerProfile::new("Test");
        profile.is_active = true;
        let profile_id = profile.id.clone();

        cx.update(|cx| {
            gpui_component::init(cx);
            let mut settings = Settings::new();
            settings.editor_settings.watch_files = false;
            settings
                .app_settings
                .synchronization_settings
                .is_synchronization_activated = true;
            settings
                .app_settings
                .synchronization_settings
                .profiles
                .push(profile);
            let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
            cx.set_global(SharedAppState::new(settings, pending_files));
            cx.set_global(WindowManager::new());
        });

        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        let window = cx
            .update(|cx| {
                cx.open_window(WindowOptions::default(), |window, cx| {
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
        (fulgur, visual_cx, profile_id)
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

                this.refresh_status_bar_labels(cx);
                assert_eq!(this.status_bar_cache.cursor_line, 1);
                assert_eq!(this.status_bar_cache.cursor_character, 4);
                assert_eq!(this.status_bar_cache.language_label, "Rust");
                assert_eq!(this.status_bar_cache.encoding_label, "ISO-8859-1");
                assert_eq!(this.status_bar_cache.line_col, "Ln 2, Col 5");
            });
        });
    }

    #[gpui::test]
    fn test_status_bar_cache_uses_defaults_without_active_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                this.active_tab_index = None;

                this.refresh_status_bar_labels(cx);
                assert_eq!(this.status_bar_cache.cursor_line, 0);
                assert_eq!(this.status_bar_cache.cursor_character, 0);
                assert!(this.status_bar_cache.language_label.is_empty());
                assert_eq!(this.status_bar_cache.encoding_label, UTF_8);
                assert_eq!(this.status_bar_cache.line_col, "Ln 1, Col 1");
            });
        });
    }

    #[gpui::test]
    fn test_status_bar_sync_indicator_connected(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx, profile_id) = setup_fulgur_with_active_profile(cx);

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let state = Fulgur::shared_state(cx).sync_state_for(&profile_id);
                *state.connection_status.lock() = SynchronizationStatus::Connected;
                *state.connecting_since.lock() = None;

                let (state, show_spinner) = this.status_bar_sync_button_state(cx);
                assert_eq!(state, SyncButtonState::Connected);
                assert!(!show_spinner);
            });
        });
    }

    #[gpui::test]
    fn test_status_bar_sync_indicator_connecting_with_elapsed_delay(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx, profile_id) = setup_fulgur_with_active_profile(cx);

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let state = Fulgur::shared_state(cx).sync_state_for(&profile_id);
                *state.connection_status.lock() = SynchronizationStatus::Connecting;
                *state.connecting_since.lock() = Some(Instant::now() - Duration::from_millis(600));

                let (btn_state, show_spinner) = this.status_bar_sync_button_state(cx);
                assert_eq!(btn_state, SyncButtonState::Connecting);
                assert!(show_spinner);
            });
        });
    }

    #[gpui::test]
    fn test_status_bar_sync_indicator_connecting_before_delay(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx, profile_id) = setup_fulgur_with_active_profile(cx);

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let state = Fulgur::shared_state(cx).sync_state_for(&profile_id);
                *state.connection_status.lock() = SynchronizationStatus::Connecting;
                *state.connecting_since.lock() = Some(Instant::now() - Duration::from_millis(100));

                let (btn_state, show_spinner) = this.status_bar_sync_button_state(cx);
                assert_eq!(btn_state, SyncButtonState::Connecting);
                assert!(!show_spinner);
            });
        });
    }

    #[gpui::test]
    fn test_status_bar_sync_indicator_non_connected_maps_to_disconnected(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx, profile_id) = setup_fulgur_with_active_profile(cx);

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let state = Fulgur::shared_state(cx).sync_state_for(&profile_id);
                *state.connection_status.lock() = SynchronizationStatus::AuthenticationFailed;
                *state.connecting_since.lock() = Some(Instant::now() - Duration::from_secs(2));

                let (btn_state, show_spinner) = this.status_bar_sync_button_state(cx);
                assert_eq!(btn_state, SyncButtonState::Disconnected);
                assert!(!show_spinner);
            });
        });
    }

    #[gpui::test]
    fn test_status_bar_sync_aggregates_connected_wins_over_connecting(cx: &mut TestAppContext) {
        let mut profile_a = ServerProfile::new("Server A");
        profile_a.is_active = true;
        let id_a = profile_a.id.clone();
        let mut profile_b = ServerProfile::new("Server B");
        profile_b.is_active = true;
        let id_b = profile_b.id.clone();

        cx.update(|cx| {
            gpui_component::init(cx);
            let mut settings = Settings::new();
            settings.editor_settings.watch_files = false;
            settings
                .app_settings
                .synchronization_settings
                .is_synchronization_activated = true;
            settings
                .app_settings
                .synchronization_settings
                .profiles
                .push(profile_a);
            settings
                .app_settings
                .synchronization_settings
                .profiles
                .push(profile_b);
            let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
            cx.set_global(SharedAppState::new(settings, pending_files));
            cx.set_global(WindowManager::new());
        });

        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        let window = cx
            .update(|cx| {
                cx.open_window(WindowOptions::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                    *fulgur_slot.borrow_mut() = Some(fulgur);
                    cx.new(|_| EmptyView)
                })
            })
            .expect("failed to open test window");

        let mut visual_cx = VisualTestContext::from_window(window.into(), cx);
        visual_cx.run_until_parked();
        let fulgur = fulgur_slot.into_inner().expect("failed to capture Fulgur");

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let shared = Fulgur::shared_state(cx);
                *shared.sync_state_for(&id_a).connection_status.lock() =
                    SynchronizationStatus::Connected;
                *shared.sync_state_for(&id_b).connection_status.lock() =
                    SynchronizationStatus::Connecting;

                let (btn_state, _) = this.status_bar_sync_button_state(cx);
                assert_eq!(btn_state, SyncButtonState::Connected);
            });
        });
    }
}
