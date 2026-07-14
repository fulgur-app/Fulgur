mod actions;
mod state;
mod view;

pub use state::ShareSheetState;

use crate::fulgur::{
    Fulgur,
    settings::{ProfileId, ServerProfile},
};
use actions::spawn_profile_device_fetch;
use gpui::{Context, ParentElement, SharedString, Styled, Window, px};
use gpui_component::{
    WindowExt, notification::NotificationType, scroll::ScrollableElement, v_flex,
};
use std::{sync::Arc, time::Duration};
use view::{make_device_list, render_footer};

impl Fulgur {
    /// Return the list of profile ids whose devices should populate the share sheet.
    ///
    /// ### Returns
    /// - `Vec<ProfileId>`: Active only profile ids when the master sync switch is on, in declaration order.
    fn collect_active_profiles(&self) -> Vec<ServerProfile> {
        if !self
            .settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated
        {
            return Vec::new();
        }
        self.settings
            .app_settings
            .synchronization_settings
            .profiles
            .iter()
            .filter(|p| p.is_active)
            .cloned()
            .collect()
    }

    /// Open the share file sheet immediately and fetch each active profile's devices in the background.
    ///
    /// ### Arguments
    /// - `window`: The window context.
    /// - `cx`: The application context.
    pub fn open_share_file_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_profiles = self.collect_active_profiles();
        if active_profiles.is_empty() {
            log::warn!("Share aborted: no active profile or master sync is off");
            window.push_notification(
                (
                    NotificationType::Warning,
                    SharedString::from(
                        "No active sync profile. Enable a profile in Settings, Synchronization.",
                    ),
                ),
                cx,
            );
            return;
        }
        if let Some(previous) = self.share_sheet_state.take() {
            previous
                .active
                .store(false, std::sync::atomic::Ordering::Release);
        }
        let state = Arc::new(ShareSheetState::new(active_profiles.clone()));
        self.share_sheet_state = Some(Arc::clone(&state));
        let shared = Fulgur::shared_state(cx);
        let http_agent = Arc::clone(&shared.http_agent);
        for profile in active_profiles {
            let sync_state = shared.sync_state_for(&profile.id);
            spawn_profile_device_fetch(
                profile,
                Arc::clone(&state),
                sync_state,
                Arc::clone(&http_agent),
            );
        }
        Self::start_share_sheet_render_pump(Arc::clone(&state), window, cx);
        Self::show_share_sheet(state, window, cx);
    }

    /// Drive periodic re-renders while the share sheet is open and at least one profile is still loading.
    ///
    /// ### Arguments
    /// - `state`: Shared sheet state.
    /// - `window`: The window context.
    /// - `cx`: The application context.
    fn start_share_sheet_render_pump(
        state: Arc<ShareSheetState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity();
        window
            .spawn(cx, async move |async_cx| {
                loop {
                    async_cx
                        .background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    if !state.active.load(std::sync::atomic::Ordering::Acquire) {
                        return;
                    }
                    let _ = async_cx.update(|_, cx| {
                        entity.update(cx, |_, cx| cx.notify());
                    });
                    if state.all_settled() {
                        return;
                    }
                }
            })
            .detach();
    }

    /// Drain pending SSE restart requests posted by share-sheet device fetches.
    ///
    /// ### Arguments
    /// - `_window`: The window context.
    /// - `cx`: The application context.
    pub fn process_pending_share_sheet(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.share_sheet_state.as_ref().map(Arc::clone) else {
            return;
        };
        let to_restart: Vec<ProfileId> = std::mem::take(&mut *state.pending_sse_restarts.lock());
        for profile_id in to_restart {
            self.restart_sse_connection_for(&profile_id, cx);
        }
    }

    /// Open the share sheet UI for the given shared state. Each profile section reads live state on every render.
    ///
    /// ### Arguments
    /// - `state`: Shared sheet state.
    /// - `window`: The window context.
    /// - `cx`: The application context.
    fn show_share_sheet(state: Arc<ShareSheetState>, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity();
        let viewport_height = window.viewport_size().height;
        window.open_sheet(cx, move |sheet, _window, cx| {
            #[cfg(target_os = "linux")]
            let sheet_overhead = px(200.0);
            #[cfg(not(target_os = "linux"))]
            let sheet_overhead = px(150.0);
            let max_height = px((viewport_height - sheet_overhead).into());
            let state_for_list = Arc::clone(&state);
            let state_for_footer = Arc::clone(&state);
            sheet
                .title("Share with...")
                .size(px(400.))
                .overlay(true)
                .child(
                    v_flex()
                        .overflow_y_scrollbar()
                        .gap_2()
                        .h(max_height)
                        .child(make_device_list(&state_for_list, cx)),
                )
                .footer(render_footer(state_for_footer, entity.clone()))
        });
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::state::ProfileFetchState;
    use super::{Fulgur, ShareSheetState};
    use crate::fulgur::{
        settings::{ServerProfile, Settings},
        shared_state::SharedAppState,
        sync::share::Device,
        window_manager::WindowManager,
    };
    use gpui::{AppContext, Entity, TestAppContext, VisualTestContext, WindowOptions};
    use parking_lot::Mutex;
    use std::{cell::RefCell, path::PathBuf, sync::Arc};

    /// Initialize globals and open a test window with a Root-mounted `Fulgur`.
    fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let mut settings = Settings::new();
            settings.editor_settings.watch_files = false;
            let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
            cx.set_global(SharedAppState::new(settings, pending_files, None));
            cx.set_global(WindowManager::new());
        });
        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        let window = cx
            .update(|cx| {
                cx.open_window(WindowOptions::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                    *fulgur_slot.borrow_mut() = Some(fulgur.clone());
                    cx.new(|cx| gpui_component::Root::new(fulgur, window, cx))
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

    fn make_device(id: &str) -> Device {
        Device {
            id: id.to_string(),
            name: format!("{id}-name"),
            device_type: "desktop".to_string(),
            public_key: Some("age1dummypublickey".to_string()),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            expires_at: "2025-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_share_sheet_state_starts_with_every_profile_loading() {
        let profile_a = ServerProfile::new("A");
        let profile_b = ServerProfile::new("B");
        let id_a = profile_a.id.clone();
        let id_b = profile_b.id.clone();
        let state = ShareSheetState::new(vec![profile_a, profile_b]);
        let map = state.per_profile.read();
        assert!(matches!(map.get(&id_a), Some(ProfileFetchState::Loading)));
        assert!(matches!(map.get(&id_b), Some(ProfileFetchState::Loading)));
        drop(map);
        assert!(!state.all_settled());
    }

    #[test]
    fn test_share_sheet_state_all_settled_when_loaded_or_failed() {
        let profile_a = ServerProfile::new("A");
        let profile_b = ServerProfile::new("B");
        let id_a = profile_a.id.clone();
        let id_b = profile_b.id.clone();
        let state = ShareSheetState::new(vec![profile_a, profile_b]);
        state.per_profile.write().insert(
            id_a,
            ProfileFetchState::Loaded(Arc::new(vec![make_device("d1")])),
        );
        state
            .per_profile
            .write()
            .insert(id_b, ProfileFetchState::Failed("nope".to_string()));
        assert!(state.all_settled());
    }

    #[test]
    fn test_share_sheet_state_not_settled_with_lingering_loading() {
        let profile_a = ServerProfile::new("A");
        let profile_b = ServerProfile::new("B");
        let id_a = profile_a.id.clone();
        let state = ShareSheetState::new(vec![profile_a, profile_b]);
        state
            .per_profile
            .write()
            .insert(id_a, ProfileFetchState::Loaded(Arc::new(vec![])));
        assert!(!state.all_settled());
    }

    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
    fn test_process_pending_share_sheet_drains_sse_restart_queue(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let profile = ServerProfile::new("queued");
                let id = profile.id.clone();
                this.settings
                    .app_settings
                    .synchronization_settings
                    .profiles
                    .push(profile.clone());
                let state = Arc::new(ShareSheetState::new(vec![profile]));
                state.pending_sse_restarts.lock().push(id.clone());
                this.share_sheet_state = Some(Arc::clone(&state));
                this.process_pending_share_sheet(window, cx);
                assert!(
                    state.pending_sse_restarts.lock().is_empty(),
                    "queue must be drained on each render"
                );
            });
        });
    }

    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
    fn test_process_pending_share_sheet_no_state_is_a_noop(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.share_sheet_state = None;
                this.process_pending_share_sheet(window, cx);
            });
        });
    }
}
