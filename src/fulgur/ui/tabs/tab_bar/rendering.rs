use super::{
    CloseAllOtherTabs, CloseAllTabsAction, CloseTabAction, CloseTabsToLeft, CloseTabsToRight,
    DuplicateTab, SendTabToWindowNoOp, ShowInFileManager, tab_bar_button_factory,
};
use crate::fulgur::{
    Fulgur,
    tab::Tab,
    ui::tabs::editor_tab::TabLocation,
    ui::tabs::tab_drag::DraggedTab,
    ui::{components_utils, icons::CustomIcon},
    window_manager::WindowManager,
};
use gpui::{
    AnyElement, AppContext, ClickEvent, Context, Div, DragMoveEvent, InteractiveElement,
    IntoElement, MouseButton, ParentElement, SharedString, StatefulInteractiveElement, Styled,
    WeakEntity, div, px,
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

impl Fulgur {
    /// Compute a fingerprint that changes whenever the set of open file paths changes.
    ///
    /// ### Returns
    /// - `u64`: The computed fingerprint value.
    fn compute_tab_filename_fingerprint(&self) -> u64 {
        let mut hash: u64 = self.tabs.len() as u64;
        for tab in &self.tabs {
            if let Some(editor) = tab.as_editor()
                && let Some(path) = editor.file_path()
            {
                for byte in path.as_os_str().as_encoded_bytes() {
                    hash = hash
                        .wrapping_mul(0x0000_0100_0000_01b3)
                        .wrapping_add(*byte as u64);
                }
            }
            hash ^= tab.id() as u64;
            hash = hash.rotate_left(17);
        }
        hash
    }

    /// Rebuild the cached tab filename frequency map only when the tab list has changed.
    pub(crate) fn refresh_tab_filename_counts(&mut self) {
        let fp = self.compute_tab_filename_fingerprint();
        if fp == self.tab_filename_fp {
            return;
        }
        self.tab_filename_fp = fp;
        self.cached_tab_filename_counts = self.build_tab_filename_counts();
    }

    /// Resolve the optional tab badge label for remote editor tabs.
    ///
    /// ### Arguments
    /// - `tab`: The tab to inspect.
    ///
    /// ### Returns
    /// - `Some(&'static str)`: `"R"` when the tab points to a remote location.
    /// - `None`: For local, untitled, settings, and markdown preview tabs.
    pub(crate) fn remote_tab_indicator_label(&self, tab: &Tab) -> Option<&'static str> {
        let editor_tab = tab.as_editor()?;
        if matches!(editor_tab.location, TabLocation::Remote(_)) {
            Some("R")
        } else {
            None
        }
    }

    /// Build a per-filename tab count map for disambiguation.
    ///
    /// ### Returns
    /// - `HashMap<String, usize>`: Map of filename to number of open tabs with that filename
    pub(crate) fn build_tab_filename_counts(&self) -> HashMap<String, usize> {
        let mut filename_counts = HashMap::new();
        for tab in &self.tabs {
            if let Some(editor_tab) = tab.as_editor()
                && let Some(path) = editor_tab.file_path()
                && let Some(filename) = path.file_name().and_then(|n| n.to_str())
            {
                *filename_counts.entry(filename.to_string()).or_insert(0) += 1;
            }
        }
        filename_counts
    }

    /// Get the display title for a tab, including parent folder when duplicated.
    ///
    /// ### Arguments
    /// - `tab`: The tab to get the title for
    /// - `filename_counts`: Precomputed filename frequencies for all open tabs
    ///
    /// ### Returns
    /// - `(String, Option<String>)`: A tuple of (filename, optional parent folder)
    pub(crate) fn get_tab_display_title(
        &self,
        tab: &Tab,
        filename_counts: &HashMap<String, usize>,
    ) -> (String, Option<String>) {
        let base_title = tab.title();
        if let Some(editor_tab) = tab.as_editor()
            && let Some(path) = editor_tab.file_path()
        {
            let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let duplicate_count = filename_counts.get(filename).copied().unwrap_or(0);
            if duplicate_count > 1
                && let Some(parent) = path.parent()
                && let Some(parent_name) = parent.file_name().and_then(|n| n.to_str())
            {
                return (filename.to_string(), Some(format!("../{}", parent_name)));
            }

            return (filename.to_string(), None);
        }
        (base_title.to_string(), None)
    }

    /// Render the full tab bar
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Div`: The fully composed tab bar element
    pub fn render_tab_bar(&self, cx: &mut Context<Self>) -> Div {
        use crate::fulgur::ui::components_utils::TAB_BAR_HEIGHT;
        let mut tab_bar = div()
            .flex()
            .items_center()
            .h(TAB_BAR_HEIGHT)
            .bg(cx.theme().tab_bar);
        tab_bar = tab_bar
            .child(
                tab_bar_button_factory("new-tab", "New Tab", CustomIcon::Plus, cx.theme().border)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.new_tab(window, cx);
                    })),
            )
            .child(
                tab_bar_button_factory(
                    "open-file",
                    "Open File (+Shift - Open Path)",
                    CustomIcon::FolderOpen,
                    cx.theme().border,
                )
                .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                    if event.modifiers().shift {
                        this.show_open_from_path_dialog(window, cx);
                    } else {
                        this.open_file(window, cx);
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
                .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                    if event.modifiers().shift {
                        this.save_file_as(window, cx);
                    } else {
                        this.save_file(window, cx);
                    }
                })),
            )
            .child(
                div()
                    .id("tab-scroll-container")
                    .overflow_x_scroll()
                    .track_scroll(&self.tab_scroll_handle)
                    .flex()
                    .flex_1()
                    .items_center()
                    .children(self.render_tabs_with_slots(cx))
                    .child(
                        div()
                            .id("tab-bar-trailing")
                            .flex_1()
                            .min_w(px(0.))
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .h(TAB_BAR_HEIGHT)
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
                                    let slot = this.tabs.len();
                                    let dragged = event.drag(cx).clone();
                                    this.drag_ghost = Some((slot, dragged));
                                    cx.notify();
                                },
                            ))
                            .on_drop(cx.listener(|this, dragged: &DraggedTab, window, cx| {
                                if let Some((slot, _)) = this.drag_ghost.take() {
                                    this.handle_tab_drop(dragged, slot, window, cx);
                                }
                            })),
                    ),
            );
        tab_bar
    }

    /// Render a ghost tab shown at the insertion point during a drag operation.
    ///
    /// The ghost tab previews where the dragged tab will land when dropped. It uses
    /// a muted, semi-transparent style to distinguish it from real tabs.
    ///
    /// ### Arguments
    /// - `slot`: The insertion slot index
    /// - `dragged`: The dragged tab data (used for title and modified state)
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `AnyElement`: The rendered ghost tab element
    fn render_ghost_tab(
        &self,
        slot: usize,
        dragged: &DraggedTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
            .on_drop(cx.listener(|this, dragged: &DraggedTab, window, cx| {
                if let Some((slot, _)) = this.drag_ghost.take() {
                    this.handle_tab_drop(dragged, slot, window, cx);
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
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Vec<AnyElement>`: Tab elements, with a ghost tab inserted at the drag target
    fn render_tabs_with_slots(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let ghost = if cx.has_active_drag() {
            self.drag_ghost.as_ref().and_then(|(slot, dragged)| {
                let from = dragged.tab_index;
                let is_noop = *slot == from || *slot == from + 1;
                if is_noop {
                    None
                } else {
                    Some((*slot, dragged))
                }
            })
        } else {
            None
        };
        let filename_counts = &self.cached_tab_filename_counts;
        let capacity = self.tabs.len() + if ghost.is_some() { 1 } else { 0 };
        let mut elements: Vec<AnyElement> = Vec::with_capacity(capacity);
        if let Some((0, dragged)) = ghost {
            elements.push(self.render_ghost_tab(0, dragged, cx));
        }
        for (index, tab) in self.tabs.iter().enumerate() {
            elements.push(self.render_tab(index, tab, filename_counts, cx));
            if let Some((slot, dragged)) = ghost
                && slot == index + 1
            {
                elements.push(self.render_ghost_tab(slot, dragged, cx));
            }
        }
        elements
    }

    /// Render a single tab in the tab bar
    ///
    /// ### Arguments
    /// - `index`: Position of the tab in `self.tabs`
    /// - `tab`: The tab to render
    /// - `filename_counts`: Precomputed map of filename to occurrence count, used to show a
    ///   disambiguating folder segment when multiple open tabs share the same filename
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `AnyElement`: The fully composed tab element ready to be inserted into the tab bar
    pub fn render_tab(
        &self,
        index: usize,
        tab: &Tab,
        filename_counts: &HashMap<String, usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::fulgur::ui::components_utils::TAB_BAR_HEIGHT;

        let tab_id = tab.id();
        let is_active = match self.active_tab_index {
            Some(active_index) => index == active_index,
            None => false,
        };
        let has_tabs_on_left = index > 0;
        let has_tabs_on_right = index < self.tabs.len() - 1;
        let total_tabs = self.tabs.len();
        let file_path = tab.as_editor().and_then(|editor_tab| {
            editor_tab
                .file_path()
                .and_then(|path| path.to_str().map(|s| s.to_string()))
        });
        let has_file_path = file_path.is_some();
        let is_editor_tab = tab.as_editor().is_some();
        let other_windows: Vec<(String, WeakEntity<Fulgur>)> = {
            let manager = cx.global::<WindowManager>();
            let current_window_id = self.window_id;
            manager
                .get_all_window_ids()
                .into_iter()
                .filter(|id| *id != current_window_id)
                .filter_map(|id| {
                    manager
                        .get_window_name(id)
                        .map(|name| name.to_string())
                        .zip(manager.get_window(id))
                })
                .collect()
        };
        let source_entity = cx.entity().downgrade();
        let cached_file_size = tab
            .as_editor()
            .and_then(|editor_tab| editor_tab.file_size_bytes)
            .map(components_utils::format_file_size);
        let cached_last_modified = tab
            .as_editor()
            .and_then(|editor_tab| editor_tab.file_last_modified)
            .and_then(components_utils::format_system_time);
        let mut tab_div = div()
            .id(("tab", tab_id))
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
                cx.listener(move |this, _, window, cx: &mut Context<'_, Fulgur>| {
                    if !is_active {
                        this.set_active_tab(index, window, cx);
                    }
                }),
            );
        if is_active {
            tab_div = tab_div.bg(cx.theme().tab_active).border_b_0();
            self.set_title(Some(tab.title().to_string()), cx);
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
                                .child(format!("Size: {}", size))
                                .text_xs()
                                .text_color(cx.theme().muted_foreground),
                        );
                    }
                    if let Some(ref last_modified) = last_modified {
                        details = details.child(
                            div()
                                .child(format!("Last Modified: {}", last_modified))
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
        let (filename, folder) = self.get_tab_display_title(tab, filename_counts);
        let modified_indicator = if tab.is_modified() { " •" } else { "" };
        let mut title_container = div().flex().items_center().gap_1().pl_1();
        if let Some(remote_indicator) = self.remote_tab_indicator_label(tab) {
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
                .child(format!("{}{}", filename, modified_indicator)),
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
                Button::new(("close-tab", tab_id))
                    .icon(CustomIcon::Close)
                    .ghost()
                    .xsmall()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.close_tab(tab_id, window, cx);
                    })),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, _, window, cx| {
                    this.close_tab(tab_id, window, cx);
                }),
            );
        if !is_markdown_preview {
            let is_source = cx.has_active_drag()
                && self
                    .drag_ghost
                    .as_ref()
                    .map(|(_, d)| d.tab_index == index)
                    .unwrap_or(false);
            if is_source {
                tab_with_content = tab_with_content.opacity(0.45);
            }
            tab_with_content = tab_with_content
                .on_drag(
                    DraggedTab {
                        tab_index: index,
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
                .on_drop(cx.listener(|this, dragged: &DraggedTab, window, cx| {
                    if let Some((slot, _)) = this.drag_ghost.take() {
                        this.handle_tab_drop(dragged, slot, window, cx);
                    }
                }));
        }
        let tab_with_content = tab_with_content.context_menu(move |this, window, cx| {
            let other_windows_clone = other_windows.clone();
            let source_entity_clone = source_entity.clone();
            let this = this
                .menu_with_disabled(
                    crate::fulgur::ui::components_utils::reveal_in_file_manager_label(),
                    Box::new(ShowInFileManager(index)),
                    !has_file_path,
                )
                .menu_with_disabled(
                    "Duplicate Tab",
                    Box::new(DuplicateTab(index)),
                    !is_editor_tab,
                );
            let this = if is_editor_tab {
                this.submenu("Send to...", window, cx, move |sub, _window, _cx| {
                    let mut sub = sub;
                    for (name, weak_tgt) in &other_windows_clone {
                        let label = format!("Window {}", name);
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
                    Box::new(CloseTabsToLeft(index)),
                    !has_tabs_on_left,
                )
                .menu_with_disabled(
                    "Close Tabs to the Right",
                    Box::new(CloseTabsToRight(index)),
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
                    Box::new(CloseAllOtherTabs(index)),
                    total_tabs <= 1,
                )
        });
        tab_with_content.into_any_element()
    }
}
