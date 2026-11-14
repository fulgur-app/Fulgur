use super::sync::SynchronizationSettings;
use gpui_kit::SharedString;
use gpui_kit::component::scroll::ScrollbarMode;
use serde::{Deserialize, Serialize};

/// How a tab's color tag is shown in the tab bar.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum TabColorStyle {
    /// Render the tab title in the tag color.
    #[default]
    TextColor,
    /// Render a small colored dot before the tab title.
    Dot,
}

/// How the window title bar and the tab bar are laid out.
///
/// Supported on every platform: macOS keeps room for the traffic lights, Windows and
/// Linux gain a burger menu holding the application menus and the window controls.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, Debug)]
pub enum TitleBarStyle {
    /// A dedicated title bar row above a separate tab bar row.
    #[default]
    Classic,
    /// A single row holding the window controls, the window badge and the tab bar.
    Unified,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    pub confirm_exit: bool,
    #[serde(default = "default_debug_mode")]
    pub debug_mode: bool,
    pub theme: SharedString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scrollbar_show: Option<ScrollbarMode>,
    pub synchronization_settings: SynchronizationSettings,
    /// How a tab's color tag is displayed in the tab bar.
    #[serde(default)]
    pub tab_color_style: TabColorStyle,
    #[serde(default = "default_persist_unsaved_buffers")]
    pub persist_unsaved_buffers: bool,
    /// How the title bar and the tab bar are laid out.
    #[serde(default)]
    pub title_bar_style: TitleBarStyle,
    /// Whether background events also post to the operating system's notification center.
    #[serde(default = "default_system_notifications")]
    pub system_notifications: bool,
}

/// Default value for `debug_mode` setting
///
/// ### Returns
/// - `false`: disable debug mode by default
fn default_debug_mode() -> bool {
    false
}

/// Default value for `system_notifications` setting
///
/// ### Returns
/// - `true`: surface background events in the OS notification center by default
fn default_system_notifications() -> bool {
    true
}

/// Default value for `persist_unsaved_buffers` setting
///
/// ### Returns
/// - `true`: persist unsaved buffer content across restarts by default
fn default_persist_unsaved_buffers() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self::new()
    }
}

impl AppSettings {
    /// Create a new app settings instance
    ///
    /// ### Returns
    /// - `AppSettings`: The new app settings instance
    #[must_use]
    pub fn new() -> Self {
        Self {
            confirm_exit: true,
            theme: "Default Light".into(),
            scrollbar_show: None,
            synchronization_settings: SynchronizationSettings::new(),
            debug_mode: false,
            tab_color_style: TabColorStyle::TextColor,
            persist_unsaved_buffers: default_persist_unsaved_buffers(),
            title_bar_style: TitleBarStyle::Classic,
            system_notifications: default_system_notifications(),
        }
    }

    /// Whether the window should render the unified title bar that embeds the tab bar
    ///
    /// ### Returns
    /// - `true` when the title bar and the tab bar share a single row, `false` otherwise
    #[must_use]
    pub fn uses_unified_title_bar(&self) -> bool {
        self.title_bar_style == TitleBarStyle::Unified
    }
}
