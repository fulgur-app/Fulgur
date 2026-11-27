use super::actions::{FoldAll, FoldAtLevel, ToggleFold, UnfoldAll};
use super::keybindings::KEY_CONTEXT_FULGUR;
use crate::fulgur::editor_tab::{FoldCommand, MAX_FOLD_LEVEL};
use crate::fulgur::ui::components_utils::action_chord;
use gpui_kit::component::{h_flex, menu::PopupMenu};
use gpui_kit::{Action, ParentElement, SharedString, Styled, transparent_white};

/// Get the action that applies a folding command to the active editor.
///
/// ### Arguments
/// - `command`: The folding command
///
/// ### Returns
/// - `Box<dyn Action>`: The action bound to the command's keyboard shortcut
pub fn active_editor_fold_action(command: FoldCommand) -> Box<dyn Action> {
    match command {
        FoldCommand::FoldAll => Box::new(FoldAll),
        FoldCommand::UnfoldAll => Box::new(UnfoldAll),
        FoldCommand::FoldAtLevel(level) => Box::new(FoldAtLevel(level)),
        FoldCommand::ToggleAtCursor => Box::new(ToggleFold),
    }
}

/// Fill a context submenu with the folding commands.
///
/// ### Arguments
/// - `menu`: The submenu to fill
/// - `fold_depth`: The deepest fold level of the buffer; deeper levels are listed disabled
/// - `include_toggle`: Whether to list Toggle Fold, which acts on the buffer's caret
/// - `action_for`: Builds the action dispatched for each command
///
/// ### Returns
/// - `PopupMenu`: The filled submenu
pub fn folding_popup_menu(
    mut menu: PopupMenu,
    fold_depth: usize,
    include_toggle: bool,
    action_for: impl Fn(FoldCommand) -> Box<dyn Action>,
) -> PopupMenu {
    if include_toggle {
        menu = chord_menu_item(
            menu,
            "Toggle Fold",
            action_for(FoldCommand::ToggleAtCursor),
            false,
        );
    }
    menu = chord_menu_item(menu, "Fold All", action_for(FoldCommand::FoldAll), false);
    menu = chord_menu_item(
        menu,
        "Unfold All",
        action_for(FoldCommand::UnfoldAll),
        false,
    )
    .separator();
    for level in 1..=MAX_FOLD_LEVEL {
        menu = chord_menu_item(
            menu,
            format!("Fold Level {level}"),
            action_for(FoldCommand::FoldAtLevel(level)),
            level > fold_depth,
        );
    }
    menu
}

/// Add a popup menu item whose keyboard shortcut shows every keystroke of a chord.
///
/// A plain `PopupMenu::menu` item displays only the first keystroke of its binding.
///
/// ### Arguments
/// - `menu`: The menu to add the item to
/// - `label`: The item label
/// - `action`: The action the item dispatches, whose binding is displayed
/// - `disabled`: Whether the item is disabled
///
/// ### Returns
/// - `PopupMenu`: The menu with the item appended
fn chord_menu_item(
    menu: PopupMenu,
    label: impl Into<SharedString>,
    action: Box<dyn Action>,
    disabled: bool,
) -> PopupMenu {
    let label = label.into();
    let displayed_action = action.boxed_clone();
    menu.menu_element_with_disabled(action, disabled, move |window, _cx| {
        let chord = action_chord(displayed_action.as_ref(), KEY_CONTEXT_FULGUR, window)
            .unwrap_or_default()
            .into_iter()
            .map(|kbd| kbd.p_0().flex_nowrap().border_0().bg(transparent_white()));
        h_flex()
            .flex_1()
            .gap_3()
            .justify_between()
            .child(label.clone())
            .child(h_flex().gap_1().children(chord))
    })
}
