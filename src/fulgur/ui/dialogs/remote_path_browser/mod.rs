mod paths;
mod title;
mod worker;

pub(crate) use paths::normalize_remote_browser_path;
pub use title::BrowserDialogTitle;

use crate::fulgur::sync::ssh::{
    credentials::{SshCredKey, SshCredentialCache},
    pool::SshSessionPool,
    sftp::RemoteDirectoryEntry,
};
use gpui_kit::component::{
    ActiveTheme,
    input::{InputEvent, InputState},
    v_flex,
};
use gpui_kit::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, Styled,
    Subscription, Window, div,
};

use crate::fulgur::ui::menus::{GoToParentFolder, KEY_CONTEXT_PATH_BROWSER};

use super::file_browser::{
    BrowserEntry, build_browser_entry, render_browser_list, render_browser_path_field,
    replace_with_parent_folder,
};
use futures::channel::oneshot;
use paths::remote_parent_folder;
use std::{
    sync::{Arc, mpsc},
    time::Duration,
};
use worker::{BrowserListRequest, spawn_browser_worker};

const BROWSER_REFRESH_DEBOUNCE_MS: u64 = 300;
const BROWSER_REFRESH_DEBOUNCE: Duration = Duration::from_millis(BROWSER_REFRESH_DEBOUNCE_MS);
const BROWSER_WORKER_DISCONNECTED_MESSAGE: &str =
    "Remote browser worker stopped. Re-open the browser dialog.";

/// Connection details used by the remote path browser when listing directories.
#[derive(Clone)]
pub struct RemotePathBrowserConnection {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub credential_key: SshCredKey,
    pub ssh_session_cache: Arc<parking_lot::Mutex<SshCredentialCache>>,
    pub ssh_session_pool: Arc<SshSessionPool>,
}

/// Browser state for navigating remote directories over SFTP.
pub struct RemotePathBrowser {
    input: Entity<InputState>,
    entries: Vec<RemoteDirectoryEntry>,
    notice: Option<String>,
    worker_tx: mpsc::Sender<BrowserListRequest>,
    debounce_generation: u64,
    refresh_generation: u64,
    is_loading: bool,
    error_message: Option<String>,
    #[allow(dead_code, reason = "RAII guard: keeps the subscription alive")]
    input_subscription: Subscription,
}

impl From<&RemoteDirectoryEntry> for BrowserEntry {
    fn from(e: &RemoteDirectoryEntry) -> Self {
        build_browser_entry(
            gpui_kit::SharedString::from(e.full_path.clone()),
            e.is_dir,
            &e.name,
            &e.full_path,
            '/',
        )
    }
}

impl RemotePathBrowser {
    /// Create a remote path browser entity.
    ///
    /// ### Arguments
    /// - `window`: Parent window.
    /// - `cx`: Component context.
    /// - `initial_path`: Initial path displayed in the input field.
    /// - `initial_entries`: Optional preloaded entries for the initial directory.
    /// - `notice`: Optional informational message for the user.
    /// - `connection`: Remote connection metadata + credential-cache access.
    ///
    /// ### Returns
    /// - `RemotePathBrowser`: Initialized remote browser state.
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        initial_path: &str,
        initial_entries: Vec<RemoteDirectoryEntry>,
        notice: Option<String>,
        connection: &RemotePathBrowserConnection,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Enter a remote path..."));
        input.update(cx, |state, cx| {
            state.set_value(initial_path, window, cx);
        });
        let input_subscription = cx.subscribe(&input, |this: &mut Self, _, ev: &InputEvent, cx| {
            if let InputEvent::Change = ev {
                this.schedule_refresh(cx);
            }
        });
        let worker_tx = spawn_browser_worker(connection.clone());

        let mut this = Self {
            input,
            entries: initial_entries,
            notice,
            worker_tx,
            debounce_generation: 0,
            refresh_generation: 0,
            is_loading: false,
            error_message: None,
            input_subscription,
        };
        if this.entries.is_empty() {
            this.dispatch_refresh(cx);
        }
        this
    }

    /// Return the input entity used by this browser.
    ///
    /// ### Returns
    /// - `&Entity<InputState>`: Input state backing the path field.
    pub fn input(&self) -> &Entity<InputState> {
        &self.input
    }

    /// Replace the input with its parent folder, which the refresh then lists.
    ///
    /// ### Arguments
    /// - `_action`: The parent folder action.
    /// - `window`: Window hosting the browser.
    /// - `cx`: Component context.
    fn go_to_parent_folder(
        &mut self,
        _action: &GoToParentFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        replace_with_parent_folder(&self.input, remote_parent_folder, window, cx);
    }

    /// Schedule a debounced remote-directory refresh for the current input path.
    ///
    /// ### Arguments
    /// - `cx`: Component context.
    fn schedule_refresh(&mut self, cx: &mut Context<Self>) {
        self.debounce_generation = self.debounce_generation.wrapping_add(1);
        let generation = self.debounce_generation;
        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(BROWSER_REFRESH_DEBOUNCE)
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

    /// Dispatch a remote-directory refresh request to the browser worker.
    ///
    /// ### Arguments
    /// - `cx`: Component context.
    fn dispatch_refresh(&mut self, cx: &mut Context<Self>) {
        let raw = self.input.read(cx).value().to_string();
        self.refresh_generation = self.refresh_generation.wrapping_add(1);
        let generation = self.refresh_generation;
        self.is_loading = true;
        self.error_message = None;
        self.notice = None;

        let (response_tx, response_rx) = oneshot::channel();
        // A failed send drops `response_tx`, so `response_rx` resolves to `Canceled` below.
        let _ = self.worker_tx.send(BrowserListRequest {
            raw_input: raw,
            response_tx,
        });

        let weak = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = response_rx
                .await
                .unwrap_or_else(|_| Err(BROWSER_WORKER_DISCONNECTED_MESSAGE.to_string()));

            let Some(entity) = weak.upgrade() else {
                return;
            };
            entity.update(cx, |this, cx| {
                if this.refresh_generation != generation {
                    return;
                }
                this.is_loading = false;
                match result {
                    Ok(entries) => {
                        this.entries = entries;
                        this.error_message = None;
                    }
                    Err(message) => {
                        this.entries.clear();
                        this.error_message = Some(message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for RemotePathBrowser {
    /// Render the remote path browser input + listing.
    ///
    /// ### Arguments
    /// - `_window`: Parent window (unused).
    /// - `cx`: Component context.
    ///
    /// ### Returns
    /// - `impl IntoElement`: Rendered browser UI.
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_parent_folder = remote_parent_folder(&self.input.read(cx).value()).is_some();
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

        if let Some(notice) = &self.notice {
            container = container.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(notice.clone()),
            );
        }

        if let Some(message) = &self.error_message {
            container = container.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(message.clone()),
            );
        }

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

#[cfg(test)]
mod tests {
    // Test fixtures cast small integer constants to f32; precision loss is irrelevant.
    #![allow(clippy::cast_precision_loss)]
    use super::super::file_browser::{
        BROWSER_ROW_HEIGHT_PX, MAX_VISIBLE_BROWSER_ROWS, browser_list_height,
    };
    use super::BROWSER_REFRESH_DEBOUNCE_MS;
    use gpui_kit::px;

    #[test]
    fn browser_list_height_is_fixed_to_ten_rows() {
        let expected = px(MAX_VISIBLE_BROWSER_ROWS as f32 * BROWSER_ROW_HEIGHT_PX);
        assert_eq!(browser_list_height(), expected);
    }

    #[test]
    fn browser_refresh_debounce_is_300ms() {
        assert_eq!(BROWSER_REFRESH_DEBOUNCE_MS, 300);
    }
}
