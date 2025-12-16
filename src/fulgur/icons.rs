use gpui::*;
use gpui_component::Icon;

#[derive(Clone)]
pub enum CustomIcon {
    ALargeSmall,
    Asterisk,
    Bold,
    CaseSensitive,
    ChevronDown,
    ChevronUp,
    CircleCheck,
    CircleX,
    Close,
    Code,
    Computer,
    File,
    FileCode,
    FolderOpen,
    GitHub,
    Globe,
    Heading1,
    Heading2,
    Heading3,
    Heading4,
    Heading5,
    Info,
    List,
    ListNumbered,
    Italic,
    Laptop,
    Link,
    Minus,
    Plus,
    Quote,
    Replace,
    ReplaceAll,
    Save,
    Search,
    Separator,
    Server,
    Strikethrough,
    Table,
    TaskList,
    TriangleAlert,
    Upload,
    WholeWord,
    WindowClose,
    WindowMaximize,
    WindowMinimize,
    WindowRestore,
    Zap,
    ZapOff,
}

impl CustomIcon {
    // Get the path to the icon
    // @return: The path to the icon
    pub fn path(self) -> SharedString {
        match self {
            Self::ALargeSmall => "icons/a-large-small.svg",
            Self::Asterisk => "icons/asterisk.svg",
            Self::Bold => "icons/bold.svg",
            Self::CaseSensitive => "icons/case-sensitive.svg",
            Self::ChevronDown => "icons/chevron-down.svg",
            Self::ChevronUp => "icons/chevron-up.svg",
            Self::Close => "icons/close.svg",
            Self::Code => "icons/code.svg",
            Self::CircleCheck => "icons/circle-check.svg",
            Self::CircleX => "icons/circle-x.svg",
            Self::Computer => "icons/computer.svg",
            Self::File => "icons/file.svg",
            Self::FileCode => "icons/file-code.svg",
            Self::FolderOpen => "icons/folder-open.svg",
            Self::GitHub => "icons/github.svg",
            Self::Globe => "icons/globe.svg",
            Self::Heading1 => "icons/heading-1.svg",
            Self::Heading2 => "icons/heading-2.svg",
            Self::Heading3 => "icons/heading-3.svg",
            Self::Heading4 => "icons/heading-4.svg",
            Self::Heading5 => "icons/heading-5.svg",
            Self::Info => "icons/info.svg",
            Self::List => "icons/list.svg",
            Self::ListNumbered => "icons/list-ordered.svg",
            Self::Italic => "icons/italic.svg",
            Self::Laptop => "icons/laptop.svg",
            Self::Link => "icons/link.svg",
            Self::Minus => "icons/minus.svg",
            Self::Plus => "icons/plus.svg",
            Self::Quote => "icons/quote.svg",
            Self::Replace => "icons/replace.svg",
            Self::ReplaceAll => "icons/replace-all.svg",
            Self::Save => "icons/save.svg",
            Self::Search => "icons/search.svg",
            Self::Separator => "icons/separator-horizontal.svg",
            Self::Server => "icons/server.svg",
            Self::Table => "icons/table.svg",
            Self::TaskList => "icons/list-todo.svg",
            Self::TriangleAlert => "icons/triangle-alert.svg",
            Self::Upload => "icons/upload.svg",
            Self::Strikethrough => "icons/strikethrough.svg",
            Self::WholeWord => "icons/whole-word.svg",
            Self::WindowClose => "icons/window-close.svg",
            Self::WindowMaximize => "icons/window-maximize.svg",
            Self::WindowMinimize => "icons/window-minimize.svg",
            Self::WindowRestore => "icons/window-restore.svg",
            Self::Zap => "icons/zap.svg",
            Self::ZapOff => "icons/zap-off.svg",
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
