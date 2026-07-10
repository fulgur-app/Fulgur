use crate::fulgur::{
    Fulgur,
    tab::{Tab, TabId},
    ui::tabs::editor_tab::{EditorTab, TabTransferData},
};
use gpui::{App, Context, Window};

impl Fulgur {
    /// Extract all transferable state from an editor tab identified by `tab_id`.
    ///
    /// ### Arguments
    /// - `tab_id`: The unique ID of the tab to snapshot
    /// - `cx`: The application context (read-only)
    ///
    /// ### Returns
    /// - `Some(TabTransferData)`: Snapshot of all transferable tab state
    /// - `None`: If `tab_id` does not refer to an existing editor tab
    pub fn extract_tab_transfer_data(&self, tab_id: TabId, cx: &App) -> Option<TabTransferData> {
        let tab = self.tabs.iter().find(|t| t.id() == tab_id)?;
        let editor = tab.as_editor()?;
        let content_state = editor.content.read(cx);
        Some(TabTransferData {
            title: editor.title.clone(),
            content: content_state.text().to_string(),
            location: editor.location.clone(),
            modified: editor.modified,
            original_content_hash: editor.original_content_hash,
            original_content_len: editor.original_content_len,
            encoding: editor.encoding.clone(),
            lossy_decode: editor.lossy_decode,
            language: editor.language,
            show_markdown_toolbar: editor.show_markdown_toolbar,
            show_markdown_preview: editor.show_markdown_preview,
            file_size_bytes: editor.file_size_bytes,
            file_last_modified: editor.file_last_modified,
            cursor_position: content_state.cursor_position(),
            csv_view_mode: editor.csv_view_mode,
            csv_delimiter: editor.csv_delimiter,
            log_view: editor.log_view,
        })
    }

    /// Create a tab in this window from a deferred transfer payload.
    ///
    /// Called from the render loop when `pending_tab_transfer` is set. Creates
    /// the `EditorTab`, starts file-watching if applicable, scrolls to and
    /// focuses the new tab, then persists state.
    ///
    /// ### Arguments
    /// - `window`: The target window context
    /// - `cx`: The application context
    pub fn handle_pending_tab_transfer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(data) = self.pending_tab_transfer.take() {
            let id = self.allocate_tab_id();
            let local_path = data.location.local_path().cloned();
            let cursor_position = data.cursor_position;
            let tab =
                EditorTab::from_transfer(id, data, window, cx, &self.settings.editor_settings);
            let is_log_view = tab.log_view;
            self.tabs.push(Tab::Editor(tab));
            self.active_tab_id = Some(id);
            self.request_tab_scroll(id, cx);
            self.pending_transfer_scroll = Some(cursor_position);
            if let Some(path) = local_path {
                self.watch_file(&path);
            }
            if is_log_view {
                self.activate_log_view(id, window, cx);
            }
            self.focus_active_tab(window, cx);
            self.save_state_async(cx, window);
            cx.notify();
        }
    }

    /// Scroll the active transferred tab to the saved cursor position.
    ///
    /// Called from the render loop one frame after `handle_pending_tab_transfer`,
    /// ensuring the `InputState` has gone through layout so `scroll_to` can
    /// compute pixel offsets. The field is set by `handle_pending_tab_transfer`
    /// and consumed here on the following render cycle.
    ///
    /// ### Arguments
    /// - `window`: The target window context
    /// - `cx`: The application context
    pub fn handle_pending_transfer_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(position) = self.pending_transfer_scroll.take()
            && let Some(Tab::Editor(editor_tab)) = self.active_tab()
        {
            editor_tab.content.clone().update(cx, |state, cx| {
                state.set_cursor_position(position, window, cx);
            });
        }
    }

    /// Remove a tab that was sent to another window.
    ///
    /// Called from the render loop when `pending_tab_removal` is set. Uses
    /// `remove_tab_by_id` so focus management, file-unwatching, and linked
    /// markdown-preview cleanup all happen correctly. If the removed tab was
    /// the last one in this window, the window itself is closed.
    ///
    /// ### Arguments
    /// - `window`: The source window context
    /// - `cx`: The application context
    pub fn handle_pending_tab_removal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab_id) = self.pending_tab_removal.take() {
            self.remove_tab_by_id(tab_id, window, cx);
            if self.tabs.is_empty() {
                window.remove_window();
                return;
            }
            self.save_state_async(cx, window);
        }
    }
}
