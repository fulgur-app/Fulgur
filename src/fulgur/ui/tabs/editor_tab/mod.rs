mod constructors;
pub mod hex_color_provider;
mod location;
mod navigation;
mod operations;

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests;

pub use location::TabLocation;
pub use navigation::{Jump, extract_line_number};

use gpui::{Context, Entity, SharedString, Window};
use gpui_component::input::{InputState, Rope, TabSize};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::SystemTime;

use crate::fulgur::languages::supported_languages::SupportedLanguage;
use crate::fulgur::settings::EditorSettings;

/// A single editor tab with its content and file metadata
#[derive(Clone)]
pub struct EditorTab {
    pub id: usize,
    pub title: SharedString,
    pub content: Entity<InputState>,
    pub location: TabLocation,
    pub modified: bool,
    pub original_content_hash: u64,
    pub original_content_len: usize,
    pub encoding: String,
    pub language: SupportedLanguage,
    pub show_markdown_toolbar: bool,
    pub show_markdown_preview: bool,
    pub file_size_bytes: Option<u64>,
    pub file_last_modified: Option<SystemTime>,
}

/// All state required to transfer an editor tab between windows
pub struct TabTransferData {
    pub title: SharedString,
    pub content: String,
    pub location: TabLocation,
    pub modified: bool,
    pub original_content_hash: u64,
    pub original_content_len: usize,
    pub encoding: String,
    pub language: SupportedLanguage,
    pub show_markdown_toolbar: bool,
    pub show_markdown_preview: bool,
    pub file_size_bytes: Option<u64>,
    pub file_last_modified: Option<SystemTime>,
    pub cursor_position: gpui_component::input::Position,
}

/// Parameters for creating an editor tab as a duplicate of another
pub struct FromDuplicateParams {
    pub id: usize,
    pub title: SharedString,
    pub current_content: String,
    pub encoding: String,
    pub language: SupportedLanguage,
}

impl EditorTab {
    /// Return the local filesystem path, if this tab holds a local file.
    ///
    /// ### Returns
    /// - `Some(&PathBuf)`: The local path.
    /// - `None`: If the tab is remote or untitled.
    pub fn file_path(&self) -> Option<&PathBuf> {
        self.location.local_path()
    }
}

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

fn fnv1a_update(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Build a lightweight content fingerprint from a UTF-8 string.
///
/// ### Arguments
/// - `content`: The content string to fingerprint
///
/// ### Returns
/// - `(u64, usize)`: `(hash, byte_len)` for the content
pub(crate) fn content_fingerprint_from_str(content: &str) -> (u64, usize) {
    let hash = fnv1a_update(FNV_OFFSET_BASIS, content.as_bytes());
    (hash, content.len())
}

/// Build a lightweight content fingerprint from a rope.
///
/// ### Arguments
/// - `content`: The rope buffer to fingerprint
///
/// ### Returns
/// - `(u64, usize)`: `(hash, byte_len)` for the content
pub(crate) fn content_fingerprint_from_rope(content: &Rope) -> (u64, usize) {
    let mut hash = FNV_OFFSET_BASIS;
    let mut byte_len = 0;
    for chunk in content.chunks() {
        hash = fnv1a_update(hash, chunk.as_bytes());
        byte_len += chunk.len();
    }
    (hash, byte_len)
}

/// Parameters for creating an editor tab from a file
pub struct FromFileParams {
    pub id: usize,
    pub path: std::path::PathBuf,
    pub contents: String,
    pub encoding: String,
    pub is_modified: bool,
}

/// Create a new input state with syntax highlighting
///
/// ### Arguments
/// - `window`: The window to create the input state in
/// - `cx`: The application context
/// - `language_name`: The language registry name for syntax highlighting
/// - `content`: The content of the input state
/// - `settings`: The settings for the input state
///
/// ### Returns
/// - `InputState`: The new input state
fn make_input_state(
    window: &mut Window,
    cx: &mut Context<InputState>,
    language_name: &str,
    content: Option<String>,
    settings: &EditorSettings,
) -> InputState {
    let mut state = InputState::new(window, cx)
        .code_editor(language_name.to_string())
        .line_number(settings.show_line_numbers)
        .indent_guides(settings.show_indent_guides)
        .tab_size(TabSize {
            tab_size: settings.tab_size,
            hard_tabs: !settings.use_spaces,
        })
        .soft_wrap(settings.soft_wrap)
        .show_whitespaces(settings.show_whitespaces)
        .default_value(content.unwrap_or_default());

    if settings.highlight_colors {
        state.lsp.document_color_provider =
            Some(Rc::new(hex_color_provider::ColorHighlightProvider));
    }

    state
}
