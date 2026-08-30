#[cfg(target_os = "macos")]
use crate::fulgur::settings::TitleBarStyle;
use crate::fulgur::{Fulgur, settings::TabColorStyle};
use gpui::{Anchor, App, Entity, IntoElement};
use gpui_component::{
    Sizable,
    button::Button,
    menu::{DropdownMenu, PopupMenuItem},
};

/// Render the tab color style chooser as a compact dropdown.
///
/// ### Arguments
/// - `entity`: The Fulgur entity, read for the current style and updated on change
/// - `cx`: The application context
///
/// ### Returns
/// - `impl IntoElement`: The small dropdown button reflecting the current style
pub(super) fn render_tab_color_style_select(
    entity: &Entity<Fulgur>,
    cx: &App,
) -> impl IntoElement + use<> {
    let current = entity.read(cx).settings.app_settings.tab_color_style;
    let label = match current {
        TabColorStyle::TextColor => "Colored Title",
        TabColorStyle::Dot => "Colored Dot",
    };
    let entity = entity.clone();
    Button::new("tab-color-style")
        .label(label)
        .dropdown_caret(true)
        .outline()
        .small()
        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
            let entity_text = entity.clone();
            let entity_dot = entity.clone();
            menu.item(
                PopupMenuItem::new("Colored Title")
                    .checked(current == TabColorStyle::TextColor)
                    .on_click(move |_, _, cx| {
                        set_tab_color_style(&entity_text, TabColorStyle::TextColor, cx);
                    }),
            )
            .item(
                PopupMenuItem::new("Colored Dot")
                    .checked(current == TabColorStyle::Dot)
                    .on_click(move |_, _, cx| {
                        set_tab_color_style(&entity_dot, TabColorStyle::Dot, cx);
                    }),
            )
        })
}

/// Persist a new tab color style selection and propagate it to open windows.
///
/// ### Arguments
/// - `entity`: The Fulgur entity to update
/// - `style`: The newly selected tab color style
/// - `cx`: The application context
fn set_tab_color_style(entity: &Entity<Fulgur>, style: TabColorStyle, cx: &mut App) {
    entity.update(cx, |this, cx| {
        this.settings.app_settings.tab_color_style = style;
        if let Err(e) = this.update_and_propagate_settings(cx) {
            log::error!("Failed to save settings: {e}");
        }
    });
}

/// Render the title bar style chooser as a compact dropdown (macOS only).
///
/// ### Arguments
/// - `entity`: The Fulgur entity, read for the current style and updated on change
/// - `cx`: The application context
///
/// ### Returns
/// - `impl IntoElement`: The small dropdown button reflecting the current style
#[cfg(target_os = "macos")]
pub(super) fn render_title_bar_style_select(
    entity: &Entity<Fulgur>,
    cx: &App,
) -> impl IntoElement + use<> {
    let current = entity.read(cx).settings.app_settings.title_bar_style;
    let label = match current {
        TitleBarStyle::Classic => "Separate Title Bar",
        TitleBarStyle::Unified => "Tabs in Title Bar",
    };
    let entity = entity.clone();
    Button::new("title-bar-style")
        .label(label)
        .dropdown_caret(true)
        .outline()
        .small()
        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
            let entity_classic = entity.clone();
            let entity_unified = entity.clone();
            menu.item(
                PopupMenuItem::new("Separate Title Bar")
                    .checked(current == TitleBarStyle::Classic)
                    .on_click(move |_, _, cx| {
                        set_title_bar_style(&entity_classic, TitleBarStyle::Classic, cx);
                    }),
            )
            .item(
                PopupMenuItem::new("Tabs in Title Bar")
                    .checked(current == TitleBarStyle::Unified)
                    .on_click(move |_, _, cx| {
                        set_title_bar_style(&entity_unified, TitleBarStyle::Unified, cx);
                    }),
            )
        })
}

/// Persist a new title bar style selection and propagate it to open windows.
///
/// ### Arguments
/// - `entity`: The Fulgur entity to update
/// - `style`: The newly selected title bar style
/// - `cx`: The application context
#[cfg(target_os = "macos")]
fn set_title_bar_style(entity: &Entity<Fulgur>, style: TitleBarStyle, cx: &mut App) {
    entity.update(cx, |this, cx| {
        this.settings.app_settings.title_bar_style = style;
        if let Err(e) = this.update_and_propagate_settings(cx) {
            log::error!("Failed to save settings: {e}");
        }
    });
}
