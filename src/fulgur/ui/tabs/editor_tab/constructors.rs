use super::{
    EditorTab, FromDuplicateParams, FromFileParams, TabLocation, TabTransferData, initial_csv_state,
};
use crate::fulgur::files::file_operations::RemoteFileResult;
use crate::fulgur::languages::supported_languages::{
    language_from_content, language_registry_name,
};
use crate::fulgur::settings::EditorSettings;
use crate::fulgur::sync::ssh::url::RemoteSpec;
use crate::fulgur::ui::components_utils::{UNTITLED, UTF_8};
use crate::fulgur::ui::tabs::tab::TabId;
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
        id: TabId,
        title: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
    ) -> Self {
        let language = crate::fulgur::languages::supported_languages::SupportedLanguage::Plain;
        let (original_content_hash, original_content_len) = super::content_fingerprint_from_str("");
        let content = cx.new(|cx| {
            super::make_input_state(
                window,
                cx,
                language_registry_name(&language),
                None,
                settings,
                false,
            )
        });
        Self {
            id,
            title: title.into(),
            content,
            location: TabLocation::Untitled,
            modified: false,
            original_content_hash,
            original_content_len,
            encoding: UTF_8.to_string(),
            lossy_decode: false,
            language,
            show_markdown_toolbar: settings.markdown_settings.show_markdown_toolbar,
            show_markdown_preview: settings.markdown_settings.show_markdown_preview,
            file_size_bytes: None,
            file_last_modified: None,
            large_file: false,
            csv_view_mode: super::CsvViewMode::Text,
            csv_delimiter: crate::fulgur::files::csv_support::DEFAULT_DELIMITER,
            csv_table: None,
            csv_table_source_hash: 0,
            log_view: false,
            log_follow: true,
            log_full: false,
            log_content: None,
            content_subscription: None,
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
        id: TabId,
        contents: &str,
        file_name: String,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
    ) -> Self {
        let language = language_from_content(&file_name, contents);
        let large_file = super::is_large_file(contents.len());
        let (csv_view_mode, csv_delimiter) = initial_csv_state(language, contents);
        let (original_content_hash, original_content_len) = super::content_fingerprint_from_str("");
        let content = cx.new(|cx| {
            super::make_input_state(
                window,
                cx,
                language_registry_name(&language),
                Some(contents.to_string()),
                settings,
                large_file,
            )
        });
        Self {
            id,
            title: file_name.into(),
            content,
            location: TabLocation::Untitled,
            modified: true,
            original_content_hash,
            original_content_len,
            encoding: UTF_8.to_string(),
            lossy_decode: false,
            language,
            show_markdown_toolbar: settings.markdown_settings.show_markdown_toolbar,
            show_markdown_preview: settings.markdown_settings.show_markdown_preview,
            file_size_bytes: None,
            file_last_modified: None,
            large_file,
            csv_view_mode,
            csv_delimiter,
            csv_table: None,
            csv_table_source_hash: 0,
            log_view: false,
            log_follow: true,
            log_full: false,
            log_content: None,
            content_subscription: None,
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
        let (original_content_hash, original_content_len) =
            super::content_fingerprint_from_str(&params.contents);
        let file_name = params
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(UNTITLED)
            .to_string();

        let language = language_from_content(&file_name, &params.contents);
        let large_file = super::is_large_file(content_len);
        let (csv_view_mode, csv_delimiter) = initial_csv_state(language, &params.contents);
        let content = cx.new(|cx| {
            super::make_input_state(
                window,
                cx,
                language_registry_name(&language),
                Some(params.contents.clone()),
                settings,
                large_file,
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
            location: TabLocation::Local(params.path),
            modified: params.is_modified,
            original_content_hash,
            original_content_len,
            encoding: params.encoding,
            lossy_decode: false,
            language,
            show_markdown_toolbar: settings.markdown_settings.show_markdown_toolbar,
            show_markdown_preview: settings.markdown_settings.show_markdown_preview,
            file_size_bytes: Some(content_len as u64),
            file_last_modified: Some(SystemTime::now()),
            large_file,
            csv_view_mode,
            csv_delimiter,
            csv_table: None,
            csv_table_source_hash: 0,
            log_view: false,
            log_follow: true,
            log_full: false,
            log_content: None,
            content_subscription: None,
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
        let large_file = super::is_large_file(params.current_content.len());
        let (csv_view_mode, csv_delimiter) =
            initial_csv_state(params.language, &params.current_content);
        let (original_content_hash, original_content_len) = super::content_fingerprint_from_str("");
        let content = cx.new(|cx| {
            super::make_input_state(
                window,
                cx,
                language_registry_name(&params.language),
                Some(params.current_content),
                settings,
                large_file,
            )
        });
        Self {
            id: params.id,
            title: params.title,
            content,
            location: TabLocation::Untitled,
            modified: true,
            original_content_hash,
            original_content_len,
            encoding: params.encoding,
            lossy_decode: params.lossy_decode,
            language: params.language,
            show_markdown_toolbar: settings.markdown_settings.show_markdown_toolbar,
            show_markdown_preview: settings.markdown_settings.show_markdown_preview,
            file_size_bytes: None,
            file_last_modified: None,
            large_file,
            csv_view_mode,
            csv_delimiter,
            csv_table: None,
            csv_table_source_hash: 0,
            log_view: false,
            log_follow: true,
            log_full: false,
            log_content: None,
            content_subscription: None,
        }
    }

    /// Create a new tab associated with a remote file.
    ///
    /// ### Arguments
    /// - `id`: The ID of the tab
    /// - `spec`: The parsed remote file specification
    /// - `window`: The window to create the tab in
    /// - `cx`: The application context
    /// - `settings`: The settings for the input state
    ///
    /// ### Returns
    /// - `EditorTab`: The new tab
    pub fn from_remote(
        id: TabId,
        spec: RemoteSpec,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
    ) -> Self {
        let file_name = spec
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&spec.path)
            .to_string();
        let language = language_from_content(&file_name, "");
        let (csv_view_mode, csv_delimiter) = initial_csv_state(language, "");
        let (original_content_hash, original_content_len) = super::content_fingerprint_from_str("");
        let content = cx.new(|cx| {
            super::make_input_state(
                window,
                cx,
                language_registry_name(&language),
                None,
                settings,
                false,
            )
        });
        Self {
            id,
            title: file_name.into(),
            content,
            location: TabLocation::Remote(spec),
            modified: false,
            original_content_hash,
            original_content_len,
            encoding: UTF_8.to_string(),
            lossy_decode: false,
            language,
            show_markdown_toolbar: settings.markdown_settings.show_markdown_toolbar,
            show_markdown_preview: settings.markdown_settings.show_markdown_preview,
            file_size_bytes: None,
            file_last_modified: None,
            large_file: false,
            csv_view_mode,
            csv_delimiter,
            csv_table: None,
            csv_table_source_hash: 0,
            log_view: false,
            log_follow: true,
            log_full: false,
            log_content: None,
            content_subscription: None,
        }
    }

    /// Create a new tab from a successfully loaded remote file.
    ///
    /// ### Arguments
    /// - `id`: The ID of the tab
    /// - `result`: The successfully loaded remote file result
    /// - `window`: The window to create the tab in
    /// - `cx`: The application context
    /// - `settings`: The settings for the input state
    ///
    /// ### Returns
    /// - `EditorTab`: The new tab with remote file content
    pub fn from_remote_loaded(
        id: TabId,
        result: RemoteFileResult,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
    ) -> Self {
        let file_name = result
            .spec
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&result.spec.path)
            .to_string();
        let language = language_from_content(&file_name, &result.content);
        let (csv_view_mode, csv_delimiter) = initial_csv_state(language, &result.content);
        let (original_content_hash, original_content_len) =
            super::content_fingerprint_from_str(&result.content);
        let large_file = super::is_large_file(original_content_len);
        let content = cx.new(|cx| {
            super::make_input_state(
                window,
                cx,
                language_registry_name(&language),
                Some(result.content),
                settings,
                large_file,
            )
        });
        Self {
            id,
            title: file_name.into(),
            content,
            location: TabLocation::Remote(result.spec),
            modified: false,
            original_content_hash,
            original_content_len,
            encoding: result.encoding,
            lossy_decode: result.lossy,
            language,
            show_markdown_toolbar: settings.markdown_settings.show_markdown_toolbar,
            show_markdown_preview: settings.markdown_settings.show_markdown_preview,
            file_size_bytes: Some(result.file_size as u64),
            file_last_modified: Some(SystemTime::now()),
            large_file,
            csv_view_mode,
            csv_delimiter,
            csv_table: None,
            csv_table_source_hash: 0,
            log_view: false,
            log_follow: true,
            log_full: false,
            log_content: None,
            content_subscription: None,
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
        id: TabId,
        data: TabTransferData,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
    ) -> Self {
        let cursor_position = data.cursor_position;
        let large_file = super::is_large_file(data.content.len());
        let content = cx.new(|cx| {
            super::make_input_state(
                window,
                cx,
                language_registry_name(&data.language),
                Some(data.content),
                settings,
                large_file,
            )
        });
        content.update(cx, |state, cx| {
            state.set_cursor_position(cursor_position, window, cx);
        });
        Self {
            id,
            title: data.title,
            content,
            location: data.location,
            modified: data.modified,
            original_content_hash: data.original_content_hash,
            original_content_len: data.original_content_len,
            encoding: data.encoding,
            lossy_decode: data.lossy_decode,
            language: data.language,
            show_markdown_toolbar: data.show_markdown_toolbar,
            show_markdown_preview: data.show_markdown_preview,
            file_size_bytes: data.file_size_bytes,
            file_last_modified: data.file_last_modified,
            large_file,
            csv_view_mode: data.csv_view_mode,
            csv_delimiter: data.csv_delimiter,
            csv_table: None,
            csv_table_source_hash: 0,
            log_view: data.log_view,
            log_follow: true,
            log_full: false,
            log_content: None,
            content_subscription: None,
        }
    }
}
