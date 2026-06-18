use crate::fulgur::{
    Fulgur,
    tab::Tab,
    ui::{
        components_utils::UNTITLED,
        tabs::{
            editor_tab::{EditorTab, FromDuplicateParams},
            settings_tab::SettingsTab,
        },
    },
};
use gpui::{App, Context, SharedString, Window};
use gpui_component::select::{SearchableVec, SelectEvent};

impl Fulgur {
    /// Create a new tab
    ///
    /// ### Arguments
    /// - `window`: The window to create the tab in
    /// - `cx`: The application context
    pub fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab = Tab::Editor(EditorTab::new(
            self.next_tab_id,
            format!("{} {}", UNTITLED, self.next_tab_id),
            window,
            cx,
            &self.settings.editor_settings,
        ));
        self.tabs.push(tab);
        self.active_tab_index = Some(self.tabs.len() - 1);
        self.pending_tab_scroll = Some(self.tabs.len() - 1);
        self.next_tab_id += 1;
        self.focus_active_tab(window, cx);
        self.save_state_async(cx, window);
        cx.notify();
    }

    /// Return the id of the last tab when it is an empty, unsaved scratch buffer
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(usize)`: The id of the reusable last editor tab.
    /// - `None`: If the last tab is missing, not an editor, has a file, or has non-whitespace content.
    fn reusable_scratch_tab_id(&self, cx: &App) -> Option<usize> {
        let Tab::Editor(editor) = self.tabs.last()? else {
            return None;
        };
        let is_blank = editor
            .content
            .read(cx)
            .text()
            .chunks()
            .all(|chunk| chunk.chars().all(char::is_whitespace));
        if editor.location.is_untitled() && is_blank {
            Some(editor.id)
        } else {
            None
        }
    }

    /// Place a freshly built editor tab, reusing a trailing empty scratch tab
    ///
    /// ### Arguments
    /// - `tab`: The editor tab to place
    /// - `window`: The window context
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `usize`: The index where the tab was placed.
    pub fn place_editor_tab_reusing_scratch(
        &mut self,
        tab: Tab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> usize {
        if let Some(scratch_id) = self.reusable_scratch_tab_id(cx) {
            self.remove_tab_by_id(scratch_id, window, cx);
        }
        self.tabs.push(tab);
        let index = self.tabs.len() - 1;
        self.active_tab_index = Some(index);
        self.pending_tab_scroll = Some(index);
        index
    }

    /// Open settings in a new tab or switch to existing settings tab
    ///
    /// ### Arguments
    /// - `window`: The window to open settings in
    /// - `cx`: The application context
    pub fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.iter().position(|t| matches!(t, Tab::Settings(_))) {
            self.set_active_tab(index, window, cx);
        } else {
            let tab = SettingsTab::new(
                self.next_tab_id,
                &self.settings.editor_settings.font_family,
                window,
                cx,
            );
            let font_select_subscription = cx.subscribe(
                &tab.font_family_select,
                |this: &mut Self,
                 _,
                 ev: &SelectEvent<SearchableVec<SharedString>>,
                 cx: &mut Context<Self>| {
                    if let SelectEvent::Confirm(Some(value)) = ev {
                        this.settings.editor_settings.font_family = value.to_string();
                        let _ = this.update_and_propagate_settings(cx);
                    }
                },
            );
            self.font_select_subscription = Some(font_select_subscription);
            let settings_tab = Tab::Settings(tab);
            self.tabs.push(settings_tab);
            self.active_tab_index = Some(self.tabs.len() - 1);
            self.pending_tab_scroll = Some(self.tabs.len() - 1);
            self.next_tab_id += 1;
            self.save_state_async(cx, window);
            cx.notify();
        }
    }

    /// Duplicate a tab and insert it immediately to the right of the original
    ///
    /// The duplicate is an editor tab with the same content, language, and encoding, but no
    /// file path (it is treated as unsaved). Only editor tabs can be duplicated; calling this
    /// with the index of a non-editor tab is a no-op.
    ///
    /// ### Arguments
    /// - `index`: The index of the tab to duplicate
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn duplicate_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Tab::Editor(editor_tab)) = self.tabs.get(index) else {
            return;
        };
        let current_content = editor_tab.content.read(cx).text().to_string();
        let language = editor_tab.language;
        let raw_title = editor_tab.title.to_string();
        let encoding = editor_tab.encoding.clone();
        let lossy_decode = editor_tab.lossy_decode;
        let settings = self.settings.editor_settings.clone();
        let clean_title: SharedString = raw_title.trim_end_matches(" •").trim().to_string().into();
        let new_tab = Tab::Editor(EditorTab::from_duplicate(
            FromDuplicateParams {
                id: self.next_tab_id,
                title: clean_title,
                current_content,
                encoding,
                lossy_decode,
                language,
            },
            window,
            cx,
            &settings,
        ));
        let insert_pos = index + 1;
        self.tabs.insert(insert_pos, new_tab);
        self.active_tab_index = Some(insert_pos);
        self.pending_tab_scroll = Some(insert_pos);
        self.next_tab_id += 1;
        self.focus_active_tab(window, cx);
        self.save_state_async(cx, window);
        cx.notify();
    }
}
