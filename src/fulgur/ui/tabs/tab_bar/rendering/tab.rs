use crate::fulgur::ui::tabs::tab_bar::{
    CloseAllOtherTabs, CloseAllTabsAction, CloseTabAction, CloseTabsToLeft, CloseTabsToRight,
    CopyPath, DuplicateTab, RenameTab, SendTabToWindowNoOp, SetTabColor, ShowInFileManager, TabBar,
    TabBarEvent,
};
use crate::fulgur::{
    Fulgur, settings::TabColorStyle, tab::Tab, ui::icons::CustomIcon,
    ui::tabs::color_tag::ColorTag, ui::tabs::tab_drag::DraggedTab,
};
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::{
    ActiveTheme, Sizable, StyledExt,
    button::{Button, ButtonVariants},
    h_flex,
    menu::{ContextMenuExt, PopupMenuItem},
    tooltip::Tooltip,
    v_flex,
};
use gpui_kit::{
    AnyElement, AppContext, Context, DragMoveEvent, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Role, StatefulInteractiveElement, Styled, div,
};
use std::collections::HashMap;
use std::rc::Rc;

impl TabBar {
    /// Render a single tab in the tab bar
    ///
    /// ### Arguments
    /// - `fulgur`: The owning window to read tab and window state from
    /// - `index`: Position of the tab in the owning window's tab list
    /// - `tab`: The tab to render
    /// - `filename_counts`: Precomputed map of filename to occurrence count, used to show a
    ///   disambiguating folder segment when multiple open tabs share the same filename
    /// - `cx`: The tab bar context
    ///
    /// ### Returns
    /// - `AnyElement`: The fully composed tab element ready to be inserted into the tab bar
    pub(super) fn render_tab(
        &self,
        fulgur: &Fulgur,
        index: usize,
        tab: &Tab,
        filename_counts: &HashMap<String, usize>,
        cx: &Context<Self>,
    ) -> AnyElement {
        use crate::fulgur::ui::components_utils::TAB_BAR_HEIGHT;

        let tab_id = tab.id();
        let is_active = fulgur.active_tab_id == Some(tab_id);
        let has_tabs_on_left = index > 0;
        let has_tabs_on_right = index < fulgur.tabs.len() - 1;
        let total_tabs = fulgur.tabs.len();
        let has_file_path = tab
            .as_editor()
            .and_then(|editor_tab| editor_tab.file_path())
            .is_some_and(|path| path.to_str().is_some());
        let is_editor_tab = tab.as_editor().is_some();
        let is_renameable = tab
            .as_editor()
            .is_some_and(crate::fulgur::editor_tab::EditorTab::is_renameable);
        let current_window_id = fulgur.window_id;
        let source_entity = self.fulgur.clone();
        // A markdown preview tab inherits the color tag of its source editor tab.
        let color_tag = if let Some(editor_tab) = tab.as_editor() {
            editor_tab.color_tag
        } else if let Some(preview) = tab.as_markdown_preview() {
            let source_id = preview.source_tab_id;
            fulgur
                .tabs
                .iter()
                .find_map(|other| match other.read(cx) {
                    Tab::Editor(editor_tab) if editor_tab.id == source_id => {
                        Some(editor_tab.color_tag)
                    }
                    _ => None,
                })
                .flatten()
        } else {
            None
        };
        let mut tab_div = div()
            .id(("tab", tab_id.0))
            .role(Role::Tab)
            .aria_selected(is_active)
            .aria_label(tab.title())
            .test_support()
            .flex()
            .items_center()
            .h(TAB_BAR_HEIGHT)
            .px_2()
            .gap_2()
            .border_r_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _, _window, cx| {
                    if !is_active {
                        cx.emit(TabBarEvent::Activate(tab_id));
                    }
                }),
            );
        if is_active {
            tab_div = tab_div.bg(cx.theme().tab_active).border_b_0();
        } else {
            tab_div = tab_div
                .bg(cx.theme().tab)
                .hover(|this| this.bg(cx.theme().muted))
                .cursor_pointer();
        }
        if Self::has_tab_tooltip(tab) {
            let owner = self.fulgur.clone();
            tab_div =
                tab_div.tooltip(move |window, cx| {
                    let content = Rc::new(
                        Self::resolve_tab_tooltip_content(&owner, tab_id, cx).unwrap_or_default(),
                    );
                    Tooltip::element(move |_, cx| {
                        let details = h_flex().gap_4().justify_between().children(
                            content.details.iter().map(|detail| {
                                div()
                                    .child(detail.clone())
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                            }),
                        );
                        v_flex()
                            .gap_1()
                            .py_2()
                            .px_1()
                            .child(
                                h_flex()
                                    .gap_3()
                                    .child(CustomIcon::File.icon())
                                    .child(content.header.clone())
                                    .text_sm()
                                    .font_semibold(),
                            )
                            .child(details)
                    })
                    .build(window, cx)
                });
        }
        let (filename, folder) = Self::get_tab_display_title(tab, filename_counts);
        let modified_indicator = if tab.is_modified() { " •" } else { "" };
        let color_style = fulgur.settings.app_settings.tab_color_style;
        let theme_title_color = if is_active {
            cx.theme().tab_active_foreground
        } else {
            cx.theme().tab_foreground
        };
        let title_color = match (color_tag, color_style) {
            (Some(color), TabColorStyle::TextColor) => color.to_hsla(cx),
            _ => theme_title_color,
        };
        let mut title_container = div().flex().items_center().gap_1().pl_1();
        if let (Some(color), TabColorStyle::Dot) = (color_tag, color_style) {
            title_container =
                title_container.child(div().size_2().rounded_full().mr_1().bg(color.to_hsla(cx)));
        }
        if let Some(remote_indicator) = Self::remote_tab_indicator_label(tab) {
            title_container = title_container.child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(if is_active {
                        cx.theme().tab_active_foreground.opacity(0.50)
                    } else {
                        cx.theme().primary.opacity(0.80)
                    })
                    .child(remote_indicator),
            );
        }
        title_container = title_container.child(
            div()
                .text_sm()
                .text_color(title_color)
                .child(format!("{filename}{modified_indicator}")),
        );
        if let Some(folder_path) = folder {
            title_container = title_container.child(
                div()
                    .text_xs()
                    .italic()
                    .text_color(if is_active {
                        cx.theme().tab_active_foreground
                    } else {
                        cx.theme().tab_foreground
                    })
                    .child(folder_path),
            );
        }
        let is_markdown_preview = tab.as_markdown_preview().is_some();
        let title = tab.title();
        let is_modified = tab.is_modified();
        let mut tab_with_content = tab_div
            .child(title_container)
            .child(
                Button::new(("close-tab", tab_id.0))
                    .icon(CustomIcon::Close)
                    .accessibility_label(format!("Close {title}"))
                    .ghost()
                    .xsmall()
                    .cursor_pointer()
                    .on_click(cx.listener(move |_, _, _window, cx| {
                        cx.stop_propagation();
                        cx.emit(TabBarEvent::Close(tab_id));
                    })),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |_, _, _window, cx| {
                    cx.emit(TabBarEvent::Close(tab_id));
                }),
            );
        if !is_markdown_preview {
            let is_source = cx.has_active_drag()
                && self
                    .drag_ghost
                    .as_ref()
                    .is_some_and(|(_, d)| d.tab_id == tab_id);
            if is_source {
                tab_with_content = tab_with_content.opacity(0.45);
            }
            tab_with_content = tab_with_content
                .on_drag(
                    DraggedTab {
                        tab_id,
                        title,
                        is_modified,
                    },
                    |dragged, _, _, cx| cx.new(|_| dragged.clone()),
                )
                .on_drag_move::<DraggedTab>(cx.listener(
                    move |this, event: &DragMoveEvent<DraggedTab>, _window, cx| {
                        let cursor = event.event.position;
                        let bounds = event.bounds;
                        if cursor.x < bounds.origin.x
                            || cursor.x > bounds.origin.x + bounds.size.width
                            || cursor.y < bounds.origin.y
                            || cursor.y > bounds.origin.y + bounds.size.height
                        {
                            return;
                        }
                        let slot = if cursor.x < bounds.origin.x + bounds.size.width * 0.5 {
                            index
                        } else {
                            index + 1
                        };
                        let dragged = event.drag(cx).clone();
                        this.drag_ghost = Some((slot, dragged));
                        cx.notify();
                    },
                ))
                .on_drop(cx.listener(|this, dragged: &DraggedTab, _window, cx| {
                    if let Some((slot, _)) = this.drag_ghost.take() {
                        cx.emit(TabBarEvent::Drop {
                            dragged: dragged.clone(),
                            slot,
                        });
                    }
                }));
        }
        let tab_with_content = tab_with_content.context_menu(move |this, window, cx| {
            let source_entity_clone = source_entity.clone();
            let this = this
                .menu_with_disabled(
                    crate::fulgur::ui::components_utils::reveal_in_file_manager_label(),
                    Box::new(ShowInFileManager(tab_id)),
                    !has_file_path,
                )
                .menu_with_disabled("Copy path", Box::new(CopyPath(tab_id)), !has_file_path)
                .menu_with_disabled(
                    "Duplicate Tab",
                    Box::new(DuplicateTab(tab_id)),
                    !is_editor_tab,
                )
                .menu_with_disabled("Rename Tab...", Box::new(RenameTab(tab_id)), !is_renameable);
            let this = if is_editor_tab {
                this.submenu("Send to...", window, cx, move |sub, _window, cx| {
                    let other_windows = Self::other_windows(current_window_id, cx);
                    let mut sub = sub;
                    for (name, weak_tgt) in &other_windows {
                        let label = format!("Window {name}");
                        let src = source_entity_clone.clone();
                        let tgt = weak_tgt.clone();
                        sub = sub.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                            let Some(src_entity) = src.upgrade() else {
                                return;
                            };
                            let Some(tgt_entity) = tgt.upgrade() else {
                                return;
                            };
                            let data = src_entity.update(cx, |fulgur, cx| {
                                fulgur.extract_tab_transfer_data(tab_id, cx)
                            });
                            let Some(data) = data else { return };
                            tgt_entity.update(cx, |fulgur, cx| {
                                fulgur.pending_tab_transfer = Some(data);
                                cx.notify();
                            });
                            src_entity.update(cx, |fulgur, cx| {
                                fulgur.pending_tab_removal = Some(tab_id);
                                cx.notify();
                            });
                        }));
                    }
                    if !other_windows.is_empty() {
                        sub = sub.separator();
                    }
                    let src = source_entity_clone.clone();
                    sub = sub.item(PopupMenuItem::new("New Window").on_click(move |_, _, cx| {
                        let Some(src_entity) = src.upgrade() else {
                            return;
                        };
                        src_entity.update(cx, |fulgur, cx| {
                            fulgur.move_tab_to_new_window(tab_id, cx);
                        });
                    }));
                    sub
                })
            } else {
                this.menu_with_disabled("Send to...", Box::new(SendTabToWindowNoOp), true)
            };
            let this = if is_editor_tab {
                let current = color_tag;
                this.submenu("Color Tag", window, cx, move |sub, _window, _cx| {
                    let mut sub = sub;
                    for color in ColorTag::all() {
                        let checked = current == Some(color);
                        sub = sub.menu_element_with_check(
                            checked,
                            Box::new(SetTabColor(tab_id, Some(color))),
                            move |_window, cx| {
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(div().size_3().rounded_full().bg(color.to_hsla(cx)))
                                    .child(color.label())
                            },
                        );
                    }
                    sub.separator().menu_with_check(
                        "None",
                        current.is_none(),
                        Box::new(SetTabColor(tab_id, None)),
                    )
                })
            } else {
                this.menu_with_disabled("Color Tag", Box::new(SetTabColor(tab_id, None)), true)
            };
            this.separator()
                .menu("Close Tab", Box::new(CloseTabAction(tab_id)))
                .menu_with_disabled(
                    "Close Tabs to the Left",
                    Box::new(CloseTabsToLeft(tab_id)),
                    !has_tabs_on_left,
                )
                .menu_with_disabled(
                    "Close Tabs to the Right",
                    Box::new(CloseTabsToRight(tab_id)),
                    !has_tabs_on_right,
                )
                .separator()
                .menu_with_disabled(
                    "Close All Tabs",
                    Box::new(CloseAllTabsAction),
                    total_tabs == 0,
                )
                .menu_with_disabled(
                    "Close All Other Tabs",
                    Box::new(CloseAllOtherTabs(tab_id)),
                    total_tabs <= 1,
                )
        });
        tab_with_content.into_any_element()
    }
}
