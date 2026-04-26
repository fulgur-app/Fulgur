use crate::fulgur::sync::ssh::{
    self,
    credentials::{SshCredKey, SshCredentialCache},
    pool::{PooledSession, SshSessionPool},
    sftp::RemoteDirectoryEntry,
    url::RemoteSpec,
};
use gpui::{
    AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement, Styled, Subscription, Window, div, px,
};
use gpui_component::{
    ActiveTheme, h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement,
    v_flex,
};
use std::{
    sync::{Arc, mpsc},
    time::Duration,
};

use crate::fulgur::ui::icons::CustomIcon;

const MAX_VISIBLE_BROWSER_ROWS: usize = 10;
const BROWSER_ROW_HEIGHT_PX: f32 = 28.0;
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
    _input_subscription: Subscription,
}

/// Background-worker request for one remote browser listing.
struct BrowserListRequest {
    raw_input: String,
    response_tx: mpsc::Sender<Result<Vec<RemoteDirectoryEntry>, String>>,
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
        initial_path: String,
        initial_entries: Vec<RemoteDirectoryEntry>,
        notice: Option<String>,
        connection: RemotePathBrowserConnection,
    ) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Enter a remote path...")
                .default_value(initial_path)
        });

        let _input_subscription =
            cx.subscribe(&input, |this: &mut Self, _, ev: &InputEvent, cx| {
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
            _input_subscription,
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
        let input_entity = self.input.clone();
        let mut container = v_flex().w_full().gap_1().child(Input::new(&self.input));

        if let Some(notice) = &self.notice {
            container = container.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(notice.clone()),
            );
        }

        if self.is_loading {
            container = container.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Loading remote directory..."),
            );
        } else if let Some(message) = &self.error_message {
            container = container.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(message.clone()),
            );
        }

        if !self.entries.is_empty() {
            let list_height = browser_list_height();
            let mut list = v_flex().overflow_y_scrollbar().h(list_height).w_full();
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
                    format!("{}/", entry.name)
                } else {
                    entry.name.clone()
                };
                let font_weight = if is_dir {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                };
                let row = h_flex()
                    .id(gpui::SharedString::from(full_path.clone()))
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
                            format!("{full_path}/")
                        } else {
                            full_path.clone()
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

/// Compute a fixed browser-list height so long directories become scrollable.
///
/// ### Returns
/// - `Pixels`: Height fixed to 10 rows, forcing scrollbar overflow after that.
fn browser_list_height() -> gpui::Pixels {
    px(MAX_VISIBLE_BROWSER_ROWS as f32 * BROWSER_ROW_HEIGHT_PX)
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

    if trimmed.ends_with('/') {
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

/// Normalize remote browser path text into an absolute path.
///
/// ### Arguments
/// - `path`: Input path from the browser field.
///
/// ### Returns
/// - `String`: Normalized absolute remote path.
fn normalize_remote_browser_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "~" || trimmed.starts_with("~/") {
        return ssh::REMOTE_ROOT_PATH.to_string();
    }
    if trimmed.starts_with('/') {
        if trimmed.len() > 1 {
            return trimmed.trim_end_matches('/').to_string();
        }
        return ssh::REMOTE_ROOT_PATH.to_string();
    }
    format!("/{}", trimmed.trim_start_matches('/'))
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

    for attempt in 0..=1 {
        let pooled = checkout_browser_session(connection, &directory)?;
        let listing_result: Result<Vec<RemoteDirectoryEntry>, String> = (|| {
            let parent = ssh::sftp::closest_existing_remote_directory(pooled.session(), &directory)
                .map_err(|e| e.user_message())?;
            ssh::sftp::list_remote_directory(pooled.session(), &parent)
                .map_err(|e| e.user_message())
        })();

        match listing_result {
            Ok(mut entries) => {
                if !filter.is_empty() {
                    let filter_lower = filter.to_lowercase();
                    entries.retain(|entry| entry.name.to_lowercase().starts_with(&filter_lower));
                }
                return Ok(entries);
            }
            Err(_) if attempt == 0 => {
                pooled.invalidate();
            }
            Err(error) => {
                pooled.invalidate();
                return Err(error);
            }
        }
    }

    Err("Failed to list remote directory".to_string())
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
    let password = connection
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
        .checkout_or_connect(&spec, &connection.user, &password, |_, _, _| {
            ssh::HostKeyDecision::Reject
        })
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

#[cfg(test)]
mod tests {
    use super::{
        BROWSER_REFRESH_DEBOUNCE_MS, BROWSER_ROW_HEIGHT_PX, MAX_VISIBLE_BROWSER_ROWS,
        browser_list_height, normalize_remote_browser_path, parse_remote_browser_input,
    };
    use gpui::px;

    #[test]
    fn parse_remote_browser_input_handles_directory_path() {
        let (directory, filter) = parse_remote_browser_input("/var/log/");
        assert_eq!(directory, "/var/log");
        assert!(filter.is_empty());
    }

    #[test]
    fn parse_remote_browser_input_handles_file_prefix() {
        let (directory, filter) = parse_remote_browser_input("/var/log/sys");
        assert_eq!(directory, "/var/log");
        assert_eq!(filter, "sys");
    }

    #[test]
    fn normalize_remote_browser_path_defaults_to_root() {
        assert_eq!(
            normalize_remote_browser_path(""),
            super::ssh::REMOTE_ROOT_PATH
        );
        assert_eq!(
            normalize_remote_browser_path("~/tmp"),
            super::ssh::REMOTE_ROOT_PATH
        );
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
