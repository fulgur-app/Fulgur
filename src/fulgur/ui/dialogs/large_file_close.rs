use anyhow::anyhow;
use gpui_kit::component::{WindowExt, notification::NotificationType};
use gpui_kit::{App, Context, SharedString, Window};
use std::path::Path;

use crate::fulgur::Fulgur;
use crate::fulgur::files::file_operations::{
    EncodedContents, RemoteSaveContinuation, RemoteSaveRequest, encode_for_save,
};
use crate::fulgur::state::file_modified_system_time;
use crate::fulgur::sync::ssh::url::RemoteSpec;
use crate::fulgur::ui::tabs::editor_tab::{EditorTab, TabLocation};
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::utils::atomic_write::atomic_write_file;

impl Fulgur {
    /// Whether closing would silently drop an editor tab's buffer because the
    /// session snapshot cannot hold it, under this window's persistence setting.
    ///
    /// ### Arguments
    /// - `editor_tab`: The editor tab to inspect
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `bool`: `true` when the close flows must offer to save the tab first
    pub(crate) fn editor_needs_large_file_close_warning(
        &self,
        editor_tab: &EditorTab,
        cx: &App,
    ) -> bool {
        editor_tab
            .needs_large_file_close_warning(self.settings.app_settings.persist_unsaved_buffers, cx)
    }

    /// Collect the ids of editor tabs whose buffer would be silently dropped on
    /// close because it is too large to persist.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Vec<TabId>`: The ids of the tabs needing a large-file warning, in tab order
    pub fn large_unpersistable_tabs(&self, cx: &App) -> Vec<TabId> {
        self.tabs
            .iter()
            .filter_map(|tab| {
                let editor_tab = tab.read(cx).as_editor()?;
                self.editor_needs_large_file_close_warning(editor_tab, cx)
                    .then_some(editor_tab.id)
            })
            .collect()
    }

    /// Save a tab named by a large-file close warning, then run `resume`.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to save
    /// - `resume`: Continues the close once the tab is saved
    /// - `window`: The window owning the tab
    /// - `cx`: The application context
    pub(super) fn save_tab_for_close(
        &mut self,
        tab_id: TabId,
        resume: RemoteSaveContinuation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let location = self
            .tab_entity_of(tab_id, cx)
            .and_then(|tab| Some(tab.read(cx).as_editor()?.location.clone()));
        let result = match location {
            Some(TabLocation::Local(path)) => self
                .write_tab_blocking(tab_id, &path, cx)
                .map(|()| resume(self, window, cx)),
            Some(TabLocation::Remote(spec)) => {
                self.save_remote_tab_for_close(tab_id, spec, resume, window, cx)
            }
            Some(TabLocation::Untitled | TabLocation::Shared(_)) => {
                self.save_tab_as_for_close(tab_id, resume, window, cx);
                Ok(())
            }
            None => Err(anyhow!("tab no longer exists")),
        };
        if let Err(e) = result {
            self.report_close_save_failure(tab_id, &e, window, cx);
        }
    }

    /// Ask where to save an untitled or shared tab, write it there, then run `resume`.
    ///
    /// Cancelling the path picker stops the close.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to save
    /// - `resume`: Continues the close once the tab is saved
    /// - `window`: The window owning the tab
    /// - `cx`: The application context
    fn save_tab_as_for_close(
        &mut self,
        tab_id: TabId,
        resume: RemoteSaveContinuation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path_future) = self.prompt_for_save_as_path(tab_id, cx) else {
            return;
        };
        cx.spawn_in(window, async move |view, window| {
            let Some(path) = path_future.await else {
                log::debug!("Save As cancelled from a large-file close warning");
                return;
            };
            window
                .update(|window, cx| {
                    _ = view.update(cx, |this, cx| {
                        match this.write_tab_blocking(tab_id, &path, cx) {
                            Ok(()) => {
                                let canonical = std::fs::canonicalize(&path).unwrap_or(path);
                                this.retarget_tab_to_saved_file(tab_id, &canonical, cx);
                                resume(this, window, cx);
                            }
                            Err(e) => this.report_close_save_failure(tab_id, &e, window, cx),
                        }
                    });
                })
                .ok();
        })
        .detach();
    }

    /// Dispatch the SSH save of a remote tab, running `resume` once it succeeds.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to save
    /// - `spec`: The remote location of the tab
    /// - `resume`: Continues the close once the tab is saved
    /// - `window`: The window owning the tab
    /// - `cx`: The application context
    ///
    /// ### Errors
    /// - Returns an error if the tab is missing or its content cannot be encoded
    ///   losslessly.
    ///
    /// ### Returns
    /// - `Ok(())`: The save was dispatched
    /// - `Err(anyhow::Error)`: Nothing was sent to the remote host
    fn save_remote_tab_for_close(
        &mut self,
        tab_id: TabId,
        spec: RemoteSpec,
        resume: RemoteSaveContinuation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let (contents, bytes) = self.encode_tab_for_close(tab_id, cx)?;
        let request = RemoteSaveRequest {
            tab_id,
            spec,
            contents,
            bytes,
            on_saved: Some(resume),
        };
        self.save_remote_file(window, cx, request);
        Ok(())
    }

    /// Encode a tab's buffer for a close-time save, which cannot stop to
    /// confirm a lossy conversion.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to encode
    /// - `cx`: The application context
    ///
    /// ### Errors
    /// - Returns an error if the tab is missing, is not an editor tab, was decoded
    ///   lossily, or holds text its encoding cannot represent.
    ///
    /// ### Returns
    /// - `Ok((String, Vec<u8>))`: The buffer text and its encoded bytes
    /// - `Err(anyhow::Error)`: The tab must be saved manually
    fn encode_tab_for_close(&self, tab_id: TabId, cx: &App) -> anyhow::Result<(String, Vec<u8>)> {
        let tab = self
            .tab_entity_of(tab_id, cx)
            .ok_or_else(|| anyhow!("tab no longer exists"))?;
        let tab = tab.read(cx);
        let editor_tab = tab
            .as_editor()
            .ok_or_else(|| anyhow!("tab is not an editor tab"))?;
        if editor_tab.lossy_decode {
            return Err(anyhow!(
                "file was decoded lossily; save it manually to confirm the conversion"
            ));
        }
        let contents = editor_tab.content.read(cx).text().to_string();
        match encode_for_save(&contents, &editor_tab.encoding) {
            EncodedContents::Encoded(bytes) => Ok((contents, bytes)),
            EncodedContents::Lossy => Err(anyhow!(
                "content cannot be represented in {}; save it manually",
                editor_tab.encoding
            )),
        }
    }

    /// Encode and write a tab to a local path synchronously, updating its baseline.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to write
    /// - `path`: The destination file
    /// - `cx`: The application context
    ///
    /// ### Errors
    /// - Returns an error if the tab is missing, is not an editor tab, its
    ///   content cannot be encoded losslessly, or the disk write fails.
    ///
    /// ### Returns
    /// - `Ok(())`: The file was written and the tab marked as saved
    /// - `Err(anyhow::Error)`: The file could not be written
    pub(super) fn write_tab_blocking(
        &mut self,
        tab_id: TabId,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let (_, bytes) = self.encode_tab_for_close(tab_id, cx)?;
        let byte_len = bytes.len();
        atomic_write_file(path, &bytes)?;
        self.record_local_file_save(path);
        let saved_modified = file_modified_system_time(path);
        self.update_editor_tab(tab_id, cx, |editor_tab, cx| {
            editor_tab.mark_as_saved(cx);
            editor_tab.record_file_metadata(byte_len, saved_modified);
            cx.notify();
        });
        Ok(())
    }

    /// Log and notify a failed close-time save.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab that could not be saved
    /// - `error`: The save error
    /// - `window`: The window showing the notification
    /// - `cx`: The application context
    fn report_close_save_failure(
        &self,
        tab_id: TabId,
        error: &anyhow::Error,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let filename = self.tab_filename(tab_id, cx);
        log::error!("Failed to save large file '{filename}' on close: {error}");
        window.push_notification(
            (
                NotificationType::Error,
                SharedString::from(format!("Failed to save '{filename}': {error}")),
            ),
            cx,
        );
    }

    /// The display filename for a tab, for use in warning messages.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to describe
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `String`: The file name, or the tab title when there is no path
    pub(super) fn tab_filename(&self, tab_id: TabId, cx: &App) -> String {
        self.tab_entity_of(tab_id, cx)
            .and_then(|tab| {
                let tab = tab.read(cx);
                let editor_tab = tab.as_editor()?;
                let name = editor_tab
                    .file_path()
                    .and_then(|path| path.file_name())
                    .and_then(|name| name.to_str())
                    .map_or_else(|| editor_tab.title.to_string(), str::to_string);
                Some(name)
            })
            .unwrap_or_else(|| "file".to_string())
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use crate::fulgur::{
        Fulgur,
        editor_tab::{TabLocation, replace_editor_text},
        tab::TabId,
    };
    use crate::test_support::setup_fulgur;
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};
    use std::path::Path;
    use tempfile::TempDir;

    /// "café" as stored in windows-1252, where the accented letter is the single byte 0xE9.
    const LEGACY_BYTES: &[u8] = b"caf\xE9";

    /// Point the window's tab at a windows-1252 file on disk and type `text` into it.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state owning the tab
    /// - `visual_cx`: The visual test context of the window
    /// - `path`: The legacy-encoded file the tab is backed by
    /// - `text`: The edited buffer content
    /// - `lossy_decode`: Whether the file is flagged as decoded with replacement characters
    ///
    /// ### Returns
    /// - `TabId`: The id of the prepared tab
    fn open_legacy_tab(
        fulgur: &Entity<Fulgur>,
        visual_cx: &mut VisualTestContext,
        path: &Path,
        text: &str,
        lossy_decode: bool,
    ) -> TabId {
        std::fs::write(path, LEGACY_BYTES).expect("failed to write legacy file");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let tab = this.tabs.last().expect("expected a tab").clone();
                tab.update(cx, |tab, cx| {
                    let editor_tab = tab.as_editor_mut().expect("expected an editor tab");
                    editor_tab.location = TabLocation::Local(path.to_path_buf());
                    editor_tab.encoding = "windows-1252".to_string();
                    editor_tab.lossy_decode = lossy_decode;
                    editor_tab.content.update(cx, |state, cx| {
                        replace_editor_text(state, text, window, cx);
                    });
                });
                tab.read(cx).id()
            })
        })
    }

    /// Run the blocking close-time save and check it refused without touching the tab or file.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state owning the tab
    /// - `visual_cx`: The visual test context of the window
    /// - `tab_id`: The tab to save
    /// - `path`: The tab's backing file
    fn assert_blocking_save_refused(
        fulgur: &Entity<Fulgur>,
        visual_cx: &mut VisualTestContext,
        tab_id: TabId,
        path: &Path,
    ) {
        visual_cx.update(|_, cx| {
            fulgur.update(cx, |this, cx| {
                assert!(this.write_tab_blocking(tab_id, path, cx).is_err());
                let tab = this.tab_entity_of(tab_id, cx).expect("tab should exist");
                assert!(
                    tab.read(cx).as_editor().expect("editor tab").modified,
                    "a refused save must keep the edits dirty"
                );
            });
        });
        assert_eq!(
            std::fs::read(path).expect("file should exist"),
            LEGACY_BYTES
        );
    }

    #[gpui_kit::test]
    fn test_blocking_save_refuses_unmappable_content(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("large.txt");
        let tab_id = open_legacy_tab(&fulgur, &mut visual_cx, &path, "café ☃", false);

        assert_blocking_save_refused(&fulgur, &mut visual_cx, tab_id, &path);
    }

    #[gpui_kit::test]
    fn test_blocking_save_refuses_lossy_decoded_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("large.txt");
        let tab_id = open_legacy_tab(&fulgur, &mut visual_cx, &path, "café", true);

        assert_blocking_save_refused(&fulgur, &mut visual_cx, tab_id, &path);
    }
}
