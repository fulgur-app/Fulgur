use super::{EditorTab, FromDuplicateParams, FromFileParams, TabTransferData};
use crate::fulgur::languages::supported_languages::{
    language_from_content, language_registry_name,
};
use crate::fulgur::settings::EditorSettings;
use crate::fulgur::ui::components_utils::{UNTITLED, UTF_8};
use gpui::{App, AppContext, SharedString, Window};
use std::time::SystemTime;

impl EditorTab {
    /// Create a new tab
    ///
    /// ### Arguments
    /// - `id`: The ID of the tab
    /// - `title`: The title of the tab
    /// - `window`: The window to create the tab in
    /// - `cx`: The application context
    /// - `settings`: The settings for the input state
    ///
    /// ### Returns
    /// - `EditorTab`: The new tab
    pub fn new(
        id: usize,
        title: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
    ) -> Self {
        let language = crate::fulgur::languages::supported_languages::SupportedLanguage::Plain;
        let content = cx.new(|cx| {
            super::make_input_state(
                window,
                cx,
                language_registry_name(&language),
                None,
                settings,
            )
        });
        Self {
            id,
            title: title.into(),
            content,
            file_path: None,
            modified: false,
            original_content: String::new(),
            encoding: UTF_8.to_string(),
            language,
            show_markdown_toolbar: settings.markdown_settings.show_markdown_toolbar,
            show_markdown_preview: settings.markdown_settings.show_markdown_preview,
            file_size_bytes: None,
            file_last_modified: None,
        }
    }

    /// Create a new tab from content with a given file name (no path).
    /// Used for shared files from sync server.
    ///
    /// ### Arguments
    /// - `id`: The ID of the tab
    /// - `contents`: The contents of the file
    /// - `file_name`: The name of the file (displayed in tab bar)
    /// - `window`: The window to create the tab in
    /// - `cx`: The application context
    /// - `settings`: The settings for the input state
    ///
    /// ### Returns
    /// - `EditorTab`: The new tab
    pub fn from_content(
        id: usize,
        contents: String,
        file_name: String,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
    ) -> Self {
        let language = language_from_content(&file_name, &contents);
        let content = cx.new(|cx| {
            super::make_input_state(
                window,
                cx,
                language_registry_name(&language),
                Some(contents.clone()),
                settings,
            )
        });
        Self {
            id,
            title: file_name.into(),
            content,
            file_path: None,                 // No path - forces "Save as..." dialog
            modified: true,                  // Mark as modified
            original_content: String::new(), // Empty so check_modified() keeps it as modified
            encoding: UTF_8.to_string(),
            language,
            show_markdown_toolbar: settings.markdown_settings.show_markdown_toolbar,
            show_markdown_preview: settings.markdown_settings.show_markdown_preview,
            file_size_bytes: None,
            file_last_modified: None,
        }
    }

    /// Create a new tab from a file
    ///
    /// ### Arguments
    /// - `params`: The parameters for creating the tab
    /// - `window`: The window to create the tab in
    /// - `cx`: The application context
    /// - `settings`: The settings for the input state
    ///
    /// ### Returns
    /// - `EditorTab`: The new tab
    pub fn from_file(
        params: FromFileParams,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
    ) -> Self {
        let content_len = params.contents.len();
        let file_name = params
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(UNTITLED)
            .to_string();

        let language = language_from_content(&file_name, &params.contents);
        let content = cx.new(|cx| {
            super::make_input_state(
                window,
                cx,
                language_registry_name(&language),
                Some(params.contents.clone()),
                settings,
            )
        });
        let title = format!(
            "{}{}",
            file_name,
            if params.is_modified { " •" } else { "" }
        );
        Self {
            id: params.id,
            title: title.into(),
            content,
            file_path: Some(params.path),
            modified: params.is_modified,
            original_content: params.contents,
            encoding: params.encoding,
            language,
            show_markdown_toolbar: settings.markdown_settings.show_markdown_toolbar,
            show_markdown_preview: settings.markdown_settings.show_markdown_preview,
            file_size_bytes: Some(content_len as u64),
            file_last_modified: Some(SystemTime::now()),
        }
    }

    /// Create a new tab as a duplicate of an existing editor tab.
    ///
    /// The new tab has no file path (forcing "Save As..." on first save), is marked as modified,
    /// and inherits the language and encoding from the source tab.
    ///
    /// ### Arguments
    /// - `params`: The parameters collected from the source tab
    /// - `window`: The window to create the tab in
    /// - `cx`: The application context
    /// - `settings`: The settings for the input state
    ///
    /// ### Returns
    /// - `EditorTab`: The new duplicated tab
    pub fn from_duplicate(
        params: FromDuplicateParams,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
    ) -> Self {
        let content = cx.new(|cx| {
            super::make_input_state(
                window,
                cx,
                language_registry_name(&params.language),
                Some(params.current_content),
                settings,
            )
        });
        Self {
            id: params.id,
            title: params.title,
            content,
            file_path: None,
            modified: true,
            original_content: String::new(),
            encoding: params.encoding,
            language: params.language,
            show_markdown_toolbar: settings.markdown_settings.show_markdown_toolbar,
            show_markdown_preview: settings.markdown_settings.show_markdown_preview,
            file_size_bytes: None,
            file_last_modified: None,
        }
    }

    /// Recreate a tab in a new window from transferred state.
    ///
    /// Preserves text content, file path, unsaved-change state, language, encoding,
    /// markdown panel visibility, and cursor position. Called by the target window
    /// when processing a deferred `pending_tab_transfer`.
    ///
    /// ### Arguments
    /// - `id`: The new tab ID, allocated by the receiving window
    /// - `data`: All state captured from the source tab
    /// - `window`: The target window context
    /// - `cx`: The application context
    /// - `settings`: The receiving window's editor settings
    ///
    /// ### Returns
    /// - `EditorTab`: The newly created tab, ready to be pushed onto the tab list
    pub fn from_transfer(
        id: usize,
        data: TabTransferData,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
    ) -> Self {
        let cursor_position = data.cursor_position;
        let content = cx.new(|cx| {
            super::make_input_state(
                window,
                cx,
                language_registry_name(&data.language),
                Some(data.content),
                settings,
            )
        });
        content.update(cx, |state, cx| {
            state.set_cursor_position(cursor_position, window, cx);
        });
        Self {
            id,
            title: data.title,
            content,
            file_path: data.file_path,
            modified: data.modified,
            original_content: data.original_content,
            encoding: data.encoding,
            language: data.language,
            show_markdown_toolbar: data.show_markdown_toolbar,
            show_markdown_preview: data.show_markdown_preview,
            file_size_bytes: data.file_size_bytes,
            file_last_modified: data.file_last_modified,
        }
    }
}
