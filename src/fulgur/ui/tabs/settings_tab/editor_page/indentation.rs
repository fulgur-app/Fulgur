use super::number_field::{make_number_field, slider_val_to_tab_size};
use crate::fulgur::{Fulgur, settings::EditorSettings};
use gpui_kit::component::setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem};
use gpui_kit::{App, Entity};

/// Create the Indentation group of the Editor settings page
///
/// ### Arguments
/// - `entity`: The Fulgur entity
/// - `default_editor_settings`: The default editor settings, used for reset values
///
/// ### Returns
/// - `SettingGroup`: The Indentation settings group
pub(super) fn indentation_group(
    entity: &Entity<Fulgur>,
    default_editor_settings: &EditorSettings,
) -> SettingGroup {
    SettingGroup::new().title("Indentation").items(vec![
        SettingItem::new(
            "Tab Size",
            make_number_field(
                "editor-tab-size".into(),
                &NumberFieldOptions {
                    min: 2.0,
                    max: 12.0,
                    step: 2.0,
                },
                {
                    let entity = entity.clone();
                    move |cx: &App| {
                        // tab_size is a tiny value (2-12); the f64 cast is exact.
                        #[allow(clippy::cast_precision_loss)]
                        let tab_size =
                            entity.read(cx).settings.editor_settings.tab_size as f64;
                        tab_size
                    }
                },
                {
                    let entity = entity.clone();
                    move |val: f64, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.settings.editor_settings.tab_size =
                                slider_val_to_tab_size(val);
                            let _ = this.update_and_propagate_settings(cx);
                        });
                    }
                },
            ),
        )
        .description("Number of spaces for indentation. Takes effect on new tabs."),
        SettingItem::new(
            "Use Spaces for Tabs",
            SettingField::switch(
                {
                    let entity = entity.clone();
                    move |cx: &App| entity.read(cx).settings.editor_settings.use_spaces
                },
                {
                    let entity = entity.clone();
                    move |val: bool, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.settings.editor_settings.use_spaces = val;
                            let _ = this.update_and_propagate_settings(cx);
                        });
                    }
                },
            )
            .default_value(default_editor_settings.use_spaces),
        )
        .description("Insert spaces when pressing Tab instead of a tab character. Takes effect on new tabs."),
        SettingItem::new(
            "Show Indent Guides",
            SettingField::switch(
                {
                    let entity = entity.clone();
                    move |cx: &App| {
                        entity.read(cx).settings.editor_settings.show_indent_guides
                    }
                },
                {
                    let entity = entity.clone();
                    move |val: bool, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.settings.editor_settings.show_indent_guides = val;
                            let _ = this.update_and_propagate_settings(cx);
                        });
                    }
                },
            )
            .default_value(default_editor_settings.show_indent_guides),
        )
        .description("Show vertical lines indicating indentation levels."),
        SettingItem::new(
            "Smart Indentation",
            SettingField::switch(
                {
                    let entity = entity.clone();
                    move |cx: &App| entity.read(cx).settings.editor_settings.smart_indent
                },
                {
                    let entity = entity.clone();
                    move |val: bool, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.settings.editor_settings.smart_indent = val;
                            let _ = this.update_and_propagate_settings(cx);
                        });
                    }
                },
            )
            .default_value(default_editor_settings.smart_indent),
        )
        .description(
            "Keep the current indentation on a new line and add a level after an opening delimiter.",
        ),
        SettingItem::new(
            "Auto Close Pairs",
            SettingField::switch(
                {
                    let entity = entity.clone();
                    move |cx: &App| entity.read(cx).settings.editor_settings.auto_close_pairs
                },
                {
                    let entity = entity.clone();
                    move |val: bool, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.settings.editor_settings.auto_close_pairs = val;
                            let _ = this.update_and_propagate_settings(cx);
                        });
                    }
                },
            )
            .default_value(default_editor_settings.auto_close_pairs),
        )
        .description(
            "Insert the matching bracket or quote while typing, using rules specific to the language.",
        ),
    ])
}
