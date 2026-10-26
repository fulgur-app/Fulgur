use anyhow::anyhow;
use gpui_kit::Context;

use crate::fulgur::Fulgur;
use crate::fulgur::files::file_operations::{EncodedContents, encode_for_save};
use crate::fulgur::ui::tabs::editor_tab::TabLocation;
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::utils::atomic_write::atomic_write_file;

impl Fulgur {
    /// Collect the ids of local editor tabs whose unsaved changes would be
    /// silently dropped on close because the content is too large to persist.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Vec<TabId>`: The ids of large, modified local tabs, in tab order
    pub fn large_modified_local_tabs(&self, cx: &gpui_kit::App) -> Vec<TabId> {
        self.tabs
            .iter()
            .filter_map(|tab| {
                let editor_tab = tab.read(cx).as_editor()?;
                editor_tab
                    .needs_large_file_close_warning(cx)
                    .then_some(editor_tab.id)
            })
            .collect()
    }

    /// Encode and write a local tab to disk synchronously, updating its baseline.
    ///
    /// ### Arguments
    /// - `tab_id`: The local tab to write
    /// - `cx`: The application context
    ///
    /// ### Errors
    /// Returns an error if the tab is missing, is not a local editor tab, its
    /// content cannot be encoded losslessly, or the disk write fails.
    ///
    /// ### Returns
    /// - `Ok(())`: The file was written and the tab marked as saved
    /// - `Err(anyhow::Error)`: The file could not be written
    pub(super) fn save_local_tab_blocking(
        &mut self,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let tab = self
            .tab_entity_of(tab_id, cx)
            .ok_or_else(|| anyhow!("tab no longer exists"))?;
        let (path, content_entity, encoding, lossy_decode) = {
            let tab = tab.read(cx);
            let editor_tab = tab
                .as_editor()
                .ok_or_else(|| anyhow!("tab is not an editor tab"))?;
            let TabLocation::Local(path) = &editor_tab.location else {
                return Err(anyhow!("tab is not a local file"));
            };
            (
                path.clone(),
                editor_tab.content.clone(),
                editor_tab.encoding.clone(),
                editor_tab.lossy_decode,
            )
        };
        if lossy_decode {
            return Err(anyhow!(
                "file was decoded lossily; save it manually to confirm the conversion"
            ));
        }
        let contents = content_entity.read(cx).text().to_string();
        let bytes = match encode_for_save(&contents, &encoding) {
            EncodedContents::Encoded(bytes) => bytes,
            EncodedContents::Lossy => {
                return Err(anyhow!(
                    "content cannot be represented in {encoding}; save it manually"
                ));
            }
        };
        let byte_len = bytes.len();
        atomic_write_file(&path, &bytes)?;
        self.record_local_file_save(&path);
        self.update_editor_tab(tab_id, cx, |editor_tab, cx| {
            editor_tab.mark_as_saved(cx);
            editor_tab.update_file_tooltip_cache(byte_len);
            cx.notify();
        });
        Ok(())
    }

    /// The display filename for a tab, for use in warning messages.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to describe
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `String`: The file name, or the tab title when there is no path
    pub(super) fn tab_filename(&self, tab_id: TabId, cx: &gpui_kit::App) -> String {
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
                assert!(this.save_local_tab_blocking(tab_id, cx).is_err());
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
