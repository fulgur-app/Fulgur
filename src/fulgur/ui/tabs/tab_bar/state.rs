use crate::fulgur::{
    Fulgur,
    tab::{Tab, TabId},
    ui::tabs::editor_tab::TabLocation,
    ui::tabs::tab_drag::DraggedTab,
};
use gpui::{Context, Entity, EventEmitter, ScrollHandle, WeakEntity, Window};
use std::collections::HashMap;

/// The tab bar at the top of the window, rendered as its own entity
pub(crate) struct TabBar {
    pub(super) fulgur: WeakEntity<Fulgur>,
    pub(super) scroll_handle: ScrollHandle,
    pub(super) drag_ghost: Option<(usize, DraggedTab)>,
    pub(super) pending_scroll: Option<TabId>,
}

/// Typed events emitted by the tab bar toward the owning `Fulgur` window
#[derive(Clone)]
pub(crate) enum TabBarEvent {
    NewTab,
    OpenFile,
    OpenPath,
    SaveFile,
    SaveFileAs,
    Activate(TabId),
    Close(TabId),
    Drop { dragged: DraggedTab, slot: usize },
}

impl EventEmitter<TabBarEvent> for TabBar {}

impl TabBar {
    /// Create a new tab bar view
    ///
    /// ### Arguments
    /// - `fulgur`: Weak handle to the owning window entity the bar reads its tabs from
    ///
    /// ### Returns
    /// - `TabBar`: The new tab bar view
    pub(crate) fn new(fulgur: WeakEntity<Fulgur>) -> Self {
        Self {
            fulgur,
            scroll_handle: ScrollHandle::new(),
            drag_ghost: None,
            pending_scroll: None,
        }
    }

    /// Request a deferred scroll to the given tab
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to scroll into view
    /// - `cx`: The tab bar context
    pub(crate) fn request_scroll_to(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        self.pending_scroll = Some(tab_id);
        cx.notify();
    }

    /// Scroll the tab at the given position into view immediately
    ///
    /// ### Arguments
    /// - `index`: Position of the tab in the owning window's tab list
    pub(crate) fn scroll_to_index(&self, index: usize) {
        self.scroll_handle.scroll_to_item(index);
    }

    /// Resolve a pending scroll request once layout bounds are available
    ///
    /// ### Arguments
    /// - `fulgur_entity`: The owning window entity used to resolve the tab position
    /// - `cx`: The tab bar context
    pub(super) fn process_pending_scroll(
        &mut self,
        fulgur_entity: &Entity<Fulgur>,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab_id) = self.pending_scroll {
            let Some(index) = fulgur_entity.read(cx).tab_index_of(tab_id) else {
                self.pending_scroll = None;
                return;
            };
            if self.scroll_handle.bounds_for_item(0).is_some() {
                self.scroll_handle.scroll_to_item(index);
                self.pending_scroll = None;
            } else {
                cx.notify();
            }
        }
    }

    /// Build a per-filename tab count map for disambiguation.
    ///
    /// ### Arguments
    /// - `tabs`: The open tabs to count filenames over
    ///
    /// ### Returns
    /// - `HashMap<String, usize>`: Map of filename to number of open tabs with that filename
    pub(crate) fn build_tab_filename_counts(tabs: &[Tab]) -> HashMap<String, usize> {
        let mut filename_counts = HashMap::new();
        for tab in tabs {
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
                return (filename.to_string(), Some(format!("../{parent_name}")));
            }

            return (filename.to_string(), None);
        }
        (base_title.to_string(), None)
    }

    /// Resolve the optional tab badge label for remote editor tabs.
    ///
    /// ### Arguments
    /// - `tab`: The tab to inspect.
    ///
    /// ### Returns
    /// - `Some(&'static str)`: `"R"` when the tab points to a remote location.
    /// - `None`: For local, untitled, settings, and markdown preview tabs.
    pub(crate) fn remote_tab_indicator_label(tab: &Tab) -> Option<&'static str> {
        let editor_tab = tab.as_editor()?;
        if matches!(editor_tab.location, TabLocation::Remote(_)) {
            Some("R")
        } else {
            None
        }
    }
}

impl Fulgur {
    /// Dispatch a tab bar event to the matching window-level handler
    ///
    /// ### Arguments
    /// - `event`: The tab bar event to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(crate) fn on_tab_bar_event(
        &mut self,
        event: &TabBarEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TabBarEvent::NewTab => self.new_tab(window, cx),
            TabBarEvent::OpenFile => self.open_file(window, cx),
            TabBarEvent::OpenPath => self.show_open_from_path_dialog(window, cx),
            TabBarEvent::SaveFile => self.save_file(window, cx),
            TabBarEvent::SaveFileAs => self.save_file_as(window, cx),
            TabBarEvent::Activate(tab_id) => {
                if let Some(index) = self.tab_index_of(*tab_id) {
                    self.set_active_tab(index, window, cx);
                }
            }
            TabBarEvent::Close(tab_id) => self.close_tab(*tab_id, window, cx),
            TabBarEvent::Drop { dragged, slot } => {
                self.handle_tab_drop(dragged, *slot, window, cx);
            }
        }
    }

    /// Request a deferred scroll of the tab bar to the given tab
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to scroll into view on the next tab bar render
    /// - `cx`: The application context
    pub(crate) fn request_tab_scroll(&self, tab_id: TabId, cx: &mut gpui::App) {
        self.tab_bar.update(cx, |bar, cx| {
            bar.request_scroll_to(tab_id, cx);
        });
    }
}
