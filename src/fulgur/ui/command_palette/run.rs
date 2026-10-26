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
            has_remote_history: !Self::shared_state(cx).remote_history.is_empty(),
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
            PaletteCommand::CloseWindow => self.request_window_close(window, cx),
            PaletteCommand::ClearRecentFiles => self.clear_recent_files(cx),
            PaletteCommand::ClearRemoteHistory => Self::clear_remote_history(cx),
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

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::{CommandPaletteEvent, PaletteCommand, PaletteContext};
    use crate::fulgur::{
        Fulgur,
        languages::supported_languages::SupportedLanguage,
        settings::ServerProfile,
        tab::Tab,
        ui::tabs::editor_tab::{EditorTab, TabLocation},
    };
    use crate::test_support::{setup_fulgur, temp_test_path};
    use gpui_kit::{App, Context, Entity, TestAppContext, VisualTestContext, Window};

    /// Reshape the active editor tab, then snapshot the palette context of its window.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state owning the active editor tab
    /// - `visual_cx`: The visual test context of that window
    /// - `configure`: Applied to the active editor tab before the snapshot is taken
    ///
    /// ### Returns
    /// - `PaletteContext`: The context derived from the reshaped window state
    fn context_with_active_editor(
        fulgur: &Entity<Fulgur>,
        visual_cx: &mut VisualTestContext,
        configure: impl FnOnce(&mut EditorTab),
    ) -> PaletteContext {
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                this.update_active_editor_tab(cx, |editor, _| configure(editor))
                    .expect("the test window should start on an editor tab");
                this.palette_context(cx)
            })
        })
    }

    /// Run a command as if the user had confirmed it in the palette.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state receiving the palette event
    /// - `command`: The confirmed command
    /// - `window`: The window to run the command in
    /// - `cx`: The application context
    fn run(
        fulgur: &mut Fulgur,
        command: PaletteCommand,
        window: &mut Window,
        cx: &mut Context<Fulgur>,
    ) {
        fulgur.on_command_palette_event(CommandPaletteEvent::Run(command), window, cx);
    }

    /// Read the active editor tab's text.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state owning the active editor tab
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `String`: The buffer content of the active editor tab
    fn active_text(fulgur: &Fulgur, cx: &App) -> String {
        fulgur
            .get_active_editor_tab(cx)
            .expect("expected an active editor tab")
            .content
            .read(cx)
            .text()
            .to_string()
    }

    // ========== palette_context ==========

    #[gpui_kit::test]
    fn an_untitled_buffer_offers_an_editor_without_a_path(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        let context = context_with_active_editor(&fulgur, &mut visual_cx, |_| {});

        assert_eq!(
            context,
            PaletteContext {
                tab_count: 1,
                active_tab_index: Some(0),
                has_editor_tab: true,
                ..PaletteContext::default()
            }
        );
    }

    #[gpui_kit::test]
    fn a_window_without_tabs_derives_the_empty_context(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        let context = visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.close_all_tabs(window, cx);
                this.palette_context(cx)
            })
        });

        assert_eq!(context, PaletteContext::default());
    }

    #[gpui_kit::test]
    fn both_markdown_flavours_count_as_markdown(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        for language in [
            SupportedLanguage::Markdown,
            SupportedLanguage::MarkdownInline,
        ] {
            let context = context_with_active_editor(&fulgur, &mut visual_cx, |editor| {
                editor.language = language;
                editor.location = TabLocation::Local(temp_test_path("fulgur-palette-notes.md"));
            });

            assert!(context.is_markdown, "{language:?} should count as Markdown");
            assert!(context.has_local_path);
            assert!(!context.is_csv);
            assert!(!context.log_toggle_available);
        }
    }

    #[gpui_kit::test]
    fn a_csv_tab_is_flagged_as_csv(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        let context = context_with_active_editor(&fulgur, &mut visual_cx, |editor| {
            editor.language = SupportedLanguage::Csv;
        });

        assert!(context.is_csv);
        assert!(!context.is_markdown);
    }

    #[gpui_kit::test]
    fn the_large_file_guard_reaches_the_context(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        let context = context_with_active_editor(&fulgur, &mut visual_cx, |editor| {
            editor.language = SupportedLanguage::Markdown;
            editor.large_file = true;
        });

        assert!(context.is_large_file);
        assert!(context.is_markdown);
    }

    #[gpui_kit::test]
    fn the_log_toggle_follows_the_local_file_extension(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        let log_context = context_with_active_editor(&fulgur, &mut visual_cx, |editor| {
            editor.location = TabLocation::Local(temp_test_path("fulgur-palette-app.log"));
        });
        assert!(log_context.log_toggle_available);
        assert!(log_context.has_local_path);

        let rust_context = context_with_active_editor(&fulgur, &mut visual_cx, |editor| {
            editor.location = TabLocation::Local(temp_test_path("fulgur-palette-main.rs"));
        });
        assert!(!rust_context.log_toggle_available);
    }

    #[gpui_kit::test]
    fn a_settings_tab_hides_the_editor_state_of_the_tab_behind_it(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        context_with_active_editor(&fulgur, &mut visual_cx, |editor| {
            editor.language = SupportedLanguage::Markdown;
            editor.location = TabLocation::Local(temp_test_path("fulgur-palette-app.log"));
        });

        let context = visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.open_settings(window, cx);
                this.palette_context(cx)
            })
        });

        assert_eq!(
            context,
            PaletteContext {
                tab_count: 2,
                active_tab_index: Some(1),
                ..PaletteContext::default()
            }
        );
    }

    #[gpui_kit::test]
    fn recent_files_and_active_profiles_come_from_the_window_settings(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        let (inactive_profile, active_profile) = visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                this.settings
                    .recent_files
                    .add_file(temp_test_path("fulgur-palette-recent.txt"));
                let sync = &mut this.settings.app_settings.synchronization_settings;
                sync.is_synchronization_activated = true;
                sync.profiles.push(ServerProfile::new("Inactive"));
                let inactive_profile = this.palette_context(cx);

                let sync = &mut this.settings.app_settings.synchronization_settings;
                sync.profiles[0].is_active = true;
                (inactive_profile, this.palette_context(cx))
            })
        });

        assert!(inactive_profile.has_recent_files);
        assert!(!inactive_profile.has_active_sync_profile);
        assert!(active_profile.has_active_sync_profile);
    }

    #[gpui_kit::test]
    fn the_remote_history_reaches_the_context_and_its_command_clears_it(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        let (empty, filled, cleared) = visual_cx.update(|window, cx| {
            let empty = fulgur.read(cx).palette_context(cx);
            Fulgur::record_remote_history_url("ssh://host/".to_string(), cx);
            let filled = fulgur.read(cx).palette_context(cx);
            fulgur.update(cx, |this, cx| {
                run(this, PaletteCommand::ClearRemoteHistory, window, cx);
            });
            (empty, filled, fulgur.read(cx).palette_context(cx))
        });

        assert!(!empty.has_remote_history);
        assert!(filled.has_remote_history);
        assert!(
            !cleared.has_remote_history,
            "running the command must forget every remote URL"
        );
    }

    // ========== dispatch ==========

    #[gpui_kit::test]
    fn new_file_opens_and_activates_a_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                run(this, PaletteCommand::NewFile, window, cx);

                assert_eq!(this.tabs.len(), 2);
                assert_eq!(this.active_tab_index(cx), Some(1));
            });
        });
    }

    #[gpui_kit::test]
    fn next_and_previous_tab_wrap_around_the_strip(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                // Three tabs, so stepping forward and backward land on different tabs.
                this.new_tab(window, cx);
                this.new_tab(window, cx);
                assert_eq!(this.active_tab_index(cx), Some(2));

                run(this, PaletteCommand::NextTab, window, cx);
                assert_eq!(this.active_tab_index(cx), Some(0));

                run(this, PaletteCommand::PreviousTab, window, cx);
                assert_eq!(this.active_tab_index(cx), Some(2));

                run(this, PaletteCommand::PreviousTab, window, cx);
                assert_eq!(this.active_tab_index(cx), Some(1));
            });
        });
    }

    #[gpui_kit::test]
    fn duplicate_active_tab_copies_the_active_buffer_next_to_it(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.update_active_editor_tab(cx, |editor, cx| {
                    editor.content.update(cx, |content, cx| {
                        content.set_value("duplicated text", window, cx);
                    });
                });
                run(this, PaletteCommand::DuplicateActiveTab, window, cx);

                assert_eq!(this.tabs.len(), 3);
                assert_eq!(this.active_tab_index(cx), Some(2));
                assert_eq!(active_text(this, cx), "duplicated text");
            });
        });
    }

    #[gpui_kit::test]
    fn close_other_tabs_keeps_only_the_active_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.new_tab(window, cx);
                this.set_active_tab(1, window, cx);
                let active_id = this.active_tab(cx).map(Tab::id);

                run(this, PaletteCommand::CloseOtherTabs, window, cx);

                assert_eq!(this.tabs.len(), 1);
                assert_eq!(this.active_tab(cx).map(Tab::id), active_id);
            });
        });
    }

    #[gpui_kit::test]
    fn an_active_tab_command_without_tabs_is_a_no_op(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.close_all_tabs(window, cx);

                run(this, PaletteCommand::DuplicateActiveTab, window, cx);

                assert_eq!(this.tabs, [] as [Entity<Tab>; 0]);
            });
        });
    }

    #[gpui_kit::test]
    fn toggle_markdown_preview_opens_then_closes_the_preview_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        context_with_active_editor(&fulgur, &mut visual_cx, |editor| {
            editor.language = SupportedLanguage::Markdown;
        });

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                run(this, PaletteCommand::ToggleMarkdownPreview, window, cx);
                assert_eq!(this.tabs.len(), 2);
                assert!(matches!(this.active_tab(cx), Some(Tab::MarkdownPreview(_))));

                run(this, PaletteCommand::ToggleMarkdownPreview, window, cx);
                assert_eq!(this.tabs.len(), 1);
                assert!(matches!(this.active_tab(cx), Some(Tab::Editor(_))));
            });
        });
    }

    #[gpui_kit::test]
    fn find_in_file_toggles_the_search_bar(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                run(this, PaletteCommand::FindInFile, window, cx);
                assert!(this.search_bar.read(cx).is_visible());

                run(this, PaletteCommand::FindInFile, window, cx);
                assert!(!this.search_bar.read(cx).is_visible());
            });
        });
    }

    #[gpui_kit::test]
    fn find_and_replace_keeps_the_search_bar_open(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                run(this, PaletteCommand::FindAndReplace, window, cx);
                assert!(this.search_bar.read(cx).is_visible());

                // Unlike Find, Find and Replace never toggles the bar shut.
                run(this, PaletteCommand::FindAndReplace, window, cx);
                assert!(this.search_bar.read(cx).is_visible());
            });
        });
    }
}
