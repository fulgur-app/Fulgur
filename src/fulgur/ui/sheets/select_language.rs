use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    ActiveTheme, Placement, WindowExt, h_flex, scroll::ScrollableElement, v_flex,
};

use crate::fulgur::{
    Fulgur,
    ui::languages::{SupportedLanguage, pretty_name},
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
            let language = language.clone();
            entity.update(cx, |this, cx| {
                if let Some(index) = this.active_tab_index {
                    if let Some(tab) = this.tabs.get_mut(index) {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.force_language(
                                window,
                                cx,
                                language,
                                &this.settings.editor_settings,
                            );
                        }
                    }
                }
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
    if let Some(current) = current_language {
        current == *language
    } else {
        false
    }
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
                    language.clone(),
                    is_current_language(&language, current_language),
                    cx,
                )
            })
            .collect::<Vec<_>>(),
    )
}

impl Fulgur {
    /// Render the select language sheet.
    ///
    /// ### Parameters:
    /// - `window`: The window to render the sheet in.
    /// - `cx`: The context to render the sheet in.
    pub fn render_select_language_sheet(&self, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity();
        let current_language = match self.active_tab_index {
            Some(index) => {
                if let Some(editor_tab) = self.tabs[index].as_editor() {
                    Some(editor_tab.language.clone())
                } else {
                    Some(SupportedLanguage::Plain)
                }
            }
            None => None,
        };
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
