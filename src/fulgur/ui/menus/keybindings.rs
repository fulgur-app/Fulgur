use super::actions::{
    CloseAllFiles, CloseFile, FindInFile, JumpToLine, NewFile, NewWindow, NextTab, OpenFile,
    OpenPath, OpenRemote, PreviousTab, PrintFile, Quit, SaveFile, SaveFileAs, ToggleColorPicker,
};
use gpui::KeyBinding;

/// Key context set on the application content element, used to scope keybindings.
pub const KEY_CONTEXT_FULGUR: &str = "Fulgur";

/// Context predicate for keybindings scoped to the application content.
const SCOPED_BINDING_PREDICATE: &str = "Fulgur || (Fulgur > Input)";

/// Keybinding action target used to map shortcuts to dispatchable Fulgur actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum KeybindingDispatchAction {
    OpenFile,
    NewFile,
    OpenPath,
    OpenRemote,
    NewWindow,
    CloseFile,
    CloseAllFiles,
    Quit,
    SaveFile,
    SaveFileAs,
    FindInFile,
    NextTab,
    PreviousTab,
    JumpToLine,
    PrintFile,
    ToggleColorPicker,
}

/// A platform keybinding dispatch specification used to build runtime keybindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct KeybindingDispatchSpec {
    keystroke: &'static str,
    action: KeybindingDispatchAction,
}

impl KeybindingDispatchSpec {
    /// Create a keybinding dispatch specification.
    ///
    /// ### Parameters:
    /// - `keystroke`: The key combination string consumed by GPUI.
    /// - `action`: The action to dispatch for this keybinding.
    ///
    /// ### Returns:
    /// - `Self`: The new keybinding dispatch specification.
    const fn new(keystroke: &'static str, action: KeybindingDispatchAction) -> Self {
        Self { keystroke, action }
    }

    /// Convert this dispatch specification into a GPUI keybinding instance.
    ///
    /// ### Returns:
    /// - `KeyBinding`: The runtime keybinding bound to the configured action, scoped
    ///   to this action's key context.
    fn into_key_binding(self) -> KeyBinding {
        let context = self.action.key_context();
        match self.action {
            KeybindingDispatchAction::OpenFile => {
                KeyBinding::new(self.keystroke, OpenFile, context)
            }
            KeybindingDispatchAction::NewFile => KeyBinding::new(self.keystroke, NewFile, context),
            KeybindingDispatchAction::OpenPath => {
                KeyBinding::new(self.keystroke, OpenPath, context)
            }
            KeybindingDispatchAction::OpenRemote => {
                KeyBinding::new(self.keystroke, OpenRemote, context)
            }
            KeybindingDispatchAction::NewWindow => {
                KeyBinding::new(self.keystroke, NewWindow, context)
            }
            KeybindingDispatchAction::CloseFile => {
                KeyBinding::new(self.keystroke, CloseFile, context)
            }
            KeybindingDispatchAction::CloseAllFiles => {
                KeyBinding::new(self.keystroke, CloseAllFiles, context)
            }
            KeybindingDispatchAction::Quit => KeyBinding::new(self.keystroke, Quit, context),
            KeybindingDispatchAction::SaveFile => {
                KeyBinding::new(self.keystroke, SaveFile, context)
            }
            KeybindingDispatchAction::SaveFileAs => {
                KeyBinding::new(self.keystroke, SaveFileAs, context)
            }
            KeybindingDispatchAction::FindInFile => {
                KeyBinding::new(self.keystroke, FindInFile, context)
            }
            KeybindingDispatchAction::NextTab => KeyBinding::new(self.keystroke, NextTab, context),
            KeybindingDispatchAction::PreviousTab => {
                KeyBinding::new(self.keystroke, PreviousTab, context)
            }
            KeybindingDispatchAction::JumpToLine => {
                KeyBinding::new(self.keystroke, JumpToLine, context)
            }
            KeybindingDispatchAction::PrintFile => {
                KeyBinding::new(self.keystroke, PrintFile, context)
            }
            KeybindingDispatchAction::ToggleColorPicker => {
                KeyBinding::new(self.keystroke, ToggleColorPicker, context)
            }
        }
    }
}

impl KeybindingDispatchAction {
    /// Get the key context under which this action's binding is active.
    ///
    /// ### Returns
    /// - `Some(&'static str)`: The key context the binding is scoped to
    /// - `None`: The binding is global
    const fn key_context(self) -> Option<&'static str> {
        match self {
            Self::OpenFile
            | Self::NewFile
            | Self::OpenPath
            | Self::OpenRemote
            | Self::NewWindow
            | Self::Quit => None,
            Self::CloseFile
            | Self::CloseAllFiles
            | Self::SaveFile
            | Self::SaveFileAs
            | Self::FindInFile
            | Self::NextTab
            | Self::PreviousTab
            | Self::JumpToLine
            | Self::PrintFile
            | Self::ToggleColorPicker => Some(SCOPED_BINDING_PREDICATE),
        }
    }
}

/// Build platform-specific keybinding dispatch specifications.
///
/// ### Returns:
/// `Vec<KeybindingDispatchSpec>`: The complete keybinding-to-action mapping for this platform.
fn default_keybinding_dispatch_specs() -> Vec<KeybindingDispatchSpec> {
    vec![
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-o", KeybindingDispatchAction::OpenFile),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-o", KeybindingDispatchAction::OpenFile),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-n", KeybindingDispatchAction::NewFile),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-n", KeybindingDispatchAction::NewFile),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-shift-o", KeybindingDispatchAction::OpenPath),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-shift-o", KeybindingDispatchAction::OpenPath),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-shift-r", KeybindingDispatchAction::OpenRemote),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-shift-r", KeybindingDispatchAction::OpenRemote),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-shift-n", KeybindingDispatchAction::NewWindow),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-shift-n", KeybindingDispatchAction::NewWindow),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-w", KeybindingDispatchAction::CloseFile),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-w", KeybindingDispatchAction::CloseFile),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-shift-w", KeybindingDispatchAction::CloseAllFiles),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-shift-w", KeybindingDispatchAction::CloseAllFiles),
        KeybindingDispatchSpec::new("cmd-q", KeybindingDispatchAction::Quit),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("alt-f4", KeybindingDispatchAction::Quit),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-s", KeybindingDispatchAction::SaveFile),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-s", KeybindingDispatchAction::SaveFile),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-shift-s", KeybindingDispatchAction::SaveFileAs),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-shift-s", KeybindingDispatchAction::SaveFileAs),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-f", KeybindingDispatchAction::FindInFile),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-f", KeybindingDispatchAction::FindInFile),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-shift-right", KeybindingDispatchAction::NextTab),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-shift-right", KeybindingDispatchAction::NextTab),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-shift-left", KeybindingDispatchAction::PreviousTab),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-shift-left", KeybindingDispatchAction::PreviousTab),
        KeybindingDispatchSpec::new("ctrl-g", KeybindingDispatchAction::JumpToLine),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-p", KeybindingDispatchAction::PrintFile),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-p", KeybindingDispatchAction::PrintFile),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-shift-c", KeybindingDispatchAction::ToggleColorPicker),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-shift-c", KeybindingDispatchAction::ToggleColorPicker),
    ]
}

/// Build the default runtime keybindings for the application.
///
/// ### Returns:
/// - `Vec<KeyBinding>`: The platform-specific list of GPUI keybindings.
pub fn build_default_key_bindings() -> Vec<KeyBinding> {
    default_keybinding_dispatch_specs()
        .into_iter()
        .map(KeybindingDispatchSpec::into_key_binding)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        KeybindingDispatchAction, build_default_key_bindings, default_keybinding_dispatch_specs,
    };
    use core::prelude::v1::test;
    use std::collections::HashSet;

    fn has_binding(
        specs: &[super::KeybindingDispatchSpec],
        keystroke: &str,
        action: KeybindingDispatchAction,
    ) -> bool {
        specs
            .iter()
            .any(|spec| spec.keystroke == keystroke && spec.action == action)
    }

    #[test]
    fn test_default_keybinding_dispatch_specs_include_core_editor_actions() {
        let specs = default_keybinding_dispatch_specs();

        #[cfg(target_os = "macos")]
        assert!(has_binding(
            &specs,
            "cmd-o",
            KeybindingDispatchAction::OpenFile
        ));
        #[cfg(not(target_os = "macos"))]
        assert!(has_binding(
            &specs,
            "ctrl-o",
            KeybindingDispatchAction::OpenFile
        ));

        #[cfg(target_os = "macos")]
        assert!(has_binding(
            &specs,
            "cmd-s",
            KeybindingDispatchAction::SaveFile
        ));
        #[cfg(not(target_os = "macos"))]
        assert!(has_binding(
            &specs,
            "ctrl-s",
            KeybindingDispatchAction::SaveFile
        ));

        assert!(has_binding(
            &specs,
            "ctrl-g",
            KeybindingDispatchAction::JumpToLine
        ));
    }

    #[test]
    fn test_default_keybinding_dispatch_specs_include_platform_quit_shortcuts() {
        let specs = default_keybinding_dispatch_specs();
        assert!(has_binding(&specs, "cmd-q", KeybindingDispatchAction::Quit));

        #[cfg(not(target_os = "macos"))]
        assert!(has_binding(
            &specs,
            "alt-f4",
            KeybindingDispatchAction::Quit
        ));
    }

    #[test]
    fn test_default_keybinding_dispatch_specs_do_not_duplicate_same_shortcut_action() {
        let specs = default_keybinding_dispatch_specs();
        let unique_pairs: HashSet<(&str, KeybindingDispatchAction)> = specs
            .iter()
            .map(|spec| (spec.keystroke, spec.action))
            .collect();

        assert_eq!(unique_pairs.len(), specs.len());
    }

    #[test]
    fn test_build_default_key_bindings_matches_dispatch_spec_count() {
        let specs = default_keybinding_dispatch_specs();
        let keybindings = build_default_key_bindings();
        assert_eq!(keybindings.len(), specs.len());
    }

    #[test]
    fn test_scoped_predicate_matches_editor_input_depth_but_not_modal_inputs() {
        let scoped =
            gpui::KeyBindingContextPredicate::parse(super::SCOPED_BINDING_PREDICATE).unwrap();
        let input = gpui::KeyBindingContextPredicate::parse("Input").unwrap();

        let editor_stack = [
            gpui::KeyContext::parse("Fulgur").unwrap(),
            gpui::KeyContext::parse("Input").unwrap(),
        ];
        assert_eq!(
            scoped.depth_of(&editor_stack),
            input.depth_of(&editor_stack),
            "scoped bindings must match at the same depth as gpui-component's \
             Input bindings, so Fulgur's later registration wins the tie"
        );

        let dialog_stack = [
            gpui::KeyContext::parse("Dialog").unwrap(),
            gpui::KeyContext::parse("Input").unwrap(),
        ];
        assert_eq!(
            scoped.depth_of(&dialog_stack),
            None,
            "scoped bindings must not fire inside modal inputs"
        );

        let content_stack = [gpui::KeyContext::parse("Fulgur").unwrap()];
        assert!(
            scoped.depth_of(&content_stack).is_some(),
            "scoped bindings must fire when the app content itself is focused"
        );
    }

    #[test]
    fn test_window_level_actions_are_global_and_editor_actions_are_scoped() {
        let window_level = [
            KeybindingDispatchAction::OpenFile,
            KeybindingDispatchAction::NewFile,
            KeybindingDispatchAction::OpenPath,
            KeybindingDispatchAction::OpenRemote,
            KeybindingDispatchAction::NewWindow,
            KeybindingDispatchAction::Quit,
        ];
        for action in window_level {
            assert_eq!(action.key_context(), None, "{action:?} should be global");
        }

        let editor_scoped = [
            KeybindingDispatchAction::CloseFile,
            KeybindingDispatchAction::CloseAllFiles,
            KeybindingDispatchAction::SaveFile,
            KeybindingDispatchAction::SaveFileAs,
            KeybindingDispatchAction::FindInFile,
            KeybindingDispatchAction::NextTab,
            KeybindingDispatchAction::PreviousTab,
            KeybindingDispatchAction::JumpToLine,
            KeybindingDispatchAction::PrintFile,
            KeybindingDispatchAction::ToggleColorPicker,
        ];
        for action in editor_scoped {
            assert_eq!(
                action.key_context(),
                Some(super::SCOPED_BINDING_PREDICATE),
                "{action:?} should be scoped to the application content"
            );
        }
    }
}
