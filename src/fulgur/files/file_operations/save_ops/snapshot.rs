use super::super::encode_owned_for_save;
use crate::fulgur::editor_tab::{ContentRevision, EditorTab, content_fingerprint_from_str};
use gpui_kit::App;
use gpui_kit::component::input::Rope;

/// Editor text captured on the UI thread for a save.
pub(super) struct SaveSnapshot {
    /// The buffer as it was when the save was requested
    text: Rope,
    /// Revision of the buffer when the snapshot was taken
    revision: ContentRevision,
    /// Whether the tab tracks its saved baseline without a content hash
    large_file: bool,
}

/// Identity of the editor text a save snapshot holds.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SnapshotFingerprint {
    /// Revision of the buffer when the snapshot was taken
    pub(crate) revision: ContentRevision,
    /// Fingerprint of the snapshot text (`0` for a large-file tab)
    pub(crate) content_hash: u64,
    /// UTF-8 byte length of the snapshot text
    pub(crate) content_len: usize,
}

/// A save snapshot serialized and encoded off the UI thread.
pub(super) struct PreparedSave {
    /// Identity of the snapshot text
    pub(super) fingerprint: SnapshotFingerprint,
    /// The encoded bytes, or the snapshot text when the target encoding cannot
    /// represent it
    pub(super) encoded: Result<Vec<u8>, String>,
}

/// Encoded bytes of a save snapshot, ready to be written.
pub(crate) struct EncodedSave {
    /// The file content to write
    pub(crate) bytes: Vec<u8>,
    /// The encoding label `bytes` are written in
    pub(crate) encoding: String,
    /// Identity of the editor text `bytes` encode
    pub(crate) fingerprint: SnapshotFingerprint,
}

impl SaveSnapshot {
    /// Capture an editor tab's buffer for a save.
    ///
    /// ### Arguments
    /// - `editor_tab`: The tab being saved
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `SaveSnapshot`: The buffer and its revision at the time of the call
    pub(super) fn capture(editor_tab: &EditorTab, cx: &App) -> Self {
        Self {
            text: editor_tab.content.read(cx).text().clone(),
            revision: editor_tab.content_revision(cx),
            large_file: editor_tab.large_file,
        }
    }

    /// Materialize the snapshot text. Linear in the buffer size.
    ///
    /// ### Returns
    /// - `String`: The buffer text as it was when the snapshot was taken
    pub(super) fn into_string(self) -> String {
        self.text.to_string()
    }

    /// Serialize, fingerprint and encode the snapshot. Linear in the buffer
    /// size, so it must run on a background executor.
    ///
    /// ### Arguments
    /// - `encoding`: The target encoding label
    ///
    /// ### Returns
    /// - `PreparedSave`: The fingerprint of the text and its encoded bytes
    pub(super) fn prepare(self, encoding: &str) -> PreparedSave {
        let (revision, large_file) = (self.revision, self.large_file);
        let contents = self.into_string();
        let (content_hash, content_len) = if large_file {
            (0, contents.len())
        } else {
            content_fingerprint_from_str(&contents)
        };
        PreparedSave {
            fingerprint: SnapshotFingerprint {
                revision,
                content_hash,
                content_len,
            },
            encoded: encode_owned_for_save(contents, encoding),
        }
    }
}
