//! Rendering of the command palette dialog.

use super::commands::KeybindingHint;
use super::{CommandPalette, PaletteCommand, PaletteGroup};
use gpui_kit::component::{
    ActiveTheme, Icon, WindowExt,
    command::{Command, CommandGroup, CommandItem},
    h_flex,
    kbd::Kbd,
};
use gpui_kit::{
    App, Context, Focusable, IntoElement, ParentElement, Styled, Window,
    prelude::FluentBuilder as _, px,
};
use std::{cell::Cell, rc::Rc};

/// The palette dialog's width, wide enough for a command label and its keybinding hint.
const PALETTE_WIDTH: f32 = 560.;

/// The tallest the command list grows before it scrolls.
const PALETTE_MAX_LIST_HEIGHT: f32 = 420.;

impl CommandPalette {
    /// Open the dialog hosting the palette.
    ///
    /// ### Arguments
    /// - `window`: The window to open the dialog in
    /// - `cx`: The application context
    pub(super) fn open_dialog(window: &mut Window, cx: &mut Context<Self>) {
        let palette = cx.entity();
        let focus_on_mount = Rc::new(Cell::new(true));
        window.open_dialog(cx, move |dialog, _, _| {
            let content_palette = palette.clone();
            let close_palette = palette.clone();
            let focus_on_mount = focus_on_mount.clone();
            dialog
                .close_button(false)
                .w(px(PALETTE_WIDTH))
                .p_0()
                .on_close(move |_, _, cx| {
                    close_palette.update(cx, |this, cx| {
                        this.is_open = false;
                        cx.notify();
                    });
                })
                .content(move |content, window, cx| {
                    let palette = content_palette.clone();
                    if focus_on_mount.replace(false) {
                        let palette = palette.clone();
                        window.defer(cx, move |window, cx| {
                            let focus_handle = palette.read(cx).state.read(cx).focus_handle(cx);
                            window.focus(&focus_handle, cx);
                        });
                    }
                    content.child(build_command(&palette, cx))
                })
        });
    }
}

/// Build the palette element for the layout the palette entity currently holds.
///
/// ### Arguments
/// - `palette`: The palette entity to read the layout and interaction state from
/// - `cx`: The application context
///
/// ### Returns
/// - `Command`: The configured palette element
fn build_command(palette: &gpui_kit::Entity<CommandPalette>, cx: &mut App) -> Command {
    let (state, layout) = {
        let this = palette.read(cx);
        (this.state.clone(), this.layout.clone())
    };
    let confirm_palette = palette.clone();

    layout.into_iter().fold(
        Command::new(&state)
            .bordered(false)
            .placeholder("Type a command...")
            .max_h(px(PALETTE_MAX_LIST_HEIGHT))
            .on_confirm(move |index, window, cx| {
                confirm_palette.update(cx, |this, cx| {
                    this.confirm(index, window, cx);
                });
            })
            .footer(render_footer),
        |command, (group, commands)| command.group(build_group(group, &commands)),
    )
}

/// Build one titled section of the palette.
///
/// ### Arguments
/// - `group`: The group being rendered, providing the heading
/// - `commands`: The commands the group offers, in order
///
/// ### Returns
/// - `CommandGroup`: The section, with one item per command
fn build_group(group: PaletteGroup, commands: &[PaletteCommand]) -> CommandGroup {
    commands.iter().fold(
        CommandGroup::new().label(group.label()),
        |section, command| section.item(build_item(*command)),
    )
}

/// Build the palette row for a single command.
///
/// ### Arguments
/// - `command`: The command to render
///
/// ### Returns
/// - `CommandItem`: The row, searchable on its label and keywords
fn build_item(command: PaletteCommand) -> CommandItem {
    let hint = command.keybinding_hint();
    CommandItem::new()
        .label(command.label())
        .keywords(command.keywords().iter().copied())
        .child(move |window: &mut Window, cx: &mut App| {
            render_row(command, hint.as_ref(), window, cx)
        })
}

/// Render a command's icon, label and keybinding hint.
///
/// ### Arguments
/// - `command`: The command the row stands for
/// - `hint`: The keybinding hint to display, when the command has one
/// - `window`: The window the keybinding is resolved against
/// - `cx`: The application context
///
/// ### Returns
/// - `impl IntoElement`: The row element
fn render_row(
    command: PaletteCommand,
    hint: Option<&KeybindingHint>,
    window: &Window,
    cx: &App,
) -> impl IntoElement + use<> {
    let binding = hint
        .and_then(|hint| Kbd::binding_for_action(hint.action.as_ref(), Some(hint.context), window));

    h_flex()
        .w_full()
        .gap_2()
        .items_center()
        .child(
            Icon::from(command.icon())
                .size_4()
                .text_color(cx.theme().muted_foreground),
        )
        .child(command.label())
        .when_some(binding, |this, binding| this.child(binding.ml_auto()))
}

/// Render the palette's key hint footer.
///
/// ### Arguments
/// - `_state`: The palette interaction state, unused by this footer
/// - `_window`: The window the palette is rendered in
/// - `cx`: The application context
///
/// ### Returns
/// - `impl IntoElement`: The footer element
fn render_footer(
    _state: &gpui_kit::component::command::CommandState,
    _window: &mut Window,
    cx: &mut App,
) -> impl IntoElement + use<> {
    h_flex()
        .gap_3()
        .px_3()
        .py_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child("Up/Down to navigate")
        .child("Enter to run")
        .child("Escape to close")
}
