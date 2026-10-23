use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::component::{
    ActiveTheme,
    input::{InputEvent, InputState},
    v_flex,
};
use gpui_kit::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, Styled,
    Subscription, Window,
};

use super::file_browser::{
    BrowserEntry, build_browser_entry, render_browser_list, render_browser_path_field,
    replace_with_parent_folder, truncate_to_parent_folder,
};
use crate::fulgur::ui::menus::{GoToParentFolder, KEY_CONTEXT_PATH_BROWSER};

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
            gpui_kit::SharedString::from(full_path.clone()),
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
    #[allow(dead_code, reason = "RAII guard: keeps the subscription alive")]
    input_subscription: Subscription,
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

/// Truncate a local browser input to its parent folder.
///
/// ### Description
/// A bare `~` stands for the home folder, so its parent is the folder holding
/// the home folder. Roots such as `/`, `C:\` or `\\server\share` have no parent.
///
/// ### Arguments
/// - `raw`: The raw input string from the user
///
/// ### Returns
/// - `Some(String)`: The input up to and including its previous separator
/// - `None`: When the input names a root, or a single relative segment
fn local_parent_folder(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let path = if trimmed.trim_end_matches(std::path::is_separator) == "~" {
        home_dir()?.to_string_lossy().into_owned()
    } else {
        trimmed.to_owned()
    };
    // The platform decides where the roots are: `\\server\share` holds separators
    // but has no parent folder.
    let folder = Path::new(path.trim_end_matches(std::path::is_separator));
    folder
        .parent()
        .and(truncate_to_parent_folder(&path, std::path::is_separator))
        .map(str::to_owned)
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
    let Ok(read_dir) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    let filter_lower = filter.to_lowercase();
    let mut entries: Vec<PathEntry> = read_dir
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if !filter_lower.is_empty() && !name.to_lowercase().starts_with(&filter_lower) {
                return None;
            }
            // Follow symlinks for is_dir detection
            let is_dir = entry.metadata().is_ok_and(|m| m.is_dir());
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

        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Enter a file path..."));
        input.update(cx, |state, cx| {
            state.set_value(&default_path, window, cx);
        });
        let input_subscription = cx.subscribe(&input, |this: &mut Self, _, ev: &InputEvent, cx| {
            if let InputEvent::Change = ev {
                this.schedule_refresh(cx);
            }
        });

        let mut this = Self {
            input,
            entries: Vec::new(),
            debounce_generation: 0,
            refresh_generation: 0,
            input_subscription,
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

    /// Replace the input with its parent folder, which the refresh then lists.
    ///
    /// ### Arguments
    /// - `_action`: The parent folder action
    /// - `window`: The window hosting the browser
    /// - `cx`: User interface context
    fn go_to_parent_folder(
        &mut self,
        _action: &GoToParentFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        replace_with_parent_folder(&self.input, local_parent_folder, window, cx);
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
        let has_parent_folder = local_parent_folder(&self.input.read(cx).value()).is_some();
        let path_field = render_browser_path_field(
            &self.input,
            has_parent_folder,
            cx.listener(|this, _, window, cx| {
                this.go_to_parent_folder(&GoToParentFolder, window, cx);
            }),
        );
        let mut container = v_flex()
            .key_context(KEY_CONTEXT_PATH_BROWSER)
            .on_action(cx.listener(Self::go_to_parent_folder))
            .w_full()
            .gap_1()
            .child(path_field);
        if let Some(list) = render_browser_list(
            self.entries.iter().map(BrowserEntry::from),
            &self.input,
            cx.theme().muted,
            cx.theme().muted_foreground,
        ) {
            container = container.child(list);
        }
        container
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::{home_dir, local_parent_folder, parse_input_path, read_and_filter_entries};
    use core::prelude::v1::test;
    use std::path::{MAIN_SEPARATOR, Path, PathBuf};
    #[cfg(feature = "gpui-test-support")]
    use tempfile::tempdir;

    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::ui::{
        dialogs::file_browser::PARENT_FOLDER_BUTTON_ID, menus::build_default_key_bindings,
    };
    #[cfg(feature = "gpui-test-support")]
    use gpui_kit::component::input::{InputEvent, InputState};
    #[cfg(feature = "gpui-test-support")]
    use gpui_kit::{
        AppContext, Entity, Focusable, Modifiers, TestAppContext, VisualTestContext, WindowOptions,
    };

    /// Open a window hosting a path browser, its input set to a path and listed.
    ///
    /// ### Arguments
    /// - `cx`: The test application context
    /// - `path`: The path to type in the browser input
    ///
    /// ### Returns
    /// - `(Entity<PathBrowser>, Entity<InputState>, VisualTestContext)`: The browser,
    ///   its input and the visual test context of its window
    #[cfg(feature = "gpui-test-support")]
    fn open_browser_at(
        cx: &mut TestAppContext,
        path: &str,
    ) -> (
        Entity<super::PathBrowser>,
        Entity<InputState>,
        VisualTestContext,
    ) {
        cx.update(gpui_kit::init);
        let window = cx.update(|cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                cx.new(|cx| super::PathBrowser::new(window, cx))
            })
            .expect("failed to open test window")
        });
        let mut visual_cx = VisualTestContext::from_window(window.into(), cx);
        let browser = window
            .root(&mut visual_cx)
            .expect("failed to read root view from window");
        let input = browser.read_with(&visual_cx, |browser, _| browser.input.clone());
        visual_cx.update(|window, cx| {
            input.update(cx, |state, cx| {
                state.set_value(path, window, cx);
                cx.emit(InputEvent::Change);
            });
        });
        wait_for_refresh(&mut visual_cx);
        (browser, input, visual_cx)
    }

    /// Let the debounced listing run, then draw a frame so hit testing sees the result.
    ///
    /// ### Arguments
    /// - `visual_cx`: The visual test context of the browser window
    #[cfg(feature = "gpui-test-support")]
    fn wait_for_refresh(visual_cx: &mut VisualTestContext) {
        visual_cx
            .background_executor
            .advance_clock(super::PATH_BROWSER_REFRESH_DEBOUNCE);
        visual_cx.run_until_parked();
        visual_cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    /// Read the browser input value and the names of the listed entries.
    ///
    /// ### Arguments
    /// - `browser`: The path browser
    /// - `visual_cx`: The visual test context of the browser window
    ///
    /// ### Returns
    /// - `(String, Vec<String>)`: The input value and the listed entry names
    #[cfg(feature = "gpui-test-support")]
    fn browser_state(
        browser: &Entity<super::PathBrowser>,
        visual_cx: &VisualTestContext,
    ) -> (String, Vec<String>) {
        browser.read_with(visual_cx, |browser, cx| {
            (
                browser.input.read(cx).value().to_string(),
                browser
                    .entries
                    .iter()
                    .map(|entry| entry.name.clone())
                    .collect(),
            )
        })
    }

    #[test]
    fn test_local_parent_folder_drops_the_last_folder_of_a_folder_path() {
        assert_eq!(local_parent_folder("/var/log/").as_deref(), Some("/var/"));
        assert_eq!(local_parent_folder("/var/").as_deref(), Some("/"));
    }

    #[test]
    fn test_local_parent_folder_drops_only_a_partial_name() {
        assert_eq!(
            local_parent_folder("/var/log/sys").as_deref(),
            Some("/var/log/")
        );
    }

    #[test]
    fn test_local_parent_folder_trims_whitespace() {
        assert_eq!(
            local_parent_folder("  /var/log/  ").as_deref(),
            Some("/var/")
        );
    }

    #[test]
    fn test_local_parent_folder_stops_at_the_root() {
        assert_eq!(local_parent_folder("/"), None);
        assert_eq!(local_parent_folder(""), None);
        assert_eq!(local_parent_folder("notes.txt"), None);
    }

    #[test]
    fn test_local_parent_folder_keeps_the_tilde_below_home() {
        assert_eq!(local_parent_folder("~/Documents/").as_deref(), Some("~/"));
    }

    #[test]
    fn test_local_parent_folder_expands_a_bare_tilde() {
        let home = home_dir().expect("the test environment must define a home folder");
        let home_parent = local_parent_folder(&home.to_string_lossy());
        assert!(home_parent.is_some(), "the home folder must have a parent");
        assert_eq!(local_parent_folder("~"), home_parent);
        assert_eq!(
            local_parent_folder(&format!("~{MAIN_SEPARATOR}")),
            home_parent
        );
    }

    #[cfg(windows)]
    #[test]
    fn test_local_parent_folder_stops_at_windows_roots() {
        assert_eq!(local_parent_folder(r"C:\Users\").as_deref(), Some(r"C:\"));
        assert_eq!(local_parent_folder(r"C:\"), None);
        assert_eq!(local_parent_folder(r"\\server\share\"), None);
        assert_eq!(
            local_parent_folder(r"\\server\share\dir").as_deref(),
            Some(r"\\server\share\")
        );
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
    fn test_gpui_parent_folder_button_lists_the_parent_folder(cx: &mut TestAppContext) {
        let temp_dir = tempdir().expect("failed to create temp dir");
        let child_dir = temp_dir.path().join("child_dir");
        std::fs::create_dir(&child_dir).expect("failed to create child_dir fixture");
        let start = format!("{}{MAIN_SEPARATOR}", child_dir.to_string_lossy());
        let (browser, _input, mut visual_cx) = open_browser_at(cx, &start);

        let bounds = visual_cx
            .debug_bounds(PARENT_FOLDER_BUTTON_ID)
            .expect("the parent folder button should be rendered");
        visual_cx.simulate_click(bounds.center(), Modifiers::none());
        wait_for_refresh(&mut visual_cx);

        let (value, entries) = browser_state(&browser, &visual_cx);
        assert_eq!(
            value,
            format!("{}{MAIN_SEPARATOR}", temp_dir.path().to_string_lossy()),
            "the input must hold the parent folder"
        );
        assert_eq!(entries, ["child_dir"], "the parent folder must be listed");
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
    fn test_gpui_alt_up_goes_to_the_parent_folder_and_undo_restores_the_path(
        cx: &mut TestAppContext,
    ) {
        let temp_dir = tempdir().expect("failed to create temp dir");
        std::fs::write(temp_dir.path().join("alpha.txt"), "alpha")
            .expect("failed to create alpha.txt fixture");
        let start = temp_dir.path().join("al").to_string_lossy().to_string();
        let (browser, input, mut visual_cx) = open_browser_at(cx, &start);
        visual_cx.update(|window, cx| {
            cx.bind_keys(build_default_key_bindings());
            let focus_handle = input.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
        });

        visual_cx.simulate_keystrokes("alt-up");
        wait_for_refresh(&mut visual_cx);
        let (value, entries) = browser_state(&browser, &visual_cx);
        assert_eq!(
            value,
            format!("{}{MAIN_SEPARATOR}", temp_dir.path().to_string_lossy()),
            "alt-up must drop the partial name only"
        );
        assert_eq!(entries, ["alpha.txt"]);

        visual_cx.simulate_keystrokes("secondary-z");
        let (value, _) = browser_state(&browser, &visual_cx);
        assert_eq!(value, start, "undo must restore the previous path");
    }

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
            assert!(entry.name.to_lowercase().starts_with('t'));
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
    #[gpui_kit::test]
    fn test_gpui_input_change_updates_entries_list(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);

        let temp_dir = tempdir().expect("failed to create temp dir");
        std::fs::create_dir(temp_dir.path().join("alpha_dir"))
            .expect("failed to create alpha_dir fixture");
        std::fs::write(temp_dir.path().join("alpha.txt"), "alpha")
            .expect("failed to create alpha.txt fixture");
        std::fs::write(temp_dir.path().join("beta.txt"), "beta")
            .expect("failed to create beta.txt fixture");

        let window = cx.update(|cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
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
                cx.emit(InputEvent::Change);
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
    #[gpui_kit::test]
    fn test_gpui_input_change_to_invalid_path_clears_entries(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);

        let temp_dir = tempdir().expect("failed to create temp dir");
        std::fs::write(temp_dir.path().join("alpha.txt"), "alpha")
            .expect("failed to create alpha.txt fixture");

        let window = cx.update(|cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
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
                cx.emit(InputEvent::Change);
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
                cx.emit(InputEvent::Change);
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
