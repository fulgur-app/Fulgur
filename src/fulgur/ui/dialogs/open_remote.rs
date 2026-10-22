use std::{cell::Cell, rc::Rc};

use gpui_kit::component::{
    WindowExt, button::ButtonVariant, dialog::DialogButtonProps, notification::NotificationType,
};
use gpui_kit::{
    AppContext, Context, Entity, Focusable, ParentElement, SharedString, Styled, Window, div, px,
};

use crate::fulgur::{
    Fulgur,
    files::file_operations::RemoteBrowseResult,
    sync::ssh::{REMOTE_ROOT_PATH, credentials::SshCredKey, url::parse_remote_url},
    ui::dialogs::{
        remote_path_browser::{
            BrowserDialogTitle, RemotePathBrowser, RemotePathBrowserConnection,
            normalize_remote_browser_path,
        },
        remote_url_combo::RemoteUrlCombo,
    },
};

impl Fulgur {
    /// Show the open remote file dialog.
    ///
    /// ### Arguments
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    pub fn show_open_remote_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_remote_url_dialog(window, cx);
    }

    /// Open the remote file URL dialog, whose input doubles as a remote history combo box.
    ///
    /// ### Arguments
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Entity<RemoteUrlCombo>`: The URL combo box of the opened dialog
    fn open_remote_url_dialog(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<RemoteUrlCombo> {
        let entity = cx.entity().clone();
        let remembered_url = self.last_failed_remote_open_url.clone().unwrap_or_default();
        let combo = cx.new(|cx| RemoteUrlCombo::new(&remembered_url, window, cx));
        let combo_for_dialog = combo.clone();
        // The builder runs on every render, and refocusing the input each time
        // would pull the focus back from the history menu as soon as it opens.
        let has_initialized_focus = Rc::new(Cell::new(false));
        window.open_alert_dialog(cx, move |modal, window, cx| {
            let input = combo_for_dialog.read(cx).input().clone();
            if !has_initialized_focus.get() {
                let focus_handle = input.read(cx).focus_handle(cx);
                window.focus(&focus_handle, cx);
                has_initialized_focus.set(true);
            }
            let entity_ok = entity.clone();
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
                .close_button(false)
                .child(combo_for_dialog.clone())
                .on_ok(move |_, window: &mut Window, cx| {
                    let url = input.read(cx).value().to_string();
                    match parse_remote_url(&url) {
                        Ok(spec) => {
                            entity_ok.update(cx, |_, cx| {
                                // Defer remote-auth dialog opening until this URL dialog has fully closed.
                                cx.defer_in(window, move |this, window, cx| {
                                    this.open_remote_url_from_dialog(window, cx, spec);
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
        combo
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
            ssh_session_cache: Fulgur::shared_state(cx).ssh_session_cache.clone(),
            ssh_session_pool: Fulgur::shared_state(cx).ssh_session_pool.clone(),
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

        window.open_alert_dialog(cx, move |modal, window, cx| {
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
                .close_button(false)
                .child(browser_for_ok.clone())
                .on_ok(move |_, window: &mut Window, cx| {
                    let raw_path = browser_for_ok.read(cx).input().read(cx).value().to_string();
                    let selected_path = normalize_remote_browser_path(&raw_path);
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

/// Format a directory path for the browser input so it behaves as a folder context.
///
/// ### Arguments
/// - `path`: Directory path to display.
///
/// ### Returns
/// - `String`: Browser input value ending with `/` (except for root).
fn browser_directory_input(path: &str) -> String {
    let normalized = normalize_remote_browser_path(path);
    if normalized == REMOTE_ROOT_PATH {
        REMOTE_ROOT_PATH.to_string()
    } else {
        format!("{normalized}/")
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {

    use gpui_kit::component::{WindowExt, input::InputState};
    use gpui_kit::{Entity, Focusable, Modifiers, TestAppContext, VisualTestContext};

    use crate::fulgur::{
        Fulgur,
        ui::{
            dialogs::remote_url_combo::{FORGET_REMOTE_URL_BUTTON_ID, REMOTE_HISTORY_BUTTON_ID},
            menus::build_default_key_bindings,
        },
    };
    use crate::test_support::setup_fulgur_with_root as setup_fulgur;

    /// Seed the remote history and open the URL dialog with the app keybindings.
    ///
    /// ### Arguments
    /// - `cx`: The test application context
    /// - `urls`: The URLs to remember, oldest first
    ///
    /// ### Returns
    /// - `(Entity<InputState>, VisualTestContext)`: The dialog URL input and the
    ///   visual test context
    fn open_dialog_with_history(
        cx: &mut TestAppContext,
        urls: &[&str],
    ) -> (Entity<InputState>, VisualTestContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let input = visual_cx.update(|window, cx| {
            cx.bind_keys(build_default_key_bindings());
            for url in urls {
                Fulgur::record_remote_history_url((*url).to_string(), cx);
            }
            let combo = fulgur.update(cx, |this, cx| this.open_remote_url_dialog(window, cx));
            combo.read(cx).input().clone()
        });
        settle(&mut visual_cx);
        (input, visual_cx)
    }

    /// Seed two URLs, open the URL dialog and click its history button.
    ///
    /// ### Arguments
    /// - `cx`: The test application context
    ///
    /// ### Returns
    /// - `(Entity<InputState>, VisualTestContext)`: The dialog URL input and the
    ///   visual test context, with the history menu open on `ssh://newest/etc/hosts`
    ///   followed by `ssh://older/`
    fn open_history_menu(cx: &mut TestAppContext) -> (Entity<InputState>, VisualTestContext) {
        let (input, mut visual_cx) =
            open_dialog_with_history(cx, &["ssh://older/", "ssh://newest/etc/hosts"]);
        click(&mut visual_cx, REMOTE_HISTORY_BUTTON_ID);
        (input, visual_cx)
    }

    /// Let pending work run and draw a new frame, so hit testing sees the result.
    ///
    /// ### Arguments
    /// - `visual_cx`: The visual test context of the window
    fn settle(visual_cx: &mut VisualTestContext) {
        visual_cx.run_until_parked();
        visual_cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    /// Click the element rendered under a debug selector, then settle.
    ///
    /// ### Arguments
    /// - `visual_cx`: The visual test context of the window
    /// - `selector`: The debug selector of the element to click
    fn click(visual_cx: &mut VisualTestContext, selector: &'static str) {
        let bounds = visual_cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} should be rendered"));
        visual_cx.simulate_click(bounds.center(), Modifiers::none());
        settle(visual_cx);
    }

    /// Debug selector of the remove button of a history menu entry.
    ///
    /// ### Description
    /// `debug_bounds` only takes static selectors, so the built selector is leaked,
    /// a few bytes per call in a test process.
    ///
    /// ### Arguments
    /// - `index`: Position of the entry in the menu
    ///
    /// ### Returns
    /// - `&'static str`: The selector of that entry's remove button
    fn forget_button(index: usize) -> &'static str {
        Box::leak(format!("{FORGET_REMOTE_URL_BUTTON_ID}-{index}").into_boxed_str())
    }

    /// Whether the history menu shows an entry at a position.
    ///
    /// ### Arguments
    /// - `visual_cx`: The visual test context of the window
    /// - `index`: Position of the entry in the menu
    ///
    /// ### Returns
    /// - `bool`: `true` when the entry was rendered in the last frame
    fn menu_shows_entry(visual_cx: &mut VisualTestContext, index: usize) -> bool {
        visual_cx.debug_bounds(forget_button(index)).is_some()
    }

    /// Read the remote history URLs.
    ///
    /// ### Arguments
    /// - `visual_cx`: The visual test context of the window
    ///
    /// ### Returns
    /// - `Vec<String>`: The remembered URLs, most recent first
    fn history_urls(visual_cx: &mut VisualTestContext) -> Vec<String> {
        visual_cx.update(|_window, cx| Fulgur::shared_state(cx).remote_history.urls().to_vec())
    }

    /// Read the dialog input value, whether it holds the focus, and whether a dialog is open.
    ///
    /// ### Arguments
    /// - `input`: The dialog URL input
    /// - `visual_cx`: The visual test context of the window
    ///
    /// ### Returns
    /// - `(String, bool, bool)`: The input value, its focus state and the dialog state
    fn dialog_state(
        input: &Entity<InputState>,
        visual_cx: &mut VisualTestContext,
    ) -> (String, bool, bool) {
        visual_cx.update(|window, cx| {
            (
                input.read(cx).value().to_string(),
                input.read(cx).focus_handle(cx).is_focused(window),
                window.has_active_dialog(cx),
            )
        })
    }

    #[gpui_kit::test]
    fn test_picking_a_history_url_with_the_keyboard_fills_the_input(cx: &mut TestAppContext) {
        let (input, mut visual_cx) = open_history_menu(cx);
        visual_cx.simulate_keystrokes("down down enter");
        settle(&mut visual_cx);

        let (value, input_focused, dialog_open) = dialog_state(&input, &mut visual_cx);
        assert_eq!(value, "ssh://older/", "the second entry must be picked");
        assert!(input_focused, "the input must get the focus back");
        assert!(
            dialog_open,
            "confirming a menu entry must not submit the dialog"
        );
        assert!(!menu_shows_entry(&mut visual_cx, 0), "the menu must close");
    }

    #[gpui_kit::test]
    fn test_escape_closes_the_history_menu_but_not_the_dialog(cx: &mut TestAppContext) {
        let (input, mut visual_cx) = open_history_menu(cx);
        visual_cx.simulate_keystrokes("escape");
        settle(&mut visual_cx);

        let (value, input_focused, dialog_open) = dialog_state(&input, &mut visual_cx);
        assert_eq!(
            value, "",
            "dismissing the menu must leave the input untouched"
        );
        assert!(input_focused, "the input must get the focus back");
        assert!(dialog_open, "escape must only close the history menu");
        assert!(!menu_shows_entry(&mut visual_cx, 0), "the menu must close");
    }

    #[gpui_kit::test]
    fn test_alt_down_opens_the_history_menu_from_the_input(cx: &mut TestAppContext) {
        let (input, mut visual_cx) =
            open_dialog_with_history(cx, &["ssh://older/", "ssh://newest/etc/hosts"]);
        visual_cx.simulate_keystrokes("alt-down");
        settle(&mut visual_cx);
        assert!(
            menu_shows_entry(&mut visual_cx, 1),
            "alt-down must open the menu on every entry"
        );

        visual_cx.simulate_keystrokes("down enter");
        settle(&mut visual_cx);
        let (value, input_focused, dialog_open) = dialog_state(&input, &mut visual_cx);
        assert_eq!(
            value, "ssh://newest/etc/hosts",
            "the menu must have the focus"
        );
        assert!(input_focused, "the input must get the focus back");
        assert!(dialog_open);
    }

    #[gpui_kit::test]
    fn test_removing_an_entry_keeps_the_menu_open_and_the_input_untouched(cx: &mut TestAppContext) {
        let (input, mut visual_cx) = open_history_menu(cx);
        click(&mut visual_cx, forget_button(0));

        assert_eq!(history_urls(&mut visual_cx), ["ssh://older/"]);
        let (value, _, dialog_open) = dialog_state(&input, &mut visual_cx);
        assert_eq!(value, "", "removing an entry must not pick it");
        assert!(dialog_open);
        assert!(
            menu_shows_entry(&mut visual_cx, 0) && !menu_shows_entry(&mut visual_cx, 1),
            "the open menu must list the remaining entry only"
        );

        visual_cx.simulate_keystrokes("down enter");
        settle(&mut visual_cx);
        let (value, input_focused, _) = dialog_state(&input, &mut visual_cx);
        assert_eq!(value, "ssh://older/", "the menu must keep the focus");
        assert!(input_focused);
    }

    #[gpui_kit::test]
    fn test_removing_the_last_entry_closes_the_menu_and_refocuses_the_input(
        cx: &mut TestAppContext,
    ) {
        let (input, mut visual_cx) = open_dialog_with_history(cx, &["ssh://only/"]);
        click(&mut visual_cx, REMOTE_HISTORY_BUTTON_ID);
        click(&mut visual_cx, forget_button(0));

        assert_eq!(history_urls(&mut visual_cx), Vec::<String>::new());
        assert!(!menu_shows_entry(&mut visual_cx, 0), "the menu must close");
        let (value, input_focused, dialog_open) = dialog_state(&input, &mut visual_cx);
        assert_eq!(value, "");
        assert!(input_focused, "the input must get the focus back");
        assert!(dialog_open);
    }

    #[gpui_kit::test]
    fn test_show_open_remote_dialog_does_not_panic(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.show_open_remote_dialog(window, cx);
            });
        });
    }
}
