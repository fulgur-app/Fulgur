use gpui::*;
use gpui_component::Icon;

#[derive(Clone)]
pub enum CustomIcon {
    ALargeSmall,
    Asterisk,
    CaseSensitive,
    ChevronDown,
    ChevronUp,
    Close,
    File,
    FolderOpen,
    GitHub,
    Globe,
    Plus,
    Replace,
    ReplaceAll,
    Save,
    Search,
    WholeWord,
    WindowClose,
    WindowMaximize,
    WindowMinimize,
    WindowRestore,
}

impl CustomIcon {
    // Get the path to the icon
    // @return: The path to the icon
    pub fn path(self) -> SharedString {
        match self {
            Self::ALargeSmall => "icons/a-large-small.svg",
            Self::Asterisk => "icons/asterisk.svg",
            Self::CaseSensitive => "icons/case-sensitive.svg",
            Self::ChevronDown => "icons/chevron-down.svg",
            Self::ChevronUp => "icons/chevron-up.svg",
            Self::Close => "icons/close.svg",
            Self::File => "icons/file.svg",
            Self::FolderOpen => "icons/folder-open.svg",
            Self::GitHub => "icons/github.svg",
            Self::Globe => "icons/globe.svg",
            Self::Plus => "icons/plus.svg",
            Self::Replace => "icons/replace.svg",
            Self::ReplaceAll => "icons/replace-all.svg",
            Self::Save => "icons/save.svg",
            Self::Search => "icons/search.svg",
            Self::WholeWord => "icons/whole-word.svg",
            Self::WindowClose => "icons/window-close.svg",
            Self::WindowMaximize => "icons/window-maximize.svg",
            Self::WindowMinimize => "icons/window-minimize.svg",
            Self::WindowRestore => "icons/window-restore.svg",
        }
        .into()
    }

    // Create an Icon from this CustomIcon
    // @return: The Icon
    pub fn icon(self) -> Icon {
        Icon::default().path(self.path())
    }

    // Return the icon as a Entity<Icon>
    // @param cx: The application context
    // @return: The icon as a Entity<Icon>
    pub fn view(self, cx: &mut App) -> Entity<Icon> {
        self.icon().view(cx)
    }
}

impl From<CustomIcon> for Icon {
    // Convert a CustomIcon to an Icon
    // @param val: The CustomIcon to convert
    // @return: The Icon
    fn from(val: CustomIcon) -> Self {
        Icon::default().path(val.path())
    }
}

impl From<CustomIcon> for AnyElement {
    // Convert a CustomIcon to an AnyElement
    // @param val: The CustomIcon to convert
    // @return: The AnyElement
    fn from(val: CustomIcon) -> Self {
        Icon::default().path(val.path()).into_any_element()
    }
}
