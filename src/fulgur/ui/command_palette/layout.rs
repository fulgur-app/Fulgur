//! The palette layout: the window state a command's availability depends on, and the
//! grouped command list derived from it.

use super::commands::{PaletteCommand, PaletteGroup};

/// A snapshot of the window state that decides which commands the palette offers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaletteContext {
    /// The number of open tabs in this window.
    pub tab_count: usize,
    /// The position of the active tab, when there is one.
    pub active_tab_index: Option<usize>,
    /// Whether the active tab is an editor tab.
    pub has_editor_tab: bool,
    /// Whether the active tab holds a file on the local filesystem.
    pub has_local_path: bool,
    /// Whether the active tab's language is one of the Markdown flavours.
    pub is_markdown: bool,
    /// Whether the active tab's language is CSV.
    pub is_csv: bool,
    /// Whether the active tab was opened under the large-file guard.
    pub is_large_file: bool,
    /// Whether the active tab's file is eligible for the log tail view.
    pub log_toggle_available: bool,
    /// Whether the recent files list holds at least one entry.
    pub has_recent_files: bool,
    /// Whether synchronization is on and at least one profile is active.
    pub has_active_sync_profile: bool,
}

/// Build the palette layout for a window state: the groups that have at least one
/// available command, each with the commands it offers.
///
/// ### Arguments
/// - `context`: The window state snapshot taken when the palette opened
///
/// ### Returns
/// - `Vec<(PaletteGroup, Vec<PaletteCommand>)>`: The non-empty groups, in render order
pub fn build_palette_layout(context: &PaletteContext) -> Vec<(PaletteGroup, Vec<PaletteCommand>)> {
    PaletteGroup::ALL
        .into_iter()
        .filter_map(|group| {
            let commands: Vec<PaletteCommand> = PaletteCommand::ALL
                .into_iter()
                .filter(|command| command.group() == group && command.is_available(context))
                .collect();
            (!commands.is_empty()).then_some((group, commands))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{PaletteContext, build_palette_layout};
    use crate::fulgur::ui::command_palette::commands::{PaletteCommand, PaletteGroup};
    use crate::fulgur::ui::menus::{KEY_CONTEXT_FULGUR, KEY_CONTEXT_INPUT};
    use core::prelude::v1::test;
    use std::collections::HashSet;

    /// Build a context in which every command in the catalogue is available.
    fn permissive_context() -> PaletteContext {
        PaletteContext {
            tab_count: 3,
            active_tab_index: Some(1),
            has_editor_tab: true,
            has_local_path: true,
            is_markdown: true,
            is_csv: true,
            is_large_file: false,
            log_toggle_available: true,
            has_recent_files: true,
            has_active_sync_profile: true,
        }
    }

    fn flattened(layout: &[(PaletteGroup, Vec<PaletteCommand>)]) -> Vec<PaletteCommand> {
        layout
            .iter()
            .flat_map(|(_, commands)| commands.iter().copied())
            .collect()
    }

    #[test]
    fn the_catalogue_lists_every_command_exactly_once() {
        let mut seen = HashSet::new();
        for command in PaletteCommand::ALL {
            assert!(
                seen.insert(command),
                "{command:?} appears twice in PaletteCommand::ALL"
            );
        }
        assert_eq!(seen.len(), PaletteCommand::ALL.len());
    }

    #[test]
    fn command_labels_are_unique() {
        let mut seen = HashSet::new();
        for command in PaletteCommand::ALL {
            assert!(
                seen.insert(command.label()),
                "two commands share the label {:?}",
                command.label()
            );
        }
    }

    #[test]
    fn a_permissive_context_offers_the_whole_catalogue() {
        let layout = build_palette_layout(&permissive_context());
        let offered: HashSet<PaletteCommand> = flattened(&layout).into_iter().collect();

        for command in PaletteCommand::ALL {
            assert!(
                offered.contains(&command),
                "{command:?} is unreachable even when everything is available"
            );
        }
        assert_eq!(offered.len(), PaletteCommand::ALL.len());
    }

    #[test]
    fn every_offered_command_lands_in_its_own_group() {
        for (group, commands) in build_palette_layout(&permissive_context()) {
            for command in commands {
                assert_eq!(
                    command.group(),
                    group,
                    "{command:?} was listed under {group:?}"
                );
            }
        }
    }

    #[test]
    fn the_layout_never_yields_an_empty_group() {
        for context in [PaletteContext::default(), permissive_context()] {
            for (group, commands) in build_palette_layout(&context) {
                assert!(!commands.is_empty(), "{group:?} was built empty");
            }
        }
    }

    #[test]
    fn an_empty_window_offers_only_the_context_free_commands() {
        let offered: HashSet<PaletteCommand> =
            flattened(&build_palette_layout(&PaletteContext::default()))
                .into_iter()
                .collect();

        let expected = HashSet::from([
            PaletteCommand::NewFile,
            PaletteCommand::NewWindow,
            PaletteCommand::OpenFile,
            PaletteCommand::OpenFromPath,
            PaletteCommand::OpenRemoteFile,
            PaletteCommand::Quit,
            PaletteCommand::SelectTheme,
            PaletteCommand::GetMoreThemes,
            PaletteCommand::OpenSettings,
            PaletteCommand::CheckForUpdates,
            PaletteCommand::About,
        ]);
        assert_eq!(offered, expected);
    }

    #[test]
    fn the_large_file_guard_hides_the_preview_and_table_toggles() {
        let context = PaletteContext {
            is_large_file: true,
            ..permissive_context()
        };
        let offered: HashSet<PaletteCommand> = flattened(&build_palette_layout(&context))
            .into_iter()
            .collect();

        assert!(!offered.contains(&PaletteCommand::ToggleMarkdownPreview));
        assert!(!offered.contains(&PaletteCommand::ToggleCsvView));
        // The toolbar is text editing, not rendering, so the guard does not apply to it.
        assert!(offered.contains(&PaletteCommand::ToggleMarkdownToolbar));
    }

    #[test]
    fn the_directional_tab_closes_follow_the_active_tab_position() {
        let first = PaletteContext {
            active_tab_index: Some(0),
            ..permissive_context()
        };
        assert!(!PaletteCommand::CloseTabsToLeft.is_available(&first));
        assert!(PaletteCommand::CloseTabsToRight.is_available(&first));

        let last = PaletteContext {
            active_tab_index: Some(2),
            ..permissive_context()
        };
        assert!(PaletteCommand::CloseTabsToLeft.is_available(&last));
        assert!(!PaletteCommand::CloseTabsToRight.is_available(&last));
    }

    #[test]
    fn sharing_needs_both_an_editor_tab_and_an_active_profile() {
        let no_profile = PaletteContext {
            has_active_sync_profile: false,
            ..permissive_context()
        };
        assert!(!PaletteCommand::ShareFile.is_available(&no_profile));

        let no_tab = PaletteContext {
            has_editor_tab: false,
            ..permissive_context()
        };
        assert!(!PaletteCommand::ShareFile.is_available(&no_tab));
        assert!(PaletteCommand::ShareFile.is_available(&permissive_context()));
    }

    #[test]
    fn the_multi_cursor_commands_need_an_editor_tab() {
        let no_tab = PaletteContext {
            has_editor_tab: false,
            ..permissive_context()
        };
        for command in [
            PaletteCommand::AddCursorAbove,
            PaletteCommand::AddCursorBelow,
        ] {
            assert!(!command.is_available(&no_tab));
            assert!(command.is_available(&permissive_context()));
            assert_eq!(command.group(), PaletteGroup::Edit);
        }
    }

    #[test]
    fn keybinding_hints_resolve_in_the_context_that_binds_them() {
        // A hint only resolves inside the key context its action was bound under, so a
        // mismatch here would either show nothing or advertise a keystroke belonging to a
        // context the palette row does not speak for.
        for command in PaletteCommand::ALL {
            let Some(hint) = command.keybinding_hint() else {
                continue;
            };
            let expected_namespace = match hint.context {
                KEY_CONTEXT_FULGUR => "fulgur::",
                KEY_CONTEXT_INPUT => "input::",
                other => panic!("{command:?} hints an unknown key context: {other}"),
            };
            assert!(
                hint.action.name().starts_with(expected_namespace),
                "{command:?} hints {} under the {} context",
                hint.action.name(),
                hint.context
            );
        }
    }

    #[test]
    fn the_multi_cursor_commands_hint_the_editor_context() {
        for command in [
            PaletteCommand::AddCursorAbove,
            PaletteCommand::AddCursorBelow,
        ] {
            let hint = command
                .keybinding_hint()
                .unwrap_or_else(|| panic!("{command:?} shows no keybinding hint"));
            assert_eq!(hint.context, KEY_CONTEXT_INPUT);
        }
    }
}
