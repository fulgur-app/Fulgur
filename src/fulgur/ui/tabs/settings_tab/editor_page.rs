use crate::fulgur::{
    Fulgur,
    settings::{EditorSettings, MarkdownPreviewMode},
};
use gpui::prelude::FluentBuilder as _;
use gpui::{App, AppContext as _, Context, Entity, SharedString, Styled, Subscription, px};
use gpui_component::{
    AxisExt, Sizable,
    input::{InputEvent, InputState, NumberInput, NumberInputEvent, StepAction},
    select::{SearchableVec, Select, SelectState},
    setting::{NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage},
};
use std::rc::Rc;

type SetValFn = Rc<dyn Fn(f64, &mut App)>;

struct NumberInputState {
    input: Entity<InputState>,
    _subs: Vec<Subscription>,
}

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

/// Create a number input setting field that correctly propagates both step-button
/// clicks and manual text edits to the entity.
///
/// gpui-component's built-in `SettingField::number_input` suppresses
/// `InputEvent::Change` when step buttons are clicked (via `InputState::set_value`
/// which sets `emit_events = false`), so the entity setter is never called for
/// step interactions. This helper replicates the field while explicitly calling the
/// setter on each step.
///
/// ### Arguments
/// - `state_key`: Unique key for `use_keyed_state`
/// - `options`: Step / min / max configuration
/// - `get_val`: Reads the current value from the application state
/// - `set_val`: Writes an updated value into the application state
///
/// ### Returns
/// - `SettingField<SharedString>`: A correctly-wired number input field
fn make_number_field(
    state_key: SharedString,
    options: &NumberFieldOptions,
    get_val: impl Fn(&App) -> f64 + 'static,
    set_val: impl Fn(f64, &mut App) + 'static,
) -> SettingField<SharedString> {
    let options = options.clone();
    let get_val = Rc::new(get_val);
    let set_val: SetValFn = Rc::new(set_val);

    SettingField::render(move |options_render, window, cx| {
        let current_value = (get_val)(cx);
        let min = options.min;
        let max = options.max;
        let step = options.step;
        let set_val_step = set_val.clone();
        let set_val_change = set_val.clone();

        let state = window.use_keyed_state(
            state_key.clone(),
            cx,
            |window, cx: &mut Context<NumberInputState>| {
                let input = cx
                    .new(|cx| InputState::new(window, cx).default_value(current_value.to_string()));
                let subs = vec![
                    cx.subscribe_in(&input, window, {
                        let setter = set_val_step.clone();
                        move |_, inp, event: &NumberInputEvent, window, cx| {
                            let NumberInputEvent::Step(action) = event;
                            inp.update(cx, |inp: &mut InputState, cx| {
                                if let Ok(v) = inp.value().parse::<f64>() {
                                    let new_v = (if *action == StepAction::Increment {
                                        v + step
                                    } else {
                                        v - step
                                    })
                                    .clamp(min, max);
                                    inp.set_value(
                                        SharedString::from(new_v.to_string()),
                                        window,
                                        cx,
                                    );
                                    setter(new_v, cx);
                                }
                            });
                        }
                    }),
                    cx.subscribe_in(&input, window, {
                        move |_, inp, event: &InputEvent, _, cx| {
                            if let InputEvent::Change = event {
                                inp.update(cx, |inp: &mut InputState, cx| {
                                    if let Ok(v) = inp.value().parse::<f64>() {
                                        set_val_change(v.clamp(min, max), cx);
                                    }
                                });
                            }
                        }
                    }),
                ];
                NumberInputState { input, _subs: subs }
            },
        );

        let size = options_render.size;
        let is_horizontal = options_render.layout.is_horizontal();
        let input_entity = state.read(cx).input.clone();
        NumberInput::new(&input_entity).with_size(size).map(|this| {
            if is_horizontal {
                this.w_32()
            } else {
                this.w_full()
            }
        })
    })
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
                make_number_field(
                    "editor-font-size".into(),
                    &NumberFieldOptions {
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
                                this.settings.editor_settings.font_size =
                                    slider_val_to_font_size(val);
                                let _ = this.update_and_propagate_settings(cx);
                            });
                        }
                    },
                ),
            )
            .description("Adjust the font size for the editor (8-24)."),
        ]),
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
                            entity.read(cx).settings.editor_settings.tab_size as f64
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
