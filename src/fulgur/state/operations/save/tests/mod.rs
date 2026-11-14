mod active_index;
mod conflicts;
mod read_only;
mod restore;
mod snapshot;

use crate::fulgur::{
    Fulgur,
    editor_tab::{EditorTab, TabLocation},
    shared_state::SharedAppState,
    state::persistence::{SerializedWindowBounds, TabState, WindowState, WindowsState},
    sync::{share::ShareOrigin, ssh::url::RemoteSpec},
};
use gpui_kit::{Entity, VisualTestContext};
use std::sync::Arc;
use time::macros::datetime;

/// Give the first tab a location and some dirty content, then snapshot the window.
pub(super) fn tab_states_with(
    fulgur: &Entity<Fulgur>,
    cx: &mut VisualTestContext,
    location: TabLocation,
    persist_unsaved_buffers: bool,
) -> Vec<TabState> {
    configured_tab_states(
        fulgur,
        cx,
        location,
        persist_unsaved_buffers,
        "dirty content",
        |_| {},
    )
}

/// Give the first tab a location, buffer text and extra editor state, then snapshot the window.
///
/// ### Arguments
/// - `fulgur`: The application entity whose first tab is configured
/// - `cx`: The visual test context containing the application window
/// - `location`: The location assigned to the first tab
/// - `persist_unsaved_buffers`: The value of the unsaved buffer persistence setting
/// - `text`: The buffer text written into the first tab
/// - `configure`: Extra changes applied to the first editor tab before the snapshot
///
/// ### Returns
/// - `Vec<TabState>`: The persisted tabs of the window snapshot
pub(super) fn configured_tab_states(
    fulgur: &Entity<Fulgur>,
    cx: &mut VisualTestContext,
    location: TabLocation,
    persist_unsaved_buffers: bool,
    text: &str,
    configure: impl FnOnce(&mut EditorTab),
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
                        content.set_value(text, window, cx);
                    });
                    configure(editor_tab);
                }
            });
        });
    });
    fulgur.read_with(cx, |this, cx| {
        this.build_window_state_without_bounds(cx).tabs
    })
}

/// Build the remote location used by the remote tab snapshot tests.
///
/// ### Returns
/// - `TabLocation`: A remote file on `example.com` opened as `alice`
pub(super) fn sample_remote_location() -> TabLocation {
    TabLocation::Remote(RemoteSpec {
        host: "example.com".to_string(),
        port: 22,
        user: Some("alice".to_string()),
        path: "/srv/remote.txt".to_string(),
        password_in_url: None,
    })
}

/// Build the origin of a received share for persistence tests.
///
/// ### Returns
/// - `ShareOrigin`: A share sent by "Work laptop" with a fixed date and size
pub(super) fn sample_share_origin() -> ShareOrigin {
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
pub(super) fn startup_snapshot(tab: TabState) -> WindowsState {
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
pub(super) fn restore_and_snapshot(
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
pub(super) fn assert_recovery_content(tab: &TabState, expected: &str) {
    let content = tab
        .content
        .as_ref()
        .map(|content| content.to_text().into_owned());
    assert_eq!(content.as_deref(), Some(expected));
}
