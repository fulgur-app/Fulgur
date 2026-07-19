use crate::fulgur::{
    Fulgur, languages::supported_languages::SupportedLanguage, settings::MarkdownPreviewMode,
    tab::Tab, ui::tabs::markdown_preview_tab::MarkdownPreviewTab,
};
use gpui::{AppContext, Context, SharedString, Window};
use gpui_component::text::TextViewState;

impl Fulgur {
    /// Open or close the Markdown preview tab.
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
        let editor_id = match self.active_tab(cx) {
            Some(Tab::Editor(editor_tab)) => editor_tab.id,
            Some(Tab::MarkdownPreview(preview_tab)) => preview_tab.source_tab_id,
            _ => return,
        };
        let existing_preview_id = self.tabs.iter().map(|t| t.read(cx)).find_map(|t| match t {
            Tab::MarkdownPreview(p) if p.source_tab_id == editor_id => Some(p.id),
            _ => None,
        });
        if let Some(preview_id) = existing_preview_id {
            self.remove_tab_by_id(preview_id, window, cx);
        } else {
            let Some(editor_tab) = self.get_active_editor_tab(cx) else {
                return;
            };
            if editor_tab.large_file {
                return;
            }
            let title = SharedString::from(format!("Preview - {}", editor_tab.title));
            let content = editor_tab.content.clone();
            let editor_pos = self.active_tab_index(cx).unwrap_or(0);
            let view_state = cx.new(|cx| TextViewState::markdown("", cx));
            let preview_tab = Tab::MarkdownPreview(MarkdownPreviewTab {
                id: self.allocate_tab_id(),
                title,
                source_tab_id: editor_id,
                content,
                view_state,
            });
            self.tabs
                .insert(editor_pos + 1, preview_tab.into_entity(cx));
            self.set_active_tab(editor_pos + 1, window, cx);
        }
    }

    /// Insert Markdown preview tabs for all eligible editor tabs.
    ///
    /// ### Arguments
    /// - `cx`: The application context, used to allocate per-preview view state
    pub fn insert_preview_tabs_for_markdown(&mut self, cx: &mut Context<Self>) {
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
            let info = match self.tabs.get(actual_idx).map(|tab| tab.read(cx)) {
                Some(Tab::Editor(et))
                    if !et.large_file
                        && (et.language == SupportedLanguage::Markdown
                            || et.language == SupportedLanguage::MarkdownInline) =>
                {
                    Some((et.id, et.title.clone(), et.content.clone()))
                }
                _ => None,
            };
            if let Some((editor_id, title, content)) = info {
                let view_state = cx.new(|cx| TextViewState::markdown("", cx));
                let preview_tab = Tab::MarkdownPreview(MarkdownPreviewTab {
                    id: self.allocate_tab_id(),
                    title: SharedString::from(format!("Preview - {title}")),
                    source_tab_id: editor_id,
                    content,
                    view_state,
                });
                self.tabs
                    .insert(actual_idx + 1, preview_tab.into_entity(cx));
                offset += 1;
            }
        }
    }

    /// Insert a Markdown preview tab after the given editor tab if conditions are met.
    ///
    /// ### Arguments
    /// - `editor_tab_index`: Index of the editor tab in `self.tabs`
    /// - `cx`: The application context, used to allocate per-preview view state
    pub fn maybe_open_markdown_preview_for_editor(
        &mut self,
        editor_tab_index: usize,
        cx: &mut Context<Self>,
    ) {
        let settings = &self.settings.editor_settings.markdown_settings;
        if settings.preview_mode != MarkdownPreviewMode::DedicatedTab
            || !settings.show_markdown_preview
        {
            return;
        }
        let info = match self.tabs.get(editor_tab_index).map(|tab| tab.read(cx)) {
            Some(Tab::Editor(et))
                if !et.large_file
                    && (et.language == SupportedLanguage::Markdown
                        || et.language == SupportedLanguage::MarkdownInline) =>
            {
                Some((et.id, et.title.clone(), et.content.clone()))
            }
            _ => None,
        };
        if let Some((editor_id, title, content)) = info {
            let view_state = cx.new(|cx| TextViewState::markdown("", cx));
            let preview_tab = Tab::MarkdownPreview(MarkdownPreviewTab {
                id: self.allocate_tab_id(),
                title: SharedString::from(format!("Preview - {title}")),
                source_tab_id: editor_id,
                content,
                view_state,
            });
            self.tabs
                .insert(editor_tab_index + 1, preview_tab.into_entity(cx));
        }
    }
}
