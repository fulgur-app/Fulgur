use crate::fulgur::{
    Fulgur, editor_tab::TabLocation, tab::Tab, ui::components_utils::UNTITLED,
    utils::atomic_write::atomic_write_file,
};
use gpui::{Context, Focusable, SharedString, Window};
use gpui_component::{WindowExt, notification::NotificationType};
use std::path::Path;

impl Fulgur {
    /// Save a file
    ///
    /// ### Arguments
    /// - `window`: The window to save the file in
    /// - `cx`: The application context
    pub fn save_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        let Some(active_tab_index) = self.active_tab_index else {
            return;
        };
        let active_tab = &self.tabs[active_tab_index];
        let (tab_id, location, content_entity) = match active_tab {
            Tab::Editor(editor_tab) => (
                editor_tab.id,
                editor_tab.location.clone(),
                editor_tab.content.clone(),
            ),
            Tab::Settings(_) | Tab::MarkdownPreview(_) => return,
        };
        if matches!(location, TabLocation::Untitled) {
            self.save_file_as(window, cx);
            return;
        }
        let contents = content_entity.read(cx).text().to_string();
        match location {
            TabLocation::Local(path) => {
                log::debug!("Saving file: {:?} ({} bytes)", path, contents.len());
                if let Err(e) = atomic_write_file(&path, contents.as_bytes()) {
                    log::error!("Failed to save file {path:?}: {e}");
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
                    window.push_notification(
                        (
                            NotificationType::Error,
                            SharedString::from(format!("Failed to save '{file_name}': {e}")),
                        ),
                        cx,
                    );
                    return;
                }
                log::debug!("File saved successfully: {path:?}");
                self.file_watch_state
                    .last_file_saves
                    .insert(path.clone(), std::time::Instant::now());
                if let Tab::Editor(editor_tab) = &mut self.tabs[active_tab_index] {
                    editor_tab.mark_as_saved(cx);
                    editor_tab.update_file_tooltip_cache(contents.len());
                }
                cx.notify();
            }
            TabLocation::Remote(spec) => {
                self.save_remote_file(window, cx, tab_id, spec, contents);
            }
            TabLocation::Untitled => {}
        }
    }

    /// Save a file as
    ///
    /// ### Arguments
    /// - `window`: The window to save the file as in
    /// - `cx`: The application context
    pub fn save_file_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        let Some(active_tab_index) = self.active_tab_index else {
            return;
        };
        let (content_entity, directory, suggested_filename) = match &self.tabs[active_tab_index] {
            Tab::Editor(editor_tab) => {
                let dir = if let Some(path) = editor_tab.file_path() {
                    path.parent()
                        .unwrap_or(std::path::Path::new("."))
                        .to_path_buf()
                } else {
                    std::env::current_dir().unwrap_or_default()
                };
                let suggested = editor_tab.get_suggested_filename();
                (editor_tab.content.clone(), dir, suggested)
            }
            Tab::Settings(_) | Tab::MarkdownPreview(_) => return,
        };
        let path_future = cx.prompt_for_new_path(&directory, suggested_filename.as_deref());
        cx.spawn_in(window, async move |view, window| {
            let path = path_future.await.ok()?.ok()??;
            let contents = window
                .update(|_, cx| content_entity.read(cx).text().to_string())
                .ok()?;
            log::debug!("Saving file as: {:?} ({} bytes)", path, contents.len());
            if let Err(e) = atomic_write_file(&path, contents.as_bytes()) {
                log::error!("Failed to save file {path:?}: {e}");
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
                let message = SharedString::from(format!("Failed to save '{file_name}': {e}"));
                window
                    .update(|_, cx| {
                        _ = view.update(cx, |this, cx| {
                            this.pending_notification = Some((NotificationType::Error, message));
                            cx.notify();
                        });
                    })
                    .ok()?;
                return None;
            }
            log::debug!("File saved successfully as: {path:?}");
            window
                .update(|window, cx| {
                    _ = view.update(cx, |this, cx| {
                        let old_path = if let Some(Tab::Editor(editor_tab)) =
                            this.tabs.get(active_tab_index)
                        {
                            editor_tab.file_path().cloned()
                        } else {
                            None
                        };
                        if let Some(old_path) = old_path {
                            this.unwatch_file(&old_path);
                        }
                        this.file_watch_state
                            .last_file_saves
                            .insert(path.clone(), std::time::Instant::now());
                        if let Some(Tab::Editor(editor_tab)) = this.tabs.get_mut(active_tab_index) {
                            editor_tab.location = TabLocation::Local(path.clone());
                            editor_tab.title = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or(UNTITLED)
                                .to_string()
                                .into();
                            editor_tab.mark_as_saved(cx);
                            editor_tab.update_file_tooltip_cache(contents.len());
                            editor_tab.update_language(window, cx, &this.settings.editor_settings);
                            cx.notify();
                        }
                        this.watch_file(&path);
                    });
                })
                .ok()?;
            Some(())
        })
        .detach();
    }

    /// Show notification when file is reloaded
    ///
    /// ### Arguments
    /// - `path`: The path to the file that was reloaded
    /// - `window`: The window to show the notification in
    /// - `cx`: The application context
    pub(crate) fn show_notification_file_reloaded(
        &self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let message = SharedString::from(format!("File {filename} has been updated externally"));
        window.push_notification((NotificationType::Info, message), cx);
    }

    /// Show notification when file is deleted
    ///
    /// ### Arguments
    /// - `path`: The path to the file that was deleted
    /// - `window`: The window to show the notification in
    /// - `cx`: The application context
    pub(crate) fn show_notification_file_deleted(
        &self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let message = SharedString::from(format!("File '{filename}' deleted externally"));
        window.push_notification((NotificationType::Warning, message), cx);
    }

    /// Show notification when file is renamed
    ///
    /// ### Arguments
    /// - `from`: The path to the file that was renamed from
    /// - `to`: The path to the file that was renamed to
    /// - `window`: The window to show the notification in
    /// - `cx`: The application context
    pub(crate) fn show_notification_file_renamed(
        &self,
        from: &Path,
        to: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old_name = from.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let new_name = to.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let message = SharedString::from(format!("File renamed: {old_name} → {new_name}"));
        window.push_notification((NotificationType::Info, message), cx);
    }

    /// Update search results if the search query has changed
    ///
    /// ### Arguments
    /// - `window`: The window containing the search bar and editor
    /// - `cx`: The application context
    pub fn update_search_if_needed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_state.show_search {
            let current_query = self.search_state.search_input.read(cx).text().to_string();
            if current_query != self.search_state.last_search_query {
                self.perform_search(window, cx);
                // Restore focus to the search input after perform_search
                let search_focus = self.search_state.search_input.read(cx).focus_handle(cx);
                window.focus(&search_focus, cx);
            }
        }
    }

    /// Open the native OS print dialog for the current document
    ///
    /// Writes the active tab's content to a temporary HTML file and opens it with
    /// the system's default browser, which automatically triggers the native print dialog.
    /// This approach works cross-platform without requiring OS-specific print APIs.
    ///
    /// ### Arguments
    /// - `window`: The window containing the editor
    /// - `cx`: The application context
    pub fn print_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active_tab_index) = self.active_tab_index else {
            return;
        };
        let (title, content) = match &self.tabs[active_tab_index] {
            Tab::Editor(editor_tab) => {
                let title = editor_tab.title.clone();
                let content = editor_tab.content.read(cx).text().to_string();
                (title, content)
            }
            Tab::Settings(_) | Tab::MarkdownPreview(_) => return,
        };
        let escaped_content = content
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let escaped_title = title
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>{escaped_title}</title>
<style>
  body {{ margin: 0; padding: 1em; font-family: monospace; white-space: pre-wrap; word-wrap: break-word; }}
  @media print {{ body {{ margin: 0; }} }}
</style>
</head>
<body>{escaped_content}</body>
<script>window.onload = function() {{ window.print(); }};</script>
</html>"#,
        );
        let temp_path =
            std::env::temp_dir().join(format!("fulgur_print_{}.html", std::process::id()));
        if let Err(e) = std::fs::write(&temp_path, html.as_bytes()) {
            log::error!("Failed to write print temp file: {e}");
            window.push_notification(
                (
                    NotificationType::Error,
                    SharedString::from(format!("Failed to prepare print: {e}")),
                ),
                cx,
            );
            return;
        }
        if let Err(e) = open::that(&temp_path) {
            log::error!("Failed to open print file: {e}");
            window.push_notification(
                (
                    NotificationType::Error,
                    SharedString::from(format!("Failed to open print dialog: {e}")),
                ),
                cx,
            );
        }
    }

    /// Handle pending jump-to-line action
    ///
    /// ### Arguments
    /// - `window`: The window containing the editor
    /// - `cx`: The application context
    pub fn handle_pending_jump_to_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(jump) = self.pending_jump.take()
            && let Some(index) = self.active_tab_index
            && let Some(Tab::Editor(editor_tab)) = self.tabs.get_mut(index)
        {
            editor_tab.jump_to_line(window, cx, jump);
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::files::file_operations::test_helpers::setup_fulgur;
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::{editor_tab::TabLocation, tab::Tab};
    #[cfg(feature = "gpui-test-support")]
    use gpui::TestAppContext;
    #[cfg(feature = "gpui-test-support")]
    use tempfile::TempDir;

    // ========== save_file tests ==========

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_save_file_writes_content_to_disk(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("save_test.txt");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(Tab::Editor(editor_tab)) = this.tabs.last_mut() {
                    editor_tab.location = TabLocation::Local(path.clone());
                }
                this.save_file(window, cx);
            });
        });

        assert!(path.exists(), "file should exist after save_file");
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_save_file_marks_tab_as_not_modified(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("mark_saved_test.txt");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(Tab::Editor(editor_tab)) = this.tabs.last_mut() {
                    editor_tab.location = TabLocation::Local(path.clone());
                    editor_tab.modified = true;
                }
                this.save_file(window, cx);
                let modified = this
                    .tabs
                    .last()
                    .and_then(|t| t.as_editor())
                    .map(|e| e.modified)
                    .unwrap_or(true);
                assert!(!modified, "tab should be marked as not modified after save");
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_save_file_is_noop_when_no_active_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.active_tab_index = None;
                this.save_file(window, cx); // Must not panic
            });
        });
    }
}
