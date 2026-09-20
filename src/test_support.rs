//! Shared helpers for GPUI-backed tests.

use gpui_kit::App;

#[cfg(feature = "gpui-test-support")]
use crate::fulgur::{
    Fulgur, WindowInit, settings::Settings, shared_state::SharedAppState,
    window_manager::WindowManager,
};
#[cfg(feature = "gpui-test-support")]
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, Context, Entity, IntoElement, Render, TestAppContext,
    VisualTestContext, Window, WindowBounds, WindowId, WindowOptions, div, point, px, size,
};
#[cfg(feature = "gpui-test-support")]
use parking_lot::Mutex;
#[cfg(feature = "gpui-test-support")]
use std::{cell::RefCell, path::PathBuf, sync::Arc};

/// Initialize GPUI Kit for a test application context.
///
/// ### Arguments
/// - `cx`: The application context to initialize
pub fn init_test_app(cx: &mut App) {
    gpui_kit::init(cx);
    // Keeps simulated input deterministic
    cx.set_reduce_motion(true);
}

/// An inert view used to host `Fulgur` when a test does not render the real UI.
#[cfg(feature = "gpui-test-support")]
pub struct EmptyView;

#[cfg(feature = "gpui-test-support")]
impl Render for EmptyView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Window size used by every test window, large enough to lay out the full UI.
#[cfg(feature = "gpui-test-support")]
const TEST_WINDOW_SIZE: (f32, f32) = (1280.0, 800.0);

/// Build an OS-agnostic temporary test path.
///
/// ### Arguments
/// - `file_name`: The file name to append to the platform temp directory
///
/// ### Returns
/// - `PathBuf`: A path under `std::env::temp_dir()` suitable for cross-platform tests
#[cfg(feature = "gpui-test-support")]
#[must_use]
pub fn temp_test_path(file_name: &str) -> PathBuf {
    std::env::temp_dir().join(file_name)
}

/// Build the default settings every test window starts from.
///
/// ### Returns
/// - `Settings`: Default settings with file watching turned off
#[cfg(feature = "gpui-test-support")]
#[must_use]
pub fn test_settings() -> Settings {
    let mut settings = Settings::new();
    settings.editor_settings.watch_files = false;
    settings
}

/// Install the application globals every `Fulgur` window depends on.
///
/// ### Arguments
/// - `cx`: The test application context
#[cfg(feature = "gpui-test-support")]
pub fn setup_test_globals(cx: &mut TestAppContext) {
    setup_test_globals_with(cx, |_| {});
}

/// Install the application globals with test specific settings.
///
/// ### Arguments
/// - `cx`: The test application context
/// - `customize`: Applied to the default test settings before they become global
#[cfg(feature = "gpui-test-support")]
pub fn setup_test_globals_with(cx: &mut TestAppContext, customize: impl FnOnce(&mut Settings)) {
    cx.update(|cx| {
        init_test_app(cx);
        let mut settings = test_settings();
        customize(&mut settings);
        let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
        cx.set_global(SharedAppState::new(settings, pending_files, None, None));
        cx.set_global(WindowManager::new());
    });
}

/// Build the window options shared by every test window.
///
/// ### Returns
/// - `WindowOptions`: Options with fixed bounds so layout and hit testing are deterministic
#[cfg(feature = "gpui-test-support")]
fn test_window_options() -> WindowOptions {
    let (width, height) = TEST_WINDOW_SIZE;
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(width), px(height)),
        })),
        ..WindowOptions::default()
    }
}

/// Create the `Fulgur` root entity for a freshly opened test window.
///
/// ### Arguments
/// - `window`: The window being opened
/// - `cx`: The application context of the window being opened
///
/// ### Returns
/// - `Entity<Fulgur>`: An empty `Fulgur` bound to this window
#[cfg(feature = "gpui-test-support")]
fn new_fulgur(window: &mut Window, cx: &mut App) -> Entity<Fulgur> {
    let window_id = window.window_handle().window_id();
    Fulgur::new(window, cx, window_id, WindowInit::Empty)
}

/// Open a window hosting `Fulgur` under an inert view, assuming globals exist.
///
/// ### Arguments
/// - `cx`: The test application context
///
/// ### Returns
/// - `(Entity<Fulgur>, VisualTestContext)`: The created Fulgur entity and its visual test context
#[cfg(feature = "gpui-test-support")]
#[allow(clippy::missing_panics_doc)]
pub fn open_inert_window(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
    let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
    let window = cx
        .update(|cx| {
            cx.open_window(test_window_options(), |window, cx| {
                *fulgur_slot.borrow_mut() = Some(new_fulgur(window, cx));
                cx.new(|_| EmptyView)
            })
        })
        .expect("failed to open test window");
    let (fulgur, _handle, visual_cx) = finish_window(window.into(), fulgur_slot, cx);
    (fulgur, visual_cx)
}

/// Open a window rendering `Fulgur` inside a `Root`, assuming globals exist.
///
/// ### Arguments
/// - `cx`: The test application context
///
/// ### Returns
/// - `(Entity<Fulgur>, AnyWindowHandle, VisualTestContext)`: The Fulgur entity, its window handle
///   and the visual test context
#[cfg(feature = "gpui-test-support")]
#[allow(clippy::missing_panics_doc)]
pub fn open_rendered_window(
    cx: &mut TestAppContext,
) -> (Entity<Fulgur>, AnyWindowHandle, VisualTestContext) {
    let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
    let window = cx
        .update(|cx| {
            cx.open_window(test_window_options(), |window, cx| {
                let fulgur = new_fulgur(window, cx);
                *fulgur_slot.borrow_mut() = Some(fulgur.clone());
                cx.new(|cx| gpui_kit::component::Root::new(fulgur, window, cx))
            })
        })
        .expect("failed to open test window");
    finish_window(window.into(), fulgur_slot, cx)
}

/// Drain pending effects and unwrap the captured `Fulgur` entity.
///
/// ### Arguments
/// - `handle`: The handle of the window that was just opened
/// - `fulgur_slot`: The slot the window builder wrote its `Fulgur` entity into
/// - `cx`: The test application context
///
/// ### Returns
/// - `(Entity<Fulgur>, AnyWindowHandle, VisualTestContext)`: The Fulgur entity, the handle it was
///   given and the visual test context
#[cfg(feature = "gpui-test-support")]
fn finish_window(
    handle: AnyWindowHandle,
    fulgur_slot: RefCell<Option<Entity<Fulgur>>>,
    cx: &TestAppContext,
) -> (Entity<Fulgur>, AnyWindowHandle, VisualTestContext) {
    let visual_cx = VisualTestContext::from_window(handle, cx);
    visual_cx.run_until_parked();
    let fulgur = fulgur_slot
        .into_inner()
        .expect("failed to capture Fulgur entity");
    (fulgur, handle, visual_cx)
}

/// Install the test globals and open a window hosting `Fulgur` under an inert view.
///
/// ### Arguments
/// - `cx`: The test application context
///
/// ### Returns
/// - `(Entity<Fulgur>, VisualTestContext)`: The created Fulgur entity and its visual test context
#[cfg(feature = "gpui-test-support")]
pub fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
    setup_test_globals(cx);
    open_inert_window(cx)
}

/// Install the test globals and open a window rendering `Fulgur` inside a `Root`.
///
/// ### Arguments
/// - `cx`: The test application context
///
/// ### Returns
/// - `(Entity<Fulgur>, VisualTestContext)`: The created Fulgur entity and its visual test context
#[cfg(feature = "gpui-test-support")]
pub fn setup_fulgur_with_root(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
    let (fulgur, _handle, visual_cx) = open_fulgur_with_root(cx);
    (fulgur, visual_cx)
}

/// Install the test globals, render `Fulgur` and hand back its window handle.
///
/// ### Arguments
/// - `cx`: The test application context
///
/// ### Returns
/// - `(Entity<Fulgur>, AnyWindowHandle, VisualTestContext)`: The Fulgur entity, its window handle
///   and the visual test context
#[cfg(feature = "gpui-test-support")]
pub fn open_fulgur_with_root(
    cx: &mut TestAppContext,
) -> (Entity<Fulgur>, AnyWindowHandle, VisualTestContext) {
    setup_test_globals(cx);
    open_rendered_window(cx)
}

/// Open an additional `Fulgur` window without creating a visual test context.
///
/// ### Arguments
/// - `cx`: The test application context
///
/// ### Returns
/// - `(WindowId, Entity<Fulgur>)`: The new window's id and its Fulgur entity
#[cfg(feature = "gpui-test-support")]
#[allow(clippy::missing_panics_doc)]
pub fn open_window_with_fulgur(cx: &mut TestAppContext) -> (WindowId, Entity<Fulgur>) {
    let window_id_slot: RefCell<Option<WindowId>> = RefCell::new(None);
    let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
    cx.update(|cx| {
        cx.open_window(test_window_options(), |window, cx| {
            *window_id_slot.borrow_mut() = Some(window.window_handle().window_id());
            *fulgur_slot.borrow_mut() = Some(new_fulgur(window, cx));
            cx.new(|_| EmptyView)
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
