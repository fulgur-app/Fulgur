use crate::fulgur::{Fulgur, settings::EditorSettings, ui::themes::CURSOR_BLINK_IDLE_TIMEOUT};
use gpui_kit::component::setting::{SettingField, SettingGroup, SettingItem};
use gpui_kit::{App, Entity};

/// Create the Display group of the Editor settings page
///
/// ### Arguments
/// - `entity`: The Fulgur entity
/// - `default_editor_settings`: The default editor settings, used for reset values
///
/// ### Returns
/// - `SettingGroup`: The Display settings group
pub(super) fn display_group(
    entity: &Entity<Fulgur>,
    default_editor_settings: &EditorSettings,
) -> SettingGroup {
    SettingGroup::new().title("Display").items(vec![
        SettingItem::new(
            "Show Line Numbers",
            SettingField::switch(
                {
                    let entity = entity.clone();
                    move |cx: &App| {
                        entity.read(cx).settings.editor_settings.show_line_numbers
                    }
                },
                {
                    let entity = entity.clone();
                    move |val: bool, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.settings.editor_settings.show_line_numbers = val;
                            let _ = this.update_and_propagate_settings(cx);
                        });
                    }
                },
            )
            .default_value(default_editor_settings.show_line_numbers),
        )
        .description("Display line numbers in the editor gutter."),
        SettingItem::new(
            "Soft Wrap",
            SettingField::switch(
                {
                    let entity = entity.clone();
                    move |cx: &App| entity.read(cx).settings.editor_settings.soft_wrap
                },
                {
                    let entity = entity.clone();
                    move |val: bool, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.settings.editor_settings.soft_wrap = val;
                            let _ = this.update_and_propagate_settings(cx);
                        });
                    }
                },
            )
            .default_value(default_editor_settings.soft_wrap),
        )
        .description("Wrap long lines to the next line instead of scrolling."),
        SettingItem::new(
            "Show Whitespaces",
            SettingField::switch(
                {
                    let entity = entity.clone();
                    move |cx: &App| {
                        entity.read(cx).settings.editor_settings.show_whitespaces
                    }
                },
                {
                    let entity = entity.clone();
                    move |val: bool, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.settings.editor_settings.show_whitespaces = val;
                            let _ = this.update_and_propagate_settings(cx);
                        });
                    }
                },
            )
            .default_value(default_editor_settings.show_whitespaces),
        )
        .description("Show whitespace characters (spaces and tabs) in the editor."),
        SettingItem::new(
            "Highlight Colors",
            SettingField::switch(
                {
                    let entity = entity.clone();
                    move |cx: &App| {
                        entity.read(cx).settings.editor_settings.highlight_colors
                    }
                },
                {
                    let entity = entity.clone();
                    move |val: bool, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.settings.editor_settings.highlight_colors = val;
                            let _ = this.update_and_propagate_settings(cx);
                        });
                    }
                },
            )
            .default_value(default_editor_settings.highlight_colors),
        )
        .description(
            "Show colored backgrounds for hex color codes (#RGB and #RRGGBB) in the editor.",
        ),
        SettingItem::new(
            "Blinking Cursor",
            SettingField::switch(
                {
                    let entity = entity.clone();
                    move |cx: &App| entity.read(cx).settings.editor_settings.cursor_blink
                },
                {
                    let entity = entity.clone();
                    move |val: bool, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.settings.editor_settings.cursor_blink = val;
                            let _ = this.update_and_propagate_settings(cx);
                        });
                    }
                },
            )
            .default_value(default_editor_settings.cursor_blink),
        )
        .description(format!(
            "Blink the text cursor. It stops blinking and stays visible after {} seconds without typing or moving it.",
            CURSOR_BLINK_IDLE_TIMEOUT.as_secs()
        )),
    ])
}
