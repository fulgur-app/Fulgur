use crate::fulgur::{
    Fulgur,
    settings::{EditorSettings, MarkdownPreviewMode},
};
use gpui::{App, Entity, SharedString, Styled, px};
use gpui_component::{
    select::{SearchableVec, Select, SelectState},
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage},
};

/// Convert a slider `f64` value to a `f32` font size.
///
/// The slider API always provides `f64`; font sizes in the UI range (8–24) are
/// well within `f32` range, so the narrowing cast is safe at this boundary.
#[allow(clippy::cast_possible_truncation)]
fn slider_val_to_font_size(val: f64) -> f32 {
    val as f32
}

/// Convert a slider `f64` value to a `usize` tab size.
///
/// The slider API always provides `f64`; tab sizes in the UI range (2–12) are
/// non-negative integers well within `usize` range, so the cast is safe here.
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
fn slider_val_to_tab_size(val: f64) -> usize {
    val as usize
}

/// Create the Editor settings page
///
/// ### Arguments
/// - `entity`: The Fulgur entity
///
/// ### Returns
/// - `SettingPage`: The Editor settings page
pub fn create_editor_page(
    entity: &Entity<Fulgur>,
    font_family_select: Entity<SelectState<SearchableVec<SharedString>>>,
) -> SettingPage {
    let default_editor_settings = EditorSettings::new();
    SettingPage::new("Editor").default_open(true).groups(vec![
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
                SettingField::number_input(
                    NumberFieldOptions {
                        min: 8.0,
                        max: 24.0,
                        step: 2.0,
                    },
                    {
                        let entity = entity.clone();
                        move |cx: &App| {
                            f64::from(entity.read(cx).settings.editor_settings.font_size)
                        }
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
                )
                .default_value(f64::from(default_editor_settings.font_size)),
            )
            .description("Adjust the font size for the editor (8-24)."),
        ]),
        SettingGroup::new().title("Indentation").items(vec![
            SettingItem::new(
                "Tab Size",
                SettingField::number_input(
                    NumberFieldOptions {
                        min: 2.0,
                        max: 12.0,
                        step: 2.0,
                    },
                    {
                        let entity = entity.clone();
                        move |cx: &App| entity.read(cx).settings.editor_settings.tab_size as f64
                    },
                    {
                        let entity = entity.clone();
                        move |val: f64, cx: &mut App| {
                            entity.update(cx, |this, cx| {
                                this.settings.editor_settings.tab_size = slider_val_to_tab_size(val);
                                let _ = this.update_and_propagate_settings(cx);
                            });
                        }
                    },
                )
                .default_value(default_editor_settings.tab_size as f64),
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
        ]),
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
        ]),
        SettingGroup::new().title("Markdown").items(vec![
            SettingItem::new(
                "Preview Mode",
                SettingField::dropdown(
                    vec![
                        ("dedicated_tab".into(), "Preview Tab".into()),
                        ("panel".into(), "Preview Panel".into()),
                    ],
                    {
                        let entity = entity.clone();
                        move |cx: &App| match entity
                            .read(cx)
                            .settings
                            .editor_settings
                            .markdown_settings
                            .preview_mode
                        {
                            MarkdownPreviewMode::DedicatedTab => "dedicated_tab".into(),
                            MarkdownPreviewMode::Panel => "panel".into(),
                        }
                    },
                    {
                        let entity = entity.clone();
                        move |val: SharedString, cx: &mut App| {
                            entity.update(cx, |this, cx| {
                                this.settings.editor_settings.markdown_settings.preview_mode =
                                    match val.as_ref() {
                                        "panel" => MarkdownPreviewMode::Panel,
                                        _ => MarkdownPreviewMode::DedicatedTab,
                                    };
                                let _ = this.update_and_propagate_settings(cx);
                            });
                        }
                    },
                )
                .default_value(SharedString::from("dedicated_tab")),
            )
            .description("How the Markdown preview is displayed."),
            SettingItem::new(
                "Show Preview by default",
                SettingField::switch(
                    {
                        let entity = entity.clone();
                        move |cx: &App| {
                            entity
                                .read(cx)
                                .settings
                                .editor_settings
                                .markdown_settings
                                .show_markdown_preview
                        }
                    },
                    {
                        let entity = entity.clone();
                        move |val: bool, cx: &mut App| {
                            entity.update(cx, |this, cx| {
                                this.settings
                                    .editor_settings
                                    .markdown_settings
                                    .show_markdown_preview = val;
                                let _ = this.update_and_propagate_settings(cx);
                            });
                        }
                    },
                )
                .default_value(
                    default_editor_settings
                        .markdown_settings
                        .show_markdown_preview,
                ),
            )
            .description("Show preview when opening Markdown files."),
            SettingItem::new(
                "Show Toolbar by default",
                SettingField::switch(
                    {
                        let entity = entity.clone();
                        move |cx: &App| {
                            entity
                                .read(cx)
                                .settings
                                .editor_settings
                                .markdown_settings
                                .show_markdown_toolbar
                        }
                    },
                    {
                        let entity = entity.clone();
                        move |val: bool, cx: &mut App| {
                            entity.update(cx, |this, cx| {
                                this.settings
                                    .editor_settings
                                    .markdown_settings
                                    .show_markdown_toolbar = val;
                                let _ = this.update_and_propagate_settings(cx);
                            });
                        }
                    },
                )
                .default_value(
                    default_editor_settings
                        .markdown_settings
                        .show_markdown_toolbar,
                ),
            )
            .description("Show toolbar by default when opening Markdown files."),
        ]),
        SettingGroup::new().title("File Monitoring").items(vec![
            SettingItem::new(
                "Watch Files",
                SettingField::switch(
                    {
                        let entity = entity.clone();
                        move |cx: &App| entity.read(cx).settings.editor_settings.watch_files
                    },
                    {
                        let entity = entity.clone();
                        move |val: bool, cx: &mut App| {
                            entity.update(cx, |this, cx| {
                                this.settings.editor_settings.watch_files = val;
                                if val {
                                    this.start_file_watcher();
                                } else {
                                    this.stop_file_watcher();
                                }
                                let _ = this.update_and_propagate_settings(cx);
                            });
                        }
                    },
                )
                .default_value(default_editor_settings.watch_files),
            )
            .description("Monitor files for external changes."),
        ]),
    ])
}
