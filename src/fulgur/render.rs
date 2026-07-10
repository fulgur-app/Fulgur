use crate::fulgur::{
    Fulgur,
    tab::Tab,
    ui::{
        self, dialogs::about::about, notifications::update_notification::make_update_notification,
    },
};
use crate::register_action;
use gpui::{
    Anchor, App, Context, ExternalPaths, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, Window, div, px,
};
use gpui_component::{ActiveTheme, Root, StyledExt, WindowExt, v_flex};

impl Focusable for Fulgur {
    /// Get the focus handle for the Fulgur instance
    ///
    /// ### Arguments
    /// - `_cx`: The application context
    ///
    /// ### Returns
    /// - `FocusHandle`: The focus handle for the Fulgur instance
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Fulgur {
    /// Render the Fulgur instance
    ///
    /// ### Arguments
    /// - `window`: The window to render the Fulgur instance in
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered Fulgur instance
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.process_pending_initial_active_tab_activation(window, cx);
        self.process_window_state_updates(window, cx);
        Self::process_update_notifications(window, cx);
        self.process_pending_files_from_macos(window, cx);
        #[cfg(target_os = "windows")]
        self.process_pending_ipc_commands(window, cx);
        self.process_shared_files_from_sync(window, cx);
        self.process_pending_remote_files(window, cx);
        self.process_pending_share_sheet(window, cx);
        if self.tabs.is_empty() {
            self.active_tab_id = None;
        }
        self.propagate_settings_to_tabs(window, cx);
        self.track_newly_rendered_tabs(cx);
        self.handle_pending_transfer_scroll(window, cx);
        self.handle_pending_tab_transfer(window, cx);
        self.handle_pending_tab_removal(window, cx);
        self.handle_pending_jump_to_line(window, cx);
        self.update_modified_status(cx);
        self.prune_markdown_preview_cache(cx);
        self.refresh_window_title(cx);
        let app_content = self.build_app_content_with_actions(self.active_tab_index(), window, cx);
        self.assemble_ui_tree(app_content, window, cx)
    }
}

impl Fulgur {
    /// Process update notifications from the background update checker
    ///
    /// ### Arguments
    /// - `window`: The window to display the notification in
    /// - `cx`: The application context
    fn process_update_notifications(window: &mut Window, cx: &mut Context<Self>) {
        let update_info = {
            let shared = Fulgur::shared_state(cx);
            shared.update_info.lock().take()
        };
        if let Some(update_info) = update_info {
            let notification = make_update_notification(&update_info);
            window.push_notification(notification, cx);
        }
    }

    /// Build the main application content with all action handlers
    ///
    /// ### Arguments
    /// - `active_tab_index`: The index of the currently active tab (if any)
    /// - `window`: The window to build the content for
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The fully constructed content area with all action handlers attached
    fn build_app_content_with_actions(
        &mut self,
        active_tab_index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let mut app_content = div()
            .id("app-content")
            .size_full()
            .relative()
            .group("")
            .flex()
            .flex_col()
            .gap_0()
            .track_focus(&self.focus_handle);
        register_action!(app_content, cx, ui::menus::NewFile => new_tab);
        register_action!(app_content, cx, ui::menus::OpenFile => open_file);
        register_action!(app_content, cx, ui::menus::OpenPath => show_open_from_path_dialog);
        register_action!(app_content, cx, ui::menus::OpenRemote => show_open_remote_dialog);
        register_action!(app_content, cx, ui::menus::CloseAllFiles => close_all_tabs);
        register_action!(app_content, cx, ui::menus::SaveFile => save_file);
        register_action!(app_content, cx, ui::menus::SaveFileAs => save_file_as);
        register_action!(app_content, cx, ui::menus::Quit => quit);
        register_action!(app_content, cx, ui::menus::SettingsTab => open_settings);
        register_action!(app_content, cx, ui::menus::FindInFile => find_in_file);
        register_action!(app_content, cx, ui::menus::ToggleColorPicker => toggle_color_picker);
        register_action!(app_content, cx, ui::menus::NextTab => on_next_tab);
        register_action!(app_content, cx, ui::menus::PreviousTab => on_previous_tab);
        register_action!(app_content, cx, ui::menus::JumpToLine => show_jump_to_line_dialog);
        register_action!(app_content, cx, ui::menus::SelectTheme => select_theme_sheet);
        register_action!(app_content, cx, ui::menus::About => call about);
        register_action!(app_content, cx, ui::menus::SwitchTheme => switch_to_theme(.0, no_window));
        register_action!(app_content, cx, ui::tabs::tab_bar::CloseTabAction => on_close_tab_action(&action));
        register_action!(app_content, cx, ui::tabs::tab_bar::CloseTabsToLeft => on_close_tabs_to_left(&action));
        register_action!(app_content, cx, ui::tabs::tab_bar::CloseTabsToRight => on_close_tabs_to_right(&action));
        register_action!(app_content, cx, ui::tabs::tab_bar::CloseAllTabsAction => on_close_all_tabs_action(&action));
        register_action!(app_content, cx, ui::tabs::tab_bar::CloseAllOtherTabs => on_close_all_other_tabs_action(&action));
        register_action!(app_content, cx, ui::tabs::tab_bar::ShowInFileManager => on_show_in_file_manager(&action));
        register_action!(app_content, cx, ui::tabs::tab_bar::CopyPath => on_copy_path(&action));
        register_action!(app_content, cx, ui::tabs::tab_bar::DuplicateTab => on_duplicate_tab(&action));
        register_action!(app_content, cx, ui::menus::OpenRecentFile => do_open_recent_file(.0));
        register_action!(app_content, cx, ui::menus::CheckForUpdates => check_for_updates);
        register_action!(app_content, cx, ui::menus::GetTheme => call_no_args ui::tabs::tab_bar::open_theme_repository);
        register_action!(app_content, cx, ui::menus::NewWindow => open_new_window(cx_only));
        register_action!(app_content, cx, ui::menus::ClearRecentFiles => clear_recent_files(cx_only));
        register_action!(app_content, cx, ui::menus::CloseFile => close_active_tab);
        register_action!(app_content, cx, ui::menus::PrintFile => print_file);
        register_action!(app_content, cx, ui::menus::DockActivateTab => handle_dock_activate_tab(&action));
        register_action!(app_content, cx, ui::menus::DockActivateTabByTitle => handle_dock_activate_tab_by_title(&action));
        app_content =
            app_content.on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.handle_external_paths_drop(paths, window, cx);
            }));
        let search_bar_visible = self.search_bar.read(cx).is_visible();
        app_content = app_content
            .child(self.tab_bar.clone())
            .child(self.render_content_area(active_tab_index, window, cx))
            .children(self.render_markdown_bar(cx))
            .children(self.render_csv_toolbar(cx))
            .children(search_bar_visible.then(|| self.search_bar.clone()))
            .children(self.render_color_picker_bar(cx));
        if let Some(Tab::Editor(_)) = self.active_tab() {
            app_content = app_content.child(self.status_bar.clone());
        }
        app_content = app_content.child(Self::render_external_file_drop_overlay(cx));
        app_content
    }

    /// Assemble the final UI tree with all layers
    ///
    /// ### Arguments
    /// - `app_content`: The main content area (from `build_app_content_with_actions()`)
    /// - `window`: The window to assemble the UI for
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The complete UI tree ready to be rendered
    fn assemble_ui_tree(
        &self,
        app_content: impl IntoElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Create root layout: TitleBar OUTSIDE of focus-tracked content
        // This is critical for Windows hit-testing to work!
        let root_content = v_flex()
            .size_full()
            .child(self.title_bar.clone())
            .child(app_content);
        let mut root = div()
            .size_full()
            .child(root_content)
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
            .children(Root::render_dialog_layer(window, cx));
        if let Some((position, menu)) = self
            .editor_context_menu
            .as_ref()
            .map(|(pos, menu)| (*pos, menu.clone()))
        {
            root = root.child(
                gpui::deferred(
                    gpui::anchored()
                        .position(position)
                        .snap_to_window_with_margin(px(8.))
                        .anchor(Anchor::TopLeft)
                        .child(
                            div()
                                .font_family(cx.theme().font_family.clone())
                                .cursor_default()
                                .child(menu),
                        ),
                )
                .with_priority(1),
            );
        }
        root
    }

    /// Activate the initially restored tab after the first render pass.
    ///
    /// The first render builds the window's `Root` layers. Startup flows that can open
    /// dialogs (like remote password prompts triggered by `set_active_tab`) must wait
    /// until that initial render has completed.
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    fn process_pending_initial_active_tab_activation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.has_rendered_once {
            self.has_rendered_once = true;
            if self.pending_initial_active_tab.is_some() {
                cx.notify();
            }
            return;
        }

        if let Some(tab_id) = self.pending_initial_active_tab.take()
            && let Some(index) = self.tab_index_of(tab_id)
        {
            self.set_active_tab(index, window, cx);
        }
    }

    /// Refresh the window title from the active tab's title
    ///
    /// ### Arguments
    /// - `cx`: The application context
    fn refresh_window_title(&self, cx: &mut Context<Self>) {
        let title = self.active_tab().map(|tab| tab.title().to_string());
        if let Some(title) = title {
            self.set_title(Some(title), cx);
        }
    }

    /// Render a visual overlay while external files are being dragged over the window.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered overlay
    fn render_external_file_drop_overlay(cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("external-file-drop-overlay")
            .invisible()
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .left_0()
            .flex()
            .justify_center()
            .items_center()
            .border_2()
            .border_dashed()
            .border_color(cx.theme().primary.opacity(0.7))
            .bg(cx.theme().muted.opacity(0.4))
            .on_drag_move::<ExternalPaths>(|_, _, _| {})
            .group_drag_over::<ExternalPaths>("", gpui::Styled::visible)
            .child(
                div()
                    .px_4()
                    .py_2()
                    .rounded_sm()
                    .text_color(cx.theme().primary)
                    .font_bold()
                    .child("Drop files to open"),
            )
    }
}
