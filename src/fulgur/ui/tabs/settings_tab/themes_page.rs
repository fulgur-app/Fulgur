use gpui::{Entity, IntoElement, ParentElement, SharedString, Styled, div};
use gpui_component::{
    ActiveTheme, Sizable, StyledExt,
    button::Button,
    h_flex,
    setting::{SettingGroup, SettingItem, SettingPage},
    v_flex,
};

use crate::fulgur::{
    Fulgur,
    settings::Themes,
    ui::{icons::CustomIcon, themes},
};

impl Fulgur {
    /// Create the Themes settings page
    ///
    /// ### Arguments
    /// - `entity`: The Fulgur entity
    /// - `themes`: The themes to display
    ///
    /// ### Returns
    /// - `SettingPage`: The Themes settings page
    pub fn create_themes_page(entity: Entity<Self>, themes: &Themes) -> SettingPage {
        let mut user_theme_items = Vec::new();
        let mut default_theme_items = Vec::new();
        for theme in &themes.user_themes {
            let theme_name = theme.name.clone();
            let theme_author = theme.author.clone();
            let theme_path = theme.path.clone();
            let themes_info = theme
                .themes
                .iter()
                .map(|t| format!("{} ({})", t.name, t.mode))
                .collect::<Vec<String>>()
                .join(", ");
            let button_id = SharedString::from(format!("delete-theme-{}", theme_name));
            user_theme_items.push(SettingItem::render({
                let entity = entity.clone();
                move |_options, _window, cx| {
                    let theme_path = theme_path.clone();
                    let entity_clone = entity.clone();
                    h_flex()
                        .w_full()
                        .justify_between()
                        .gap_3()
                        .child(
                            v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .font_semibold()
                                        .child(format!("{} by {}", theme_name, theme_author)),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(themes_info.clone()),
                                ),
                        )
                        .child(
                            Button::new(button_id.clone())
                                .icon(CustomIcon::Close)
                                .small()
                                .cursor_pointer()
                                .on_click(move |_, _window, cx| {
                                    if let Err(e) = std::fs::remove_file(&theme_path) {
                                        log::error!(
                                            "Failed to delete theme file {}: {}",
                                            theme_path.display(),
                                            e
                                        );
                                    } else {
                                        log::info!("Deleted theme file: {:?}", theme_path);
                                    }
                                    let entity_for_update = entity_clone.clone();
                                    entity_clone.update(cx, |this, cx| {
                                        themes::reload_themes_and_update(
                                            &this.settings,
                                            entity_for_update,
                                            cx,
                                        );
                                    });
                                }),
                        )
                        .into_any_element()
                }
            }));
        }
        for theme in &themes.default_themes {
            let theme_name = theme.name.clone();
            let theme_author = theme.author.clone();
            let themes_info = theme
                .themes
                .iter()
                .map(|t| format!("{} ({})", t.name, t.mode))
                .collect::<Vec<String>>()
                .join(", ");
            default_theme_items.push(SettingItem::render(move |_options, _window, cx| {
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .font_semibold()
                            .child(format!("{} by {} (Default)", theme_name, theme_author)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(themes_info.clone()),
                    )
                    .into_any_element()
            }));
        }
        let mut groups = Vec::new();
        if !user_theme_items.is_empty() {
            groups.push(
                SettingGroup::new()
                    .title("User Themes")
                    .items(user_theme_items),
            );
        }
        if !default_theme_items.is_empty() {
            groups.push(
                SettingGroup::new()
                    .title("Default Themes")
                    .items(default_theme_items),
            );
        }
        SettingPage::new("Themes").groups(groups)
    }
}
