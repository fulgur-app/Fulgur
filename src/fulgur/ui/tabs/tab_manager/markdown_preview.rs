use crate::fulgur::{
    Fulgur,
    languages::supported_languages::SupportedLanguage,
    settings::MarkdownPreviewMode,
    tab::Tab,
    ui::tabs::{markdown_preview_tab::MarkdownPreviewTab, tab::TabId},
};
use gpui::{App, AppContext, Context, Entity, SharedString, Window};
use gpui_component::{input::InputState, text::TextViewState};

impl Fulgur {
    /// Build a Markdown preview tab bound to a source editor tab
    ///
    /// ### Arguments
    /// - `source_tab_id`: Identifier of the editor tab the preview mirrors
    /// - `source_title`: Title of the source editor tab, used to derive the preview title
    /// - `content`: Input state of the source editor tab, rendered by the preview
    /// - `cx`: The application context, used to allocate the per-preview view state
    ///
    /// ### Returns
    /// - `Tab`: A `Tab::MarkdownPreview` carrying a freshly allocated tab id and view state
    fn build_preview_tab(
        &mut self,
        source_tab_id: TabId,
        source_title: &str,
        content: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Tab {
        let view_state = cx.new(|cx| TextViewState::markdown("", cx));
        Tab::MarkdownPreview(MarkdownPreviewTab {
            id: self.allocate_tab_id(),
            title: SharedString::from(format!("Preview - {source_title}")),
            source_tab_id,
            content,
            view_state,
        })
    }

    /// Collect the data needed to preview the editor tab at the given position
    ///
    /// ### Arguments
    /// - `tab_index`: Position of the candidate editor tab in `self.tabs`
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some((TabId, SharedString, Entity<InputState>))`: The tab is a Markdown editor tab eligible for a preview
    /// - `None`: The position holds no tab, a non-editor tab, a large file, or a non-Markdown language
    fn markdown_preview_source_at(
        &self,
        tab_index: usize,
        cx: &App,
    ) -> Option<(TabId, SharedString, Entity<InputState>)> {
        match self.tabs.get(tab_index).map(|tab| tab.read(cx)) {
            Some(Tab::Editor(editor_tab))
                if !editor_tab.large_file
                    && (editor_tab.language == SupportedLanguage::Markdown
                        || editor_tab.language == SupportedLanguage::MarkdownInline) =>
            {
                Some((
                    editor_tab.id,
                    editor_tab.title.clone(),
                    editor_tab.content.clone(),
                ))
            }
            _ => None,
        }
    }

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
            let source_title = editor_tab.title.clone();
            let content = editor_tab.content.clone();
            let editor_pos = self.active_tab_index(cx).unwrap_or(0);
            let preview_tab = self.build_preview_tab(editor_id, &source_title, content, cx);
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
            if let Some((editor_id, title, content)) =
                self.markdown_preview_source_at(actual_idx, cx)
            {
                let preview_tab = self.build_preview_tab(editor_id, &title, content, cx);
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
        if let Some((editor_id, title, content)) =
            self.markdown_preview_source_at(editor_tab_index, cx)
        {
            let preview_tab = self.build_preview_tab(editor_id, &title, content, cx);
            self.tabs
                .insert(editor_tab_index + 1, preview_tab.into_entity(cx));
        }
    }
}
