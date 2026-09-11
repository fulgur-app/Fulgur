//! The command palette catalogue.

use super::layout::PaletteContext;
use crate::fulgur::ui::{
    icons::CustomIcon,
    menus::{
        About, CheckForUpdates, ClearRecentFiles, CloseAllFiles, CloseFile, FindAndReplace,
        FindInFile, GetTheme, JumpToLine, KEY_CONTEXT_FULGUR, KEY_CONTEXT_INPUT, NewFile,
        NewWindow, NextTab, OpenFile, OpenPath, OpenRemote, PreviousTab, PrintFile, Quit, SaveFile,
        SaveFileAs, SelectTheme, SettingsTab, ToggleColorPicker,
    },
};
use gpui_kit::Action;
use gpui_kit::base::input::{AddCursorAbove, AddCursorBelow};

/// A titled section of the command palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaletteGroup {
    File,
    Edit,
    Search,
    View,
    Tabs,
    Application,
}

impl PaletteGroup {
    /// Every group, in the order the palette renders them.
    pub const ALL: [Self; 6] = [
        Self::File,
        Self::Edit,
        Self::Search,
        Self::View,
        Self::Tabs,
        Self::Application,
    ];

    /// Get the heading rendered above this group's commands.
    ///
    /// ### Returns
    /// - `&'static str`: The group heading
    pub const fn label(self) -> &'static str {
        match self {
            Self::File => "File",
            Self::Edit => "Edit",
            Self::Search => "Search",
            Self::View => "View",
            Self::Tabs => "Tabs",
            Self::Application => "Application",
        }
    }
}

/// The keybinding hint rendered at the end of a palette row.
pub struct KeybindingHint {
    /// The action whose bound keystroke is displayed.
    pub action: Box<dyn Action>,
    /// The key context the action's binding is registered in.
    pub context: &'static str,
}

impl KeybindingHint {
    /// Build a hint for an action bound in the application key context.
    ///
    /// ### Arguments
    /// - `action`: The action whose bound keystroke to display
    ///
    /// ### Returns
    /// - `Self`: The hint, resolved against the Fulgur key context
    fn fulgur(action: impl Action) -> Self {
        Self {
            action: Box::new(action),
            context: KEY_CONTEXT_FULGUR,
        }
    }

    /// Build a hint for an action the editor binds under its own key context.
    ///
    /// ### Arguments
    /// - `action`: The action whose bound keystroke to display
    ///
    /// ### Returns
    /// - `Self`: The hint, resolved against the editor key context
    fn editor(action: impl Action) -> Self {
        Self {
            action: Box::new(action),
            context: KEY_CONTEXT_INPUT,
        }
    }
}

/// A single command offered by the command palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaletteCommand {
    NewFile,
    NewWindow,
    OpenFile,
    OpenFromPath,
    OpenRemoteFile,
    SaveFile,
    SaveFileAs,
    PrintFile,
    ShareFile,
    CloseFile,
    CloseAllFiles,
    ClearRecentFiles,
    Quit,
    AddCursorAbove,
    AddCursorBelow,
    FindInFile,
    FindAndReplace,
    JumpToLine,
    SelectLanguage,
    ToggleMarkdownPreview,
    ToggleMarkdownToolbar,
    ToggleCsvView,
    ToggleLogView,
    ToggleColorPicker,
    SelectTheme,
    GetMoreThemes,
    NextTab,
    PreviousTab,
    RenameActiveTab,
    DuplicateActiveTab,
    CopyActiveTabPath,
    ShowActiveTabInFileManager,
    CloseTabsToLeft,
    CloseTabsToRight,
    CloseOtherTabs,
    OpenSettings,
    CheckForUpdates,
    About,
}

impl PaletteCommand {
    /// Every command in the catalogue, in the order the palette renders them.
    ///
    /// Grouping is derived from [`Self::group`], so a command added here lands in its
    /// group automatically and cannot go missing from the palette.
    pub const ALL: [Self; 38] = [
        Self::NewFile,
        Self::NewWindow,
        Self::OpenFile,
        Self::OpenFromPath,
        Self::OpenRemoteFile,
        Self::SaveFile,
        Self::SaveFileAs,
        Self::PrintFile,
        Self::ShareFile,
        Self::CloseFile,
        Self::CloseAllFiles,
        Self::ClearRecentFiles,
        Self::Quit,
        Self::AddCursorAbove,
        Self::AddCursorBelow,
        Self::FindInFile,
        Self::FindAndReplace,
        Self::JumpToLine,
        Self::SelectLanguage,
        Self::ToggleMarkdownPreview,
        Self::ToggleMarkdownToolbar,
        Self::ToggleCsvView,
        Self::ToggleLogView,
        Self::ToggleColorPicker,
        Self::SelectTheme,
        Self::GetMoreThemes,
        Self::NextTab,
        Self::PreviousTab,
        Self::RenameActiveTab,
        Self::DuplicateActiveTab,
        Self::CopyActiveTabPath,
        Self::ShowActiveTabInFileManager,
        Self::CloseTabsToLeft,
        Self::CloseTabsToRight,
        Self::CloseOtherTabs,
        Self::OpenSettings,
        Self::CheckForUpdates,
        Self::About,
    ];

    /// Get the group this command is listed under.
    ///
    /// ### Returns
    /// - `PaletteGroup`: The owning group
    pub const fn group(self) -> PaletteGroup {
        match self {
            Self::NewFile
            | Self::NewWindow
            | Self::OpenFile
            | Self::OpenFromPath
            | Self::OpenRemoteFile
            | Self::SaveFile
            | Self::SaveFileAs
            | Self::PrintFile
            | Self::ShareFile
            | Self::CloseFile
            | Self::CloseAllFiles
            | Self::ClearRecentFiles
            | Self::Quit => PaletteGroup::File,
            Self::AddCursorAbove | Self::AddCursorBelow => PaletteGroup::Edit,
            Self::FindInFile | Self::FindAndReplace | Self::JumpToLine => PaletteGroup::Search,
            Self::SelectLanguage
            | Self::ToggleMarkdownPreview
            | Self::ToggleMarkdownToolbar
            | Self::ToggleCsvView
            | Self::ToggleLogView
            | Self::ToggleColorPicker
            | Self::SelectTheme
            | Self::GetMoreThemes => PaletteGroup::View,
            Self::NextTab
            | Self::PreviousTab
            | Self::RenameActiveTab
            | Self::DuplicateActiveTab
            | Self::CopyActiveTabPath
            | Self::ShowActiveTabInFileManager
            | Self::CloseTabsToLeft
            | Self::CloseTabsToRight
            | Self::CloseOtherTabs => PaletteGroup::Tabs,
            Self::OpenSettings | Self::CheckForUpdates | Self::About => PaletteGroup::Application,
        }
    }

    /// Get the label shown in the palette, which is also matched against the query.
    ///
    /// ### Returns
    /// - `&'static str`: The command label
    pub const fn label(self) -> &'static str {
        match self {
            Self::NewFile => "New file",
            Self::NewWindow => "New window",
            Self::OpenFile => "Open file...",
            Self::OpenFromPath => "Open from path...",
            Self::OpenRemoteFile => "Open remote file...",
            Self::SaveFile => "Save",
            Self::SaveFileAs => "Save as...",
            Self::PrintFile => "Print...",
            Self::ShareFile => "Share file...",
            Self::CloseFile => "Close file",
            Self::CloseAllFiles => "Close all files",
            Self::ClearRecentFiles => "Clear recent files",
            Self::Quit => "Quit Fulgur",
            Self::AddCursorAbove => "Add cursor above",
            Self::AddCursorBelow => "Add cursor below",
            Self::FindInFile => "Find in file",
            Self::FindAndReplace => "Find and replace",
            Self::JumpToLine => "Jump to line...",
            Self::SelectLanguage => "Select language...",
            Self::ToggleMarkdownPreview => "Toggle Markdown preview",
            Self::ToggleMarkdownToolbar => "Toggle Markdown toolbar",
            Self::ToggleCsvView => "Toggle CSV table view",
            Self::ToggleLogView => "Toggle log view",
            Self::ToggleColorPicker => "Toggle color picker",
            Self::SelectTheme => "Select theme...",
            Self::GetMoreThemes => "Get more themes...",
            Self::NextTab => "Next tab",
            Self::PreviousTab => "Previous tab",
            Self::RenameActiveTab => "Rename tab...",
            Self::DuplicateActiveTab => "Duplicate tab",
            Self::CopyActiveTabPath => "Copy file path",
            Self::ShowActiveTabInFileManager => "Show in file manager",
            Self::CloseTabsToLeft => "Close tabs to the left",
            Self::CloseTabsToRight => "Close tabs to the right",
            Self::CloseOtherTabs => "Close other tabs",
            Self::OpenSettings => "Open settings",
            Self::CheckForUpdates => "Check for updates",
            Self::About => "About Fulgur",
        }
    }

    /// Get the extra search terms matched alongside the label.
    ///
    /// ### Returns
    /// - `&'static [&'static str]`: The synonyms a user is likely to type instead of the label
    pub const fn keywords(self) -> &'static [&'static str] {
        match self {
            Self::NewFile => &["create", "blank", "buffer", "untitled"],
            Self::NewWindow => &["create", "split"],
            Self::OpenFile => &["load", "browse", "import"],
            Self::OpenFromPath => &["load", "location", "directory", "folder"],
            Self::OpenRemoteFile => &["ssh", "sftp", "server", "host"],
            Self::SaveFile => &["write", "store", "persist"],
            Self::SaveFileAs => &["write", "copy", "duplicate", "rename"],
            Self::PrintFile => &["paper", "pdf"],
            Self::ShareFile => &["send", "sync", "device", "encrypt"],
            Self::CloseFile => &["quit tab", "dismiss"],
            Self::CloseAllFiles => &["dismiss", "everything"],
            Self::ClearRecentFiles => &["history", "forget", "purge"],
            Self::Quit => &["exit", "close application"],
            Self::AddCursorAbove => &["multi cursor", "multiple carets", "column", "up"],
            Self::AddCursorBelow => &["multi cursor", "multiple carets", "column", "down"],
            Self::FindInFile => &["search", "locate", "grep"],
            Self::FindAndReplace => &["search", "substitute", "swap"],
            Self::JumpToLine => &["goto", "go to", "line number"],
            Self::SelectLanguage => &["syntax", "highlighting", "grammar", "mode"],
            Self::ToggleMarkdownPreview => &["md", "render", "html"],
            Self::ToggleMarkdownToolbar => &["md", "format", "bold", "italic"],
            Self::ToggleCsvView => &["spreadsheet", "table", "columns", "grid"],
            Self::ToggleLogView => &["tail", "follow", "trace"],
            Self::ToggleColorPicker => &["colour", "hex", "rgb", "swatch"],
            Self::SelectTheme => &["colour scheme", "appearance", "dark", "light"],
            Self::GetMoreThemes => &["download", "repository", "install"],
            Self::NextTab => &["forward", "right", "cycle"],
            Self::PreviousTab => &["back", "left", "cycle"],
            Self::RenameActiveTab => &["title", "label"],
            Self::DuplicateActiveTab => &["clone", "copy"],
            Self::CopyActiveTabPath => &["clipboard", "location", "filename"],
            Self::ShowActiveTabInFileManager => &["reveal", "finder", "explorer", "folder"],
            Self::CloseTabsToLeft => &["before", "dismiss"],
            Self::CloseTabsToRight => &["after", "dismiss"],
            Self::CloseOtherTabs => &["dismiss", "keep only"],
            Self::OpenSettings => &["preferences", "options", "configuration"],
            Self::CheckForUpdates => &["upgrade", "version", "release"],
            Self::About => &["version", "credits", "licence", "license"],
        }
    }

    /// Get the leading icon for this command.
    ///
    /// ### Returns
    /// - `CustomIcon`: The icon rendered at the start of the command's row
    pub const fn icon(self) -> CustomIcon {
        match self {
            Self::NewFile | Self::PrintFile => CustomIcon::File,
            Self::NewWindow => CustomIcon::Plus,
            Self::OpenFile | Self::OpenFromPath | Self::ShowActiveTabInFileManager => {
                CustomIcon::FolderOpen
            }
            Self::OpenRemoteFile => CustomIcon::Server,
            Self::SaveFile | Self::SaveFileAs => CustomIcon::Save,
            Self::ShareFile => CustomIcon::Upload,
            Self::CloseFile
            | Self::CloseAllFiles
            | Self::CloseTabsToLeft
            | Self::CloseOtherTabs => CustomIcon::Close,
            Self::ClearRecentFiles => CustomIcon::Minus,
            Self::AddCursorAbove => CustomIcon::ChevronUp,
            Self::AddCursorBelow => CustomIcon::ChevronDown,
            Self::Quit => CustomIcon::WindowClose,
            Self::FindInFile => CustomIcon::Search,
            Self::FindAndReplace => CustomIcon::Replace,
            Self::JumpToLine | Self::ToggleLogView => CustomIcon::List,
            Self::SelectLanguage => CustomIcon::Code,
            Self::ToggleMarkdownPreview => CustomIcon::FileCode,
            Self::ToggleMarkdownToolbar | Self::OpenSettings => CustomIcon::Menu,
            Self::ToggleCsvView => CustomIcon::Table,
            Self::ToggleColorPicker | Self::SelectTheme => CustomIcon::Palette,
            Self::GetMoreThemes => CustomIcon::Globe,
            Self::NextTab | Self::CloseTabsToRight => CustomIcon::ChevronRight,
            Self::PreviousTab => CustomIcon::ChevronLeft,
            Self::RenameActiveTab => CustomIcon::ALargeSmall,
            Self::DuplicateActiveTab | Self::CopyActiveTabPath => CustomIcon::Copy,
            Self::CheckForUpdates => CustomIcon::CircleCheck,
            Self::About => CustomIcon::Info,
        }
    }

    /// Get the keybinding hint displayed alongside this command.
    ///
    /// ### Returns
    /// - `Some(KeybindingHint)`: The action to resolve a keystroke from, and its context
    /// - `None`: The command has no dedicated action, so it shows no keybinding hint
    pub fn keybinding_hint(self) -> Option<KeybindingHint> {
        match self {
            Self::NewFile => Some(KeybindingHint::fulgur(NewFile)),
            Self::NewWindow => Some(KeybindingHint::fulgur(NewWindow)),
            Self::OpenFile => Some(KeybindingHint::fulgur(OpenFile)),
            Self::OpenFromPath => Some(KeybindingHint::fulgur(OpenPath)),
            Self::OpenRemoteFile => Some(KeybindingHint::fulgur(OpenRemote)),
            Self::SaveFile => Some(KeybindingHint::fulgur(SaveFile)),
            Self::SaveFileAs => Some(KeybindingHint::fulgur(SaveFileAs)),
            Self::PrintFile => Some(KeybindingHint::fulgur(PrintFile)),
            Self::CloseFile => Some(KeybindingHint::fulgur(CloseFile)),
            Self::CloseAllFiles => Some(KeybindingHint::fulgur(CloseAllFiles)),
            Self::ClearRecentFiles => Some(KeybindingHint::fulgur(ClearRecentFiles)),
            Self::Quit => Some(KeybindingHint::fulgur(Quit)),
            Self::FindInFile => Some(KeybindingHint::fulgur(FindInFile)),
            Self::FindAndReplace => Some(KeybindingHint::fulgur(FindAndReplace)),
            Self::JumpToLine => Some(KeybindingHint::fulgur(JumpToLine)),
            Self::ToggleColorPicker => Some(KeybindingHint::fulgur(ToggleColorPicker)),
            Self::SelectTheme => Some(KeybindingHint::fulgur(SelectTheme)),
            Self::GetMoreThemes => Some(KeybindingHint::fulgur(GetTheme)),
            Self::NextTab => Some(KeybindingHint::fulgur(NextTab)),
            Self::PreviousTab => Some(KeybindingHint::fulgur(PreviousTab)),
            Self::OpenSettings => Some(KeybindingHint::fulgur(SettingsTab)),
            Self::CheckForUpdates => Some(KeybindingHint::fulgur(CheckForUpdates)),
            Self::About => Some(KeybindingHint::fulgur(About)),
            Self::AddCursorAbove => Some(KeybindingHint::editor(AddCursorAbove)),
            Self::AddCursorBelow => Some(KeybindingHint::editor(AddCursorBelow)),
            Self::ShareFile
            | Self::SelectLanguage
            | Self::ToggleMarkdownPreview
            | Self::ToggleMarkdownToolbar
            | Self::ToggleCsvView
            | Self::ToggleLogView
            | Self::RenameActiveTab
            | Self::DuplicateActiveTab
            | Self::CopyActiveTabPath
            | Self::ShowActiveTabInFileManager
            | Self::CloseTabsToLeft
            | Self::CloseTabsToRight
            | Self::CloseOtherTabs => None,
        }
    }

    /// Check whether this command applies to the window state described by `context`.
    ///
    /// Commands that do not apply are left out of the palette entirely rather than
    /// rendered disabled, so the list only ever offers commands that do something.
    ///
    /// ### Arguments
    /// - `context`: The window state snapshot taken when the palette opened
    ///
    /// ### Returns
    /// - `bool`: Whether the command should be offered
    pub const fn is_available(self, context: &PaletteContext) -> bool {
        match self {
            Self::NewFile
            | Self::NewWindow
            | Self::OpenFile
            | Self::OpenFromPath
            | Self::OpenRemoteFile
            | Self::Quit
            | Self::SelectTheme
            | Self::GetMoreThemes
            | Self::OpenSettings
            | Self::CheckForUpdates
            | Self::About => true,
            Self::SaveFile
            | Self::SaveFileAs
            | Self::PrintFile
            | Self::FindInFile
            | Self::FindAndReplace
            | Self::JumpToLine
            | Self::SelectLanguage
            | Self::ToggleColorPicker
            | Self::AddCursorAbove
            | Self::AddCursorBelow
            | Self::DuplicateActiveTab => context.has_editor_tab,
            Self::ShareFile => context.has_editor_tab && context.has_active_sync_profile,
            Self::CloseFile | Self::CloseAllFiles | Self::RenameActiveTab => context.tab_count > 0,
            Self::ClearRecentFiles => context.has_recent_files,
            Self::ToggleMarkdownPreview => context.is_markdown && !context.is_large_file,
            Self::ToggleMarkdownToolbar => context.is_markdown,
            Self::ToggleCsvView => context.is_csv && !context.is_large_file,
            Self::ToggleLogView => context.log_toggle_available,
            Self::NextTab | Self::PreviousTab | Self::CloseOtherTabs => context.tab_count > 1,
            Self::CopyActiveTabPath | Self::ShowActiveTabInFileManager => context.has_local_path,
            Self::CloseTabsToLeft => match context.active_tab_index {
                Some(index) => index > 0,
                None => false,
            },
            Self::CloseTabsToRight => match context.active_tab_index {
                Some(index) => index + 1 < context.tab_count,
                None => false,
            },
        }
    }
}
