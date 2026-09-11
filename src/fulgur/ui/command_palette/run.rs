//! The window side of the command palette: snapshotting the state the palette filters
//! on, opening it, and running the command the user confirmed.

use super::{CommandPaletteEvent, PaletteCommand, PaletteContext};
use crate::fulgur::{
    Fulgur,
    languages::supported_languages::SupportedLanguage,
    tab::Tab,
    ui::{
        bars::status_bar::StatusBarEvent,
        dialogs::about::about,
        log_view::log_toggle_available,
        tabs::{
            editor_tab::EditorTab,
            tab_bar::{
                CloseAllOtherTabs, CloseTabsToLeft, CloseTabsToRight, CopyPath, DuplicateTab,
                RenameTab, ShowInFileManager, open_theme_repository,
            },
        },
    },
};
use gpui_kit::base::input::{AddCursorAbove, AddCursorBelow};
use gpui_kit::{Action, App, Context, Window};

impl Fulgur {
    /// Snapshot the window state the palette filters its command list on.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `PaletteContext`: The state deciding which commands the palette offers
    fn palette_context(&self, cx: &App) -> PaletteContext {
        let editor_tab = self.active_tab(cx).and_then(Tab::as_editor);
        let language = editor_tab.map_or(SupportedLanguage::Plain, |tab| tab.language);
        let file_path = editor_tab.and_then(EditorTab::file_path);

        PaletteContext {
            tab_count: self.tabs.len(),
            active_tab_index: self.active_tab_index(cx),
            has_editor_tab: editor_tab.is_some(),
            has_local_path: file_path.is_some(),
            is_markdown: matches!(
                language,
                SupportedLanguage::Markdown | SupportedLanguage::MarkdownInline
            ),
            is_csv: language == SupportedLanguage::Csv,
            is_large_file: editor_tab.is_some_and(|tab| tab.large_file),
            log_toggle_available: file_path
                .map(std::path::PathBuf::as_path)
                .is_some_and(log_toggle_available),
            has_recent_files: !self.settings.recent_files.get_files().is_empty(),
            has_active_sync_profile: !self.collect_active_profiles().is_empty(),
        }
    }

    /// Open the command palette, or close it when it is already showing.
    ///
    /// ### Arguments
    /// - `window`: The window hosting the palette
    /// - `cx`: The application context
    pub fn toggle_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let context = self.palette_context(cx);
        self.command_palette.update(cx, |palette, cx| {
            if palette.is_open() {
                palette.close(window, cx);
            } else {
                palette.open(&context, window, cx);
            }
        });
    }

    /// Handle a command palette event.
    ///
    /// ### Arguments
    /// - `event`: The event emitted by the palette
    /// - `window`: The window hosting the palette
    /// - `cx`: The application context
    pub(crate) fn on_command_palette_event(
        &mut self,
        event: CommandPaletteEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            CommandPaletteEvent::Run(command) => self.run_palette_command(command, window, cx),
        }
    }

    /// Run a command confirmed in the palette.
    ///
    /// ### Arguments
    /// - `command`: The confirmed command
    /// - `window`: The window to run the command in
    /// - `cx`: The application context
    fn run_palette_command(
        &mut self,
        command: PaletteCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            PaletteCommand::NewFile => self.new_tab(window, cx),
            PaletteCommand::NewWindow => self.open_new_window(cx),
            PaletteCommand::OpenFile => self.open_file(window, cx),
            PaletteCommand::OpenFromPath => self.show_open_from_path_dialog(window, cx),
            PaletteCommand::OpenRemoteFile => self.show_open_remote_dialog(window, cx),
            PaletteCommand::SaveFile => self.save_file(window, cx),
            PaletteCommand::SaveFileAs => self.save_file_as(window, cx),
            PaletteCommand::PrintFile => self.print_file(window, cx),
            PaletteCommand::ShareFile => self.open_share_file_sheet(window, cx),
            PaletteCommand::CloseFile => self.close_active_tab(window, cx),
            PaletteCommand::CloseAllFiles => self.close_all_tabs(window, cx),
            PaletteCommand::ClearRecentFiles => self.clear_recent_files(cx),
            PaletteCommand::Quit => self.quit(window, cx),
            PaletteCommand::AddCursorAbove => {
                self.dispatch_to_active_editor(Box::new(AddCursorAbove), window, cx);
            }
            PaletteCommand::AddCursorBelow => {
                self.dispatch_to_active_editor(Box::new(AddCursorBelow), window, cx);
            }
            PaletteCommand::FindInFile => self.find_in_file(window, cx),
            PaletteCommand::FindAndReplace => self.find_and_replace(window, cx),
            PaletteCommand::JumpToLine => self.show_jump_to_line_dialog(window, cx),
            PaletteCommand::SelectLanguage => self.render_select_language_sheet(window, cx),
            PaletteCommand::ToggleMarkdownPreview => {
                self.on_status_bar_event(StatusBarEvent::ToggleMarkdownPreview, window, cx);
            }
            PaletteCommand::ToggleMarkdownToolbar => {
                self.on_status_bar_event(StatusBarEvent::ToggleMarkdownToolbar, window, cx);
            }
            PaletteCommand::ToggleCsvView => {
                self.on_status_bar_event(StatusBarEvent::ToggleCsvView, window, cx);
            }
            PaletteCommand::ToggleLogView => {
                self.on_status_bar_event(StatusBarEvent::ToggleLogView, window, cx);
            }
            PaletteCommand::ToggleColorPicker => self.toggle_color_picker(window, cx),
            PaletteCommand::SelectTheme => self.select_theme_sheet(window, cx),
            PaletteCommand::GetMoreThemes => open_theme_repository(),
            PaletteCommand::NextTab => self.on_next_tab(window, cx),
            PaletteCommand::PreviousTab => self.on_previous_tab(window, cx),
            PaletteCommand::RenameActiveTab
            | PaletteCommand::DuplicateActiveTab
            | PaletteCommand::CopyActiveTabPath
            | PaletteCommand::ShowActiveTabInFileManager
            | PaletteCommand::CloseTabsToLeft
            | PaletteCommand::CloseTabsToRight
            | PaletteCommand::CloseOtherTabs => self.run_active_tab_command(command, window, cx),
            PaletteCommand::OpenSettings => self.open_settings(window, cx),
            PaletteCommand::CheckForUpdates => self.check_for_updates(window, cx),
            PaletteCommand::About => about(window, cx),
        }
    }

    /// Dispatch an editor action to the active tab's editor.
    ///
    /// ### Arguments
    /// - `action`: The editor action to dispatch
    /// - `window`: The window to run the action in
    /// - `cx`: The application context
    fn dispatch_to_active_editor(
        &mut self,
        action: Box<dyn Action>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_active_tab(window, cx);
        window.dispatch_action(action, cx);
    }

    /// Run a palette command that targets the active tab.
    ///
    /// ### Arguments
    /// - `command`: The confirmed command, which must be one of the active-tab commands
    /// - `window`: The window to run the command in
    /// - `cx`: The application context
    fn run_active_tab_command(
        &mut self,
        command: PaletteCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_id) = self.active_tab(cx).map(Tab::id) else {
            log::warn!("command palette ran {command:?} with no active tab");
            return;
        };

        match command {
            PaletteCommand::RenameActiveTab => {
                self.on_rename_tab(&RenameTab(tab_id), window, cx);
            }
            PaletteCommand::DuplicateActiveTab => {
                self.on_duplicate_tab(&DuplicateTab(tab_id), window, cx);
            }
            PaletteCommand::CopyActiveTabPath => {
                self.on_copy_path(&CopyPath(tab_id), window, cx);
            }
            PaletteCommand::ShowActiveTabInFileManager => {
                self.on_show_in_file_manager(&ShowInFileManager(tab_id), window, cx);
            }
            PaletteCommand::CloseTabsToLeft => {
                self.on_close_tabs_to_left(&CloseTabsToLeft(tab_id), window, cx);
            }
            PaletteCommand::CloseTabsToRight => {
                self.on_close_tabs_to_right(&CloseTabsToRight(tab_id), window, cx);
            }
            PaletteCommand::CloseOtherTabs => {
                self.on_close_all_other_tabs_action(&CloseAllOtherTabs(tab_id), window, cx);
            }
            _ => log::error!("{command:?} is not an active-tab command"),
        }
    }
}
