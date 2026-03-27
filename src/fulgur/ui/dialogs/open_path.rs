use std::ops::DerefMut;
use std::path::PathBuf;

use gpui::*;
use gpui_component::{
    WindowExt, button::ButtonVariant, dialog::DialogButtonProps, notification::NotificationType,
};

use super::path_browser::PathBrowser;
use crate::fulgur::Fulgur;

/// Validate a raw path string entered by the user for the open-file dialog.
///
/// ### Arguments
/// - `path_str`: The raw path string to validate (may include leading/trailing whitespace)
///
/// ### Returns
/// - `Ok(PathBuf)`: a canonicalised `PathBuf` when the string names an existing file
/// - `Err(SharedString)`: a human-readable error message for any other case
fn validate_open_path(path_str: &str) -> Result<PathBuf, SharedString> {
    let trimmed = path_str.trim();
    if trimmed.is_empty() {
        return Err(SharedString::from("Please enter a file path"));
    }
    let path = PathBuf::from(trimmed);
    if !path.exists() {
        return Err(SharedString::from(format!(
            "Path does not exist: {}",
            trimmed
        )));
    }
    if !path.is_file() {
        return Err(SharedString::from(format!(
            "Path is not a file: {}",
            trimmed
        )));
    }
    Ok(path)
}

impl Fulgur {
    pub fn show_open_from_path_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity().clone();
        let path_browser = cx.new(|cx| PathBrowser::new(window, cx));
        let input = path_browser.read(cx).input().clone();
        let input_clone = input.clone();
        window.open_alert_dialog(cx.deref_mut(), move |modal, window, cx| {
            let focus_handle = input.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
            let entity_ok = entity.clone();
            let input_ok = input_clone.clone();
            let path_browser = path_browser.clone();
            modal
                .title(div().text_size(px(16.)).child("Open file from path..."))
                .keyboard(true)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text("Cancel")
                        .cancel_variant(ButtonVariant::Secondary)
                        .ok_text("Open")
                        .ok_variant(ButtonVariant::Primary),
                )
                .overlay_closable(false)
                .close_button(false)
                .child(path_browser)
                .on_ok(move |_, window: &mut Window, cx| {
                    let path_str = input_ok.read(cx).value().to_string();
                    match validate_open_path(&path_str) {
                        Ok(path) => {
                            entity_ok.update(cx, |this, cx| {
                                this.do_open_file(window, cx, path);
                            });
                            true
                        }
                        Err(msg) => {
                            window.push_notification((NotificationType::Error, msg), cx);
                            false
                        }
                    }
                })
                .on_cancel(|_, _, _| true)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::validate_open_path;
    use core::prelude::v1::test;
    use tempfile::TempDir;

    fn make_temp_file(dir: &TempDir, name: &str, content: &str) -> std::path::PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, content).expect("failed to write temp file");
        path
    }

    // ========== validate_open_path tests ==========

    #[test]
    fn test_validate_open_path_empty_returns_error() {
        assert!(validate_open_path("").is_err());
    }

    #[test]
    fn test_validate_open_path_whitespace_only_returns_error() {
        assert!(validate_open_path("   ").is_err());
    }

    #[test]
    fn test_validate_open_path_nonexistent_path_returns_error() {
        let result = validate_open_path("/definitely_not_a_real_path_abc123/file.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_open_path_directory_returns_error() {
        let dir = TempDir::new().expect("failed to create temp dir");
        let result = validate_open_path(dir.path().to_str().unwrap());
        assert!(result.is_err(), "a directory path should not be accepted");
    }

    #[test]
    fn test_validate_open_path_valid_file_returns_pathbuf() {
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = make_temp_file(&dir, "hello.txt", "hi");
        let result = validate_open_path(path.to_str().unwrap());
        assert!(result.is_ok(), "an existing file path should be accepted");
        assert_eq!(result.unwrap(), path);
    }

    #[test]
    fn test_validate_open_path_trims_whitespace() {
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = make_temp_file(&dir, "trim.txt", "data");
        let padded = format!("  {}  ", path.to_str().unwrap());
        let result = validate_open_path(&padded);
        assert!(
            result.is_ok(),
            "leading/trailing whitespace should be trimmed before validation"
        );
    }

    // ========== show_open_from_path_dialog smoke test ==========

    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::{
        Fulgur, settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
    };
    #[cfg(feature = "gpui-test-support")]
    use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
    #[cfg(feature = "gpui-test-support")]
    use parking_lot::Mutex;
    #[cfg(feature = "gpui-test-support")]
    use std::{cell::RefCell, rc::Rc, sync::Arc};

    #[cfg(feature = "gpui-test-support")]
    fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
        cx.update(gpui_component::init);
        cx.update(|cx| {
            cx.set_global(SharedAppState::new(
                Settings::new(),
                Arc::new(Mutex::new(Vec::new())),
            ));
            cx.set_global(WindowManager::new());
        });
        let fulgur_slot: Rc<RefCell<Option<Entity<Fulgur>>>> = Rc::new(RefCell::new(None));
        let slot = Rc::clone(&fulgur_slot);
        let window = cx
            .update(|cx| {
                cx.open_window(Default::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                    *slot.borrow_mut() = Some(fulgur.clone());
                    cx.new(|cx| gpui_component::Root::new(fulgur, window, cx))
                })
            })
            .expect("failed to open test window");
        let fulgur = fulgur_slot
            .borrow_mut()
            .take()
            .expect("expected fulgur entity");
        let visual_cx = VisualTestContext::from_window(window.into(), cx);
        (fulgur, visual_cx)
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_show_open_from_path_dialog_does_not_panic(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.show_open_from_path_dialog(window, cx);
            });
        });
        // If we reach this point, the dialog opened without panicking
    }
}
