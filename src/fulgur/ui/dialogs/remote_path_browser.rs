use crate::fulgur::sync::ssh::{
    self,
    auth::NoPrompt,
    credentials::{SshCredKey, SshCredentialCache},
    pool::{PooledSession, SessionAttempt, SshSessionPool, retry_once_on_reused_session},
    sftp::RemoteDirectoryEntry,
    url::RemoteSpec,
};
use gpui_kit::component::{
    ActiveTheme, Sizable, h_flex,
    input::{InputEvent, InputState},
    spinner::Spinner,
    v_flex,
};
use gpui_kit::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, Styled,
    Subscription, Window, div, prelude::FluentBuilder, px,
};

use crate::fulgur::ui::{
    icons::CustomIcon,
    menus::{GoToParentFolder, KEY_CONTEXT_PATH_BROWSER},
};

use super::file_browser::{
    BrowserEntry, build_browser_entry, render_browser_list, render_browser_path_field,
    replace_with_parent_folder, truncate_to_parent_folder,
};
use std::{
    sync::{Arc, mpsc},
    time::Duration,
};

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

/// Background-worker request for one remote browser listing.
struct BrowserListRequest {
    raw_input: String,
    response_tx: mpsc::Sender<Result<Vec<RemoteDirectoryEntry>, String>>,
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

        let weak = cx.entity().downgrade();
        let worker_tx = self.worker_tx.clone();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let (response_tx, response_rx) = mpsc::channel();
                    worker_tx
                        .send(BrowserListRequest {
                            raw_input: raw,
                            response_tx,
                        })
                        .map_err(|_| BROWSER_WORKER_DISCONNECTED_MESSAGE.to_string())?;
                    response_rx
                        .recv()
                        .map_err(|_| BROWSER_WORKER_DISCONNECTED_MESSAGE.to_string())?
                })
                .await;

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

/// Parse a browser input path into `(directory_to_list, filter_prefix)`.
///
/// ### Arguments
/// - `raw`: Raw input path string.
///
/// ### Returns
/// - `(String, String)`: `(directory, filter)` pair.
fn parse_remote_browser_input(raw: &str) -> (String, String) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (ssh::REMOTE_ROOT_PATH.to_string(), String::new());
    }

    if trimmed == "~" || trimmed.ends_with('/') {
        return (normalize_remote_browser_path(trimmed), String::new());
    }

    if let Some(idx) = trimmed.rfind('/') {
        let parent = if idx == 0 {
            ssh::REMOTE_ROOT_PATH.to_string()
        } else {
            normalize_remote_browser_path(&trimmed[..idx])
        };
        let filter = trimmed[idx + 1..].to_string();
        return (parent, filter);
    }

    (ssh::REMOTE_ROOT_PATH.to_string(), trimmed.to_string())
}

/// Normalize remote browser path text into an absolute or home-relative path.
///
/// ### Arguments
/// - `path`: Input path from the browser field.
///
/// ### Returns
/// - `String`: Normalized absolute or home-relative remote path.
pub(crate) fn normalize_remote_browser_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return ssh::REMOTE_ROOT_PATH.to_string();
    }
    if trimmed == "~" || trimmed.starts_with("~/") {
        return trimmed.trim_end_matches('/').to_string();
    }
    if trimmed.starts_with('/') {
        if trimmed.len() > 1 {
            return trimmed.trim_end_matches('/').to_string();
        }
        return ssh::REMOTE_ROOT_PATH.to_string();
    }
    format!("/{}", trimmed.trim_start_matches('/'))
}

/// Truncate a remote browser input to its parent folder.
///
/// ### Arguments
/// - `raw`: Raw input path string.
///
/// ### Returns
/// - `Some(String)`: The normalized input up to and including its previous `/`.
/// - `None`: When the input normalizes to the remote root or to the home folder `~`.
fn remote_parent_folder(raw: &str) -> Option<String> {
    let path = normalize_remote_browser_path(raw);
    truncate_to_parent_folder(&path, |c| c == '/').map(str::to_owned)
}

/// Fetch and filter remote directory entries for one browser input value.
///
/// ### Arguments
/// - `connection`: Remote connection + credential-cache + session-pool context.
/// - `raw_input`: Path string from the browser input.
///
/// ### Returns
/// - `Ok(Vec<RemoteDirectoryEntry>)`: Entries matching the current prefix filter.
/// - `Err(String)`: If credentials are missing or remote listing failed.
fn list_entries_for_input(
    connection: &RemotePathBrowserConnection,
    raw_input: &str,
) -> Result<Vec<RemoteDirectoryEntry>, String> {
    let (directory, filter) = parse_remote_browser_input(raw_input);

    let mut entries = retry_once_on_reused_session(
        || match checkout_browser_session(connection, &directory) {
            Ok(pooled) => pooled.run(|session| {
                ssh::sftp::closest_existing_remote_directory(session, &directory)
                    .and_then(|parent| ssh::sftp::list_remote_directory(session, &parent))
                    .map_err(|e| e.user_message())
            }),
            Err(error) => SessionAttempt::checkout_failed(error),
        },
        || true,
    )?;
    if !filter.is_empty() {
        let filter_lower = filter.to_lowercase();
        entries.retain(|entry| entry.name.to_lowercase().starts_with(&filter_lower));
    }
    Ok(entries)
}

/// Check out an SSH session for one browser listing call.
///
/// ### Arguments
/// - `connection`: Remote host + credential-cache + session-pool metadata.
/// - `directory`: Current directory context used to seed the remote spec.
///
/// ### Returns
/// - `Ok(PooledSession)`: Pool-managed session ready for SFTP listings.
/// - `Err(String)`: Credentials are missing or SSH connection failed.
fn checkout_browser_session(
    connection: &RemotePathBrowserConnection,
    directory: &str,
) -> Result<PooledSession, String> {
    let cached_auth = connection
        .ssh_session_cache
        .lock()
        .get(&connection.credential_key)
        .cloned()
        .ok_or_else(|| {
            "SSH credentials are no longer available. Re-open the remote URL.".to_string()
        })?;

    let spec = RemoteSpec {
        host: connection.host.clone(),
        port: connection.port,
        user: Some(connection.user.clone()),
        path: directory.to_string(),
        password_in_url: None,
    };

    connection
        .ssh_session_pool
        .checkout_or_connect(
            &spec,
            &connection.user,
            Some(&cached_auth),
            |_, _, _| ssh::HostKeyDecision::Reject,
            &mut NoPrompt,
        )
        .map(|(session, _)| session)
        .map_err(|e| e.user_message())
}

/// Spawn a browser worker thread that pulls fresh sessions from the global pool per request.
///
/// ### Arguments
/// - `connection`: Remote connection + credential-cache + session-pool metadata.
///
/// ### Returns
/// - `mpsc::Sender<BrowserListRequest>`: Request channel for asynchronous listing jobs.
fn spawn_browser_worker(
    connection: RemotePathBrowserConnection,
) -> mpsc::Sender<BrowserListRequest> {
    let (request_tx, request_rx) = mpsc::channel::<BrowserListRequest>();
    std::thread::spawn(move || {
        while let Ok(request) = request_rx.recv() {
            let mut latest_request = request;
            while let Ok(next) = request_rx.try_recv() {
                let _ = latest_request.response_tx.send(Err(
                    "Remote browser request superseded by newer input".to_string(),
                ));
                latest_request = next;
            }

            let result = list_entries_for_input(&connection, &latest_request.raw_input);
            let _ = latest_request.response_tx.send(result);
        }
    });
    request_tx
}

/// Title component for the remote path browser dialog.
///
/// Observes the browser entity and re-renders whenever its loading state
/// changes, showing a spinner aligned to the right while a listing is in
/// flight.
pub struct BrowserDialogTitle {
    browser: Entity<RemotePathBrowser>,
    #[allow(dead_code, reason = "RAII guard: keeps the subscription alive")]
    browser_subscription: Subscription,
}

impl BrowserDialogTitle {
    /// Create a title that tracks the loading state of a remote browser.
    ///
    /// ### Arguments
    /// - `browser`: Remote path browser entity to observe.
    /// - `cx`: Component context.
    ///
    /// ### Returns
    /// - `BrowserDialogTitle`: Initialized title component.
    pub fn new(browser: Entity<RemotePathBrowser>, cx: &mut Context<Self>) -> Self {
        let browser_subscription = cx.observe(&browser, |_, _, cx| {
            cx.notify();
        });
        Self {
            browser,
            browser_subscription,
        }
    }
}

impl Render for BrowserDialogTitle {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_loading = self.browser.read(cx).is_loading;
        h_flex()
            .w_full()
            .items_center()
            .child(div().text_size(px(16.)).child("Browse remote files..."))
            .when(is_loading, |this| {
                this.child(
                    div()
                        .ml_auto()
                        .child(Spinner::new().icon(CustomIcon::LoaderCircle).small()),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    // Test fixtures cast small integer constants to f32; precision loss is irrelevant.
    #![allow(clippy::cast_precision_loss)]
    use super::super::file_browser::{
        BROWSER_ROW_HEIGHT_PX, MAX_VISIBLE_BROWSER_ROWS, browser_list_height,
    };
    use super::{
        BROWSER_REFRESH_DEBOUNCE_MS, normalize_remote_browser_path, parse_remote_browser_input,
        remote_parent_folder,
    };
    use gpui_kit::px;

    #[test]
    fn parse_remote_browser_input_handles_directory_path() {
        let (directory, filter) = parse_remote_browser_input("/var/log/");
        assert_eq!(directory, "/var/log");
        assert_eq!(filter, "");
    }

    #[test]
    fn parse_remote_browser_input_handles_file_prefix() {
        let (directory, filter) = parse_remote_browser_input("/var/log/sys");
        assert_eq!(directory, "/var/log");
        assert_eq!(filter, "sys");
    }

    #[test]
    fn parse_remote_browser_input_lists_the_home_folder() {
        for input in ["~", "~/"] {
            let (directory, filter) = parse_remote_browser_input(input);
            assert_eq!(directory, "~", "{input:?}");
            assert_eq!(filter, "", "{input:?}");
        }
        assert_eq!(
            parse_remote_browser_input("~/notes/"),
            ("~/notes".to_string(), String::new())
        );
        assert_eq!(
            parse_remote_browser_input("~/no"),
            ("~".to_string(), "no".to_string())
        );
    }

    #[test]
    fn normalize_remote_browser_path_defaults_to_root() {
        assert_eq!(
            normalize_remote_browser_path(""),
            super::ssh::REMOTE_ROOT_PATH
        );
    }

    #[test]
    fn normalize_remote_browser_path_keeps_home_relative_paths() {
        let cases = [
            ("~", "~"),
            ("~/", "~"),
            (" ~/tmp ", "~/tmp"),
            ("~/tmp/", "~/tmp"),
        ];
        for (input, expected) in cases {
            assert_eq!(normalize_remote_browser_path(input), expected, "{input:?}");
        }
    }

    #[test]
    fn remote_parent_folder_drops_the_last_folder_of_a_folder_path() {
        assert_eq!(remote_parent_folder("/var/log/").as_deref(), Some("/var/"));
        assert_eq!(remote_parent_folder("/var/").as_deref(), Some("/"));
    }

    #[test]
    fn remote_parent_folder_drops_only_a_partial_name() {
        assert_eq!(
            remote_parent_folder("/var/log/sys").as_deref(),
            Some("/var/log/")
        );
    }

    #[test]
    fn remote_parent_folder_normalizes_relative_input() {
        assert_eq!(remote_parent_folder("var/log").as_deref(), Some("/var/"));
    }

    #[test]
    fn remote_parent_folder_stops_at_the_root() {
        assert_eq!(remote_parent_folder("/"), None);
        assert_eq!(remote_parent_folder(""), None);
    }

    #[test]
    fn remote_parent_folder_stays_within_the_home_folder() {
        assert_eq!(
            remote_parent_folder("~/notes/todo").as_deref(),
            Some("~/notes/")
        );
        assert_eq!(remote_parent_folder("~/notes").as_deref(), Some("~/"));
        assert_eq!(remote_parent_folder("~/"), None);
        assert_eq!(remote_parent_folder("~"), None);
    }

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
