use crate::fulgur::{
    Fulgur,
    tab::Tab,
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
    pub fn extract_tab_transfer_data(&self, tab_id: usize, cx: &App) -> Option<TabTransferData> {
        let tab = self.tabs.iter().find(|t| t.id() == tab_id)?;
        let editor = tab.as_editor()?;
        let content_state = editor.content.read(cx);
        Some(TabTransferData {
            title: editor.title.clone(),
            content: content_state.text().to_string(),
            file_path: editor.file_path.clone(),
            modified: editor.modified,
            original_content_hash: editor.original_content_hash,
            original_content_len: editor.original_content_len,
            encoding: editor.encoding.clone(),
            language: editor.language,
            show_markdown_toolbar: editor.show_markdown_toolbar,
            show_markdown_preview: editor.show_markdown_preview,
            file_size_bytes: editor.file_size_bytes,
            file_last_modified: editor.file_last_modified,
            cursor_position: content_state.cursor_position(),
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
            let id = self.next_tab_id;
            self.next_tab_id += 1;
            let file_path = data.file_path.clone();
            let cursor_position = data.cursor_position;
            let tab =
                EditorTab::from_transfer(id, data, window, cx, &self.settings.editor_settings);
            self.tabs.push(Tab::Editor(tab));
            let new_index = self.tabs.len() - 1;
            self.active_tab_index = Some(new_index);
            self.pending_tab_scroll = Some(new_index);
            self.pending_transfer_scroll = Some(cursor_position);
            if let Some(path) = file_path {
                self.watch_file(&path);
            }
            self.focus_active_tab(window, cx);
            if let Err(e) = self.save_state(cx, window) {
                log::error!(
                    "Failed to save state after receiving tab from another window: {}",
                    e
                );
            }
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
            && let Some(index) = self.active_tab_index
            && let Some(Tab::Editor(editor_tab)) = self.tabs.get(index)
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
            if let Err(e) = self.save_state(cx, window) {
                log::error!(
                    "Failed to save state after sending tab to another window: {}",
                    e
                );
            }
        }
    }
}
