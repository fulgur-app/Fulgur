use super::super::persistence::{
    SerializedRemoteSpec, SerializedWindowBounds, TabContent, TabState, WindowState, WindowsState,
    get_file_modified_time,
};
use crate::fulgur::{Fulgur, editor_tab::TabLocation, tab::Tab, ui::components_utils::UNTITLED};
use gpui_kit::{App, Window};

impl Fulgur {
    /// Save the current app state to disk (saves all windows in multi-window mode)
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `window`: The window to save (needed for window bounds)
    ///
    /// ### Errors
    /// - Returns an error if the state cannot be persisted (no state database
    ///   available, or the transaction failed).
    ///
    /// ### Returns
    /// - `Ok(())`: If the app state was saved successfully
    /// - `Err(anyhow::Error)`: If the app state could not be saved
    pub fn save_state(&self, cx: &App, window: &Window) -> anyhow::Result<()> {
        log::debug!("Saving application state...");
        let windows_state = self.build_windows_state(cx, window);
        let window_count = windows_state.windows.len();
        let tab_count = self.tabs.len();
        let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
        shared.state_writer.save_blocking(windows_state)?;
        log::debug!(
            "Application state saved successfully ({window_count} windows, {tab_count} tabs in this window)"
        );
        Ok(())
    }

    /// Save the state of every other window, dropping this one from the database.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Errors
    /// - Returns an error if the state cannot be persisted (no state database
    ///   available, or the transaction failed).
    ///
    /// ### Returns
    /// - `Ok(())`: If the remaining windows were saved successfully
    /// - `Err(anyhow::Error)`: If the state could not be saved
    pub fn save_state_without_this_window(&self, cx: &App) -> anyhow::Result<()> {
        log::debug!(
            "Saving application state without window {:?}...",
            self.window_id
        );
        let windows_state = self.collect_windows_state(cx, None);
        let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
        shared.state_writer.save_blocking(windows_state)?;
        Ok(())
    }

    /// Persist the session when the application quits, whatever triggered the quit.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub fn register_app_quit_state_save(cx: &mut App) {
        cx.on_app_quit(|cx| {
            Self::save_state_on_app_quit(cx);
            async {}
        })
        .detach();
    }

    /// Snapshot every registered window and write the session synchronously.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    fn save_state_on_app_quit(cx: &mut App) {
        let window_manager = cx.global::<crate::fulgur::window_manager::WindowManager>();
        let entities: Vec<_> = window_manager
            .get_all_window_ids()
            .into_iter()
            .filter_map(|window_id| {
                let entity = window_manager.get_window(window_id)?.upgrade()?;
                Some((window_id, entity))
            })
            .collect();
        if entities.is_empty() {
            log::debug!("No window registered at quit, keeping the persisted session");
            return;
        }
        let handles = cx.windows();
        let mut windows_state = WindowsState { windows: vec![] };
        for (window_id, entity) in entities {
            let with_live_bounds = handles
                .iter()
                .find(|handle| handle.window_id() == window_id)
                .and_then(|handle| {
                    handle
                        .update(cx, |_, window, cx| {
                            entity.read(cx).build_window_state(cx, window)
                        })
                        .ok()
                });
            let snapshot = with_live_bounds
                .unwrap_or_else(|| entity.read(cx).build_window_state_without_bounds(cx));
            windows_state.windows.push(snapshot);
        }
        let window_count = windows_state.windows.len();
        let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
        match shared.state_writer.save_blocking(windows_state) {
            Ok(()) => log::info!("Application state saved on quit ({window_count} windows)"),
            Err(e) => log::error!("Failed to save app state on quit: {e}"),
        }
    }

    /// Save the current app state to disk without blocking the UI thread.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `window`: The window to save (needed for window bounds)
    pub fn save_state_async(&self, cx: &App, window: &Window) {
        log::debug!("Saving application state (async)...");
        let windows_state = self.build_windows_state(cx, window);
        let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
        shared.state_writer.save_async(windows_state);
    }

    /// Assemble the full multi-window state snapshot for persistence.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `window`: The current window (needed for its bounds)
    ///
    /// ### Returns
    /// - `WindowsState`: The snapshot of all open windows
    fn build_windows_state(&self, cx: &App, window: &Window) -> WindowsState {
        self.collect_windows_state(cx, Some(window))
    }

    /// Assemble a multi-window state snapshot, optionally leaving this window out.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `current_window`: This window, needed for its bounds; `None` omits it
    ///
    /// ### Returns
    /// - `WindowsState`: The snapshot of the registered windows
    fn collect_windows_state(&self, cx: &App, current_window: Option<&Window>) -> WindowsState {
        let window_manager = cx.global::<crate::fulgur::window_manager::WindowManager>();
        let mut windows_state = WindowsState { windows: vec![] };
        let current_window_id = self.window_id;
        let all_window_ids = window_manager.get_all_window_ids();
        for window_id in &all_window_ids {
            if *window_id == current_window_id {
                if let Some(window) = current_window {
                    windows_state
                        .windows
                        .push(self.build_window_state(cx, window));
                }
            } else if let Some(weak_entity) = window_manager.get_window(*window_id)
                && let Some(entity) = weak_entity.upgrade()
            {
                windows_state
                    .windows
                    .push(entity.read(cx).build_window_state_without_bounds(cx));
            }
        }
        windows_state
    }

    /// Build tab states for all tabs in this window
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Vec<TabState>`: The tab states for all tabs
    fn build_tab_states(&self, cx: &App) -> Vec<TabState> {
        let persist_unsaved = self.settings.app_settings.persist_unsaved_buffers;
        let mut tab_states = Vec::new();
        for tab in &self.tabs {
            if let Some(editor_tab) = tab.read(cx).as_editor() {
                let tab_state = match &editor_tab.location {
                    TabLocation::Local(path) => {
                        if persist_unsaved
                            && editor_tab.content_differs_from_original(cx)
                            && !editor_tab.content_too_large_to_persist(cx)
                        {
                            let current_content = editor_tab.content.read(cx).text().clone();
                            TabState {
                                tab_id: editor_tab.id.0,
                                title: editor_tab.title.to_string(),
                                log_view: editor_tab.log_view,
                                color_tag: editor_tab.color_tag.map(|c| c.key().to_string()),
                                file_path: Some(path.clone()),
                                content: Some(TabContent::Rope(current_content)),
                                last_saved: get_file_modified_time(path),
                                remote: None,
                                share: None,
                            }
                        } else {
                            TabState {
                                tab_id: editor_tab.id.0,
                                title: editor_tab.title.to_string(),
                                log_view: editor_tab.log_view,
                                color_tag: editor_tab.color_tag.map(|c| c.key().to_string()),
                                file_path: Some(path.clone()),
                                content: None,
                                last_saved: None,
                                remote: None,
                                share: None,
                            }
                        }
                    }
                    TabLocation::Remote(remote_spec) => {
                        let content = if persist_unsaved
                            && editor_tab.content_differs_from_original(cx)
                            && !editor_tab.content_too_large_to_persist(cx)
                        {
                            Some(TabContent::Rope(editor_tab.content.read(cx).text().clone()))
                        } else {
                            None
                        };
                        TabState {
                            tab_id: editor_tab.id.0,
                            title: editor_tab.title.to_string(),
                            log_view: editor_tab.log_view,
                            color_tag: editor_tab.color_tag.map(|c| c.key().to_string()),
                            file_path: None,
                            content,
                            last_saved: None,
                            remote: Some(SerializedRemoteSpec::from_remote_spec(remote_spec)),
                            share: None,
                        }
                    }
                    TabLocation::Untitled | TabLocation::Shared(_) => {
                        if !persist_unsaved {
                            log::debug!(
                                "Not persisting untitled tab '{}': unsaved buffer persistence is disabled",
                                editor_tab.title
                            );
                            continue;
                        }
                        if editor_tab.content_too_large_to_persist(cx) {
                            log::warn!(
                                "Not persisting untitled tab '{}': content exceeds the large-file threshold",
                                editor_tab.title
                            );
                            continue;
                        }
                        let current_content =
                            TabContent::Rope(editor_tab.content.read(cx).text().clone());
                        if current_content.is_empty() && editor_tab.title.starts_with(UNTITLED) {
                            continue;
                        }
                        TabState {
                            tab_id: editor_tab.id.0,
                            title: editor_tab.title.to_string(),
                            log_view: editor_tab.log_view,
                            color_tag: editor_tab.color_tag.map(|c| c.key().to_string()),
                            file_path: None,
                            content: Some(current_content),
                            last_saved: None,
                            remote: None,
                            share: editor_tab.location.share_origin().cloned(),
                        }
                    }
                };
                tab_states.push(tab_state);
            }
        }
        tab_states
    }

    /// Compute the active tab index relative to the editor-only tab list for state persistence.
    ///
    /// Preview tabs are not saved, so the persisted active index must refer to an editor tab.
    /// If the active tab is a preview tab, the index of its source editor tab is returned.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `tab_states`: The editor tabs that will actually be persisted
    ///
    /// ### Returns
    /// - `Some(usize)`: the active editor tab index
    /// - `None`: if the active tab is Settings or its editor is not persisted
    fn active_editor_index_for_state(&self, cx: &App, tab_states: &[TabState]) -> Option<usize> {
        let active = self.active_tab_index(cx)?;
        let active_tab = self.tabs.get(active)?.read(cx);
        let editor_tab_id = match active_tab {
            Tab::Editor(et) => et.id,
            Tab::MarkdownPreview(pt) => pt.source_tab_id,
            Tab::Settings(_) => return None,
        };
        tab_states
            .iter()
            .position(|tab_state| tab_state.tab_id == editor_tab_id.0)
    }

    /// Build `WindowState` for this window without window bounds (for cross-window saves)
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `WindowState`: The `WindowState` for this window (with cached bounds)
    pub fn build_window_state_without_bounds(&self, cx: &App) -> WindowState {
        let window_bounds = self.cached_window_bounds.clone().unwrap_or_default();
        let tabs = self.build_tab_states(cx);
        let active_tab_index = self.active_editor_index_for_state(cx, &tabs);
        WindowState {
            window_id: self.persistent_window_id,
            tabs,
            active_tab_index,
            window_bounds,
        }
    }

    /// Build `WindowState` for this window (with window bounds)
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `window`: The window (needed for bounds)
    ///
    /// ### Returns
    /// - `WindowState`: The `WindowState` for this window
    pub fn build_window_state(&self, cx: &App, window: &Window) -> WindowState {
        let display_id = window
            .display(cx)
            .and_then(|d| u32::try_from(u64::from(d.id())).ok());
        let window_bounds =
            SerializedWindowBounds::from_gpui_bounds(window.window_bounds(), display_id);
        let tabs = self.build_tab_states(cx);
        let active_tab_index = self.active_editor_index_for_state(cx, &tabs);
        WindowState {
            window_id: self.persistent_window_id,
            tabs,
            active_tab_index,
            window_bounds,
        }
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {

    use crate::fulgur::{
        Fulgur,
        editor_tab::TabLocation,
        shared_state::SharedAppState,
        state::persistence::{
            SerializedRemoteSpec, SerializedWindowBounds, TabContent, TabState, WindowState,
            WindowsState,
        },
        sync::share::ShareOrigin,
        ui::components_utils::UNTITLED,
    };
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};
    use time::macros::datetime;

    use std::{fs, sync::Arc};
    use tempfile::TempDir;

    use crate::test_support::setup_fulgur_with_root as setup_fulgur;

    /// Give the first tab a location and some dirty content, then snapshot the window.
    fn tab_states_with(
        fulgur: &Entity<Fulgur>,
        cx: &mut VisualTestContext,
        location: TabLocation,
        persist_unsaved_buffers: bool,
    ) -> Vec<TabState> {
        cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.settings.app_settings.persist_unsaved_buffers = persist_unsaved_buffers;
                let tab = this
                    .tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone();
                tab.update(cx, |tab, cx| {
                    if let Some(editor_tab) = tab.as_editor_mut() {
                        editor_tab.location = location;
                        editor_tab.content.update(cx, |content, cx| {
                            content.set_value("dirty content", window, cx);
                        });
                    }
                });
            });
        });
        fulgur.read_with(cx, |this, cx| {
            this.build_window_state_without_bounds(cx).tabs
        })
    }

    /// Build the origin of a received share for persistence tests.
    ///
    /// ### Returns
    /// - `ShareOrigin`: A share sent by "Work laptop" with a fixed date and size
    fn sample_share_origin() -> ShareOrigin {
        ShareOrigin {
            source_device_name: Some("Work laptop".to_string()),
            shared_at: Some(datetime!(2026-09-22 12:30:00 UTC)),
            size_bytes: 13,
        }
    }

    /// Wrap one persisted tab in the startup snapshot shape used by restoration.
    ///
    /// ### Arguments
    /// - `tab`: The persisted tab to place in the only restored window
    ///
    /// ### Returns
    /// - `WindowsState`: A single-window startup snapshot containing `tab`
    fn startup_snapshot(tab: TabState) -> WindowsState {
        WindowsState {
            windows: vec![WindowState {
                window_id: 42,
                tabs: vec![tab],
                active_tab_index: Some(0),
                window_bounds: SerializedWindowBounds::default(),
            }],
        }
    }

    /// Restore a startup snapshot and immediately capture the next persisted window state.
    ///
    /// ### Arguments
    /// - `fulgur`: The application entity whose tabs should be restored
    /// - `cx`: The visual test context containing the application window
    /// - `state`: The startup snapshot to restore
    ///
    /// ### Returns
    /// - `WindowState`: The window snapshot produced immediately after restoration
    fn restore_and_snapshot(
        fulgur: &Entity<Fulgur>,
        cx: &mut VisualTestContext,
        state: WindowsState,
    ) -> WindowState {
        cx.update(|window, cx| {
            let restore_state = Arc::clone(&cx.global::<SharedAppState>().restore_state);
            *restore_state.lock() = state.windows.into_iter().map(Some).collect();
            fulgur.update(cx, |this, cx| {
                this.settings.app_settings.persist_unsaved_buffers = true;
                this.load_state(window, cx, 0);
            });
        });
        fulgur.read_with(cx, crate::fulgur::Fulgur::build_window_state_without_bounds)
    }

    /// Assert that a persisted tab still carries the expected recovery text.
    ///
    /// ### Arguments
    /// - `tab`: The persisted tab whose optional recovery content is inspected
    /// - `expected`: The recovery text expected in `tab`
    fn assert_recovery_content(tab: &TabState, expected: &str) {
        let content = tab
            .content
            .as_ref()
            .map(|content| content.to_text().into_owned());
        assert_eq!(content.as_deref(), Some(expected));
    }

    #[gpui_kit::test]
    fn dirty_file_tab_persists_content_when_setting_is_enabled(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let tabs = tab_states_with(
            &fulgur,
            &mut visual_cx,
            TabLocation::Local("/tmp/notes.txt".into()),
            true,
        );
        assert_eq!(tabs.len(), 1);
        assert!(
            tabs[0].content.is_some(),
            "unsaved content must be persisted when the setting is enabled"
        );
    }

    #[gpui_kit::test]
    fn active_index_ignores_tabs_omitted_from_snapshot(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        let snapshot = visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.settings.app_settings.persist_unsaved_buffers = true;
                this.open_settings(window, cx);
                this.new_tab(window, cx);

                let active_tab = this
                    .active_tab_entity(cx)
                    .expect("expected the new editor tab to be active");
                active_tab.update(cx, |tab, cx| {
                    tab.as_editor_mut()
                        .expect("expected an editor tab")
                        .content
                        .update(cx, |content, cx| {
                            content.set_value("persisted active tab", window, cx);
                        });
                });

                this.build_window_state_without_bounds(cx)
            })
        });

        assert_eq!(snapshot.tabs.len(), 1);
        assert_eq!(snapshot.active_tab_index, Some(0));
    }

    #[gpui_kit::test]
    fn restore_keeps_active_tab_identity_when_an_earlier_tab_is_skipped(cx: &mut TestAppContext) {
        let temp_dir = TempDir::new().expect("create temp dir");
        let state = WindowsState {
            windows: vec![WindowState {
                window_id: 42,
                tabs: vec![
                    TabState {
                        tab_id: 10,
                        title: "missing.txt".to_string(),
                        file_path: Some(temp_dir.path().join("missing.txt")),
                        content: None,
                        last_saved: None,
                        remote: None,
                        log_view: false,
                        color_tag: None,
                        share: None,
                    },
                    TabState {
                        tab_id: 11,
                        title: "active.txt".to_string(),
                        file_path: None,
                        content: Some(TabContent::from("active content")),
                        last_saved: None,
                        remote: None,
                        log_view: false,
                        color_tag: None,
                        share: None,
                    },
                    TabState {
                        tab_id: 12,
                        title: "other.txt".to_string(),
                        file_path: None,
                        content: Some(TabContent::from("other content")),
                        last_saved: None,
                        remote: None,
                        log_view: false,
                        color_tag: None,
                        share: None,
                    },
                ],
                active_tab_index: Some(1),
                window_bounds: SerializedWindowBounds::default(),
            }],
        };
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        let restored = restore_and_snapshot(&fulgur, &mut visual_cx, state);

        assert_eq!(
            restored
                .tabs
                .iter()
                .map(|tab| tab.tab_id)
                .collect::<Vec<_>>(),
            vec![11, 12]
        );
        assert_eq!(restored.active_tab_index, Some(0));
    }

    #[gpui_kit::test]
    fn restore_consumes_the_window_slot_and_keeps_other_slots(cx: &mut TestAppContext) {
        let untouched_window = WindowState {
            window_id: 43,
            tabs: vec![TabState {
                tab_id: 20,
                title: UNTITLED.to_string(),
                file_path: None,
                content: Some(TabContent::from("payload of a window not yet opened")),
                last_saved: None,
                remote: None,
                log_view: false,
                color_tag: None,
                share: None,
            }],
            active_tab_index: Some(0),
            window_bounds: SerializedWindowBounds::default(),
        };
        let mut state = startup_snapshot(TabState {
            tab_id: 10,
            title: UNTITLED.to_string(),
            file_path: None,
            content: Some(TabContent::from("restored payload")),
            last_saved: None,
            remote: None,
            log_view: false,
            color_tag: None,
            share: None,
        });
        state.windows.push(untouched_window);
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        let restored = restore_and_snapshot(&fulgur, &mut visual_cx, state);
        assert_recovery_content(&restored.tabs[0], "restored payload");

        let slots = visual_cx.update(|_, cx| {
            let restore_state = Arc::clone(&cx.global::<SharedAppState>().restore_state);
            let slots = restore_state.lock();
            (
                slots.len(),
                slots[0].is_none(),
                slots[1].as_ref().map(|window| window.window_id),
            )
        });
        assert_eq!(
            slots,
            (2, true, Some(43)),
            "the restored slot must be released while the other window keeps its payload and index"
        );

        let restored_again = visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.load_state(window, cx, 0);
                this.build_window_state_without_bounds(cx)
            })
        });
        assert_eq!(
            restored_again
                .tabs
                .iter()
                .map(|tab| tab.tab_id)
                .collect::<Vec<_>>(),
            vec![10],
            "a consumed slot must leave the window as it is instead of restoring again"
        );
    }

    #[gpui_kit::test]
    fn restored_local_recovery_survives_repeated_snapshots(cx: &mut TestAppContext) {
        let temp_dir = TempDir::new().expect("create temp dir");
        let path = temp_dir.path().join("notes.txt");
        fs::write(&path, "saved on disk").expect("write saved file");
        let restored = TabState {
            tab_id: 7,
            title: "notes.txt".to_string(),
            file_path: Some(path.clone()),
            content: Some(TabContent::from("recovered local edits")),
            last_saved: None,
            remote: None,
            log_view: false,
            color_tag: None,
            share: None,
        };
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        let first = restore_and_snapshot(&fulgur, &mut visual_cx, startup_snapshot(restored));
        assert_recovery_content(&first.tabs[0], "recovered local edits");

        let second = restore_and_snapshot(
            &fulgur,
            &mut visual_cx,
            WindowsState {
                windows: vec![first],
            },
        );
        assert_recovery_content(&second.tabs[0], "recovered local edits");
        assert_eq!(fs::read_to_string(path).unwrap(), "saved on disk");
    }

    #[gpui_kit::test]
    fn restored_remote_recovery_survives_repeated_snapshots(cx: &mut TestAppContext) {
        let restored = TabState {
            tab_id: 8,
            title: "remote.txt".to_string(),
            file_path: None,
            content: Some(TabContent::from("recovered remote edits")),
            last_saved: None,
            remote: Some(SerializedRemoteSpec {
                host: "example.com".to_string(),
                port: 22,
                user: "alice".to_string(),
                path: "/srv/remote.txt".to_string(),
            }),
            log_view: false,
            color_tag: None,
            share: None,
        };
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        let first = restore_and_snapshot(&fulgur, &mut visual_cx, startup_snapshot(restored));
        assert_recovery_content(&first.tabs[0], "recovered remote edits");

        let second = restore_and_snapshot(
            &fulgur,
            &mut visual_cx,
            WindowsState {
                windows: vec![first],
            },
        );
        assert_recovery_content(&second.tabs[0], "recovered remote edits");
    }

    #[gpui_kit::test]
    fn dirty_file_tab_persists_path_only_when_setting_is_disabled(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let tabs = tab_states_with(
            &fulgur,
            &mut visual_cx,
            TabLocation::Local("/tmp/notes.txt".into()),
            false,
        );
        assert_eq!(tabs.len(), 1, "the tab itself must still be restored");
        assert!(
            tabs[0].content.is_none(),
            "unsaved content must not be persisted when the setting is disabled"
        );
        assert!(tabs[0].file_path.is_some());
    }

    #[gpui_kit::test]
    fn untitled_tab_is_dropped_when_setting_is_disabled(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let tabs = tab_states_with(&fulgur, &mut visual_cx, TabLocation::Untitled, false);
        assert!(
            tabs.is_empty(),
            "an untitled tab carries nothing but its unsaved content, so it must be dropped"
        );
    }

    #[gpui_kit::test]
    fn untitled_tab_is_persisted_when_setting_is_enabled(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let tabs = tab_states_with(&fulgur, &mut visual_cx, TabLocation::Untitled, true);
        assert_eq!(tabs.len(), 1);
        assert!(tabs[0].content.is_some());
    }

    #[gpui_kit::test]
    fn shared_tab_persists_its_origin_when_setting_is_enabled(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let origin = sample_share_origin();
        let tabs = tab_states_with(
            &fulgur,
            &mut visual_cx,
            TabLocation::Shared(origin.clone()),
            true,
        );
        assert_eq!(tabs.len(), 1);
        assert!(tabs[0].content.is_some());
        assert!(tabs[0].file_path.is_none());
        assert_eq!(tabs[0].share.as_ref(), Some(&origin));
    }

    #[gpui_kit::test]
    fn saved_shared_tab_no_longer_persists_its_origin(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let tabs = tab_states_with(
            &fulgur,
            &mut visual_cx,
            TabLocation::Local("/tmp/received.txt".into()),
            true,
        );
        assert_eq!(tabs.len(), 1);
        assert!(
            tabs[0].share.is_none(),
            "a shared tab saved to a file must go back to the regular file state"
        );
    }

    #[gpui_kit::test]
    fn restored_shared_tab_keeps_its_origin(cx: &mut TestAppContext) {
        let origin = sample_share_origin();
        let restored = TabState {
            tab_id: 9,
            title: "received.md".to_string(),
            file_path: None,
            content: Some(TabContent::from("# received")),
            last_saved: None,
            remote: None,
            log_view: false,
            color_tag: None,
            share: Some(origin.clone()),
        };
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        let snapshot = restore_and_snapshot(&fulgur, &mut visual_cx, startup_snapshot(restored));

        assert_recovery_content(&snapshot.tabs[0], "# received");
        assert_eq!(snapshot.tabs[0].share.as_ref(), Some(&origin));
    }
}
