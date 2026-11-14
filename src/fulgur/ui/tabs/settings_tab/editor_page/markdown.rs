use crate::fulgur::{
    Fulgur,
    settings::{EditorSettings, MarkdownPreviewMode},
};
use gpui_kit::component::setting::{SettingField, SettingGroup, SettingItem};
use gpui_kit::{App, Entity, SharedString};

/// Create the Markdown group of the Editor settings page
///
/// ### Arguments
/// - `entity`: The Fulgur entity
/// - `default_editor_settings`: The default editor settings, used for reset values
///
/// ### Returns
/// - `SettingGroup`: The Markdown settings group
pub(super) fn markdown_group(
    entity: &Entity<Fulgur>,
    default_editor_settings: &EditorSettings,
) -> SettingGroup {
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
            "Load Remote Images",
            SettingField::switch(
                {
                    let entity = entity.clone();
                    move |cx: &App| {
                        entity.read(cx).settings.editor_settings.markdown_settings.load_remote_images
                    }
                },
                {
                    let entity = entity.clone();
                    move |val: bool, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.settings.editor_settings.markdown_settings.load_remote_images = val;
                            let _ = this.update_and_propagate_settings(cx);
                        });
                    }
                },
            )
            .default_value(default_editor_settings.markdown_settings.load_remote_images),
        )
        .description("Allow previews to load images from the internet and local network. This can reveal your IP address and when you open a document."),
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
        SettingItem::new(
            "Limit Preview Width",
            SettingField::switch(
                {
                    let entity = entity.clone();
                    move |cx: &App| {
                        entity
                            .read(cx)
                            .settings
                            .editor_settings
                            .markdown_settings
                            .limit_preview_width
                    }
                },
                {
                    let entity = entity.clone();
                    move |val: bool, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.settings
                                .editor_settings
                                .markdown_settings
                                .limit_preview_width = val;
                            let _ = this.update_and_propagate_settings(cx);
                        });
                    }
                },
            )
            .default_value(
                default_editor_settings
                    .markdown_settings
                    .limit_preview_width,
            ),
        )
        .description("Cap the preview at 800px and center it in its panel."),
        SettingItem::new(
            "Render Frontmatter",
            SettingField::switch(
                {
                    let entity = entity.clone();
                    move |cx: &App| {
                        entity
                            .read(cx)
                            .settings
                            .editor_settings
                            .markdown_settings
                            .render_frontmatter
                    }
                },
                {
                    let entity = entity.clone();
                    move |val: bool, cx: &mut App| {
                        entity.update(cx, |this, cx| {
                            this.settings
                                .editor_settings
                                .markdown_settings
                                .render_frontmatter = val;
                            let _ = this.update_and_propagate_settings(cx);
                        });
                    }
                },
            )
            .default_value(
                default_editor_settings
                    .markdown_settings
                    .render_frontmatter,
            ),
        )
        .description("Show YAML frontmatter as a metadata list instead of a code block."),
    ])
}
