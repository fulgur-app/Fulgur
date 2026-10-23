//! URL input of the Open Remote File dialog, doubling as a combo box over the remote history.

use crate::fulgur::{
    Fulgur,
    sync::ssh::url::{format_remote_history_url, parse_remote_url},
    ui::{
        icons::CustomIcon,
        menus::{KEY_CONTEXT_REMOTE_URL_COMBO, ShowRemoteUrlHistory},
    },
};
use gpui_kit::component::{
    Disableable, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputState},
    menu::{PopupMenu, PopupMenuItem},
    popover::Popover,
};
use gpui_kit::{
    Anchor, AnyElement, App, AppContext, Context, DismissEvent, Entity, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, Styled, Subscription, WeakEntity,
    Window, div,
};

/// Element id of the button listing the remote history URLs.
pub(super) const REMOTE_HISTORY_BUTTON_ID: &str = "open-remote-history";

/// Label of the button listing the remote history URLs.
const REMOTE_HISTORY_BUTTON_LABEL: &str = "Recent remote URLs";

/// Element id of the popover hosting the remote history menu.
const REMOTE_HISTORY_POPOVER_ID: &str = "open-remote-history-popover";

/// Element id prefix of the buttons forgetting one remote history URL.
pub(super) const FORGET_REMOTE_URL_BUTTON_ID: &str = "forget-remote-url";

/// Label of the buttons forgetting one remote history URL.
const FORGET_REMOTE_URL_LABEL: &str = "Remove from history";

/// URL input whose history menu lists the remote history URLs.
pub struct RemoteUrlCombo {
    input: Entity<InputState>,
    history_open: bool,
    history_menu: Option<Entity<PopupMenu>>,
    history_menu_subscription: Option<Subscription>,
}

impl RemoteUrlCombo {
    /// Create the combo box around a fresh URL input.
    ///
    /// ### Arguments
    /// - `initial_url`: Text the input starts with
    /// - `window`: The window hosting the dialog
    /// - `cx`: The combo box context
    ///
    /// ### Returns
    /// - `RemoteUrlCombo`: The combo box with its history menu closed
    pub fn new(initial_url: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let initial_url = initial_url.to_string();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("ssh://user@host/path/to/file")
                .default_value(initial_url)
        });
        Self {
            input,
            history_open: false,
            history_menu: None,
            history_menu_subscription: None,
        }
    }

    /// Return the URL input.
    ///
    /// ### Returns
    /// - `&Entity<InputState>`: The input holding the URL to open
    pub fn input(&self) -> &Entity<InputState> {
        &self.input
    }

    /// Open the history menu from the keyboard.
    ///
    /// ### Arguments
    /// - `_action`: The shortcut action
    /// - `_window`: The window hosting the dialog
    /// - `cx`: The combo box context
    fn show_history(
        &mut self,
        _action: &ShowRemoteUrlHistory,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.history_open || Fulgur::shared_state(cx).remote_history.is_empty() {
            return;
        }
        self.history_open = true;
        cx.notify();
    }

    /// Follow an open state change requested by the chevron button.
    ///
    /// ### Arguments
    /// - `open`: Whether the menu must be open
    /// - `cx`: The combo box context
    fn set_history_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if open {
            self.history_open = true;
            cx.notify();
        } else {
            self.close_history(cx);
        }
    }

    /// Close the history menu, dropping it so it is rebuilt from the history next time.
    ///
    /// ### Arguments
    /// - `cx`: The combo box context
    fn close_history(&mut self, cx: &mut Context<Self>) {
        self.history_open = false;
        self.history_menu = None;
        self.history_menu_subscription = None;
        cx.notify();
    }

    /// Return the history menu, building and focusing it the first time it is shown.
    ///
    /// ### Arguments
    /// - `window`: The window hosting the dialog
    /// - `cx`: The combo box context
    ///
    /// ### Returns
    /// - `Entity<PopupMenu>`: The menu listing the remote history URLs
    fn history_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<PopupMenu> {
        if let Some(menu) = &self.history_menu {
            return menu.clone();
        }
        let combo = cx.entity().downgrade();
        let input = self.input.clone();
        let menu = PopupMenu::build(window, cx, move |menu, _window, cx| {
            build_history_menu(menu, &combo, &input, cx)
        });
        self.history_menu_subscription =
            Some(cx.subscribe(&menu, |this, _menu, _: &DismissEvent, cx| {
                this.close_history(cx);
            }));
        menu.focus_handle(cx).focus(window, cx);
        self.history_menu = Some(menu.clone());
        menu
    }

    /// Forget one URL, then refresh the open menu or close it when nothing is left.
    ///
    /// ### Arguments
    /// - `url`: The URL to forget
    /// - `window`: The window hosting the dialog
    /// - `cx`: The combo box context
    fn forget_history_url(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        Fulgur::forget_remote_history_url(url, cx);
        if Fulgur::shared_state(cx).remote_history.is_empty() {
            // The chevron button turns disabled and takes the menu with it.
            self.close_history(cx);
            let focus_handle = self.input.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
            return;
        }
        let Some(menu) = self.history_menu.clone() else {
            return;
        };
        let combo = cx.entity().downgrade();
        let input = self.input.clone();
        menu.update(cx, |menu, cx| {
            menu.rebuild(window, cx, |menu, _window, cx| {
                build_history_menu(menu, &combo, &input, cx)
            });
        });
    }

    /// Render the chevron button, wrapped in the popover hosting the history menu.
    ///
    /// ### Arguments
    /// - `cx`: The combo box context, read for the remote history
    ///
    /// ### Returns
    /// - `AnyElement`: The button opening the history menu, disabled when the history is empty
    fn render_history_trigger(&self, cx: &mut Context<Self>) -> AnyElement {
        let button = Button::new(REMOTE_HISTORY_BUTTON_ID)
            .debug_selector(|| REMOTE_HISTORY_BUTTON_ID.to_string())
            .icon(CustomIcon::ChevronDown)
            .ghost()
            .xsmall()
            .tooltip_with_action(
                REMOTE_HISTORY_BUTTON_LABEL,
                &ShowRemoteUrlHistory,
                Some(KEY_CONTEXT_REMOTE_URL_COMBO),
            )
            .accessibility_label(REMOTE_HISTORY_BUTTON_LABEL);
        if Fulgur::shared_state(cx).remote_history.is_empty() {
            return button.disabled(true).into_any_element();
        }
        let combo = cx.entity().downgrade();
        Popover::new(REMOTE_HISTORY_POPOVER_ID)
            .appearance(false)
            // The menu dismisses itself on a click outside of it.
            .overlay_closable(false)
            .anchor(Anchor::TopRight)
            .open(self.history_open)
            .on_open_change(cx.listener(|this, open: &bool, _window, cx| {
                this.set_history_open(*open, cx);
            }))
            .trigger(button)
            .content(move |_, window, cx| match combo.upgrade() {
                Some(combo) => combo
                    .update(cx, |combo, cx| combo.history_menu(window, cx))
                    .into_any_element(),
                None => div().into_any_element(),
            })
            .into_any_element()
    }
}

impl Render for RemoteUrlCombo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context(KEY_CONTEXT_REMOTE_URL_COMBO)
            .on_action(cx.listener(Self::show_history))
            .w_full()
            .child(Input::new(&self.input).suffix(self.render_history_trigger(cx)))
    }
}

/// Fill a menu with one entry per remote history URL, most recent first.
///
/// ### Arguments
/// - `menu`: The empty menu to fill
/// - `combo`: The combo box owning the menu, told when an entry is removed
/// - `input`: The URL input, filled with the picked URL
/// - `cx`: The application context, read for the remote history
///
/// ### Returns
/// - `PopupMenu`: The menu with its entries, the one matching the input checked
fn build_history_menu(
    menu: PopupMenu,
    combo: &WeakEntity<RemoteUrlCombo>,
    input: &Entity<InputState>,
    cx: &App,
) -> PopupMenu {
    let current_url = parse_remote_url(&input.read(cx).value())
        .ok()
        .map(|spec| format_remote_history_url(&spec));
    Fulgur::shared_state(cx)
        .remote_history
        .urls()
        .iter()
        .enumerate()
        .fold(menu, |menu, (index, url)| {
            menu.item(history_menu_item(
                index,
                url,
                current_url.as_ref() == Some(url),
                combo,
                input,
            ))
        })
}

/// Build the menu entry of one remote history URL.
///
/// ### Arguments
/// - `index`: Position of the URL in the history
/// - `url`: The remembered URL
/// - `checked`: Whether the URL matches the input
/// - `combo`: The combo box owning the menu
/// - `input`: The URL input, filled with the URL when the entry is picked
///
/// ### Returns
/// - `PopupMenuItem`: The entry showing the URL and its remove button
fn history_menu_item(
    index: usize,
    url: &str,
    checked: bool,
    combo: &WeakEntity<RemoteUrlCombo>,
    input: &Entity<InputState>,
) -> PopupMenuItem {
    let row_url = url.to_string();
    let row_combo = combo.clone();
    let picked_url = url.to_string();
    let input = input.clone();
    PopupMenuItem::element(move |_window, _cx| render_history_row(index, &row_url, &row_combo))
        .checked(checked)
        .on_click(move |_, window, cx| {
            input.update(cx, |state, cx| {
                state.replace_all(picked_url.clone(), window, cx);
            });
            // Moving the focus before the menu closes keeps the menu from
            // restoring it somewhere else.
            let focus_handle = input.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
        })
}

/// Render the content of one history menu entry.
///
/// ### Arguments
/// - `index`: Position of the URL in the history
/// - `url`: The remembered URL
/// - `combo`: The combo box owning the menu, told when the URL is removed
///
/// ### Returns
/// - `impl IntoElement`: The URL followed by its remove button
fn render_history_row(
    index: usize,
    url: &str,
    combo: &WeakEntity<RemoteUrlCombo>,
) -> impl IntoElement + use<> {
    let forgotten_url = url.to_string();
    let combo = combo.clone();
    h_flex()
        .w_full()
        .gap_2()
        .justify_between()
        .child(div().flex_1().min_w_0().truncate().child(url.to_string()))
        .child(
            Button::new((FORGET_REMOTE_URL_BUTTON_ID, index))
                .debug_selector(move || format!("{FORGET_REMOTE_URL_BUTTON_ID}-{index}"))
                .icon(CustomIcon::Close)
                .ghost()
                .xsmall()
                .tooltip(FORGET_REMOTE_URL_LABEL)
                .accessibility_label(FORGET_REMOTE_URL_LABEL)
                .on_click(move |_, window, cx| {
                    // The entry would otherwise also pick the URL it shows.
                    cx.stop_propagation();
                    if let Some(combo) = combo.upgrade() {
                        combo.update(cx, |combo, cx| {
                            combo.forget_history_url(&forgotten_url, window, cx);
                        });
                    }
                }),
        )
}
