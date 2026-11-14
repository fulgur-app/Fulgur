use std::path::MAIN_SEPARATOR;
#[cfg(feature = "gpui-test-support")]
use tempfile::tempdir;

#[cfg(feature = "gpui-test-support")]
use crate::fulgur::ui::{
    dialogs::file_browser::PARENT_FOLDER_BUTTON_ID, menus::build_default_key_bindings,
};
#[cfg(feature = "gpui-test-support")]
use gpui_kit::component::input::{InputEvent, InputState};
#[cfg(feature = "gpui-test-support")]
use gpui_kit::{
    AppContext, Entity, Focusable, Modifiers, TestAppContext, VisualTestContext, WindowOptions,
};

/// Open a window hosting a path browser, its input set to a path and listed.
///
/// ### Arguments
/// - `cx`: The test application context
/// - `path`: The path to type in the browser input
///
/// ### Returns
/// - `(Entity<PathBrowser>, Entity<InputState>, VisualTestContext)`: The browser,
///   its input and the visual test context of its window
#[cfg(feature = "gpui-test-support")]
fn open_browser_at(
    cx: &mut TestAppContext,
    path: &str,
) -> (
    Entity<super::PathBrowser>,
    Entity<InputState>,
    VisualTestContext,
) {
    cx.update(gpui_kit::init);
    let window = cx.update(|cx| {
        cx.open_window(WindowOptions::default(), |window, cx| {
            cx.new(|cx| super::PathBrowser::new(window, cx))
        })
        .expect("failed to open test window")
    });
    let mut visual_cx = VisualTestContext::from_window(window.into(), cx);
    let browser = window
        .root(&mut visual_cx)
        .expect("failed to read root view from window");
    let input = browser.read_with(&visual_cx, |browser, _| browser.input.clone());
    visual_cx.update(|window, cx| {
        input.update(cx, |state, cx| {
            state.set_value(path, window, cx);
            cx.emit(InputEvent::Change);
        });
    });
    wait_for_refresh(&mut visual_cx);
    (browser, input, visual_cx)
}

/// Let the debounced listing run, then draw a frame so hit testing sees the result.
///
/// ### Arguments
/// - `visual_cx`: The visual test context of the browser window
#[cfg(feature = "gpui-test-support")]
fn wait_for_refresh(visual_cx: &mut VisualTestContext) {
    visual_cx
        .background_executor
        .advance_clock(super::PATH_BROWSER_REFRESH_DEBOUNCE);
    visual_cx.run_until_parked();
    visual_cx.update(|window, cx| window.draw(cx).clear(cx));
}

/// Read the browser input value and the names of the listed entries.
///
/// ### Arguments
/// - `browser`: The path browser
/// - `visual_cx`: The visual test context of the browser window
///
/// ### Returns
/// - `(String, Vec<String>)`: The input value and the listed entry names
#[cfg(feature = "gpui-test-support")]
fn browser_state(
    browser: &Entity<super::PathBrowser>,
    visual_cx: &VisualTestContext,
) -> (String, Vec<String>) {
    browser.read_with(visual_cx, |browser, cx| {
        (
            browser.input.read(cx).value().to_string(),
            browser
                .entries
                .iter()
                .map(|entry| entry.name.clone())
                .collect(),
        )
    })
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_gpui_parent_folder_button_lists_the_parent_folder(cx: &mut TestAppContext) {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let child_dir = temp_dir.path().join("child_dir");
    std::fs::create_dir(&child_dir).expect("failed to create child_dir fixture");
    let start = format!("{}{MAIN_SEPARATOR}", child_dir.to_string_lossy());
    let (browser, _input, mut visual_cx) = open_browser_at(cx, &start);

    let bounds = visual_cx
        .debug_bounds(PARENT_FOLDER_BUTTON_ID)
        .expect("the parent folder button should be rendered");
    visual_cx.simulate_click(bounds.center(), Modifiers::none());
    wait_for_refresh(&mut visual_cx);

    let (value, entries) = browser_state(&browser, &visual_cx);
    assert_eq!(
        value,
        format!("{}{MAIN_SEPARATOR}", temp_dir.path().to_string_lossy()),
        "the input must hold the parent folder"
    );
    assert_eq!(entries, ["child_dir"], "the parent folder must be listed");
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_gpui_alt_up_goes_to_the_parent_folder_and_undo_restores_the_path(cx: &mut TestAppContext) {
    let temp_dir = tempdir().expect("failed to create temp dir");
    std::fs::write(temp_dir.path().join("alpha.txt"), "alpha")
        .expect("failed to create alpha.txt fixture");
    let start = temp_dir.path().join("al").to_string_lossy().to_string();
    let (browser, input, mut visual_cx) = open_browser_at(cx, &start);
    visual_cx.update(|window, cx| {
        cx.bind_keys(build_default_key_bindings());
        let focus_handle = input.read(cx).focus_handle(cx);
        window.focus(&focus_handle, cx);
    });

    visual_cx.simulate_keystrokes("alt-up");
    wait_for_refresh(&mut visual_cx);
    let (value, entries) = browser_state(&browser, &visual_cx);
    assert_eq!(
        value,
        format!("{}{MAIN_SEPARATOR}", temp_dir.path().to_string_lossy()),
        "alt-up must drop the partial name only"
    );
    assert_eq!(entries, ["alpha.txt"]);

    visual_cx.simulate_keystrokes("secondary-z");
    let (value, _) = browser_state(&browser, &visual_cx);
    assert_eq!(value, start, "undo must restore the previous path");
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_gpui_input_change_updates_entries_list(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let temp_dir = tempdir().expect("failed to create temp dir");
    std::fs::create_dir(temp_dir.path().join("alpha_dir"))
        .expect("failed to create alpha_dir fixture");
    std::fs::write(temp_dir.path().join("alpha.txt"), "alpha")
        .expect("failed to create alpha.txt fixture");
    std::fs::write(temp_dir.path().join("beta.txt"), "beta")
        .expect("failed to create beta.txt fixture");

    let window = cx.update(|cx| {
        cx.open_window(WindowOptions::default(), |window, cx| {
            cx.new(|cx| super::PathBrowser::new(window, cx))
        })
        .expect("failed to open test window")
    });
    let mut visual_cx = VisualTestContext::from_window(window.into(), cx);
    let browser = window
        .root(&mut visual_cx)
        .expect("failed to read root view from window");
    let input_entity: Entity<InputState> =
        browser.read_with(&visual_cx, |browser, _| browser.input.clone());

    let filter_input = temp_dir.path().join("al").to_string_lossy().to_string();
    visual_cx.update(|window, cx| {
        input_entity.update(cx, |state, cx| {
            state.set_value(&filter_input, window, cx);
            cx.emit(InputEvent::Change);
        });
    });
    visual_cx
        .background_executor
        .advance_clock(super::PATH_BROWSER_REFRESH_DEBOUNCE);
    visual_cx.run_until_parked();

    let entries: Vec<(String, bool)> = browser.read_with(&visual_cx, |browser, _| {
        browser
            .entries
            .iter()
            .map(|entry| (entry.name.clone(), entry.is_dir))
            .collect::<Vec<_>>()
    });

    assert!(
        !entries.is_empty(),
        "entries should be populated after input change"
    );
    assert!(
        entries
            .iter()
            .all(|(name, _)| name.to_lowercase().starts_with("al")),
        "every entry should match the typed prefix"
    );
    assert!(
        entries.iter().any(|(name, _)| name == "alpha_dir"),
        "directory match should be present"
    );
    assert!(
        entries.iter().any(|(name, _)| name == "alpha.txt"),
        "file match should be present"
    );

    let first_file_idx = entries.iter().position(|(_, is_dir)| !*is_dir);
    let last_dir_idx = entries.iter().rposition(|(_, is_dir)| *is_dir);
    if let (Some(first_file), Some(last_dir)) = (first_file_idx, last_dir_idx) {
        assert!(
            last_dir < first_file,
            "directories should be listed before files"
        );
    }
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_gpui_input_change_to_invalid_path_clears_entries(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let temp_dir = tempdir().expect("failed to create temp dir");
    std::fs::write(temp_dir.path().join("alpha.txt"), "alpha")
        .expect("failed to create alpha.txt fixture");

    let window = cx.update(|cx| {
        cx.open_window(WindowOptions::default(), |window, cx| {
            cx.new(|cx| super::PathBrowser::new(window, cx))
        })
        .expect("failed to open test window")
    });
    let mut visual_cx = VisualTestContext::from_window(window.into(), cx);
    let browser = window
        .root(&mut visual_cx)
        .expect("failed to read root view from window");
    let input_entity: Entity<InputState> =
        browser.read_with(&visual_cx, |browser, _| browser.input.clone());

    let valid_input = temp_dir.path().join("al").to_string_lossy().to_string();
    visual_cx.update(|window, cx| {
        input_entity.update(cx, |state, cx| {
            state.set_value(&valid_input, window, cx);
            cx.emit(InputEvent::Change);
        });
    });
    visual_cx
        .background_executor
        .advance_clock(super::PATH_BROWSER_REFRESH_DEBOUNCE);
    visual_cx.run_until_parked();
    let has_entries = browser.read_with(&visual_cx, |browser, _| !browser.entries.is_empty());
    assert!(has_entries, "valid input should populate entries");

    let invalid_input = temp_dir
        .path()
        .join("missing_parent")
        .join("x")
        .to_string_lossy()
        .to_string();
    visual_cx.update(|window, cx| {
        input_entity.update(cx, |state, cx| {
            state.set_value(&invalid_input, window, cx);
            cx.emit(InputEvent::Change);
        });
    });
    visual_cx
        .background_executor
        .advance_clock(super::PATH_BROWSER_REFRESH_DEBOUNCE);
    visual_cx.run_until_parked();

    let is_empty = browser.read_with(&visual_cx, |browser, _| browser.entries.is_empty());
    assert!(is_empty, "invalid path should clear entries");
}
