use std::path::{Path, PathBuf};

use gpui::*;
use gpui_component::{
    ActiveTheme, h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement,
    v_flex,
};

use crate::fulgur::ui::icons::CustomIcon;

/// A single entry in the file browser list.
struct PathEntry {
    name: String,
    is_dir: bool,
    full_path: PathBuf,
}

/// A file browser widget that shows a live-updating directory listing
/// below a text input. As the user types a path, the list updates to
/// show matching files and directories.
pub struct PathBrowser {
    input: Entity<InputState>,
    entries: Vec<PathEntry>,
    _input_subscription: Subscription,
}

/// Parse the raw input string into a `(parent_directory, filter_prefix)` pair.
///
/// ### Arguments
/// - `raw`: The raw input string from the user
///
/// ### Returns
/// - `Some((parent_directory, filter_prefix))`: if the parent directory exists
/// - `None`: if the parent directory doesn't exist
fn parse_input_path(raw: &str) -> Option<(PathBuf, String)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let expanded = if trimmed == "~" || trimmed.starts_with("~/") || trimmed.starts_with("~\\") {
        let home = home_dir()?;
        if trimmed == "~" {
            home.to_string_lossy().to_string()
        } else {
            home.join(&trimmed[2..]).to_string_lossy().to_string()
        }
    } else {
        trimmed.to_string()
    };
    // Special handling for paths ending with "/." or "\." to filter dotfiles
    // Without this, PathBuf::parent() treats "." as current directory
    if (expanded.ends_with("/.") || expanded.ends_with("\\.")) && expanded.len() > 2 {
        let parent_str = &expanded[..expanded.len() - 1]; // Remove the dot, keep the separator
        let parent = PathBuf::from(parent_str);
        if parent.is_dir() {
            return Some((parent, ".".to_string()));
        }
        return None;
    }
    let path = PathBuf::from(&expanded);
    if expanded.ends_with('/') || expanded.ends_with(std::path::MAIN_SEPARATOR) {
        if path.is_dir() {
            return Some((path, String::new()));
        }
        return None;
    }
    let parent = path.parent()?;
    if !parent.is_dir() {
        return None;
    }
    let filter = path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();
    Some((parent.to_path_buf(), filter))
}

/// Read the directory and filter entries by a case-insensitive prefix.
///
/// ### Arguments
/// - `parent`: The directory to read
/// - `filter`: The case-insensitive prefix to filter entries by
///
/// ### Returns
/// - `Vec<PathEntry>`: sorted with directories first, then files,
///   alphabetical within each group. Capped at 500 entries.
fn read_and_filter_entries(parent: &Path, filter: &str) -> Vec<PathEntry> {
    let read_dir = match std::fs::read_dir(parent) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };
    let filter_lower = filter.to_lowercase();
    let mut entries: Vec<PathEntry> = read_dir
        .filter_map(|e| e.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if !filter_lower.is_empty() && !name.to_lowercase().starts_with(&filter_lower) {
                return None;
            }
            // Follow symlinks for is_dir detection
            let is_dir = entry.metadata().map(|m| m.is_dir()).unwrap_or(false);
            Some(PathEntry {
                full_path: entry.path(),
                name,
                is_dir,
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries.truncate(500);
    entries
}

/// Get the user's home directory.
///
/// ### Returns
/// - `Some<PathBuf>`: The user's home directory.
/// - `None`: If the user's home directory could not be determined.
fn home_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
}

impl PathBrowser {
    /// Create a new `PathBrowser` entity.
    ///
    /// # Arguments
    /// - `window`: The parent window
    /// - `cx`: User interface context
    ///
    /// # Returns
    /// - `PathBrowser`: a new instance
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Enter a file path..."));
        let _input_subscription =
            cx.subscribe(&input, |this: &mut Self, _, ev: &InputEvent, cx| {
                if let InputEvent::Change = ev {
                    this.refresh_entries(cx);
                    cx.notify();
                }
            });

        Self {
            input,
            entries: Vec::new(),
            _input_subscription,
        }
    }

    /// Get  a reference to the inner `InputState` entity.
    ///
    /// ### Returns
    /// - `&Entity<InputState>`: A reference to the inner `InputState` entity.
    pub fn input(&self) -> &Entity<InputState> {
        &self.input
    }

    /// Re-read the directory based on current input value.
    ///
    /// ### Arguments
    /// - `cx`: User interface context
    fn refresh_entries(&mut self, cx: &Context<Self>) {
        let raw = self.input.read(cx).value().to_string();
        self.entries = match parse_input_path(&raw) {
            Some((parent, filter)) => read_and_filter_entries(&parent, &filter),
            None => Vec::new(),
        };
    }
}

impl Render for PathBrowser {
    /// Render the path browser into a UI element.
    ///
    /// ### Arguments
    /// - `_window`: The parent window (unused)
    /// - `cx`: User interface context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered path browser.
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input_entity = self.input.clone();
        let mut container = v_flex().w_full().gap_1().child(Input::new(&self.input));
        if !self.entries.is_empty() {
            let mut list = v_flex()
                .overflow_y_scrollbar()
                .max_h(px(256.))
                .w_full()
                .border_1()
                .border_color(cx.theme().border)
                .rounded(px(4.));
            for entry in &self.entries {
                let full_path = entry.full_path.clone();
                let is_dir = entry.is_dir;
                let input_for_click = input_entity.clone();
                let icon = if is_dir {
                    CustomIcon::FolderOpen
                } else {
                    CustomIcon::File
                };
                let display_name = if is_dir {
                    format!("{}{}", entry.name, std::path::MAIN_SEPARATOR)
                } else {
                    entry.name.clone()
                };
                let font_weight = if is_dir {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                };
                let row = h_flex()
                    .id(SharedString::from(
                        entry.full_path.to_string_lossy().to_string(),
                    ))
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_2()
                    .items_center()
                    .cursor_pointer()
                    .hover(|h| h.bg(cx.theme().muted))
                    .child(
                        icon.icon()
                            .size(px(14.))
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(div().text_sm().font_weight(font_weight).child(display_name))
                    .on_click(move |_, window, cx| {
                        let new_value = if is_dir {
                            format!("{}{}", full_path.display(), std::path::MAIN_SEPARATOR)
                        } else {
                            full_path.display().to_string()
                        };
                        input_for_click.update(cx, |state, cx| {
                            state.set_value(&new_value, window, cx);
                        });
                    });
                list = list.child(row);
            }
            container = container.child(list);
        }
        container
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_input_path, read_and_filter_entries};
    use std::path::{Path, PathBuf};

    #[test]
    fn test_empty_input() {
        assert!(parse_input_path("").is_none());
        assert!(parse_input_path("   ").is_none());
    }

    #[test]
    fn test_root_slash() {
        let result = parse_input_path("/");
        assert!(result.is_some());
        let (parent, filter) = result.unwrap();
        assert_eq!(parent, PathBuf::from("/"));
        assert_eq!(filter, "");
    }

    #[test]
    fn test_directory_with_trailing_slash() {
        // /tmp/ should always exist on Unix
        let result = parse_input_path("/tmp/");
        assert!(result.is_some());
        let (parent, filter) = result.unwrap();
        assert_eq!(parent, PathBuf::from("/tmp/"));
        assert_eq!(filter, "");
    }

    #[test]
    fn test_partial_name_filter() {
        let result = parse_input_path("/tmp/test_nonexistent_prefix_xyz");
        assert!(result.is_some());
        let (parent, filter) = result.unwrap();
        assert_eq!(parent, PathBuf::from("/tmp"));
        assert_eq!(filter, "test_nonexistent_prefix_xyz");
    }

    #[test]
    fn test_nonexistent_parent() {
        let result = parse_input_path("/definitely_not_a_real_directory_abc123/foo");
        assert!(result.is_none());
    }

    #[test]
    fn test_tilde_expansion() {
        let result = parse_input_path("~/");
        assert!(result.is_some());
        let (parent, filter) = result.unwrap();
        // Should expand to home directory, not literally "~/"
        assert_ne!(parent, PathBuf::from("~/"));
        assert!(parent.is_dir());
        assert_eq!(filter, "");
    }

    #[test]
    fn test_read_and_filter_entries_nonexistent() {
        let entries = read_and_filter_entries(Path::new("/no_such_dir_abc123"), "");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_read_and_filter_entries_root() {
        let entries = read_and_filter_entries(Path::new("/"), "");
        assert!(!entries.is_empty());
        // Directories should come first
        let first_file_idx = entries.iter().position(|e| !e.is_dir);
        let last_dir_idx = entries.iter().rposition(|e| e.is_dir);
        if let (Some(first_file), Some(last_dir)) = (first_file_idx, last_dir_idx) {
            assert!(last_dir < first_file);
        }
    }

    #[test]
    fn test_read_and_filter_entries_with_filter() {
        let entries = read_and_filter_entries(Path::new("/"), "t");
        for entry in &entries {
            assert!(entry.name.to_lowercase().starts_with("t"));
        }
    }

    #[test]
    fn test_dotfile_filter() {
        // Test that "/path/." correctly filters for dotfiles
        let result = parse_input_path("/tmp/.");
        assert!(result.is_some());
        let (parent, filter) = result.unwrap();
        assert_eq!(parent, PathBuf::from("/tmp/"));
        assert_eq!(filter, ".");
    }
}
