use crate::fulgur::{Fulgur, settings::EditorSettings};
use gpui_kit::component::{
    select::{SearchableVec, SelectState},
    setting::SettingPage,
};
use gpui_kit::{Entity, SharedString};

mod display;
mod file_monitoring;
mod font;
mod indentation;
mod markdown;
mod number_field;

use display::display_group;
use file_monitoring::file_monitoring_group;
use font::font_group;
use indentation::indentation_group;
use markdown::markdown_group;

/// Create the Editor settings page
///
/// ### Arguments
/// - `entity`: The Fulgur entity
/// - `font_family_select`: The persistent font-family selector for this settings tab
///
/// ### Returns
/// - `SettingPage`: The Editor settings page
pub fn create_editor_page(
    entity: &Entity<Fulgur>,
    font_family_select: Entity<SelectState<SearchableVec<SharedString>>>,
) -> SettingPage {
    let default_editor_settings = EditorSettings::new();
    SettingPage::new("Editor").default_open(true).groups(vec![
        font_group(entity, font_family_select),
        indentation_group(entity, &default_editor_settings),
        display_group(entity, &default_editor_settings),
        markdown_group(entity, &default_editor_settings),
        file_monitoring_group(entity, &default_editor_settings),
    ])
}
