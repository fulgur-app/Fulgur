use gpui::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::{
    ActiveTheme, IndexPath, Sizable, Size,
    group_box::GroupBoxVariant,
    select::{SearchableVec, SelectState},
    setting::{SettingPage, Settings as SettingsComponent},
    v_flex,
};

use crate::fulgur::{Fulgur, ui::tabs::tab::Tab};

mod application_page;
mod editor_page;
mod themes_page;

#[derive(Clone)]
pub struct SettingsTab {
    pub id: usize,
    pub title: SharedString,
    pub font_family_select: Entity<SelectState<SearchableVec<SharedString>>>,
}

impl SettingsTab {
    /// Create a new settings tab
    ///
    /// ### Arguments
    /// - `id`: The ID of the settings tab
    /// - `current_font`: The currently selected font family
    /// - `window`: The window
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Self`: The settings tab
    pub fn new(id: usize, current_font: &str, window: &mut Window, cx: &mut App) -> Self {
        let font_family_select = Self::build_font_select(current_font, window, cx);
        Self {
            id,
            title: SharedString::from("Settings"),
            font_family_select,
        }
    }

    /// Build the font family select entity populated with all system fonts.
    ///
    /// ### Arguments
    /// - `current_font`: The currently selected font family used to set the initial selection
    /// - `window`: The window
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Entity<SelectState<SearchableVec<SharedString>>>`: The font family select state
    pub fn build_font_select(
        current_font: &str,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<SelectState<SearchableVec<SharedString>>> {
        let mut system_fonts: Vec<SharedString> = cx
            .text_system()
            .all_font_names()
            .into_iter()
            .map(SharedString::from)
            .collect();
        system_fonts.sort();
        system_fonts.dedup();
        let selected_index = system_fonts
            .iter()
            .position(|f| f.as_ref() == current_font)
            .map(|ix| IndexPath::default().row(ix));
        cx.new(|cx| {
            SelectState::new(SearchableVec::new(system_fonts), selected_index, window, cx)
                .searchable(true)
        })
    }
}

impl Fulgur {
    /// Create settings pages using the Settings component
    ///
    /// ### Arguments
    /// - `window`: The window
    /// - `cx`: The context
    ///
    /// ### Returns
    /// - `Vec<SettingPage>`: The settings pages
    fn create_settings_pages(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<SettingPage> {
        let entity = cx.entity();
        let font_family_select = self
            .tabs
            .iter()
            .find_map(|t| {
                if let Tab::Settings(s) = t {
                    Some(s.font_family_select.clone())
                } else {
                    None
                }
            })
            .expect("create_settings_pages called without a settings tab in self.tabs");
        let mut pages = vec![
            editor_page::create_editor_page(entity.clone(), font_family_select),
            application_page::create_application_page(entity.clone()),
        ];
        let themes = self.shared_state(cx).themes.lock().clone();
        if let Some(ref themes) = themes {
            pages.push(Self::create_themes_page(entity, themes));
        }
        pages
    }

    /// Render the settings
    ///
    /// ### Arguments
    /// - `window`: The window
    /// - `cx`: The context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The settings UI
    pub fn render_settings(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        const MAX_WIDTH: f32 = 1400.0;
        let show_side_borders = window.viewport_size().width >= px(MAX_WIDTH);
        div()
            .id("settings-scroll-container")
            .size_full()
            .overflow_x_scroll()
            .child(
                v_flex()
                    //.w(px(980.0))
                    .h_full()
                    .mx_auto()
                    .max_w(px(MAX_WIDTH))
                    .min_w(px(980.0))
                    .text_color(cx.theme().foreground)
                    .text_size(px(12.0))
                    .when(show_side_borders, |el: gpui::Div| {
                        el.border_l_1().border_r_1().border_color(cx.theme().border)
                    })
                    .child(
                        SettingsComponent::new("fulgur-settings")
                            .with_size(Size::Medium)
                            .with_group_variant(GroupBoxVariant::Outline)
                            .pages(self.create_settings_pages(window, cx)),
                    ),
            )
    }

    /// Clear the recent files
    ///
    /// ### Arguments
    /// - `cx`: The context
    pub fn clear_recent_files(&mut self, cx: &mut Context<Self>) {
        self.settings.recent_files.clear();
        if let Err(e) = self.update_and_propagate_settings(cx) {
            log::error!("Failed to save settings: {}", e);
        }
        let menus = crate::fulgur::ui::menus::build_menus(
            self.settings.recent_files.get_files(),
            if let Some(info) = self.shared_state(cx).update_info.lock().clone() {
                Some(info.download_url)
            } else {
                None
            },
        );
        self.update_menus(menus, cx);
    }
}
