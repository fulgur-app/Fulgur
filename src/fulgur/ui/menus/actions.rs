use gpui::{SharedString, actions};
use gpui_macros::Action;
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
    ]
);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = fulgur, no_json)]
pub struct SwitchTheme(pub SharedString);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = fulgur, no_json)]
pub struct OpenRecentFile(pub PathBuf);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = fulgur, no_json)]
pub struct DockActivateTab(pub PathBuf);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = fulgur, no_json)]
pub struct DockActivateTabByTitle(pub SharedString);
