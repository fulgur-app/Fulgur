use super::super::{DecodedContents, detect_encoding_and_decode};
use crate::fulgur::{Fulgur, tab::Tab};
use gpui::{Context, Window};
use std::path::Path;

impl Fulgur {
    /// Reload tab content from disk
    ///
    /// ### Arguments
    /// - `tab_index`: The index of the tab to reload
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn reload_tab_from_disk(
        &mut self,
        tab_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = if let Some(Tab::Editor(editor_tab)) =
            self.tabs.get(tab_index).map(|tab| tab.read(cx))
        {
            editor_tab.file_path().cloned()
        } else {
            None
        };
        let Some(path) = path else {
            return;
        };
        log::debug!("Reloading tab content from disk: {}", path.display());
        cx.spawn_in(window, async move |view, window| {
            let read_path = path.clone();
            let read_result = window
                .background_executor()
                .spawn(async move { std::fs::read(&read_path).map(detect_encoding_and_decode) })
                .await;
            match read_result {
                Ok(decoded) => {
                    window
                        .update(|window, cx| {
                            _ = view.update(cx, |this, cx| {
                                this.apply_reloaded_contents(&path, decoded, window, cx);
                            });
                        })
                        .ok();
                }
                Err(e) => {
                    log::error!("Failed to reload file {}: {e}", path.display());
                }
            }
        })
        .detach();
    }

    /// Apply freshly decoded file contents to the editor tab backing a path.
    ///
    /// ### Arguments
    /// - `path`: The path whose tab should receive the reloaded content
    /// - `decoded`: The decoded file contents produced off the UI thread
    /// - `window`: The window context
    /// - `cx`: The application context
    fn apply_reloaded_contents(
        &mut self,
        path: &Path,
        decoded: DecodedContents,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_index) = self.find_tab_by_path(&path.to_path_buf(), cx) else {
            return;
        };
        let Some(tab_entity) = self.tabs.get(tab_index).cloned() else {
            return;
        };
        let settings = self.settings.editor_settings.clone();
        tab_entity.update(cx, |tab, cx| {
            let Some(editor_tab) = tab.as_editor_mut() else {
                return;
            };
            editor_tab.content.update(cx, |input_state, cx| {
                input_state.set_value(&decoded.content, window, cx);
            });
            editor_tab.set_original_content_from_str(&decoded.content);
            editor_tab.encoding = decoded.encoding;
            editor_tab.lossy_decode = decoded.lossy;
            editor_tab.modified = false;
            editor_tab.update_file_tooltip_cache(decoded.byte_len);
            tab.update_language(window, cx, &settings);
            log::debug!("Tab reloaded successfully from disk: {}", path.display());
        });
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::editor_tab::TabLocation;

    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::files::file_operations::test_helpers::setup_fulgur;
    #[cfg(feature = "gpui-test-support")]
    use gpui::TestAppContext;
    #[cfg(feature = "gpui-test-support")]
    use tempfile::TempDir;

    // ========== reload_tab_from_disk tests ==========

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_reload_tab_from_disk_updates_content_from_file(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("reload_test.txt");
        std::fs::write(&path, "initial content").expect("failed to write initial file");

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .last()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, _cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                            editor_tab.set_original_content_from_str("initial content");
                        }
                    });
            });
        });

        std::fs::write(&path, "updated content").expect("failed to overwrite file");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.reload_tab_from_disk(0, window, cx);
            });
        });
        // The read and decode now run on the background executor and are applied
        // asynchronously, so let the spawned task complete before asserting.
        visual_cx.run_until_parked();

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let content = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .map(|e| e.content.read(cx).text().to_string())
                    .unwrap_or_default();
                assert_eq!(content, "updated content");
                let modified = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .is_none_or(|e| e.modified);
                assert!(!modified, "tab should not be marked modified after reload");
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_reload_tab_from_disk_is_noop_without_file_path(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                // The default tab created on Fulgur::new has no file_path
                let initial_content = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .map(|e| e.content.read(cx).text().to_string())
                    .unwrap_or_default();
                this.reload_tab_from_disk(0, window, cx);
                let content_after = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .map(|e| e.content.read(cx).text().to_string())
                    .unwrap_or_default();
                assert_eq!(content_after, initial_content);
            });
        });
    }
}
