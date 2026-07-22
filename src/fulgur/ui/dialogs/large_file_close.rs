use std::time::Instant;

use anyhow::anyhow;
use gpui::{Context, ParentElement, SharedString, Styled, Window, div, px};
use gpui_component::{
    WindowExt, button::Button, button::ButtonVariant, button::ButtonVariants, h_flex,
    notification::NotificationType, v_flex,
};

use crate::fulgur::Fulgur;
use crate::fulgur::files::file_operations::{EncodedContents, encode_for_save};
use crate::fulgur::ui::tabs::editor_tab::TabLocation;
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::utils::atomic_write::atomic_write_file;

/// What to do once every large-file close warning has been resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseContinuation {
    /// Quit the whole application.
    Quit,
    /// Close only this window, leaving other windows open.
    CloseWindow,
}

/// The choice the user made in a single large-file close warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LargeFileCloseChoice {
    /// Write the file to disk before continuing.
    Save,
    /// Continue without saving, dropping the unsaved changes.
    Discard,
    /// Abort the whole close.
    Cancel,
}

impl Fulgur {
    /// Collect the ids of local editor tabs whose unsaved changes would be
    /// silently dropped on close because the content is too large to persist.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Vec<TabId>`: The ids of large, modified local tabs, in tab order
    pub fn large_modified_local_tabs(&self, cx: &gpui::App) -> Vec<TabId> {
        self.tabs
            .iter()
            .filter_map(|tab| {
                let editor_tab = tab.read(cx).as_editor()?;
                if matches!(editor_tab.location, TabLocation::Local(_))
                    && editor_tab.content_too_large_to_persist(cx)
                    && editor_tab.content_differs_from_original(cx)
                {
                    Some(editor_tab.id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Whether a tab still needs a large-file close warning.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to check
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `bool`: `true` when the tab is still a modified, large local tab
    fn tab_still_needs_close_warning(&self, tab_id: TabId, cx: &gpui::App) -> bool {
        self.tab_entity_of(tab_id, cx)
            .and_then(|tab| {
                let tab = tab.read(cx);
                let editor_tab = tab.as_editor()?;
                Some(
                    matches!(editor_tab.location, TabLocation::Local(_))
                        && editor_tab.content_too_large_to_persist(cx)
                        && editor_tab.content_differs_from_original(cx),
                )
            })
            .unwrap_or(false)
    }

    /// Drive the sequential large-file close warnings, one dialog per tab.
    ///
    /// ### Arguments
    /// - `remaining`: The tabs still to prompt for, in order
    /// - `continuation`: What to do once all warnings are resolved
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn drive_large_file_close_warnings(
        &mut self,
        remaining: Vec<TabId>,
        continuation: CloseContinuation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut queue = remaining;
        let tab_id = loop {
            let Some(&next) = queue.first() else {
                self.run_close_continuation(continuation, window, cx);
                return;
            };
            if self.tab_still_needs_close_warning(next, cx) {
                break next;
            }
            queue.remove(0);
        };
        let rest = queue[1..].to_vec();
        if let Some(index) = self.tab_index_of(tab_id, cx) {
            self.set_active_tab(index, window, cx);
        }
        self.show_large_file_close_dialog(tab_id, rest, continuation, window, cx);
    }

    /// Show the drop-changes warning dialog for a single large modified tab.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab the warning is about
    /// - `rest`: The tabs still to prompt for after this one
    /// - `continuation`: What to do once all warnings are resolved
    /// - `window`: The window context
    /// - `cx`: The application context
    fn show_large_file_close_dialog(
        &mut self,
        tab_id: TabId,
        rest: Vec<TabId>,
        continuation: CloseContinuation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity().clone();
        let filename = self.tab_filename(tab_id, cx);
        window.open_alert_dialog(cx, move |modal, _, _| {
            let filename = filename.clone();
            let footer = {
                let entity = entity.clone();
                let rest = rest.clone();
                let make_button =
                    move |id: &'static str,
                          label: &'static str,
                          variant: ButtonVariant,
                          choice: LargeFileCloseChoice| {
                        let entity = entity.clone();
                        let rest = rest.clone();
                        Button::new(id).label(label).with_variant(variant).on_click(
                            move |_, window, cx| {
                                let rest = rest.clone();
                                window.close_dialog(cx);
                                entity.update(cx, |this, cx| {
                                    this.resolve_large_file_close_warning(
                                        choice,
                                        tab_id,
                                        rest,
                                        continuation,
                                        window,
                                        cx,
                                    );
                                });
                            },
                        )
                    };
                h_flex()
                    .gap_2()
                    .justify_center()
                    .child(make_button(
                        "large-file-close-cancel",
                        "Cancel",
                        ButtonVariant::Ghost,
                        LargeFileCloseChoice::Cancel,
                    ))
                    .child(make_button(
                        "large-file-close-discard",
                        "Discard",
                        ButtonVariant::Danger,
                        LargeFileCloseChoice::Discard,
                    ))
                    .child(make_button(
                        "large-file-close-save",
                        "Save",
                        ButtonVariant::Primary,
                        LargeFileCloseChoice::Save,
                    ))
            };
            modal
                .title(div().text_size(px(16.)).child("Unsaved large file"))
                .keyboard(true)
                .overlay_closable(false)
                .close_button(false)
                .child(
                    v_flex()
                        .gap_2()
                        .child(div().text_size(px(14.)).child(format!(
                            "\"{filename}\" is too large to keep in memory when Fulgur closes."
                        )))
                        .child(div().text_size(px(14.)).child(
                            "Its unsaved changes will be dropped unless you save it to disk now.",
                        )),
                )
                .footer(footer)
        });
    }

    /// Apply the user's choice from a large-file close warning.
    ///
    /// ### Arguments
    /// - `choice`: The button the user clicked
    /// - `tab_id`: The tab the warning was about
    /// - `rest`: The tabs still to prompt for after this one
    /// - `continuation`: What to do once all warnings are resolved
    /// - `window`: The window context
    /// - `cx`: The application context
    fn resolve_large_file_close_warning(
        &mut self,
        choice: LargeFileCloseChoice,
        tab_id: TabId,
        rest: Vec<TabId>,
        continuation: CloseContinuation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match choice {
            LargeFileCloseChoice::Cancel => {}
            LargeFileCloseChoice::Discard => {
                self.drive_large_file_close_warnings(rest, continuation, window, cx);
            }
            LargeFileCloseChoice::Save => match self.save_local_tab_blocking(tab_id, cx) {
                Ok(()) => {
                    self.drive_large_file_close_warnings(rest, continuation, window, cx);
                }
                Err(e) => {
                    let filename = self.tab_filename(tab_id, cx);
                    log::error!("Failed to save large file '{filename}' on close: {e}");
                    window.push_notification(
                        (
                            NotificationType::Error,
                            SharedString::from(format!("Failed to save '{filename}': {e}")),
                        ),
                        cx,
                    );
                }
            },
        }
    }

    /// Run the close continuation once all warnings have been resolved.
    ///
    /// ### Arguments
    /// - `continuation`: What to do
    /// - `window`: The window context
    /// - `cx`: The application context
    fn run_close_continuation(
        &mut self,
        continuation: CloseContinuation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match continuation {
            CloseContinuation::Quit => self.quit_inner(window, cx),
            CloseContinuation::CloseWindow => self.finish_window_close(window, cx),
        }
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
    fn save_local_tab_blocking(
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
        self.file_watch_state
            .last_file_saves
            .insert(path.clone(), Instant::now());
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
    fn tab_filename(&self, tab_id: TabId, cx: &gpui::App) -> String {
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
