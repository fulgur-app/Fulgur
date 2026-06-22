use super::widgets::SyncButtonState;
use crate::fulgur::{
    Fulgur,
    languages::supported_languages::{SupportedLanguage, pretty_name},
    sync::synchronization::SynchronizationStatus,
    ui::components_utils::{EMPTY, UTF_8},
};
use gpui::Context;
use gpui_component::input::Position;
use std::time::{Duration, Instant};

/// Delay before showing the connecting spinner (to avoid flickering on fast connections)
const CONNECTING_SPINNER_DELAY: Duration = Duration::from_millis(500);

/// Cached status bar label strings
pub(crate) struct StatusBarCache {
    active_tab_index: Option<usize>,
    cursor_line: u32,
    cursor_character: u32,
    language: Option<SupportedLanguage>,
    encoding: String,
    pub(super) line_col: String,
    pub(super) language_label: String,
    pub(super) encoding_label: String,
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
    pub(super) fn status_bar_sync_button_state(
        &self,
        cx: &Context<Self>,
    ) -> (SyncButtonState, bool) {
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
    /// ### Parameters:
    /// - `cx`: The application context.
    ///
    /// ### Returns:
    /// - `Vec<(String, String)>`: Profile name and its human-readable status label.
    pub(super) fn sync_profiles_tooltip_data(&self, cx: &Context<Self>) -> Vec<(String, String)> {
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
                *state.connecting_since.lock() = Some(
                    Instant::now()
                        .checked_sub(Duration::from_millis(600))
                        .unwrap(),
                );

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
                *state.connecting_since.lock() = Some(
                    Instant::now()
                        .checked_sub(Duration::from_millis(100))
                        .unwrap(),
                );

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
                *state.connecting_since.lock() =
                    Some(Instant::now().checked_sub(Duration::from_secs(2)).unwrap());

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
