use std::ops::DerefMut;

use gpui::{AppContext, Context, Focusable, ParentElement, SharedString, Styled, Window, div, px};
use gpui_component::{
    WindowExt, button::ButtonVariant, dialog::DialogButtonProps, input::Input,
    notification::NotificationType,
};

use crate::fulgur::{
    Fulgur,
    files::file_operations::RemoteBrowseResult,
    sync::ssh::{REMOTE_ROOT_PATH, credentials::SshCredKey, url::parse_remote_url},
    ui::dialogs::remote_path_browser::{
        BrowserDialogTitle, RemotePathBrowser, RemotePathBrowserConnection,
    },
};

impl Fulgur {
    /// Show the open remote file dialog.
    ///
    /// ### Arguments
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    pub fn show_open_remote_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity().clone();
        let remembered_url = self.last_failed_remote_open_url.clone().unwrap_or_default();
        let input = cx.new(|cx| {
            gpui_component::input::InputState::new(window, cx)
                .placeholder("ssh://user@host/path/to/file")
                .default_value(remembered_url.clone())
        });
        let input_for_ok = input.clone();
        window.open_alert_dialog(cx.deref_mut(), move |modal, window, cx| {
            let focus_handle = input.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
            let entity_ok = entity.clone();
            let input_ok = input_for_ok.clone();
            modal
                .title(div().text_size(px(16.)).child("Open remote file..."))
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
                .child(Input::new(&input))
                .on_ok(move |_, window: &mut Window, cx| {
                    let url = input_ok.read(cx).value().to_string();
                    match parse_remote_url(&url) {
                        Ok(spec) => {
                            entity_ok.update(cx, |_, cx| {
                                // Defer remote-auth dialog opening until this URL dialog has fully closed.
                                cx.defer_in(window, move |this, window, cx| {
                                    this.do_open_remote_file(window, cx, spec);
                                });
                            });
                            true
                        }
                        Err(err) => {
                            window.push_notification(
                                (
                                    NotificationType::Error,
                                    SharedString::from(err.user_message()),
                                ),
                                cx,
                            );
                            false
                        }
                    }
                })
                .on_cancel(|_, _, _| true)
        });
    }

    /// Show the remote path browser dialog for a host and preselected directory/path.
    ///
    /// ### Arguments
    /// - `window`: The window to show the dialog in.
    /// - `cx`: The application context.
    /// - `browse`: Browser payload with target directory and preloaded entries.
    pub fn show_remote_path_browser_dialog(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        browse: &RemoteBrowseResult,
    ) {
        let spec = browse.directory_spec.clone();
        let Some(user) = spec.user.clone() else {
            window.push_notification(
                (
                    NotificationType::Error,
                    SharedString::from("Cannot browse remote path: missing SSH user"),
                ),
                cx,
            );
            return;
        };

        let connection = RemotePathBrowserConnection {
            host: spec.host.clone(),
            port: spec.port,
            user: user.clone(),
            credential_key: SshCredKey::new(spec.host.clone(), spec.port, user),
            ssh_session_cache: self.shared_state(cx).ssh_session_cache.clone(),
            ssh_session_pool: self.shared_state(cx).ssh_session_pool.clone(),
        };
        let initial_path = browser_directory_input(&spec.path);
        let initial_entries = browse.entries.clone();
        let notice = browse.notice.clone();
        let browser = cx.new(|cx| {
            RemotePathBrowser::new(
                window,
                cx,
                &initial_path,
                initial_entries,
                notice,
                &connection,
            )
        });
        let browser_input = browser.read(cx).input().clone();
        let title = cx.new(|cx| BrowserDialogTitle::new(browser.clone(), cx));
        let entity = cx.entity().clone();
        let spec_for_ok = spec.clone();

        window.open_alert_dialog(cx.deref_mut(), move |modal, window, cx| {
            let focus_handle = browser_input.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);

            let browser_for_ok = browser.clone();
            let spec_for_ok = spec_for_ok.clone();
            let entity_for_ok = entity.clone();

            modal
                .title(title.clone())
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
                .child(browser_for_ok.clone())
                .on_ok(move |_, window: &mut Window, cx| {
                    let raw_path = browser_for_ok.read(cx).input().read(cx).value().to_string();
                    let selected_path = normalize_remote_browser_selection(&raw_path);
                    if selected_path.is_empty() {
                        window.push_notification(
                            (
                                NotificationType::Error,
                                SharedString::from("Please choose a remote path"),
                            ),
                            cx,
                        );
                        return false;
                    }

                    let mut open_spec = spec_for_ok.clone();
                    open_spec.path = selected_path;
                    open_spec.password_in_url = None;
                    entity_for_ok.update(cx, |_, cx| {
                        // Defer to avoid opening nested dialogs while this one is closing.
                        cx.defer_in(window, move |this, window, cx| {
                            this.do_open_remote_file(window, cx, open_spec);
                        });
                    });
                    true
                })
                .on_cancel(|_, _, _| true)
        });
    }
}

/// Normalize browser-selected remote path text for open attempts.
///
/// ### Arguments
/// - `raw`: Browser input value.
///
/// ### Returns
/// - `String`: Absolute remote path, defaulting to `/`.
fn normalize_remote_browser_selection(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return REMOTE_ROOT_PATH.to_string();
    }
    if trimmed == "~" || trimmed.starts_with("~/") {
        return REMOTE_ROOT_PATH.to_string();
    }
    if trimmed.starts_with(REMOTE_ROOT_PATH) {
        if trimmed.len() > 1 {
            return trimmed.trim_end_matches(REMOTE_ROOT_PATH).to_string();
        }
        return REMOTE_ROOT_PATH.to_string();
    }
    format!(
        "{REMOTE_ROOT_PATH}{}",
        trimmed.trim_start_matches(REMOTE_ROOT_PATH)
    )
}

/// Format a directory path for the browser input so it behaves as a folder context.
///
/// ### Arguments
/// - `path`: Directory path to display.
///
/// ### Returns
/// - `String`: Browser input value ending with `/` (except for root).
fn browser_directory_input(path: &str) -> String {
    let normalized = normalize_remote_browser_selection(path);
    if normalized == REMOTE_ROOT_PATH {
        REMOTE_ROOT_PATH.to_string()
    } else {
        format!("{normalized}/")
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use crate::fulgur::{
        Fulgur, settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
    };
    use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
    use parking_lot::Mutex;
    use std::{cell::RefCell, rc::Rc, sync::Arc};

    /// Set up a minimal Fulgur instance inside a test window.
    ///
    /// ### Arguments
    /// - `cx`: The test app context
    ///
    /// ### Returns
    /// - `(Entity<Fulgur>, VisualTestContext)`: The Fulgur entity and its visual test context
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
    fn test_show_open_remote_dialog_does_not_panic(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.show_open_remote_dialog(window, cx);
            });
        });
    }
}
