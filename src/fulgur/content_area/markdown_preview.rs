use crate::fulgur::files::csv_support::{CsvDialect, serialize_csv};
use crate::fulgur::ui::copy_button::CopyButton;
use crate::fulgur::ui::tabs::markdown_preview_source::MarkdownPreviewSource;
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::utils::markdown_images::preview_image_source;
use crate::fulgur::{Fulgur, tab::Tab};
use gpui_kit::base::text::{TaskToggleEvent, TextView};
use gpui_kit::component::{
    h_flex,
    text::{FrontmatterPlugin, MarkdownExtensions, TableData},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, Context, Div, Entity, IntoElement, ParentElement, SharedString, Styled, Window, px,
};

/// Reading width the Markdown preview is capped at when the width limit is enabled.
const MARKDOWN_PREVIEW_MAX_WIDTH: f32 = 800.0;

impl Fulgur {
    /// Bind the inline preview panel to an editor tab, reusing the current
    /// preview source when it already mirrors that tab.
    ///
    /// ### Arguments
    /// - `source_tab`: The editor tab shown next to the panel
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(Entity<MarkdownPreviewSource>)`: The preview source of the panel.
    /// - `None`: If `source_tab` is not an editor tab.
    pub(super) fn ensure_markdown_panel_preview(
        &mut self,
        source_tab: &Entity<Tab>,
        cx: &mut Context<Self>,
    ) -> Option<Entity<MarkdownPreviewSource>> {
        let source_tab_id = source_tab.read(cx).id();
        if let Some(preview) = &self.markdown_panel_preview
            && preview.read(cx).source_tab_id() == source_tab_id
        {
            return Some(preview.clone());
        }
        let preview = MarkdownPreviewSource::for_editor_tab(source_tab, cx)?;
        self.markdown_panel_preview = Some(preview.clone());
        Some(preview)
    }

    /// Drop the inline preview panel source when it mirrors a tab that no
    /// longer shows the panel, so hidden previews stop re-rendering on edits.
    ///
    /// ### Arguments
    /// - `tab_id`: The editor tab rendered without the panel
    /// - `cx`: The application context
    pub(super) fn release_markdown_panel_preview_of(&mut self, tab_id: TabId, cx: &App) {
        if self
            .markdown_panel_preview
            .as_ref()
            .is_some_and(|preview| preview.read(cx).source_tab_id() == tab_id)
        {
            self.markdown_panel_preview = None;
        }
    }

    /// Make a markdown preview's task checkboxes edit their source buffer.
    ///
    /// ### Arguments
    /// - `text_view`: The preview text view
    /// - `preview`: The preview source the text view renders
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `TextView`: The text view, with clickable task checkboxes when the
    ///   source buffer accepts edits, and static ones otherwise
    pub(super) fn with_markdown_task_toggles(
        text_view: TextView,
        preview: &Entity<MarkdownPreviewSource>,
        cx: &App,
    ) -> TextView {
        if !preview.read(cx).accepts_task_toggles(cx) {
            return text_view;
        }
        let preview = preview.downgrade();
        text_view.on_task_toggle(move |event: &TaskToggleEvent, window, cx| {
            preview
                .update(cx, |preview, cx| preview.toggle_task(event, window, cx))
                .ok();
        })
    }

    /// Build the copy affordance shown on a markdown preview code block.
    ///
    /// ### Arguments
    /// - `code`: The code block contents to place on the clipboard
    ///
    /// ### Returns
    /// - `CopyButton`: The compact copy button for that code block
    pub(super) fn markdown_code_block_copy(code: impl Into<SharedString>) -> CopyButton {
        CopyButton::new("copy-code-block").compact().value(code)
    }

    /// Build the actions row shown under a markdown preview table.
    ///
    /// ### Arguments
    /// - `table`: The table snapshot, as header cells, body rows, and the
    ///   table re-serialized to GFM Markdown
    ///
    /// ### Returns
    /// - `impl IntoElement`: A right-aligned row of copy affordances. The CSV
    ///   button is omitted when the table cannot be serialized.
    pub(super) fn markdown_table_actions(
        table: &TableData,
        _window: &mut Window,
        _cx: &mut App,
    ) -> impl IntoElement + use<> {
        let csv = markdown_table_as_csv(table);

        h_flex()
            .w_full()
            .justify_end()
            .gap_1()
            .child(
                CopyButton::new("copy-table-markdown")
                    .compact()
                    .label("MD")
                    .tooltip("Copy as Markdown")
                    .value(table.markdown.clone()),
            )
            .children(csv.map(|csv| {
                CopyButton::new("copy-table-csv")
                    .compact()
                    .label("CSV")
                    .tooltip("Copy as CSV")
                    .value(csv)
            }))
    }

    /// Build the Markdown extension registry the preview is rendered with.
    ///
    /// ### Returns
    /// - `MarkdownExtensions`: The registry, carrying the frontmatter plugin
    ///   only when the corresponding setting is enabled
    pub(super) fn markdown_preview_extensions(&self) -> MarkdownExtensions {
        let extensions = MarkdownExtensions::default();
        if self
            .settings
            .editor_settings
            .markdown_settings
            .render_frontmatter
        {
            extensions.frontmatter().plugin(FrontmatterPlugin::new())
        } else {
            extensions
        }
    }

    /// Apply the image-loading policy and lay out a Markdown preview.
    ///
    /// ### Arguments
    /// - `preview`: The preview text view to lay out
    ///
    /// ### Returns
    /// - `Div`: A full-size container holding the preview, capped at
    ///   `MARKDOWN_PREVIEW_MAX_WIDTH` and centered when the limit is enabled.
    pub(super) fn layout_markdown_preview(&self, preview: TextView) -> Div {
        let settings = &self.settings.editor_settings.markdown_settings;
        let load_remote_images = settings.load_remote_images;
        let preview =
            preview.image_source(move |uri| preview_image_source(uri, load_remote_images));
        h_flex().size_full().justify_center().child(
            preview.when(settings.limit_preview_width, |view| {
                view.max_w(px(MARKDOWN_PREVIEW_MAX_WIDTH))
            }),
        )
    }
}

/// Serialize a markdown preview table to CSV text.
///
/// ### Arguments
/// - `table`: The table snapshot taken from the rendered document
///
/// ### Returns
/// - `Some(String)`: The table as CSV, with the delimiter Fulgur's own CSV
///   view defaults to
/// - `None`: The rows could not be serialized; the reason is logged
fn markdown_table_as_csv(table: &TableData) -> Option<String> {
    serialize_csv(&table.headers, &table.rows, &CsvDialect::default())
        .inspect_err(|error| log::warn!("Cannot offer the preview table as CSV: {error}"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(headers: &[&str], rows: &[&[&str]]) -> TableData {
        TableData {
            headers: headers.iter().map(|cell| (*cell).to_string()).collect(),
            rows: rows
                .iter()
                .map(|row| row.iter().map(|cell| (*cell).to_string()).collect())
                .collect(),
            ..TableData::default()
        }
    }

    #[test]
    fn serializes_a_plain_table() {
        let csv = markdown_table_as_csv(&table(&["a", "b"], &[&["1", "2"], &["3", "4"]]));
        assert_eq!(csv.as_deref(), Some("a,b\n1,2\n3,4\n"));
    }

    #[test]
    fn quotes_cells_containing_the_delimiter() {
        let csv = markdown_table_as_csv(&table(&["name"], &[&["Doe, Jane"]]));
        assert_eq!(csv.as_deref(), Some("name\n\"Doe, Jane\"\n"));
    }

    #[test]
    fn escapes_quotes_inside_a_cell() {
        let csv = markdown_table_as_csv(&table(&["quote"], &[&["say \"hi\""]]));
        assert_eq!(csv.as_deref(), Some("quote\n\"say \"\"hi\"\"\"\n"));
    }

    /// `TableData` documents its rows as possibly ragged, so a row that is
    /// shorter or longer than the header must not cost the reader the whole
    /// CSV affordance.
    #[test]
    fn serializes_a_ragged_table() {
        let csv = markdown_table_as_csv(&table(&["a", "b", "c"], &[&["1"], &["2", "3", "4", "5"]]));
        assert_eq!(csv.as_deref(), Some("a,b,c\n1\n2,3,4,5\n"));
    }

    #[test]
    fn serializes_a_table_with_no_body_rows() {
        let csv = markdown_table_as_csv(&table(&["a", "b"], &[]));
        assert_eq!(csv.as_deref(), Some("a,b\n"));
    }
}
