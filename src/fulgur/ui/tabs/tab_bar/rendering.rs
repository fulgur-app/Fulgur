use super::{
    CloseAllOtherTabs, CloseAllTabsAction, CloseTabAction, CloseTabsToLeft, CloseTabsToRight,
    CopyPath, DuplicateTab, SendTabToWindowNoOp, ShowInFileManager, TabBar, TabBarEvent,
    tab_bar_button_factory,
};
use crate::fulgur::{
    Fulgur,
    tab::Tab,
    ui::tabs::tab_drag::DraggedTab,
    ui::{components_utils, icons::CustomIcon},
    window_manager::WindowManager,
};
use gpui::{
    AnyElement, AppContext, ClickEvent, Context, DragMoveEvent, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    WeakEntity, Window, div, px,
};
use gpui_component::{
    ActiveTheme, Sizable, StyledExt,
    button::{Button, ButtonVariants},
    h_flex,
    menu::{ContextMenuExt, PopupMenuItem},
    tooltip::Tooltip,
    v_flex,
};
use std::collections::HashMap;

impl Render for TabBar {
    /// Render the tab bar from the owning window's current tab list
    ///
    /// ### Arguments
    /// - `_window`: The window to render the tab bar in
    /// - `cx`: The tab bar context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered tab bar element
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::fulgur::ui::components_utils::TAB_BAR_HEIGHT;

        let Some(fulgur_entity) = self.fulgur.upgrade() else {
            return div().into_any_element();
        };
        self.process_pending_scroll(&fulgur_entity, cx);
        let fulgur = fulgur_entity.read(cx);
        div()
            .flex()
            .items_center()
            .h(TAB_BAR_HEIGHT)
            .bg(cx.theme().tab_bar)
            .child(
                tab_bar_button_factory("new-tab", "New Tab", CustomIcon::Plus, cx.theme().border)
                    .on_click(cx.listener(|_, _, _window, cx| {
                        cx.emit(TabBarEvent::NewTab);
                    })),
            )
            .child(
                tab_bar_button_factory(
                    "open-file",
                    "Open File (+Shift - Open Path)",
                    CustomIcon::FolderOpen,
                    cx.theme().border,
                )
                .on_click(cx.listener(|_, event: &ClickEvent, _window, cx| {
                    if event.modifiers().shift {
                        cx.emit(TabBarEvent::OpenPath);
                    } else {
                        cx.emit(TabBarEvent::OpenFile);
                    }
                })),
            )
            .child(
                tab_bar_button_factory(
                    "save-file",
                    "Save File (+Shift - Save As)",
                    CustomIcon::Save,
                    cx.theme().border,
                )
                .border_r_1()
                .on_click(cx.listener(|_, event: &ClickEvent, _window, cx| {
                    if event.modifiers().shift {
                        cx.emit(TabBarEvent::SaveFileAs);
                    } else {
                        cx.emit(TabBarEvent::SaveFile);
                    }
                })),
            )
            .child(
                div()
                    .id("tab-scroll-container")
                    .overflow_x_scroll()
                    .track_scroll(&self.scroll_handle)
                    .flex()
                    .flex_1()
                    .items_center()
                    .children(self.render_tabs_with_slots(fulgur, cx))
                    .child(
                        div()
                            .id("tab-bar-trailing")
                            .flex_1()
                            .min_w(px(0.))
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .h(TAB_BAR_HEIGHT)
                            .on_click(cx.listener(|_, event: &ClickEvent, _window, cx| {
                                if event.click_count() == 2 {
                                    cx.emit(TabBarEvent::NewTab);
                                }
                            }))
                            .on_drag_move::<DraggedTab>(cx.listener(
                                |this, event: &DragMoveEvent<DraggedTab>, _window, cx| {
                                    let cursor = event.event.position;
                                    let bounds = event.bounds;
                                    if cursor.x < bounds.origin.x
                                        || cursor.x > bounds.origin.x + bounds.size.width
                                        || cursor.y < bounds.origin.y
                                        || cursor.y > bounds.origin.y + bounds.size.height
                                    {
                                        return;
                                    }
                                    let Some(fulgur) = this.fulgur.upgrade() else {
                                        return;
                                    };
                                    let slot = fulgur.read(cx).tabs.len();
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
                            })),
                    ),
            )
            .into_any_element()
    }
}

impl TabBar {
    /// Render a ghost tab shown at the insertion point during a drag operation.
    ///
    /// The ghost tab previews where the dragged tab will land when dropped. It uses
    /// a muted, semi-transparent style to distinguish it from real tabs.
    ///
    /// ### Arguments
    /// - `slot`: The insertion slot index
    /// - `dragged`: The dragged tab data (used for title and modified state)
    /// - `cx`: The tab bar context
    ///
    /// ### Returns
    /// - `AnyElement`: The rendered ghost tab element
    fn render_ghost_tab(slot: usize, dragged: &DraggedTab, cx: &Context<Self>) -> AnyElement {
        use crate::fulgur::ui::components_utils::TAB_BAR_HEIGHT;

        let modified_indicator = if dragged.is_modified { " •" } else { "" };
        div()
            .id(("ghost-tab", slot))
            .flex()
            .items_center()
            .h(TAB_BAR_HEIGHT)
            .px_2()
            .gap_2()
            .border_r_1()
            .border_b_0()
            .border_color(cx.theme().border)
            .bg(cx.theme().tab_active)
            .opacity(0.45)
            .child(
                div()
                    .pl_1()
                    .text_sm()
                    .text_color(cx.theme().tab_active_foreground)
                    .child(format!("{}{}", dragged.title, modified_indicator)),
            )
            .on_drop(cx.listener(|this, dragged: &DraggedTab, _window, cx| {
                if let Some((slot, _)) = this.drag_ghost.take() {
                    cx.emit(TabBarEvent::Drop {
                        dragged: dragged.clone(),
                        slot,
                    });
                }
            }))
            .into_any_element()
    }

    /// Render all tabs, inserting a ghost tab at the current drag insertion point.
    ///
    /// During a drag operation, a ghost tab is rendered at the slot determined by
    /// the most recent `on_drag_move` event. The ghost is suppressed for no-op
    /// positions (where the tab would not actually move).
    ///
    /// ### Arguments
    /// - `fulgur`: The owning window to read the tab list from
    /// - `cx`: The tab bar context
    ///
    /// ### Returns
    /// - `Vec<AnyElement>`: Tab elements, with a ghost tab inserted at the drag target
    fn render_tabs_with_slots(&self, fulgur: &Fulgur, cx: &Context<Self>) -> Vec<AnyElement> {
        let ghost = if cx.has_active_drag() {
            self.drag_ghost.as_ref().and_then(|(slot, dragged)| {
                let is_noop = fulgur
                    .tab_index_of(dragged.tab_id, cx)
                    .is_some_and(|from| *slot == from || *slot == from + 1);
                if is_noop {
                    None
                } else {
                    Some((*slot, dragged))
                }
            })
        } else {
            None
        };
        let filename_counts = Self::build_tab_filename_counts(&fulgur.tabs, cx);
        let capacity = fulgur.tabs.len() + usize::from(ghost.is_some());
        let mut elements: Vec<AnyElement> = Vec::with_capacity(capacity);
        if let Some((0, dragged)) = ghost {
            elements.push(Self::render_ghost_tab(0, dragged, cx));
        }
        for (index, tab) in fulgur.tabs.iter().enumerate() {
            elements.push(self.render_tab(fulgur, index, tab.read(cx), &filename_counts, cx));
            if let Some((slot, dragged)) = ghost
                && slot == index + 1
            {
                elements.push(Self::render_ghost_tab(slot, dragged, cx));
            }
        }
        elements
    }

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
    fn render_tab(
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
        let file_path = tab.as_editor().and_then(|editor_tab| {
            editor_tab
                .file_path()
                .and_then(|path| path.to_str().map(std::string::ToString::to_string))
        });
        let has_file_path = file_path.is_some();
        let is_editor_tab = tab.as_editor().is_some();
        let other_windows: Vec<(String, WeakEntity<Fulgur>)> = {
            let manager = cx.global::<WindowManager>();
            let current_window_id = fulgur.window_id;
            manager
                .get_all_window_ids()
                .into_iter()
                .filter(|id| *id != current_window_id)
                .filter_map(|id| {
                    manager
                        .get_window_name(id)
                        .map(std::string::ToString::to_string)
                        .zip(manager.get_window(id))
                })
                .collect()
        };
        let source_entity = self.fulgur.clone();
        let cached_file_size = tab
            .as_editor()
            .and_then(|editor_tab| editor_tab.file_size_bytes)
            .map(components_utils::format_file_size);
        let cached_last_modified = tab
            .as_editor()
            .and_then(|editor_tab| editor_tab.file_last_modified)
            .and_then(components_utils::format_system_time);
        let mut tab_div = div()
            .id(("tab", tab_id.0))
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
        if let Some(path) = file_path {
            tab_div = tab_div.tooltip(move |window, cx| {
                let path_clone = path.clone();
                let file_size = cached_file_size.clone();
                let last_modified = cached_last_modified.clone();
                Tooltip::element(move |_, cx| {
                    let mut tooltip = v_flex().gap_1().py_2().px_1().child(
                        h_flex()
                            .gap_3()
                            .child(CustomIcon::File.icon())
                            .child(path_clone.clone())
                            .text_sm()
                            .font_semibold(),
                    );
                    let mut details = h_flex().gap_4().justify_between();
                    if let Some(ref size) = file_size {
                        details = details.child(
                            div()
                                .child(format!("Size: {size}"))
                                .text_xs()
                                .text_color(cx.theme().muted_foreground),
                        );
                    }
                    if let Some(ref last_modified) = last_modified {
                        details = details.child(
                            div()
                                .child(format!("Last Modified: {last_modified}"))
                                .text_xs()
                                .text_color(cx.theme().muted_foreground),
                        );
                    }
                    tooltip = tooltip.child(details);
                    tooltip
                })
                .build(window, cx)
            });
        }
        let (filename, folder) = Self::get_tab_display_title(tab, filename_counts);
        let modified_indicator = if tab.is_modified() { " •" } else { "" };
        let mut title_container = div().flex().items_center().gap_1().pl_1();
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
                .text_color(if is_active {
                    cx.theme().tab_active_foreground
                } else {
                    cx.theme().tab_foreground
                })
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
        let title: SharedString = tab.title().to_string().into();
        let is_modified = tab.is_modified();
        let mut tab_with_content = tab_div
            .child(title_container)
            .child(
                Button::new(("close-tab", tab_id.0))
                    .icon(CustomIcon::Close)
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
            let other_windows_clone = other_windows.clone();
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
                );
            let this = if is_editor_tab {
                this.submenu("Send to...", window, cx, move |sub, _window, _cx| {
                    let mut sub = sub;
                    for (name, weak_tgt) in &other_windows_clone {
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
                    if !other_windows_clone.is_empty() {
                        sub = sub.separator();
                    }
                    let src = source_entity_clone.clone();
                    sub = sub.item(PopupMenuItem::new("New Window").on_click(move |_, _, cx| {
                        let Some(src_entity) = src.upgrade() else {
                            return;
                        };
                        let data = src_entity.update(cx, |fulgur, cx| {
                            fulgur.extract_tab_transfer_data(tab_id, cx)
                        });
                        let Some(data) = data else { return };
                        src_entity.update(cx, |fulgur, cx| {
                            fulgur.pending_tab_removal = Some(tab_id);
                            cx.notify();
                        });
                        src_entity.update(cx, |fulgur, cx| {
                            fulgur.open_new_window_with_tab(data, cx);
                        });
                    }));
                    sub
                })
            } else {
                this.menu_with_disabled("Send to...", Box::new(SendTabToWindowNoOp), true)
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
