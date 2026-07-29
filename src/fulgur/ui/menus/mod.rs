//! Application menus, dock/taskbar menus, keybindings, and the action definitions they dispatch.

mod actions;
mod app_menus;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod dock;
mod keybindings;
mod update_check;

pub use actions::*;
pub use app_menus::build_menus;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use dock::DockMenuTab;
#[cfg(target_os = "macos")]
pub use dock::build_dock_menu;
pub use keybindings::{KEY_CONTEXT_FULGUR, build_default_key_bindings};
