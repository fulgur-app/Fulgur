use crate::fulgur::{
    Fulgur, settings::Settings, shared_state::SharedAppState, state::StateDb,
    window_manager::WindowManager,
};
use gpui::{
    App, AppContext, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext,
    Window, WindowId, WindowOptions, div,
};
use parking_lot::Mutex;
use std::{cell::RefCell, path::PathBuf, sync::Arc};

/// A minimal root view for windows that never need the `gpui_component` layers.
pub struct EmptyView;

impl Render for EmptyView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Build the default settings used by GPUI tests.
///
/// ### Description
/// File watching is disabled so tests never start a real `notify` watcher on
/// temporary paths.
///
/// ### Returns
/// - `Settings`: Default settings with `editor_settings.watch_files` turned off
#[must_use]
pub fn test_settings() -> Settings {
    let mut settings = Settings::new();
    settings.editor_settings.watch_files = false;
    settings
}

/// Build an OS-agnostic temporary test path.
///
/// ### Arguments
/// - `file_name`: The file name to append to the platform temp directory
///
/// ### Returns
/// - `PathBuf`: A path under `std::env::temp_dir()` suitable for cross-platform tests
#[must_use]
pub fn temp_test_path(file_name: &str) -> PathBuf {
    std::env::temp_dir().join(file_name)
}

/// Initialize `gpui_component` and install the globals `Fulgur` requires.
///
/// ### Arguments
/// - `cx`: The application context to install the globals into
/// - `settings`: The settings seeded into `SharedAppState`
/// - `state_db`: The session-state database handed to the writer thread, or `None` to
///   run without session persistence
pub fn install_test_globals(cx: &mut App, settings: Settings, state_db: Option<StateDb>) {
    gpui_component::init(cx);
    let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
    cx.set_global(SharedAppState::new(settings, pending_files, None, state_db));
    cx.set_global(WindowManager::new());
}

/// Install the globals `Fulgur` requires, without opening a window.
///
/// ### Arguments
/// - `cx`: The test application context to install the globals into
pub fn setup_test_globals(cx: &mut TestAppContext) {
    cx.update(|cx| install_test_globals(cx, test_settings(), None));
}

/// Install the globals `Fulgur` requires, backed by an in-memory session-state database.
///
/// ### Arguments
/// - `cx`: The test application context to install the globals into
#[allow(clippy::missing_panics_doc)]
pub fn setup_test_globals_with_state_db(cx: &mut TestAppContext) {
    let state_db = StateDb::open_in_memory().expect("failed to open in-memory state database");
    cx.update(|cx| install_test_globals(cx, test_settings(), Some(state_db)));
}

/// Open a test window hosting `Fulgur` under a caller-provided root view.
///
/// ### Arguments
/// - `cx`: The test application context
/// - `build_root`: Builds the window root view from the freshly created `Fulgur` entity
///
/// ### Returns
/// - `(Entity<Fulgur>, VisualTestContext)`: The window's `Fulgur` entity and its visual context
fn open_fulgur_window<V, B>(
    cx: &mut TestAppContext,
    build_root: B,
) -> (Entity<Fulgur>, VisualTestContext)
where
    V: 'static + Render,
    B: FnOnce(Entity<Fulgur>, &mut Window, &mut App) -> Entity<V>,
{
    let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
    let window = cx
        .update(|cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let window_id = window.window_handle().window_id();
                let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                *fulgur_slot.borrow_mut() = Some(fulgur.clone());
                build_root(fulgur, window, cx)
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

/// Set up a test window hosting `Fulgur` under an `EmptyView` root.
///
/// ### Arguments
/// - `cx`: The test application context
///
/// ### Returns
/// - `(Entity<Fulgur>, VisualTestContext)`: The created `Fulgur` entity and visual test context
#[allow(clippy::missing_panics_doc)]
pub fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
    setup_fulgur_with_settings(cx, test_settings())
}

/// Set up a test window hosting `Fulgur` under an `EmptyView` root, with custom settings.
///
/// ### Arguments
/// - `cx`: The test application context
/// - `settings`: The settings seeded into `SharedAppState`
///
/// ### Returns
/// - `(Entity<Fulgur>, VisualTestContext)`: The created `Fulgur` entity and visual test context
#[allow(clippy::missing_panics_doc)]
pub fn setup_fulgur_with_settings(
    cx: &mut TestAppContext,
    settings: Settings,
) -> (Entity<Fulgur>, VisualTestContext) {
    cx.update(|cx| install_test_globals(cx, settings, None));
    open_fulgur_window(cx, |_fulgur, _window, cx| cx.new(|_| EmptyView))
}

/// Set up a test window hosting `Fulgur` inside a `gpui_component::Root`.
///
///
/// ### Arguments
/// - `cx`: The test application context
///
/// ### Returns
/// - `(Entity<Fulgur>, VisualTestContext)`: The created `Fulgur` entity and visual test context
#[allow(clippy::missing_panics_doc)]
pub fn setup_fulgur_with_root(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
    setup_fulgur_with_root_and_settings(cx, test_settings())
}

/// Set up a `gpui_component::Root`-mounted test window with custom settings.
///
/// ### Arguments
/// - `cx`: The test application context
/// - `settings`: The settings seeded into `SharedAppState`
///
/// ### Returns
/// - `(Entity<Fulgur>, VisualTestContext)`: The created `Fulgur` entity and visual test context
#[allow(clippy::missing_panics_doc)]
pub fn setup_fulgur_with_root_and_settings(
    cx: &mut TestAppContext,
    settings: Settings,
) -> (Entity<Fulgur>, VisualTestContext) {
    cx.update(|cx| install_test_globals(cx, settings, None));
    open_fulgur_window(cx, |fulgur, window, cx| {
        cx.new(|cx| gpui_component::Root::new(fulgur, window, cx))
    })
}

/// Open a window on already installed globals and return its id alongside its `Fulgur`.
///
/// ### Arguments
/// - `cx`: The test application context
/// - `build_root`: Builds the window root view from the freshly created `Fulgur` entity
///
/// ### Returns
/// - `(WindowId, Entity<Fulgur>)`: The new window's id and its `Fulgur` entity
fn open_identified_window<V, B>(
    cx: &mut TestAppContext,
    build_root: B,
) -> (WindowId, Entity<Fulgur>)
where
    V: 'static + Render,
    B: FnOnce(Entity<Fulgur>, &mut Window, &mut App) -> Entity<V>,
{
    let window_id_slot: RefCell<Option<WindowId>> = RefCell::new(None);
    let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
    cx.update(|cx| {
        cx.open_window(WindowOptions::default(), |window, cx| {
            let window_id = window.window_handle().window_id();
            let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
            *window_id_slot.borrow_mut() = Some(window_id);
            *fulgur_slot.borrow_mut() = Some(fulgur.clone());
            build_root(fulgur, window, cx)
        })
        .expect("failed to open test window");
    });
    (
        window_id_slot
            .into_inner()
            .expect("failed to capture test window id"),
        fulgur_slot
            .into_inner()
            .expect("failed to capture test Fulgur entity"),
    )
}

/// Open an additional `EmptyView`-rooted window on already installed globals.
///
/// ### Arguments
/// - `cx`: The test application context
///
/// ### Returns
/// - `(WindowId, Entity<Fulgur>)`: The new window's id and its `Fulgur` entity
#[allow(clippy::missing_panics_doc)]
pub fn open_window_with_fulgur(cx: &mut TestAppContext) -> (WindowId, Entity<Fulgur>) {
    open_identified_window(cx, |_fulgur, _window, cx| cx.new(|_| EmptyView))
}

/// Open an additional `gpui_component::Root`-mounted window on already installed globals.
///
/// ### Arguments
/// - `cx`: The test application context
///
/// ### Returns
/// - `(WindowId, Entity<Fulgur>)`: The new window's id and its `Fulgur` entity
#[allow(clippy::missing_panics_doc)]
pub fn open_window_with_fulgur_root(cx: &mut TestAppContext) -> (WindowId, Entity<Fulgur>) {
    open_identified_window(cx, |fulgur, window, cx| {
        cx.new(|cx| gpui_component::Root::new(fulgur, window, cx))
    })
}
