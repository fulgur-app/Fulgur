use gpui::{Context, Focusable, ParentElement, Styled, Window, div, px};
use gpui_component::{WindowExt, button::ButtonVariant, dialog::DialogButtonProps, input::Input};

use crate::fulgur::{
    Fulgur, tab::TabId, ui::components_utils::UNTITLED, ui::tabs::editor_tab::EditorTab,
};

impl Fulgur {
    /// Show the rename tab dialog for a renameable tab
    ///
    /// ### Arguments
    /// - `tab_id`: The identifier of the tab to rename
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    pub fn show_rename_tab_dialog(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(current_title) = self.tab_entity_of(tab_id, cx).and_then(|tab| {
            tab.read(cx)
                .as_editor()
                .filter(|editor_tab| editor_tab.is_renameable())
                .map(|editor_tab| editor_tab.title.to_string())
        }) else {
            return;
        };
        let initial_value = if current_title.starts_with(UNTITLED) {
            String::new()
        } else {
            current_title
        };
        self.rename_tab_input.update(cx, |input_state, cx| {
            input_state.set_value(initial_value, window, cx);
            cx.notify();
        });
        let rename_tab_input = self.rename_tab_input.clone();
        let entity = cx.entity().clone();
        window.open_alert_dialog(cx, move |modal, window, cx| {
            let focus_handle = rename_tab_input.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
            let rename_tab_input_clone = rename_tab_input.clone();
            let entity_for_ok = entity.clone();
            modal
                .title(div().text_size(px(16.)).child("Rename tab..."))
                .keyboard(true)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text("Cancel")
                        .cancel_variant(ButtonVariant::Secondary)
                        .ok_text("Rename")
                        .ok_variant(ButtonVariant::Primary),
                )
                .close_button(false)
                .child(Input::new(&rename_tab_input))
                .on_ok(move |_, window, cx| {
                    let name = rename_tab_input_clone.read(cx).value().to_string();
                    entity_for_ok.update(cx, |this, cx| this.rename_tab(tab_id, &name, window, cx))
                })
        });
    }

    /// Whether a tab can be renamed by the user
    ///
    /// ### Arguments
    /// - `tab_id`: The identifier of the tab to inspect
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `bool`: `true` when the tab is an editor tab with no associated file
    pub fn is_tab_renameable(&self, tab_id: TabId, cx: &gpui::App) -> bool {
        self.tab_entity_of(tab_id, cx).is_some_and(|tab| {
            tab.read(cx)
                .as_editor()
                .is_some_and(EditorTab::is_renameable)
        })
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use crate::fulgur::{
        Fulgur,
        editor_tab::{EditorTab, TabLocation},
        languages::supported_languages::SupportedLanguage,
        tab::TabId,
    };
    use gpui::{Entity, TestAppContext, VisualTestContext};

    use crate::fulgur::test_support::setup_fulgur_with_root as setup_fulgur;
    /// Read the id and title of the first tab
    fn first_tab_title(fulgur: &Entity<Fulgur>, cx: &mut VisualTestContext) -> String {
        fulgur.read_with(cx, |this, cx| {
            this.tabs
                .first()
                .expect("expected at least one tab")
                .read(cx)
                .title()
                .to_string()
        })
    }

    fn first_tab_id(fulgur: &Entity<Fulgur>, cx: &mut VisualTestContext) -> TabId {
        fulgur.read_with(cx, |this, cx| {
            this.tabs
                .first()
                .expect("expected at least one tab")
                .read(cx)
                .id()
        })
    }

    #[gpui::test]
    fn test_rename_tab_sets_title(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let tab_id = first_tab_id(&fulgur, &mut visual_cx);
        let renamed = visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.rename_tab(tab_id, "  Meeting notes  ", window, cx)
            })
        });
        assert!(renamed, "an untitled tab should be renameable");
        assert_eq!(first_tab_title(&fulgur, &mut visual_cx), "Meeting notes");
    }

    #[gpui::test]
    fn test_rename_tab_rejects_blank_name(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let tab_id = first_tab_id(&fulgur, &mut visual_cx);
        let before = first_tab_title(&fulgur, &mut visual_cx);
        let renamed = visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| this.rename_tab(tab_id, "   ", window, cx))
        });
        assert!(!renamed, "a blank name must be rejected");
        assert_eq!(first_tab_title(&fulgur, &mut visual_cx), before);
    }

    #[gpui::test]
    fn test_rename_tab_truncates_long_name(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let tab_id = first_tab_id(&fulgur, &mut visual_cx);
        let long_name = "a".repeat(500);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.rename_tab(tab_id, &long_name, window, cx)
            })
        });
        assert_eq!(
            first_tab_title(&fulgur, &mut visual_cx).chars().count(),
            crate::fulgur::ui::components_utils::MAX_TAB_NAME_LENGTH
        );
    }

    #[gpui::test]
    fn test_rename_tab_applies_language_from_extension(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let tab_id = first_tab_id(&fulgur, &mut visual_cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.rename_tab(tab_id, "notes.md", window, cx)
            })
        });
        let language = fulgur.read_with(&visual_cx, |this, cx| {
            this.tabs
                .first()
                .and_then(|tab| tab.read(cx).as_editor())
                .map(|editor_tab| editor_tab.language)
        });
        assert_eq!(language, Some(SupportedLanguage::Markdown));
    }

    #[gpui::test]
    fn test_rename_tab_refuses_file_backed_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let tab_id = first_tab_id(&fulgur, &mut visual_cx);
        visual_cx.update(|_, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, _cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local("/tmp/notes.txt".into());
                        }
                    });
            });
        });
        let renamed = visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.rename_tab(tab_id, "Something else", window, cx)
            })
        });
        assert!(!renamed, "a file-backed tab must not be renameable");
        let is_renameable =
            fulgur.read_with(&visual_cx, |this, cx| this.is_tab_renameable(tab_id, cx));
        assert!(!is_renameable);
    }

    #[gpui::test]
    fn test_renamed_tab_is_suggested_as_filename(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let tab_id = first_tab_id(&fulgur, &mut visual_cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.rename_tab(tab_id, "Shopping list", window, cx)
            })
        });
        let suggested = fulgur.read_with(&visual_cx, |this, cx| {
            this.tabs
                .first()
                .and_then(|tab| tab.read(cx).as_editor())
                .and_then(EditorTab::get_suggested_filename)
        });
        assert_eq!(suggested, Some("Shopping list".to_string()));
    }

    #[gpui::test]
    fn test_renamed_empty_tab_is_persisted(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let tab_id = first_tab_id(&fulgur, &mut visual_cx);

        // A default-named empty scratch tab is not worth persisting.
        let default_tabs = fulgur.read_with(&visual_cx, |this, cx| {
            this.build_window_state_without_bounds(cx).tabs.len()
        });
        assert_eq!(default_tabs, 0);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.rename_tab(tab_id, "Groceries", window, cx)
            })
        });
        let titles: Vec<String> = fulgur.read_with(&visual_cx, |this, cx| {
            this.build_window_state_without_bounds(cx)
                .tabs
                .into_iter()
                .map(|tab_state| tab_state.title)
                .collect()
        });
        assert_eq!(titles, vec!["Groceries".to_string()]);
    }
}
