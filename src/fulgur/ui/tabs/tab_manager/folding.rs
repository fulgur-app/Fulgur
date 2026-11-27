use crate::fulgur::{
    Fulgur,
    editor_tab::{EditorTab, FoldCommand, apply_fold_command},
    tab::TabId,
    ui::{
        menus::{FoldAll, FoldAtLevel, ToggleFold, UnfoldAll},
        tabs::tab_bar::FoldTab,
    },
};
use gpui_kit::{Context, Window};

impl Fulgur {
    /// Apply a folding command to the editor of a tab.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab whose editor to fold
    /// - `command`: The folding command to apply
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn fold_tab(
        &mut self,
        tab_id: TabId,
        command: FoldCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(content) = self
            .editor_tab(tab_id, cx)
            .and_then(EditorTab::foldable_content)
        else {
            return;
        };
        content.update(cx, |state, cx| {
            apply_fold_command(state, command, window, cx);
        });
    }

    /// Apply a folding command to the active tab's editor.
    ///
    /// ### Arguments
    /// - `command`: The folding command to apply
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn fold_active_tab(
        &mut self,
        command: FoldCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab_id) = self.active_tab_id {
            self.fold_tab(tab_id, command, window, cx);
        }
    }

    /// Handle the Fold All action on the active editor.
    ///
    /// ### Arguments
    /// - `_`: The action to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_fold_all(&mut self, _: &FoldAll, window: &mut Window, cx: &mut Context<Self>) {
        self.fold_active_tab(FoldCommand::FoldAll, window, cx);
    }

    /// Handle the Unfold All action on the active editor.
    ///
    /// ### Arguments
    /// - `_`: The action to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_unfold_all(&mut self, _: &UnfoldAll, window: &mut Window, cx: &mut Context<Self>) {
        self.fold_active_tab(FoldCommand::UnfoldAll, window, cx);
    }

    /// Handle the Toggle Fold action on the active editor's caret.
    ///
    /// ### Arguments
    /// - `_`: The action to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_toggle_fold(&mut self, _: &ToggleFold, window: &mut Window, cx: &mut Context<Self>) {
        self.fold_active_tab(FoldCommand::ToggleAtCursor, window, cx);
    }

    /// Handle a Fold Level action on the active editor.
    ///
    /// ### Arguments
    /// - `action`: The action carrying the nesting level to fold
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_fold_at_level(
        &mut self,
        action: &FoldAtLevel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.fold_active_tab(FoldCommand::FoldAtLevel(action.0), window, cx);
    }

    /// Handle a folding command picked from a tab's context menu.
    ///
    /// ### Arguments
    /// - `action`: The action carrying the tab and the folding command
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_fold_tab(&mut self, action: &FoldTab, window: &mut Window, cx: &mut Context<Self>) {
        self.fold_tab(action.0, action.1, window, cx);
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use crate::fulgur::{
        Fulgur,
        editor_tab::{FoldCommand, MAX_FOLD_LEVEL},
        tab::Tab,
        ui::{
            components_utils::action_chord,
            menus::{FoldAll, FoldAtLevel, KEY_CONTEXT_FULGUR, build_default_key_bindings},
            tabs::tab_bar::FoldTab,
        },
    };
    use crate::test_support::setup_fulgur;
    use gpui_kit::component::input::{EditorState, FoldRange, Position};
    use gpui_kit::{Action, Entity, TestAppContext, VisualTestContext};

    /// Open a window whose active editor holds sixteen lines with nested fold candidates:
    /// an outer block 0..=9 holding 2..=6, which holds 3..=5, then a sibling block 11..=14.
    ///
    /// ### Arguments
    /// - `cx`: The test application context
    ///
    /// ### Returns
    /// - `(Entity<Fulgur>, Entity<EditorState>, VisualTestContext)`: The window state, the
    ///   active editor's buffer and the window's visual test context
    fn setup_nested_folds(
        cx: &mut TestAppContext,
    ) -> (Entity<Fulgur>, Entity<EditorState>, VisualTestContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let content = visual_cx.update(|_window, cx| {
            fulgur
                .read(cx)
                .active_editor_content(cx)
                .expect("the test window should start on an editor tab")
        });
        visual_cx.update(|window, cx| {
            content.update(cx, |state, cx| {
                let text: Vec<String> = (0..16).map(|line| format!("line {line}")).collect();
                state.set_value(text.join("\n"), window, cx);
            });
        });
        visual_cx.run_until_parked();
        visual_cx.update(|_window, cx| {
            content.update(cx, |state, cx| {
                state.apply_highlighter_fold_candidates(
                    vec![
                        FoldRange::new(0, 9),
                        FoldRange::new(2, 6),
                        FoldRange::new(3, 5),
                        FoldRange::new(11, 14),
                    ],
                    cx,
                );
            });
        });
        (fulgur, content, visual_cx)
    }

    /// Place the caret at the start of a line.
    ///
    /// ### Arguments
    /// - `content`: The buffer holding the caret
    /// - `line`: The 0-based line to move the caret to
    /// - `visual_cx`: The visual test context of the buffer's window
    fn place_caret(content: &Entity<EditorState>, line: u32, visual_cx: &mut VisualTestContext) {
        visual_cx.update(|window, cx| {
            content.update(cx, |state, cx| {
                state.set_cursor_position(Position { line, character: 0 }, window, cx);
            });
        });
    }

    /// Run a folding command on the active editor tab.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state owning the active editor tab
    /// - `command`: The folding command to apply
    /// - `visual_cx`: The visual test context of that window
    fn fold(fulgur: &Entity<Fulgur>, command: FoldCommand, visual_cx: &mut VisualTestContext) {
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| this.fold_active_tab(command, window, cx));
        });
    }

    /// Get the caret line of a buffer.
    ///
    /// ### Arguments
    /// - `content`: The buffer holding the caret
    /// - `visual_cx`: The visual test context of the buffer's window
    ///
    /// ### Returns
    /// - `u32`: The 0-based caret line
    fn caret_line(content: &Entity<EditorState>, visual_cx: &mut VisualTestContext) -> u32 {
        visual_cx.update(|_window, cx| content.read(cx).cursor_position().line)
    }

    /// Unfold whatever hides a line, reporting whether anything did.
    ///
    /// ### Arguments
    /// - `content`: The buffer to probe
    /// - `line`: The 0-based line to reveal
    /// - `visual_cx`: The visual test context of the buffer's window
    ///
    /// ### Returns
    /// - `bool`: Whether a fold hid the line
    fn reveal(content: &Entity<EditorState>, line: u32, visual_cx: &mut VisualTestContext) -> bool {
        visual_cx.update(|_window, cx| {
            content.update(cx, |state, cx| {
                state.unfold_at(Position { line, character: 0 }, cx)
            })
        })
    }

    #[gpui_kit::test]
    fn fold_depth_reports_the_deepest_nesting_and_large_files_report_none(cx: &mut TestAppContext) {
        let (fulgur, _content, mut visual_cx) = setup_nested_folds(cx);

        let (depth, large_depth) = visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let depth = this.get_active_editor_tab(cx).map(|tab| tab.fold_depth(cx));
                this.update_active_editor_tab(cx, |editor, _| editor.large_file = true);
                let large_depth = this.get_active_editor_tab(cx).map(|tab| tab.fold_depth(cx));
                (depth, large_depth)
            })
        });

        assert_eq!(depth, Some(3));
        assert_eq!(large_depth, Some(0));
    }

    #[gpui_kit::test]
    fn fold_all_moves_a_hidden_caret_to_the_outermost_fold_header(cx: &mut TestAppContext) {
        let (fulgur, content, mut visual_cx) = setup_nested_folds(cx);
        place_caret(&content, 4, &mut visual_cx);

        fold(&fulgur, FoldCommand::FoldAll, &mut visual_cx);

        assert_eq!(caret_line(&content, &mut visual_cx), 0);
        assert!(
            reveal(&content, 12, &mut visual_cx),
            "the sibling must fold too"
        );
    }

    #[gpui_kit::test]
    fn unfold_all_opens_every_fold(cx: &mut TestAppContext) {
        let (fulgur, content, mut visual_cx) = setup_nested_folds(cx);
        fold(&fulgur, FoldCommand::FoldAll, &mut visual_cx);

        fold(&fulgur, FoldCommand::UnfoldAll, &mut visual_cx);

        for line in [1, 4, 12] {
            assert!(
                !reveal(&content, line, &mut visual_cx),
                "line {line} is hidden"
            );
        }
    }

    #[gpui_kit::test]
    fn fold_at_level_folds_only_that_level_and_reveals_the_caret(cx: &mut TestAppContext) {
        let (fulgur, content, mut visual_cx) = setup_nested_folds(cx);
        place_caret(&content, 4, &mut visual_cx);

        fold(&fulgur, FoldCommand::FoldAtLevel(2), &mut visual_cx);

        assert_eq!(caret_line(&content, &mut visual_cx), 2);
        assert!(
            !reveal(&content, 1, &mut visual_cx),
            "level 1 must stay open"
        );
        assert!(
            !reveal(&content, 12, &mut visual_cx),
            "the level-1 sibling must stay open"
        );
        assert!(
            reveal(&content, 4, &mut visual_cx),
            "level 2 must be folded"
        );
    }

    #[gpui_kit::test]
    fn fold_at_level_leaves_a_caret_outside_the_level_in_place(cx: &mut TestAppContext) {
        let (fulgur, content, mut visual_cx) = setup_nested_folds(cx);
        place_caret(&content, 8, &mut visual_cx);

        fold(&fulgur, FoldCommand::FoldAtLevel(2), &mut visual_cx);

        assert_eq!(caret_line(&content, &mut visual_cx), 8);
    }

    #[gpui_kit::test]
    fn toggle_fold_on_a_block_header_folds_that_block(cx: &mut TestAppContext) {
        let (fulgur, content, mut visual_cx) = setup_nested_folds(cx);
        place_caret(&content, 2, &mut visual_cx);

        fold(&fulgur, FoldCommand::ToggleAtCursor, &mut visual_cx);

        assert_eq!(caret_line(&content, &mut visual_cx), 2);
        assert!(
            !reveal(&content, 1, &mut visual_cx),
            "the outer block must stay open"
        );
        assert!(
            reveal(&content, 4, &mut visual_cx),
            "the header's block must be folded"
        );
    }

    #[gpui_kit::test]
    fn toggle_fold_twice_on_a_block_header_unfolds_it_again(cx: &mut TestAppContext) {
        let (fulgur, content, mut visual_cx) = setup_nested_folds(cx);
        place_caret(&content, 2, &mut visual_cx);

        fold(&fulgur, FoldCommand::ToggleAtCursor, &mut visual_cx);
        fold(&fulgur, FoldCommand::ToggleAtCursor, &mut visual_cx);

        assert_eq!(caret_line(&content, &mut visual_cx), 2);
        assert!(
            !reveal(&content, 4, &mut visual_cx),
            "the second toggle must unfold"
        );
    }

    #[gpui_kit::test]
    fn toggle_fold_inside_a_block_folds_the_innermost_one(cx: &mut TestAppContext) {
        let (fulgur, content, mut visual_cx) = setup_nested_folds(cx);
        place_caret(&content, 4, &mut visual_cx);

        fold(&fulgur, FoldCommand::ToggleAtCursor, &mut visual_cx);

        assert_eq!(caret_line(&content, &mut visual_cx), 3);
        assert!(
            !reveal(&content, 2, &mut visual_cx),
            "the enclosing block must stay open"
        );
        assert!(
            reveal(&content, 4, &mut visual_cx),
            "the innermost block must be folded"
        );
    }

    #[gpui_kit::test]
    fn the_fold_shortcuts_display_both_keystrokes_of_their_chord(cx: &mut TestAppContext) {
        let (_fulgur, mut visual_cx) = setup_fulgur(cx);

        let keystroke_counts = visual_cx.update(|window, cx| {
            cx.bind_keys(build_default_key_bindings());
            [
                Box::new(FoldAll) as Box<dyn Action>,
                Box::new(FoldAtLevel(MAX_FOLD_LEVEL)),
            ]
            .map(|action| {
                action_chord(action.as_ref(), KEY_CONTEXT_FULGUR, window).map(|chord| chord.len())
            })
        });

        assert_eq!(
            keystroke_counts,
            [Some(2), Some(2)],
            "a cmd-k chord must not be shown as its first keystroke alone"
        );
    }

    #[gpui_kit::test]
    fn the_tab_menu_action_folds_the_tab_it_names(cx: &mut TestAppContext) {
        let (fulgur, content, mut visual_cx) = setup_nested_folds(cx);
        let folded_tab = visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let folded_tab = this.active_tab(cx).map(Tab::id).expect("an active tab");
                this.new_tab(window, cx);
                folded_tab
            })
        });

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.on_fold_tab(&FoldTab(folded_tab, FoldCommand::FoldAll), window, cx);
            });
        });

        assert!(
            reveal(&content, 1, &mut visual_cx),
            "the background tab must fold even though another tab is active"
        );
    }
}
