mod entries;
mod paths;
#[cfg(all(test, feature = "gpui-test-support"))]
mod tests;

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
    BrowserEntry, render_browser_list, render_browser_path_field, replace_with_parent_folder,
};
use crate::fulgur::ui::menus::{GoToParentFolder, KEY_CONTEXT_PATH_BROWSER};
use entries::{PathEntry, read_and_filter_entries};
use paths::{home_dir, local_parent_folder, parse_input_path};

const PATH_BROWSER_REFRESH_DEBOUNCE_MS: u64 = 150;
const PATH_BROWSER_REFRESH_DEBOUNCE: Duration =
    Duration::from_millis(PATH_BROWSER_REFRESH_DEBOUNCE_MS);

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
        let default_path = home_dir().map_or_default(|p| {
            let mut s = p.to_string_lossy().into_owned();
            s.push(std::path::MAIN_SEPARATOR);
            s
        });

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
