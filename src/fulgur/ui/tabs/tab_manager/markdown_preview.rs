use crate::fulgur::{
    Fulgur, languages::supported_languages::SupportedLanguage, settings::MarkdownPreviewMode,
    tab::Tab, ui::tabs::markdown_preview_tab::MarkdownPreviewTab,
};
use gpui::{Context, SharedString, Window};

impl Fulgur {
    /// Open or close the Markdown preview tab for the active editor tab.
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn open_markdown_preview_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.editor_settings.markdown_settings.preview_mode
            != MarkdownPreviewMode::DedicatedTab
        {
            return;
        }
        let Some(editor_tab) = self.get_active_editor_tab() else {
            return;
        };
        let editor_id = editor_tab.id;
        if let Some(preview_id) = self
            .tabs
            .iter()
            .find(|t| matches!(t, Tab::MarkdownPreview(p) if p.source_tab_id == editor_id))
            .map(|t| t.id())
        {
            self.remove_tab_by_id(preview_id, window, cx);
        } else {
            let Some(editor_tab) = self.get_active_editor_tab() else {
                return;
            };
            let title = SharedString::from(format!("Preview - {}", editor_tab.title));
            let content = editor_tab.content.clone();
            let editor_pos = self.active_tab_index.unwrap_or(0);
            let preview_tab = Tab::MarkdownPreview(MarkdownPreviewTab {
                id: self.next_tab_id,
                title,
                source_tab_id: editor_id,
                content,
            });
            self.tabs.insert(editor_pos + 1, preview_tab);
            self.pending_tab_scroll = Some(editor_pos + 1);
            self.next_tab_id += 1;
            cx.notify();
        }
    }

    /// Insert Markdown preview tabs for all eligible editor tabs.
    pub fn insert_preview_tabs_for_markdown(&mut self) {
        let settings = &self.settings.editor_settings.markdown_settings;
        if settings.preview_mode != MarkdownPreviewMode::DedicatedTab
            || !settings.show_markdown_preview
        {
            return;
        }
        let original_count = self.tabs.len();
        let mut offset = 0;
        for orig_idx in 0..original_count {
            let actual_idx = orig_idx + offset;
            let info = match self.tabs.get(actual_idx) {
                Some(Tab::Editor(et))
                    if et.language == SupportedLanguage::Markdown
                        || et.language == SupportedLanguage::MarkdownInline =>
                {
                    Some((et.id, et.title.clone(), et.content.clone()))
                }
                _ => None,
            };
            if let Some((editor_id, title, content)) = info {
                let preview_tab = Tab::MarkdownPreview(MarkdownPreviewTab {
                    id: self.next_tab_id,
                    title: SharedString::from(format!("Preview - {title}")),
                    source_tab_id: editor_id,
                    content,
                });
                self.tabs.insert(actual_idx + 1, preview_tab);
                self.next_tab_id += 1;
                offset += 1;
            }
        }
    }

    /// Insert a Markdown preview tab after the given editor tab if conditions are met.
    ///
    /// ### Arguments
    /// - `editor_tab_index`: Index of the editor tab in `self.tabs`
    pub fn maybe_open_markdown_preview_for_editor(&mut self, editor_tab_index: usize) {
        let settings = &self.settings.editor_settings.markdown_settings;
        if settings.preview_mode != MarkdownPreviewMode::DedicatedTab
            || !settings.show_markdown_preview
        {
            return;
        }
        let info = match self.tabs.get(editor_tab_index) {
            Some(Tab::Editor(et))
                if et.language == SupportedLanguage::Markdown
                    || et.language == SupportedLanguage::MarkdownInline =>
            {
                Some((et.id, et.title.clone(), et.content.clone()))
            }
            _ => None,
        };
        if let Some((editor_id, title, content)) = info {
            let preview_tab = Tab::MarkdownPreview(MarkdownPreviewTab {
                id: self.next_tab_id,
                title: SharedString::from(format!("Preview - {title}")),
                source_tab_id: editor_id,
                content,
            });
            self.tabs.insert(editor_tab_index + 1, preview_tab);
            self.next_tab_id += 1;
        }
    }
}
