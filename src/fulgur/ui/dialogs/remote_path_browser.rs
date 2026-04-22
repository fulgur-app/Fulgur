use crate::fulgur::sync::ssh::{
    self,
    credentials::{SshCredKey, SshCredentialCache},
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
use std::sync::Arc;

use crate::fulgur::ui::icons::CustomIcon;

const MAX_VISIBLE_BROWSER_ROWS: usize = 10;
const BROWSER_ROW_HEIGHT_PX: f32 = 28.0;

/// Connection details used by the remote path browser when listing directories.
#[derive(Clone)]
pub struct RemotePathBrowserConnection {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub credential_key: SshCredKey,
    pub ssh_session_cache: Arc<parking_lot::Mutex<SshCredentialCache>>,
}

/// Browser state for navigating remote directories over SFTP.
pub struct RemotePathBrowser {
    input: Entity<InputState>,
    entries: Vec<RemoteDirectoryEntry>,
    notice: Option<String>,
    connection: RemotePathBrowserConnection,
    refresh_generation: u64,
    is_loading: bool,
    error_message: Option<String>,
    _input_subscription: Subscription,
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
                    this.refresh_entries(cx);
                }
            });

        let mut this = Self {
            input,
            entries: initial_entries,
            notice,
            connection,
            refresh_generation: 0,
            is_loading: false,
            error_message: None,
            _input_subscription,
        };
        if this.entries.is_empty() {
            this.refresh_entries(cx);
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

    /// Refresh remote directory entries for the current input path.
    ///
    /// ### Arguments
    /// - `cx`: Component context.
    fn refresh_entries(&mut self, cx: &mut Context<Self>) {
        let raw = self.input.read(cx).value().to_string();
        self.refresh_generation += 1;
        let generation = self.refresh_generation;
        self.is_loading = true;
        self.error_message = None;
        self.notice = None;

        let weak = cx.entity().downgrade();
        let connection = self.connection.clone();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { list_entries_for_input(&connection, &raw) })
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
/// - `connection`: Remote connection + credential-cache context.
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
        path: directory.clone(),
        password_in_url: None,
    };

    let session = ssh::session::connect(&spec, &connection.user, &password, |_, _, _| {
        ssh::HostKeyDecision::Reject
    })
    .map_err(|e| e.user_message())?;

    let parent = ssh::sftp::closest_existing_remote_directory(&session, &directory)
        .map_err(|e| e.user_message())?;
    let mut entries =
        ssh::sftp::list_remote_directory(&session, &parent).map_err(|e| e.user_message())?;
    if !filter.is_empty() {
        let filter_lower = filter.to_lowercase();
        entries.retain(|entry| entry.name.to_lowercase().starts_with(&filter_lower));
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::{
        BROWSER_ROW_HEIGHT_PX, MAX_VISIBLE_BROWSER_ROWS, browser_list_height,
        normalize_remote_browser_path, parse_remote_browser_input,
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
}
