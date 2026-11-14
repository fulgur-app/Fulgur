use super::number_field::{make_number_field, slider_val_to_font_size};
use crate::fulgur::Fulgur;
use gpui_kit::component::{
    select::{SearchableVec, Select, SelectState},
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem},
};
use gpui_kit::{App, Entity, SharedString, Styled, px};

/// Create the Font group of the Editor settings page
///
/// ### Arguments
/// - `entity`: The Fulgur entity
/// - `font_family_select`: The persistent font-family selector for this settings tab
///
/// ### Returns
/// - `SettingGroup`: The Font settings group
pub(super) fn font_group(
    entity: &Entity<Fulgur>,
    font_family_select: Entity<SelectState<SearchableVec<SharedString>>>,
) -> SettingGroup {
    SettingGroup::new().title("Font").items(vec![
        SettingItem::new(
            "Font Family",
            SettingField::render(move |_options, _window, _cx| {
                Select::new(&font_family_select).w(px(240.))
            }),
        )
        .description("Select the font family for the editor."),
        SettingItem::new(
            "Font Size",
            make_number_field(
                "editor-font-size".into(),
                &NumberFieldOptions {
                    min: 8.0,
                    max: 24.0,
                    step: 2.0,
                },
                {
                    let entity = entity.clone();
                    move |cx: &App| f64::from(entity.read(cx).settings.editor_settings.font_size)
                },
                {
                    let entity = entity.clone();
                    move |val: f64, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.settings.editor_settings.font_size = slider_val_to_font_size(val);
                            let _ = this.update_and_propagate_settings(cx);
                        });
                    }
                },
            ),
        )
        .description("Adjust the font size for the editor (8-24)."),
    ])
}
