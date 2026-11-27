use gpui_kit::Action;
use gpui_kit::{SharedString, actions};
use std::path::PathBuf;

actions!(
    fulgur,
    [
        NoneAction,
        About,
        Quit,
        CloseWindow,
        NewFile,
        NewWindow,
        OpenFile,
        OpenPath,
        OpenRemote,
        SaveFileAs,
        SaveFile,
        CloseFile,
        CloseAllFiles,
        FindInFile,
        FindAndReplace,
        SearchNext,
        SearchPrevious,
        SettingsTab,
        GetTheme,
        NextTab,
        PreviousTab,
        JumpToLine,
        ClearRecentFiles,
        SelectTheme,
        CheckForUpdates,
        PrintFile,
        ToggleColorPicker,
        ToggleCommandPalette,
        ViewLog,
        OpenConfigFolder,
        ShowRemoteUrlHistory,
        GoToParentFolder,
        FoldAll,
        UnfoldAll,
        ToggleFold,
    ]
);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = fulgur, no_json)]
pub struct OpenRecentFile(pub PathBuf);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = fulgur, no_json)]
pub struct DockActivateTab(pub PathBuf);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = fulgur, no_json)]
pub struct DockActivateTabByTitle(pub SharedString);

/// Fold the blocks at one nesting level of the active editor (1 is the outermost)
#[derive(Action, Clone, PartialEq)]
#[action(namespace = fulgur, no_json)]
pub struct FoldAtLevel(pub usize);
