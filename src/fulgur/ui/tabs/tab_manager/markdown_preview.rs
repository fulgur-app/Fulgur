use crate::fulgur::{
    Fulgur, languages::supported_languages::SupportedLanguage, settings::MarkdownPreviewMode,
    tab::Tab, ui::tabs::markdown_preview_tab::MarkdownPreviewTab,
};
use gpui::{AppContext, Context, Entity, SharedString, Window};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::text::TextViewState;
use std::collections::HashSet;

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
            .map(super::super::tab::Tab::id)
        {
            self.remove_tab_by_id(preview_id, window, cx);
        } else {
            let Some(editor_tab) = self.get_active_editor_tab() else {
                return;
            };
            let title = SharedString::from(format!("Preview - {}", editor_tab.title));
            let content = editor_tab.content.clone();
            let editor_pos = self.active_tab_index.unwrap_or(0);
            let view_state = cx.new(|cx| TextViewState::markdown("", cx));
            let preview_tab = Tab::MarkdownPreview(MarkdownPreviewTab {
                id: self.next_tab_id,
                title,
                source_tab_id: editor_id,
                content,
                view_state,
            });
            self.tabs.insert(editor_pos + 1, preview_tab);
            self.pending_tab_scroll = Some(editor_pos + 1);
            self.next_tab_id += 1;
            cx.notify();
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
                let view_state = cx.new(|cx| TextViewState::markdown("", cx));
                let preview_tab = Tab::MarkdownPreview(MarkdownPreviewTab {
                    id: self.next_tab_id,
                    title: SharedString::from(format!("Preview - {title}")),
                    source_tab_id: editor_id,
                    content,
                    view_state,
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
            let view_state = cx.new(|cx| TextViewState::markdown("", cx));
            let preview_tab = Tab::MarkdownPreview(MarkdownPreviewTab {
                id: self.next_tab_id,
                title: SharedString::from(format!("Preview - {title}")),
                source_tab_id: editor_id,
                content,
                view_state,
            });
            self.tabs.insert(editor_tab_index + 1, preview_tab);
            self.next_tab_id += 1;
        }
    }

    /// Check whether an editor tab should be tracked as a markdown preview source.
    ///
    /// ### Arguments
    /// - `language`: The editor tab language
    /// - `show_markdown_preview`: Per-tab markdown preview toggle
    ///
    /// ### Returns
    /// - `True` when this tab should keep markdown preview cache/subscriptions alive
    fn should_track_markdown_preview_source(
        language: SupportedLanguage,
        show_markdown_preview: bool,
    ) -> bool {
        show_markdown_preview
            && matches!(
                language,
                SupportedLanguage::Markdown | SupportedLanguage::MarkdownInline
            )
    }

    /// Remove markdown preview cache entries for tabs that no longer exist.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub(crate) fn prune_markdown_preview_cache(&mut self, cx: &mut Context<Self>) {
        let source_ids: HashSet<usize> = self
            .tabs
            .iter()
            .filter_map(|tab| match tab {
                Tab::MarkdownPreview(preview_tab) => Some(preview_tab.source_tab_id),
                Tab::Editor(editor_tab)
                    if Self::should_track_markdown_preview_source(
                        editor_tab.language,
                        editor_tab.show_markdown_preview,
                    ) =>
                {
                    Some(editor_tab.id)
                }
                _ => None,
            })
            .collect();

        let before_cache = self.markdown_preview_cache.len();
        let before_subs = self.markdown_preview_subscriptions.len();
        self.markdown_preview_cache
            .retain(|tab_id, _| source_ids.contains(tab_id));
        self.markdown_preview_subscriptions
            .retain(|tab_id, _| source_ids.contains(tab_id));
        self.markdown_preview_to_refresh
            .retain(|tab_id| source_ids.contains(tab_id));
        if self.markdown_preview_cache.len() != before_cache
            || self.markdown_preview_subscriptions.len() != before_subs
        {
            cx.notify();
        }
    }

    /// Get cached markdown text for a source tab, refreshing lazily on demand.
    ///
    /// ### Arguments
    /// - `source_tab_id`: Source editor tab id for this preview
    /// - `content`: Source editor content entity
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `SharedString`: Cached markdown source text for rendering
    pub(crate) fn markdown_preview_text_for(
        &mut self,
        source_tab_id: usize,
        content: &Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> SharedString {
        let current_entity_id = content.entity_id();
        let entity_changed = self
            .markdown_preview_subscriptions
            .get(&source_tab_id)
            .is_none_or(|(entity_id, _)| *entity_id != current_entity_id);
        if entity_changed {
            let subscription =
                cx.subscribe(content, move |this: &mut Self, _, ev: &InputEvent, cx| {
                    if !matches!(ev, InputEvent::Change) {
                        return;
                    }
                    this.markdown_preview_to_refresh.insert(source_tab_id);
                    cx.notify();
                });
            self.markdown_preview_subscriptions
                .insert(source_tab_id, (current_entity_id, subscription));
        }
        let refresh_requested = self.markdown_preview_to_refresh.remove(&source_tab_id);
        let needs_refresh = entity_changed
            || refresh_requested
            || !self.markdown_preview_cache.contains_key(&source_tab_id);
        if needs_refresh {
            self.markdown_preview_cache
                .insert(source_tab_id, content.read(cx).value());
        }
        self.markdown_preview_cache
            .get(&source_tab_id)
            .cloned()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod markdown_preview_cache_unit_tests {
    use super::*;

    #[test]
    fn test_should_track_markdown_preview_source_for_markdown() {
        assert!(Fulgur::should_track_markdown_preview_source(
            SupportedLanguage::Markdown,
            true
        ));
    }

    #[test]
    fn test_should_track_markdown_preview_source_for_markdown_inline() {
        assert!(Fulgur::should_track_markdown_preview_source(
            SupportedLanguage::MarkdownInline,
            true
        ));
    }

    #[test]
    fn test_should_not_track_markdown_preview_source_when_toggle_disabled() {
        assert!(!Fulgur::should_track_markdown_preview_source(
            SupportedLanguage::Markdown,
            false
        ));
    }

    #[test]
    fn test_should_not_track_markdown_preview_source_for_non_markdown() {
        assert!(!Fulgur::should_track_markdown_preview_source(
            SupportedLanguage::Plain,
            true
        ));
    }
}
