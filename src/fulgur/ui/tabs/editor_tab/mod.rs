mod constructors;
mod csv_table;
pub mod hex_color_provider;
mod location;
mod navigation;
mod operations;

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests;

pub use csv_table::CsvTableDelegate;
pub use location::TabLocation;
pub use navigation::{Jump, extract_line_number};

use gpui::{App, AppContext, Context, Entity, SharedString, Window};
use gpui_component::input::{InputState, Rope, TabSize};
use gpui_component::table::TableState;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::SystemTime;

use crate::fulgur::files::csv_support::{DEFAULT_DELIMITER, detect_delimiter, parse_csv};
use crate::fulgur::languages::supported_languages::{SupportedLanguage, language_registry_name};
use crate::fulgur::settings::EditorSettings;
use crate::fulgur::ui::tabs::tab::TabId;

/// Byte-size threshold for large file mode.
pub const LARGE_FILE_THRESHOLD_BYTES: u64 = 50 * 1024 * 1024;

/// Whether a decoded buffer of the given byte length should open in large-file
/// mode.
///
/// ### Arguments
/// - `content_len`: Decoded content length in bytes
///
/// ### Returns
/// - `bool`: `true` when the length exceeds `LARGE_FILE_THRESHOLD_BYTES`
pub fn is_large_file(content_len: usize) -> bool {
    content_len as u64 > LARGE_FILE_THRESHOLD_BYTES
}

/// Which surface a CSV-language tab is currently editing through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsvViewMode {
    Table,
    Text,
}

/// A single editor tab with its content and file metadata
pub struct EditorTab {
    pub id: TabId,
    pub title: SharedString,
    pub content: Entity<InputState>,
    pub location: TabLocation,
    pub modified: bool,
    pub original_content_hash: u64,
    pub original_content_len: usize,
    pub encoding: String,
    /// Whether the file was decoded lossily (undecodable bytes replaced).
    pub lossy_decode: bool,
    pub language: SupportedLanguage,
    pub show_markdown_toolbar: bool,
    pub show_markdown_preview: bool,
    pub file_size_bytes: Option<u64>,
    pub file_last_modified: Option<SystemTime>,
    pub large_file: bool,
    /// Which surface a CSV tab edits through. Always `Text` for non-CSV tabs.
    pub csv_view_mode: CsvViewMode,
    /// The delimiter detected on open and preserved on save (CSV tabs only).
    pub csv_delimiter: u8,
    /// The lazily built table state, rebuilt when the source text changes.
    pub csv_table: Option<Entity<TableState<CsvTableDelegate>>>,
    /// Fingerprint of the text the current `csv_table` was parsed from.
    pub csv_table_source_hash: u64,
    /// Whether the log view (live tail) is active for this tab.
    pub log_view: bool,
    /// Whether the log view auto-scrolls to follow newly appended lines.
    pub log_follow: bool,
    /// Whether the line cap is lifted (user requested loading the full file).
    pub log_full: bool,
    /// Dedicated read-only display buffer for the tailed log, created lazily when
    /// log view first activates. Kept separate from the editable `content` so the
    /// line cap never truncates the saveable buffer.
    pub log_content: Option<Entity<InputState>>,
    /// Subscription to the content entity keeping `modified` current. Owned by
    /// the tab entity, attached by `Tab::attach_content_subscription`, and
    /// replaced whenever the content entity is swapped.
    pub(crate) content_subscription: Option<gpui::Subscription>,
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
    pub lossy_decode: bool,
    pub language: SupportedLanguage,
    pub show_markdown_toolbar: bool,
    pub show_markdown_preview: bool,
    pub file_size_bytes: Option<u64>,
    pub file_last_modified: Option<SystemTime>,
    pub cursor_position: gpui_component::input::Position,
    pub csv_view_mode: CsvViewMode,
    pub csv_delimiter: u8,
    pub log_view: bool,
}

/// Parameters for creating an editor tab as a duplicate of another
pub struct FromDuplicateParams {
    pub id: TabId,
    pub title: SharedString,
    pub current_content: String,
    pub encoding: String,
    pub lossy_decode: bool,
    pub language: SupportedLanguage,
}

/// Compute the initial CSV view mode and delimiter for a freshly opened tab.
///
/// ### Arguments
/// - `language`: The detected language of the tab
/// - `content`: The file content, used to sniff the delimiter
///
/// ### Returns
/// - `(CsvViewMode, u8)`: The initial view mode and delimiter
pub(crate) fn initial_csv_state(language: SupportedLanguage, content: &str) -> (CsvViewMode, u8) {
    if language == SupportedLanguage::Csv {
        let mode = if is_large_file(content.len()) {
            CsvViewMode::Text
        } else {
            CsvViewMode::Table
        };
        (mode, detect_delimiter(content))
    } else {
        (CsvViewMode::Text, DEFAULT_DELIMITER)
    }
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

    /// Ensure the CSV table state is built and reflects the current text.
    ///
    /// ### Arguments
    /// - `window`: The window the table is created in
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(String)`: A warning to surface when the table could not be built
    ///   safely and the tab fell back to text mode
    /// - `None`: When the table was built (or already current)
    pub fn ensure_csv_table(&mut self, window: &mut Window, cx: &mut App) -> Option<String> {
        if self.large_file {
            log::warn!("csv table requested for a large-file tab, falling back to text mode");
            self.csv_view_mode = CsvViewMode::Text;
            self.csv_table = None;
            return None;
        }
        let text = self.content.read(cx).value().to_string();
        let (hash, _len) = content_fingerprint_from_str(&text);
        if let Some(table) = &self.csv_table
            && (self.csv_table_source_hash == hash
                || table.read(cx).delegate().last_commit_hash() == Some(hash))
        {
            self.csv_table_source_hash = hash;
            return None;
        }

        let outcome = parse_csv(&text, self.csv_delimiter);
        if outcome.dropped_records > 0 {
            self.csv_view_mode = CsvViewMode::Text;
            self.csv_table = None;
            return Some(format!(
                "{} malformed CSV row(s) could not be parsed. Showing raw text to avoid data loss.",
                outcome.dropped_records
            ));
        }

        let content = self.content.clone();
        let dialog_input = cx.new(|cx| InputState::new(window, cx));
        let delegate =
            CsvTableDelegate::new(outcome.data, self.csv_delimiter, content, dialog_input);
        let table = cx.new(|cx| {
            TableState::new(delegate, window, cx)
                .cell_selectable(true)
                .row_selectable(true)
                .row_header(false)
        });
        CsvTableDelegate::attach_selection_tracking(&table, cx);

        self.csv_table = Some(table);
        self.csv_table_source_hash = hash;
        None
    }
}

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

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
    pub id: TabId,
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
/// - `large_file`: Whether to open in large-file mode
///
/// ### Returns
/// - `InputState`: The new input state
fn make_input_state(
    window: &mut Window,
    cx: &mut Context<InputState>,
    language_name: &str,
    content: Option<String>,
    settings: &EditorSettings,
    large_file: bool,
) -> InputState {
    // In large-file mode, substitute a language with no registered grammar so
    // the background tree-sitter parser short-circuits and never runs.
    let language_name = if large_file {
        language_registry_name(&SupportedLanguage::Plain)
    } else {
        language_name
    };
    let mut state = InputState::new(window, cx)
        .code_editor(language_name.to_string())
        .line_number(settings.show_line_numbers)
        .indent_guides(settings.show_indent_guides && !large_file)
        .tab_size(TabSize {
            tab_size: settings.tab_size,
            hard_tabs: !settings.use_spaces,
        })
        .soft_wrap(settings.soft_wrap && !large_file)
        .folding(!large_file)
        .show_whitespaces(settings.show_whitespaces && !large_file)
        .default_value(content.unwrap_or_default());

    if settings.highlight_colors && !large_file {
        state.lsp.document_color_provider =
            Some(Rc::new(hex_color_provider::ColorHighlightProvider));
    }

    state
}
