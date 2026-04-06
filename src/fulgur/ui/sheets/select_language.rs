use gpui::prelude::FluentBuilder;
use gpui::{
    App, Context, Div, Element, Entity, InteractiveElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, Placement, WindowExt, h_flex, scroll::ScrollableElement, v_flex,
};

use crate::fulgur::{
    Fulgur,
    languages::supported_languages::{SupportedLanguage, pretty_name},
};

/// Create a select language item
///
/// ### Parameters:
/// - `entity`: The Fulgur entity handle.
/// - `language`: The language to create the item for.
/// - `is_current_language`: Whether the language is the current language.
/// - `cx`: The application context.
///
/// ### Returns:
/// `Div`: Represents a select language item.
fn make_select_language_item(
    entity: Entity<Fulgur>,
    language: SupportedLanguage,
    is_current_language: bool,
    cx: &App,
) -> impl Element {
    let pretty_name = pretty_name(&language);
    let id = format!("Select_{}", pretty_name.clone().replace(" ", "_"));
    let id = SharedString::from(id);
    h_flex()
        .id(id)
        .justify_between()
        .my_2()
        .cursor_pointer()
        .border_1()
        .border_color(cx.theme().border)
        .child(div().p_2().text_sm().child(pretty_name))
        .when(is_current_language, |this| this.bg(cx.theme().muted))
        .hover(|this| this.bg(cx.theme().muted))
        .on_click(move |_event, window, cx| {
            entity.update(cx, |this, cx| {
                this.switch_active_tab_language(window, cx, language);
                window.close_sheet(cx);
            });
        })
}

/// Check if the given language is the current language.
///
/// ### Parameters:
/// - `language`: The language to check.
/// - `current_language`: The current language.
///
/// ### Returns:
/// `true` if the given language is the current language, otherwise `false`.
fn is_current_language(
    language: &SupportedLanguage,
    current_language: Option<SupportedLanguage>,
) -> bool {
    current_language == Some(*language)
}

/// Create a select language list.
///
/// ### Parameters:
/// - `entity`: The Fulgur entity handle.
/// - `current_language`: The current language.
/// - `cx`: The application context.
///
/// ### Returns:
/// `Div`: Represents a select language list.
fn make_select_language_list(
    entity: Entity<Fulgur>,
    current_language: Option<SupportedLanguage>,
    cx: &App,
) -> Div {
    div().gap_2().children(
        SupportedLanguage::all()
            .into_iter()
            .map(move |language| {
                make_select_language_item(
                    entity.clone(),
                    language,
                    is_current_language(&language, current_language),
                    cx,
                )
            })
            .collect::<Vec<_>>(),
    )
}

impl Fulgur {
    /// Get the language that should be highlighted as current in the select-language sheet.
    ///
    /// ### Returns:
    /// - `Some(SupportedLanguage)`: Active editor tab language, or `Plain` for non-editor tabs.
    /// - `None`: If there is no active tab.
    fn current_sheet_language(&self) -> Option<SupportedLanguage> {
        match self.active_tab_index {
            Some(index) => {
                if let Some(editor_tab) = self.tabs[index].as_editor() {
                    Some(editor_tab.language)
                } else {
                    Some(SupportedLanguage::Plain)
                }
            }
            None => None,
        }
    }

    /// Force the active editor tab language from the select-language sheet.
    ///
    /// ### Parameters:
    /// - `window`: The window context.
    /// - `cx`: The application context.
    /// - `language`: The language to apply to the active editor tab.
    fn switch_active_tab_language(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        language: SupportedLanguage,
    ) {
        if let Some(index) = self.active_tab_index
            && let Some(tab) = self.tabs.get_mut(index)
            && let Some(editor_tab) = tab.as_editor_mut()
        {
            editor_tab.force_language(window, cx, language, &self.settings.editor_settings);
        }
    }

    /// Render the select language sheet.
    ///
    /// ### Parameters:
    /// - `window`: The window to render the sheet in.
    /// - `cx`: The context to render the sheet in.
    pub fn render_select_language_sheet(&self, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity();
        let current_language = self.current_sheet_language();
        let viewport_height = window.viewport_size().height;
        let max_height = px((viewport_height - px(100.0)).into()); //TODO: Make this dynamic based on the content
        window.open_sheet_at(Placement::Left, cx, move |sheet, _window, cx| {
            sheet
                .title("Select Language")
                .size(px(400.))
                .overlay(true)
                .child(v_flex().overflow_y_scrollbar().gap_2().h(max_height).child(
                    make_select_language_list(entity.clone(), current_language, cx),
                ))
        });
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "gpui-test-support")]
    use super::Fulgur;
    use super::is_current_language;
    use crate::fulgur::languages::supported_languages::SupportedLanguage;
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::{
        settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
    };
    use core::prelude::v1::test;
    #[cfg(feature = "gpui-test-support")]
    use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
    #[cfg(feature = "gpui-test-support")]
    use parking_lot::Mutex;
    #[cfg(feature = "gpui-test-support")]
    use std::{cell::RefCell, rc::Rc, sync::Arc};

    #[test]
    fn test_is_current_language_matches_expected_language() {
        assert!(is_current_language(
            &SupportedLanguage::Rust,
            Some(SupportedLanguage::Rust)
        ));
        assert!(!is_current_language(
            &SupportedLanguage::Rust,
            Some(SupportedLanguage::Python)
        ));
        assert!(!is_current_language(&SupportedLanguage::Rust, None));
    }

    #[cfg(feature = "gpui-test-support")]
    fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
        cx.update(gpui_component::init);
        cx.update(|cx| {
            cx.set_global(SharedAppState::new(
                Settings::new(),
                Arc::new(Mutex::new(Vec::new())),
            ));
            cx.set_global(WindowManager::new());
        });

        let fulgur_slot: Rc<RefCell<Option<Entity<Fulgur>>>> = Rc::new(RefCell::new(None));
        let slot = Rc::clone(&fulgur_slot);
        let window = cx
            .update(|cx| {
                cx.open_window(Default::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                    *slot.borrow_mut() = Some(fulgur.clone());
                    cx.new(|cx| gpui_component::Root::new(fulgur, window, cx))
                })
            })
            .expect("failed to open test window");
        let fulgur = fulgur_slot
            .borrow_mut()
            .take()
            .expect("expected fulgur entity");
        let visual_cx = VisualTestContext::from_window(window.into(), cx);
        (fulgur, visual_cx)
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_current_sheet_language_reflects_active_editor_language(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        let initial_language =
            fulgur.read_with(&visual_cx, |this, _| this.current_sheet_language());
        assert_eq!(initial_language, Some(SupportedLanguage::Plain));

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.switch_active_tab_language(window, cx, SupportedLanguage::Rust);
            });
        });

        let switched_language =
            fulgur.read_with(&visual_cx, |this, _| this.current_sheet_language());
        assert_eq!(switched_language, Some(SupportedLanguage::Rust));
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_switch_active_tab_language_is_noop_without_active_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.active_tab_index = None;
                this.switch_active_tab_language(window, cx, SupportedLanguage::Rust);
            });
        });

        let current_language =
            fulgur.read_with(&visual_cx, |this, _| this.current_sheet_language());
        assert_eq!(current_language, None);
    }
}
