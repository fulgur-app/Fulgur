// The burger menu standing in for the menu bar in the unified layout on Windows and Linux

use super::CustomTitleBar;
use crate::fulgur::{
    Fulgur,
    tab::Tab,
    ui::{icons::CustomIcon, tabs::tab_bar::tab_bar_button_factory},
};
use gpui::{
    AnyElement, App, Context, FocusHandle, Focusable, IntoElement, OwnedMenu, OwnedMenuItem,
    WeakEntity, Window,
};
use gpui_component::{
    ActiveTheme, GlobalState,
    menu::{DropdownMenu, PopupMenu},
};

/// Render the burger button opening the application menus as submenus
///
/// ### Arguments
/// - `fulgur`: Weak handle to the owning window, read for the tab the actions must reach
/// - `cx`: The title bar context
///
/// ### Returns
/// - `AnyElement`: The rendered burger button
pub(super) fn render_burger_menu(
    fulgur: &WeakEntity<Fulgur>,
    cx: &mut Context<CustomTitleBar>,
) -> AnyElement {
    let fulgur = fulgur.clone();
    // Only the bottom border is kept, so the line running under the whole row stays whole.
    tab_bar_button_factory("app-menu", "Menu", CustomIcon::Menu, cx.theme().border)
        .dropdown_menu(move |menu, window, cx| build_app_menu(&fulgur, menu, window, cx))
        .into_any_element()
}

/// Resolve the handle the menu actions have to be dispatched to
///
/// ### Arguments
/// - `fulgur`: Weak handle to the owning window
/// - `cx`: The application context
///
/// ### Returns
/// - `Some(FocusHandle)`: The active editor input, or the window itself for any other tab
/// - `None`: If the owning window is gone
fn action_context(fulgur: &WeakEntity<Fulgur>, cx: &App) -> Option<FocusHandle> {
    let fulgur = fulgur.upgrade()?;
    let window = fulgur.read(cx);
    match window.active_tab(cx) {
        Some(Tab::Editor(editor_tab)) => Some(editor_tab.content.focus_handle(cx)),
        _ => Some(window.focus_handle(cx)),
    }
}

/// Build the burger popup from the menus currently registered on the application
///
/// ### Arguments
/// - `fulgur`: Weak handle to the owning window
/// - `menu`: The popup being built
/// - `window`: The window the popup belongs to
/// - `cx`: The popup context
///
/// ### Returns
/// - `PopupMenu`: The popup holding one submenu per application menu
fn build_app_menu(
    fulgur: &WeakEntity<Fulgur>,
    menu: PopupMenu,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let action_context = action_context(fulgur, cx);
    let menus: Vec<OwnedMenu> = GlobalState::global(cx).app_menus().to_vec();
    let mut menu = with_action_context(menu, action_context.as_ref());
    for app_menu in menus {
        if app_menu.disabled {
            continue;
        }
        let items = app_menu.items.clone();
        let action_context = action_context.clone();
        menu = menu.submenu(app_menu.name, window, cx, move |submenu, window, cx| {
            let submenu = with_action_context(submenu, action_context.as_ref());
            append_menu_items(submenu, &items, action_context.as_ref(), window, cx)
        });
    }
    menu
}

/// Point a popup at the handle its actions must be dispatched to
///
/// ### Arguments
/// - `menu`: The popup to configure
/// - `action_context`: The handle to dispatch to, if anything was focused
///
/// ### Returns
/// - `PopupMenu`: The popup, unchanged when nothing was focused
fn with_action_context(menu: PopupMenu, action_context: Option<&FocusHandle>) -> PopupMenu {
    match action_context {
        Some(handle) => menu.action_context(handle.clone()),
        None => menu,
    }
}

/// Translate the items of one application menu into popup entries
///
/// ### Arguments
/// - `menu`: The popup to append to
/// - `items`: The items of the application menu being translated
/// - `action_context`: The handle nested submenus must dispatch their actions to
/// - `window`: The window the popup belongs to
/// - `cx`: The popup context
///
/// ### Returns
/// - `PopupMenu`: The popup with every supported item appended
fn append_menu_items(
    mut menu: PopupMenu,
    items: &[OwnedMenuItem],
    action_context: Option<&FocusHandle>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    for item in items {
        match item {
            OwnedMenuItem::Action {
                name,
                action,
                checked,
                disabled,
                ..
            } => {
                menu = menu.menu_with_check_and_disabled(
                    name.clone(),
                    *checked,
                    action.boxed_clone(),
                    *disabled,
                );
            }
            OwnedMenuItem::Separator => menu = menu.separator(),
            OwnedMenuItem::Submenu(submenu) => {
                let nested = submenu.items.clone();
                let action_context = action_context.cloned();
                menu = menu.submenu(
                    submenu.name.clone(),
                    window,
                    cx,
                    move |nested_menu, window, cx| {
                        let nested_menu = with_action_context(nested_menu, action_context.as_ref());
                        append_menu_items(nested_menu, &nested, action_context.as_ref(), window, cx)
                    },
                );
            }
            // Platform provided menus (the macOS Services menu) have no equivalent here.
            OwnedMenuItem::SystemMenu(_) => {}
        }
    }
    menu
}
