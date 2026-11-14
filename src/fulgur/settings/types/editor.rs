use serde::{Deserialize, Serialize};

/// Determines how the Markdown preview is displayed
#[derive(Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum MarkdownPreviewMode {
    #[default]
    DedicatedTab,
    Panel,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MarkdownSettings {
    #[serde(default)]
    pub preview_mode: MarkdownPreviewMode,
    pub show_markdown_preview: bool,
    pub show_markdown_toolbar: bool,
    /// Allow Markdown previews to fetch images over HTTP and HTTPS.
    #[serde(default)]
    pub load_remote_images: bool,
    #[serde(default = "default_limit_preview_width")]
    pub limit_preview_width: bool,
    #[serde(default = "default_render_frontmatter")]
    pub render_frontmatter: bool,
}

impl Default for MarkdownSettings {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownSettings {
    /// Create a new markdown settings instance
    ///
    /// ### Returns
    /// - `MarkdownSettings`: The new markdown settings instance
    #[must_use]
    pub fn new() -> Self {
        Self {
            preview_mode: MarkdownPreviewMode::DedicatedTab,
            show_markdown_preview: true,
            show_markdown_toolbar: false,
            load_remote_images: false,
            limit_preview_width: true,
            render_frontmatter: true,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct EditorSettings {
    pub show_line_numbers: bool,
    pub show_indent_guides: bool,
    pub soft_wrap: bool,
    #[serde(default = "default_font_family")]
    pub font_family: String,
    pub font_size: f32,
    pub tab_size: usize,
    pub markdown_settings: MarkdownSettings,
    #[serde(default)]
    pub show_whitespaces: bool,
    #[serde(default = "default_watch_files")]
    pub watch_files: bool,
    #[serde(default = "default_use_spaces")]
    pub use_spaces: bool,
    #[serde(default = "default_highlight_colors")]
    pub highlight_colors: bool,
    #[serde(default = "default_auto_close_pairs")]
    pub auto_close_pairs: bool,
    #[serde(default = "default_smart_indent")]
    pub smart_indent: bool,
    /// Whether the caret blinks until the editor goes idle, instead of staying steady.
    #[serde(default = "default_cursor_blink")]
    pub cursor_blink: bool,
}

/// Default value for `watch_files` setting
///
/// ### Returns
/// - `true`: enable file watcher by default
fn default_watch_files() -> bool {
    true
}

/// Default value for `limit_preview_width` setting
///
/// ### Returns
/// - `true`: cap and center the Markdown preview by default
fn default_limit_preview_width() -> bool {
    true
}

/// Default value for `render_frontmatter` setting
///
/// ### Returns
/// - `true`: show YAML frontmatter as a metadata list by default
fn default_render_frontmatter() -> bool {
    true
}

/// Default value for `use_spaces` setting
///
/// ### Returns
/// - `true`: use spaces instead of hard tabs by default
fn default_use_spaces() -> bool {
    true
}

/// Default value for `font_family` setting
fn default_font_family() -> String {
    "Monaco".to_string()
}

/// Default value for `highlight_colors` setting
///
/// ### Returns
/// - `true`: enable hex color highlighting by default
fn default_highlight_colors() -> bool {
    true
}

/// Default value for `auto_close_pairs` setting
///
/// ### Returns
/// - `true`: insert the matching closing delimiter while typing by default
fn default_auto_close_pairs() -> bool {
    true
}

/// Default value for `smart_indent` setting
///
/// ### Returns
/// - `true`: keep and adjust indentation when pressing Enter by default
fn default_smart_indent() -> bool {
    true
}

/// Default value for `cursor_blink` setting
///
/// ### Returns
/// - `true`: blink the caret until the editor goes idle by default
fn default_cursor_blink() -> bool {
    true
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorSettings {
    /// Create a new editor settings instance
    ///
    /// ### Returns
    /// - `EditorSettings`: The new editor settings instance
    #[must_use]
    pub fn new() -> Self {
        Self {
            show_line_numbers: true,
            show_indent_guides: true,
            soft_wrap: false,
            font_family: default_font_family(),
            font_size: 14.0,
            tab_size: 4,
            show_whitespaces: false,
            markdown_settings: MarkdownSettings::new(),
            watch_files: default_watch_files(),
            use_spaces: default_use_spaces(),
            highlight_colors: default_highlight_colors(),
            auto_close_pairs: default_auto_close_pairs(),
            smart_indent: default_smart_indent(),
            cursor_blink: default_cursor_blink(),
        }
    }
}
