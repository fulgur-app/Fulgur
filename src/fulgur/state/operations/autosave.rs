use crate::fulgur::{Fulgur, shared_state::SharedAppState};
use gpui_kit::{App, Global, Task};
use std::time::Duration;

/// Quiet period after the last buffer change before the session is snapshotted.
const SESSION_AUTOSAVE_DEBOUNCE: Duration = Duration::from_millis(1500);

/// The single debounced session snapshot waiting to run.
///
/// A snapshot describes every window, so edits in several windows share one
/// timer. This stays apart from `SharedAppState` because every window observes
/// that global, and scheduling happens on every keystroke.
#[derive(Default)]
struct SessionAutosave {
    pending: Option<Task<()>>,
}

impl Global for SessionAutosave {}

/// Snapshot the session once buffers have stopped changing for a moment.
///
/// ### Arguments
/// - `cx`: The application context
pub(crate) fn schedule_session_autosave(cx: &mut App) {
    let Some(shared) = cx.try_global::<SharedAppState>() else {
        return;
    };
    if !shared.settings.app_settings.persist_unsaved_buffers
        || shared.state_writer.persistence_error().is_some()
    {
        return;
    }
    let task = cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(SESSION_AUTOSAVE_DEBOUNCE)
            .await;
        cx.update(Fulgur::autosave_session);
    });
    cx.default_global::<SessionAutosave>().pending = Some(task);
}

impl Fulgur {
    /// Hand a snapshot of the whole session to the state writer.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    fn autosave_session(cx: &mut App) {
        let Some(windows_state) = Self::build_session_snapshot(cx) else {
            log::debug!("No window registered at autosave, keeping the persisted session");
            return;
        };
        cx.global::<SharedAppState>()
            .state_writer
            .save_async(windows_state);
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::SESSION_AUTOSAVE_DEBOUNCE;
    use crate::fulgur::{
        Fulgur,
        editor_tab::{TabLocation, replace_editor_text},
        shared_state::SharedAppState,
        state::{StateDb, StateWriter, WindowsState},
        window_manager::WindowManager,
    };
    use gpui_kit::{BorrowAppContext, Entity, TestAppContext, VisualTestContext};
    use std::{path::Path, sync::Arc, time::Duration};
    use tempfile::TempDir;

    use crate::test_support::setup_fulgur_with_root as setup_fulgur;

    /// Point the state writer at a database file.
    ///
    /// ### Arguments
    /// - `cx`: The visual test context
    /// - `path`: The database file to write to
    fn install_writer(cx: &mut VisualTestContext, path: &Path) {
        let db = StateDb::open(path).expect("open the state database");
        cx.update(|_, cx| {
            cx.update_global::<SharedAppState, _>(|shared, _| {
                shared.state_writer = Arc::new(StateWriter::new(Some(db)));
            });
        });
    }

    /// Open a window registered in the manager, its writer bound to a temporary database.
    ///
    /// ### Arguments
    /// - `cx`: The test application context
    /// - `dir`: The directory holding the database
    ///
    /// ### Returns
    /// - `(Entity<Fulgur>, VisualTestContext)`: The window entity and its visual test context
    fn setup_session(
        cx: &mut TestAppContext,
        dir: &TempDir,
    ) -> (Entity<Fulgur>, VisualTestContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        install_writer(&mut visual_cx, &dir.path().join("state.db"));
        visual_cx.update(|window, cx| {
            let window_id = window.window_handle().window_id();
            let weak_fulgur = fulgur.downgrade();
            cx.update_global::<WindowManager, _>(|manager, _| {
                manager.register(window_id, weak_fulgur);
            });
        });
        (fulgur, visual_cx)
    }

    /// Replace the first tab's text the way a user edit does, emitting a change event.
    ///
    /// ### Arguments
    /// - `fulgur`: The window whose first tab is edited
    /// - `cx`: The visual test context
    /// - `text`: The new buffer text
    fn edit_first_tab(fulgur: &Entity<Fulgur>, cx: &mut VisualTestContext, text: &str) {
        cx.update(|window, cx| {
            let content = fulgur.read(cx).tabs[0]
                .read(cx)
                .as_editor()
                .expect("expected an editor tab")
                .content
                .clone();
            content.update(cx, |state, cx| {
                replace_editor_text(state, text, window, cx);
            });
        });
        cx.run_until_parked();
    }

    /// Let simulated time pass and run whatever became due.
    ///
    /// ### Arguments
    /// - `cx`: The visual test context
    /// - `duration`: The simulated time to advance
    fn advance(cx: &mut VisualTestContext, duration: Duration) {
        cx.background_executor.advance_clock(duration);
        cx.run_until_parked();
    }

    /// Flush the state writer to disk, then read back the persisted session.
    ///
    /// ### Arguments
    /// - `cx`: The visual test context
    /// - `path`: The database file
    ///
    /// ### Returns
    /// - `WindowsState`: The persisted session
    fn flush_and_load(cx: &mut VisualTestContext, path: &Path) -> WindowsState {
        cx.update(|_, cx| {
            cx.update_global::<SharedAppState, _>(|shared, _| {
                shared.state_writer = Arc::new(StateWriter::new(None));
            });
        });
        let session = StateDb::open(path)
            .expect("reopen the state database")
            .load()
            .expect("load the persisted session");
        install_writer(cx, path);
        session
    }

    /// Read the text of every persisted buffer.
    ///
    /// ### Arguments
    /// - `session`: The persisted session
    ///
    /// ### Returns
    /// - `Vec<String>`: The buffers that carry content
    fn buffer_texts(session: &WindowsState) -> Vec<String> {
        session
            .windows
            .iter()
            .flat_map(|window| &window.tabs)
            .filter_map(|tab| Some(tab.content.as_ref()?.to_text().into_owned()))
            .collect()
    }

    #[gpui_kit::test]
    fn edit_reaches_the_database_after_the_quiet_period_without_a_tab_event(
        cx: &mut TestAppContext,
    ) {
        let dir = TempDir::new().expect("create temp dir");
        let (fulgur, mut visual_cx) = setup_session(cx, &dir);

        edit_first_tab(&fulgur, &mut visual_cx, "typed before the crash");
        advance(&mut visual_cx, SESSION_AUTOSAVE_DEBOUNCE);

        let session = flush_and_load(&mut visual_cx, &dir.path().join("state.db"));
        assert_eq!(buffer_texts(&session), vec!["typed before the crash"]);
    }

    #[gpui_kit::test]
    fn each_edit_restarts_the_quiet_period(cx: &mut TestAppContext) {
        let dir = TempDir::new().expect("create temp dir");
        let state_path = dir.path().join("state.db");
        let (fulgur, mut visual_cx) = setup_session(cx, &dir);
        let half_the_period = SESSION_AUTOSAVE_DEBOUNCE / 2;

        edit_first_tab(&fulgur, &mut visual_cx, "first");
        advance(&mut visual_cx, half_the_period);
        edit_first_tab(&fulgur, &mut visual_cx, "second");
        advance(&mut visual_cx, half_the_period);
        assert!(
            flush_and_load(&mut visual_cx, &state_path)
                .windows
                .is_empty(),
            "no snapshot may run while edits keep coming"
        );

        advance(&mut visual_cx, half_the_period);
        let session = flush_and_load(&mut visual_cx, &state_path);
        assert_eq!(buffer_texts(&session), vec!["second"]);
    }

    #[gpui_kit::test]
    fn nothing_is_scheduled_when_unsaved_buffers_are_not_persisted(cx: &mut TestAppContext) {
        let dir = TempDir::new().expect("create temp dir");
        let (fulgur, mut visual_cx) = setup_session(cx, &dir);
        visual_cx.update(|_, cx| {
            cx.update_global::<SharedAppState, _>(|shared, _| {
                shared.settings.app_settings.persist_unsaved_buffers = false;
            });
        });

        edit_first_tab(&fulgur, &mut visual_cx, "must stay in memory");
        advance(&mut visual_cx, SESSION_AUTOSAVE_DEBOUNCE);

        let session = flush_and_load(&mut visual_cx, &dir.path().join("state.db"));
        assert!(session.windows.is_empty(), "no snapshot may be written");
    }

    #[gpui_kit::test]
    fn nothing_is_scheduled_for_a_large_buffer(cx: &mut TestAppContext) {
        let dir = TempDir::new().expect("create temp dir");
        let (fulgur, mut visual_cx) = setup_session(cx, &dir);
        visual_cx.update(|_, cx| {
            fulgur.update(cx, |this, cx| {
                this.update_active_editor_tab(cx, |editor_tab, _| {
                    editor_tab.large_file = true;
                });
            });
        });

        edit_first_tab(&fulgur, &mut visual_cx, "edit of a huge file");
        advance(&mut visual_cx, SESSION_AUTOSAVE_DEBOUNCE);

        let session = flush_and_load(&mut visual_cx, &dir.path().join("state.db"));
        assert!(session.windows.is_empty(), "no snapshot may be written");
    }

    #[gpui_kit::test]
    fn autosave_after_the_last_window_closed_keeps_the_persisted_session(cx: &mut TestAppContext) {
        let dir = TempDir::new().expect("create temp dir");
        let state_path = dir.path().join("state.db");
        let (fulgur, mut visual_cx) = setup_session(cx, &dir);

        edit_first_tab(&fulgur, &mut visual_cx, "saved by the close");
        visual_cx.update(|window, cx| {
            let window_id = window.window_handle().window_id();
            fulgur
                .read(cx)
                .save_state(cx, window)
                .expect("save the session");
            cx.update_global::<WindowManager, _>(|manager, _| manager.unregister(window_id));
        });
        advance(&mut visual_cx, SESSION_AUTOSAVE_DEBOUNCE);

        let session = flush_and_load(&mut visual_cx, &state_path);
        assert_eq!(
            buffer_texts(&session),
            vec!["saved by the close"],
            "a snapshot with no window must not wipe the session"
        );
    }

    #[gpui_kit::test]
    fn saving_a_file_clears_its_autosaved_buffer(cx: &mut TestAppContext) {
        let dir = TempDir::new().expect("create temp dir");
        let state_path = dir.path().join("state.db");
        let file_path = dir.path().join("notes.txt");
        let (fulgur, mut visual_cx) = setup_session(cx, &dir);
        visual_cx.update(|_, cx| {
            fulgur.update(cx, |this, cx| {
                this.update_active_editor_tab(cx, |editor_tab, _| {
                    editor_tab.location = TabLocation::Local(file_path.clone());
                });
            });
        });

        edit_first_tab(&fulgur, &mut visual_cx, "soon on disk");
        advance(&mut visual_cx, SESSION_AUTOSAVE_DEBOUNCE);
        let session = flush_and_load(&mut visual_cx, &state_path);
        assert_eq!(buffer_texts(&session), vec!["soon on disk"]);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| this.save_file(window, cx));
        });
        visual_cx.run_until_parked();
        advance(&mut visual_cx, SESSION_AUTOSAVE_DEBOUNCE);

        let session = flush_and_load(&mut visual_cx, &state_path);
        assert_eq!(
            session.windows[0].tabs.len(),
            1,
            "the tab itself must still be restored"
        );
        assert!(
            buffer_texts(&session).is_empty(),
            "a saved tab must not leave a stale dirty buffer behind for a crash to restore"
        );
    }
}
