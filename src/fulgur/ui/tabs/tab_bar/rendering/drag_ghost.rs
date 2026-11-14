use crate::fulgur::{
    Fulgur,
    ui::motion,
    ui::tabs::tab_bar::{TabBar, TabBarEvent},
    ui::tabs::tab_drag::DraggedTab,
};
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    AnyElement, Context, Entity, InteractiveElement, IntoElement, ParentElement, Styled, Window,
    div,
};

impl TabBar {
    /// Render a ghost tab shown at the insertion point during a drag operation.
    ///
    /// The ghost tab previews where the dragged tab will land when dropped. It uses
    /// a muted, semi-transparent style to distinguish it from real tabs.
    ///
    /// ### Arguments
    /// - `slot`: The insertion slot index
    /// - `dragged`: The dragged tab data (used for title and modified state)
    /// - `opacity`: The ghost opacity for this frame, from [`Self::sample_ghost_opacity`]
    /// - `cx`: The tab bar context
    ///
    /// ### Returns
    /// - `AnyElement`: The rendered ghost tab element
    fn render_ghost_tab(
        slot: usize,
        dragged: &DraggedTab,
        opacity: f32,
        cx: &Context<Self>,
    ) -> AnyElement {
        use crate::fulgur::ui::components_utils::TAB_BAR_HEIGHT;

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
            .opacity(opacity)
            .child(
                div()
                    .pl_1()
                    .text_sm()
                    .text_color(cx.theme().tab_active_foreground)
                    .child(dragged.ghost_label()),
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

    /// Resolve the ghost tab to show for the current drag, if any.
    ///
    /// ### Arguments
    /// - `fulgur_entity`: The owning window, read to locate the dragged tab
    /// - `cx`: The tab bar context
    ///
    /// ### Returns
    /// - `Some((usize, DraggedTab))`: The insertion slot and the tab being dragged
    /// - `None`: No ghost should be shown this frame
    pub(super) fn resolve_drag_ghost(
        &self,
        fulgur_entity: &Entity<Fulgur>,
        cx: &Context<Self>,
    ) -> Option<(usize, DraggedTab)> {
        if !cx.has_active_drag() {
            return None;
        }
        let fulgur = fulgur_entity.read(cx);
        let (slot, dragged) = self.drag_ghost.as_ref()?;
        let is_noop = fulgur
            .tab_index_of(dragged.tab_id, cx)
            .is_some_and(|from| *slot == from || *slot == from + 1);
        if is_noop {
            None
        } else {
            Some((*slot, dragged.clone()))
        }
    }

    /// Sample the ghost tab's opacity for the current frame.
    ///
    /// ### Arguments
    /// - `present`: Whether a ghost is shown this frame
    /// - `window`: The window being rendered
    /// - `cx`: The tab bar context
    ///
    /// ### Returns
    /// - `f32`: The opacity to paint the ghost with
    pub(super) fn sample_ghost_opacity(
        present: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> f32 {
        /// Opacity of a fully faded in ghost tab, keeping it muted against real tabs.
        const GHOST_OPACITY: f32 = 0.45;

        let sample = motion::presence("tab-drag-ghost", present, motion::affordance(), window, cx);
        sample.progress * GHOST_OPACITY
    }

    /// Render all tabs, inserting a ghost tab at the current drag insertion point.
    ///
    /// ### Arguments
    /// - `fulgur`: The owning window to read the tab list from
    /// - `ghost`: The resolved ghost slot and dragged tab, from [`Self::resolve_drag_ghost`]
    /// - `ghost_opacity`: The ghost opacity for this frame
    /// - `cx`: The tab bar context
    ///
    /// ### Returns
    /// - `Vec<AnyElement>`: Tab elements, with a ghost tab inserted at the drag target
    pub(super) fn render_tabs_with_slots(
        &self,
        fulgur: &Fulgur,
        ghost: Option<&(usize, DraggedTab)>,
        ghost_opacity: f32,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let filename_counts = Self::build_tab_filename_counts(&fulgur.tabs, cx);
        let capacity = fulgur.tabs.len() + usize::from(ghost.is_some());
        let mut elements: Vec<AnyElement> = Vec::with_capacity(capacity);
        if let Some((0, dragged)) = ghost {
            elements.push(Self::render_ghost_tab(0, dragged, ghost_opacity, cx));
        }
        for (index, tab) in fulgur.tabs.iter().enumerate() {
            elements.push(self.render_tab(fulgur, index, tab.read(cx), &filename_counts, cx));
            if let Some((slot, dragged)) = ghost
                && *slot == index + 1
            {
                elements.push(Self::render_ghost_tab(*slot, dragged, ghost_opacity, cx));
            }
        }
        elements
    }
}
