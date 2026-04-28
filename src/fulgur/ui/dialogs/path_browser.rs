use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window,
};
use gpui_component::{
    ActiveTheme,
    input::{Input, InputEvent, InputState},
    v_flex,
};

use super::file_browser::{BrowserEntry, build_browser_entry, render_browser_list};

const PATH_BROWSER_REFRESH_DEBOUNCE_MS: u64 = 150;
const PATH_BROWSER_REFRESH_DEBOUNCE: Duration =
    Duration::from_millis(PATH_BROWSER_REFRESH_DEBOUNCE_MS);

/// A single entry in the file browser list.
struct PathEntry {
    name: String,
    is_dir: bool,
    full_path: PathBuf,
}

impl From<&PathEntry> for BrowserEntry {
    fn from(e: &PathEntry) -> Self {
        let full_path = e.full_path.to_string_lossy().into_owned();
        build_browser_entry(
            gpui::SharedString::from(full_path.clone()),
            e.is_dir,
            &e.name,
            &full_path,
            std::path::MAIN_SEPARATOR,
        )
    }
}

/// A file browser widget that shows a live-updating directory listing
/// below a text input. As the user types a path, the list updates to
/// show matching files and directories.
pub struct PathBrowser {
    input: Entity<InputState>,
    entries: Vec<PathEntry>,
    debounce_generation: u64,
    refresh_generation: u64,
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
        let default_path = home_dir()
            .map(|p| {
                let mut s = p.to_string_lossy().into_owned();
                s.push(std::path::MAIN_SEPARATOR);
                s
            })
            .unwrap_or_default();

        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Enter a file path...")
                .default_value(default_path)
        });
        let _input_subscription =
            cx.subscribe(&input, |this: &mut Self, _, ev: &InputEvent, cx| {
                if let InputEvent::Change = ev {
                    this.schedule_refresh(cx);
                }
            });

        let mut this = Self {
            input,
            entries: Vec::new(),
            debounce_generation: 0,
            refresh_generation: 0,
            _input_subscription,
        };
        this.dispatch_refresh(cx);
        this
    }

    /// Get  a reference to the inner `InputState` entity.
    ///
    /// ### Returns
    /// - `&Entity<InputState>`: A reference to the inner `InputState` entity.
    pub fn input(&self) -> &Entity<InputState> {
        &self.input
    }

    /// Schedule a debounced directory refresh for the current input value.
    ///
    /// ### Description
    /// Each input change wins the race over previous pending timers via the
    /// `debounce_generation` counter, so only the most recent change triggers
    /// a `dispatch_refresh`. This prevents one stat call per keystroke on
    /// network-mounted directories.
    ///
    /// ### Arguments
    /// - `cx`: User interface context.
    fn schedule_refresh(&mut self, cx: &mut Context<Self>) {
        self.debounce_generation = self.debounce_generation.wrapping_add(1);
        let generation = self.debounce_generation;
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(PATH_BROWSER_REFRESH_DEBOUNCE)
                .await;
            let Some(entity) = weak.upgrade() else {
                return;
            };
            entity.update(cx, |this, cx| {
                if this.debounce_generation != generation {
                    return;
                }
                this.dispatch_refresh(cx);
            });
        })
        .detach();
    }

    /// Run a directory listing on the background executor and apply the result.
    ///
    /// ### Arguments
    /// - `cx`: User interface context.
    fn dispatch_refresh(&mut self, cx: &mut Context<Self>) {
        let raw = self.input.read(cx).value().to_string();
        self.refresh_generation = self.refresh_generation.wrapping_add(1);
        let generation = self.refresh_generation;
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let entries = cx
                .background_executor()
                .spawn(async move {
                    match parse_input_path(&raw) {
                        Some((parent, filter)) => read_and_filter_entries(&parent, &filter),
                        None => Vec::new(),
                    }
                })
                .await;

            let Some(entity) = weak.upgrade() else {
                return;
            };
            entity.update(cx, |this, cx| {
                if this.refresh_generation != generation {
                    return;
                }
                this.entries = entries;
                cx.notify();
            });
        })
        .detach();
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
        if let Some(list) = render_browser_list(
            self.entries.iter().map(BrowserEntry::from),
            input_entity,
            cx.theme().muted,
            cx.theme().muted_foreground,
        ) {
            container = container.child(list);
        }
        container
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_input_path, read_and_filter_entries};
    use core::prelude::v1::test;
    use std::path::{Path, PathBuf};
    #[cfg(feature = "gpui-test-support")]
    use tempfile::tempdir;

    #[cfg(feature = "gpui-test-support")]
    use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
    #[cfg(feature = "gpui-test-support")]
    use gpui_component::input::InputState;

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
        let temp_dir = std::env::temp_dir();
        let temp_str = temp_dir.to_string_lossy().to_string() + "/";
        let result = parse_input_path(&temp_str);
        assert!(result.is_some());
        let (parent, filter) = result.unwrap();
        assert!(parent.is_dir());
        assert_eq!(filter, "");
    }

    #[test]
    fn test_partial_name_filter() {
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.join("test_nonexistent_prefix_xyz");
        let result = parse_input_path(&test_path.to_string_lossy());
        assert!(result.is_some());
        let (parent, filter) = result.unwrap();
        assert_eq!(parent, temp_dir);
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
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.to_string_lossy().to_string() + "/.";
        let result = parse_input_path(&test_path);
        assert!(result.is_some());
        let (parent, filter) = result.unwrap();
        assert!(parent.is_dir());
        assert_eq!(filter, ".");
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_gpui_input_change_updates_entries_list(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);

        let temp_dir = tempdir().expect("failed to create temp dir");
        std::fs::create_dir(temp_dir.path().join("alpha_dir"))
            .expect("failed to create alpha_dir fixture");
        std::fs::write(temp_dir.path().join("alpha.txt"), "alpha")
            .expect("failed to create alpha.txt fixture");
        std::fs::write(temp_dir.path().join("beta.txt"), "beta")
            .expect("failed to create beta.txt fixture");

        let window = cx.update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
                cx.new(|cx| super::PathBrowser::new(window, cx))
            })
            .expect("failed to open test window")
        });
        let mut visual_cx = VisualTestContext::from_window(window.into(), cx);
        let browser = window
            .root(&mut visual_cx)
            .expect("failed to read root view from window");
        let input_entity: Entity<InputState> =
            browser.read_with(&visual_cx, |browser, _| browser.input.clone());

        let filter_input = temp_dir.path().join("al").to_string_lossy().to_string();
        visual_cx.update(|window, cx| {
            input_entity.update(cx, |state, cx| {
                state.set_value(&filter_input, window, cx);
            });
        });
        visual_cx
            .background_executor
            .advance_clock(super::PATH_BROWSER_REFRESH_DEBOUNCE);
        visual_cx.run_until_parked();

        let entries: Vec<(String, bool)> = browser.read_with(&visual_cx, |browser, _| {
            browser
                .entries
                .iter()
                .map(|entry| (entry.name.clone(), entry.is_dir))
                .collect::<Vec<_>>()
        });

        assert!(
            !entries.is_empty(),
            "entries should be populated after input change"
        );
        assert!(
            entries
                .iter()
                .all(|(name, _)| name.to_lowercase().starts_with("al")),
            "every entry should match the typed prefix"
        );
        assert!(
            entries.iter().any(|(name, _)| name == "alpha_dir"),
            "directory match should be present"
        );
        assert!(
            entries.iter().any(|(name, _)| name == "alpha.txt"),
            "file match should be present"
        );

        let first_file_idx = entries.iter().position(|(_, is_dir)| !*is_dir);
        let last_dir_idx = entries.iter().rposition(|(_, is_dir)| *is_dir);
        if let (Some(first_file), Some(last_dir)) = (first_file_idx, last_dir_idx) {
            assert!(
                last_dir < first_file,
                "directories should be listed before files"
            );
        }
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_gpui_input_change_to_invalid_path_clears_entries(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);

        let temp_dir = tempdir().expect("failed to create temp dir");
        std::fs::write(temp_dir.path().join("alpha.txt"), "alpha")
            .expect("failed to create alpha.txt fixture");

        let window = cx.update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
                cx.new(|cx| super::PathBrowser::new(window, cx))
            })
            .expect("failed to open test window")
        });
        let mut visual_cx = VisualTestContext::from_window(window.into(), cx);
        let browser = window
            .root(&mut visual_cx)
            .expect("failed to read root view from window");
        let input_entity: Entity<InputState> =
            browser.read_with(&visual_cx, |browser, _| browser.input.clone());

        let valid_input = temp_dir.path().join("al").to_string_lossy().to_string();
        visual_cx.update(|window, cx| {
            input_entity.update(cx, |state, cx| {
                state.set_value(&valid_input, window, cx);
            });
        });
        visual_cx
            .background_executor
            .advance_clock(super::PATH_BROWSER_REFRESH_DEBOUNCE);
        visual_cx.run_until_parked();
        let has_entries = browser.read_with(&visual_cx, |browser, _| !browser.entries.is_empty());
        assert!(has_entries, "valid input should populate entries");

        let invalid_input = temp_dir
            .path()
            .join("missing_parent")
            .join("x")
            .to_string_lossy()
            .to_string();
        visual_cx.update(|window, cx| {
            input_entity.update(cx, |state, cx| {
                state.set_value(&invalid_input, window, cx);
            });
        });
        visual_cx
            .background_executor
            .advance_clock(super::PATH_BROWSER_REFRESH_DEBOUNCE);
        visual_cx.run_until_parked();

        let is_empty = browser.read_with(&visual_cx, |browser, _| browser.entries.is_empty());
        assert!(is_empty, "invalid path should clear entries");
    }
}
