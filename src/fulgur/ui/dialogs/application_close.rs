use gpui_kit::component::{
    WindowExt, button::Button, button::ButtonVariant, button::ButtonVariants, h_flex,
    notification::NotificationType, v_flex,
};
use gpui_kit::{
    App, Context, InteractiveElement, ParentElement, SharedString, Styled, Window, WindowId, div,
    px,
};

use crate::fulgur::Fulgur;
use crate::fulgur::ui::tabs::editor_tab::ContentRevision;
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::window_manager::WindowManager;

/// What a close plan closes once every large-file warning is resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloseScope {
    /// Quit the application, warning about the tabs of every window.
    Application,
    /// Close only the initiating window, warning about its own tabs.
    Window,
}

/// Stable identity and content revision for one large-file close warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LargeFileCloseTarget {
    window_id: WindowId,
    tab_id: TabId,
    revision: ContentRevision,
}

/// State carried through a sequence of large-file close warnings.
#[derive(Debug, Clone)]
struct ClosePlan {
    initiator_window_id: WindowId,
    scope: CloseScope,
    remaining: Vec<LargeFileCloseTarget>,
    discarded: Vec<LargeFileCloseTarget>,
    exit_confirmed: bool,
}

impl ClosePlan {
    /// Create a close plan for the window that requested the close.
    ///
    /// ### Arguments
    /// - `initiator_window_id`: Stable id of the window that requested the close
    /// - `scope`: Whether the plan quits the application or closes that window
    ///
    /// ### Returns
    /// - `ClosePlan`: An empty plan ready for target collection
    fn new(initiator_window_id: WindowId, scope: CloseScope) -> Self {
        Self {
            initiator_window_id,
            scope,
            remaining: Vec::new(),
            discarded: Vec::new(),
            exit_confirmed: false,
        }
    }
}

/// The user's decision for one large-file close warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LargeFileCloseChoice {
    /// Write the file to disk before continuing.
    Save,
    /// Continue without saving, dropping the unsaved changes.
    Discard,
    /// Abort the whole close.
    Cancel,
}

impl Fulgur {
    /// Start a close plan for this window outside the current entity update.
    ///
    /// ### Arguments
    /// - `scope`: Whether the plan quits the application or closes this window
    /// - `cx`: The window entity context
    pub(crate) fn start_close_plan(&self, scope: CloseScope, cx: &mut Context<Self>) {
        let plan = ClosePlan::new(self.window_id, scope);
        cx.defer(move |cx| Self::drive_close_plan(plan, cx));
    }

    /// Describe one tab if it currently needs a large-file close warning.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable id of the tab to inspect
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(LargeFileCloseTarget)`: Stable window/tab identity and current revision
    /// - `None`: The tab is absent or does not need a warning
    fn large_file_close_target(&self, tab_id: TabId, cx: &App) -> Option<LargeFileCloseTarget> {
        let tab = self.tab_entity_of(tab_id, cx)?;
        let tab = tab.read(cx);
        let editor_tab = tab.as_editor()?;
        editor_tab
            .needs_large_file_close_warning(cx)
            .then(|| LargeFileCloseTarget {
                window_id: self.window_id,
                tab_id,
                revision: editor_tab.content_revision(cx),
            })
    }

    /// Collect every large modified local tab in this window.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Vec<LargeFileCloseTarget>`: Warning targets in tab order
    fn large_file_close_targets(&self, cx: &App) -> Vec<LargeFileCloseTarget> {
        self.tabs
            .iter()
            .filter_map(|tab| self.large_file_close_target(tab.read(cx).id(), cx))
            .collect()
    }

    /// Continue a close plan outside any entity update.
    ///
    /// The plan is revalidated whenever its current queue is exhausted. A
    /// discarded revision is exempted only while that exact content remains.
    ///
    /// ### Arguments
    /// - `plan`: Close decisions and warning targets collected so far
    /// - `cx`: The application context
    fn drive_close_plan(mut plan: ClosePlan, cx: &mut App) {
        while let Some(candidate) = plan.remaining.first().copied() {
            plan.remaining.remove(0);
            let Some((entity, handle, target)) =
                Self::resolve_close_target(candidate.window_id, candidate.tab_id, cx)
            else {
                continue;
            };
            let dispatched_plan = plan.clone();
            if handle
                .update(cx, |_, window, cx| {
                    entity.update(cx, |this, cx| {
                        if let Some(index) = this.tab_index_of(target.tab_id, cx) {
                            this.set_active_tab(index, window, cx);
                        }
                        this.show_large_file_close_dialog(target, dispatched_plan, window, cx);
                    });
                    window.activate_window();
                })
                .is_ok()
            {
                return;
            }
        }

        let unresolved = Self::collect_close_targets(&plan, cx)
            .into_iter()
            .filter(|target| !plan.discarded.contains(target))
            .collect::<Vec<_>>();
        if !unresolved.is_empty() {
            plan.remaining = unresolved;
            Self::drive_close_plan(plan, cx);
            return;
        }

        Self::finish_close_plan(plan, cx);
    }

    /// List the registered windows a close plan covers, the initiating window first.
    ///
    /// ### Arguments
    /// - `plan`: The plan whose scope selects the windows
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Vec<WindowId>`: Window ids in prompt order
    fn close_plan_window_ids(plan: &ClosePlan, cx: &App) -> Vec<WindowId> {
        let mut window_ids = cx.global::<WindowManager>().get_all_window_ids();
        if plan.scope == CloseScope::Window {
            window_ids.retain(|window_id| *window_id == plan.initiator_window_id);
        }
        window_ids
            .sort_by_key(|window_id| (*window_id != plan.initiator_window_id, window_id.as_u64()));
        window_ids
    }

    /// Collect warning targets from every live window the plan covers.
    ///
    /// ### Arguments
    /// - `plan`: The plan whose scope selects the windows
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Vec<LargeFileCloseTarget>`: All current warning targets, grouped by window
    fn collect_close_targets(plan: &ClosePlan, cx: &App) -> Vec<LargeFileCloseTarget> {
        let manager = cx.global::<WindowManager>();
        let live_window_ids = cx
            .windows()
            .into_iter()
            .map(|handle| handle.window_id())
            .collect::<Vec<_>>();
        Self::close_plan_window_ids(plan, cx)
            .into_iter()
            .filter(|window_id| live_window_ids.contains(window_id))
            .filter_map(|window_id| manager.get_window(window_id)?.upgrade())
            .flat_map(|entity| entity.read(cx).large_file_close_targets(cx))
            .collect()
    }

    /// Resolve a live window and refresh a queued tab's content revision.
    ///
    /// ### Arguments
    /// - `window_id`: Stable id of the owning window
    /// - `tab_id`: Stable id of the tab
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some((Entity, AnyWindowHandle, LargeFileCloseTarget))`: Live warning target
    /// - `None`: The window/tab disappeared or no longer needs a warning
    fn resolve_close_target(
        window_id: WindowId,
        tab_id: TabId,
        cx: &App,
    ) -> Option<(
        gpui_kit::Entity<Fulgur>,
        gpui_kit::AnyWindowHandle,
        LargeFileCloseTarget,
    )> {
        let entity = cx
            .global::<WindowManager>()
            .get_window(window_id)?
            .upgrade()?;
        let target = entity.read(cx).large_file_close_target(tab_id, cx)?;
        let handle = cx
            .windows()
            .into_iter()
            .find(|handle| handle.window_id() == window_id)?;
        Some((entity, handle, target))
    }

    /// Show one large-file close warning in the window that owns the tab.
    ///
    /// ### Arguments
    /// - `target`: Stable window/tab identity and current content revision
    /// - `plan`: Remaining close work
    /// - `window`: The owning window
    /// - `cx`: The application context
    fn show_large_file_close_dialog(
        &mut self,
        target: LargeFileCloseTarget,
        plan: ClosePlan,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity().clone();
        let filename = self.tab_filename(target.tab_id, cx);
        window.open_alert_dialog(cx, move |modal, _, _| {
            let filename = filename.clone();
            let footer = {
                let entity = entity.clone();
                let plan = plan.clone();
                let make_button =
                    move |id: &'static str,
                          label: &'static str,
                          variant: ButtonVariant,
                          choice: LargeFileCloseChoice| {
                        let entity = entity.clone();
                        let plan = plan.clone();
                        Button::new(id)
                            .label(label)
                            .with_variant(variant)
                            .debug_selector(move || id.to_string())
                            .on_click(move |_, window, cx| {
                                let plan = plan.clone();
                                window.close_dialog(cx);
                                entity.update(cx, move |this, cx| {
                                    this.resolve_large_file_close_warning(
                                        choice, target, plan, window, cx,
                                    );
                                });
                            })
                    };
                h_flex()
                    .gap_2()
                    .justify_center()
                    .child(make_button(
                        "large-file-close-cancel",
                        "Cancel",
                        ButtonVariant::Ghost,
                        LargeFileCloseChoice::Cancel,
                    ))
                    .child(make_button(
                        "large-file-close-discard",
                        "Discard",
                        ButtonVariant::Danger,
                        LargeFileCloseChoice::Discard,
                    ))
                    .child(make_button(
                        "large-file-close-save",
                        "Save",
                        ButtonVariant::Primary,
                        LargeFileCloseChoice::Save,
                    ))
            };
            modal
                .title(div().text_size(px(16.)).child("Unsaved large file"))
                .keyboard(true)
                .close_button(false)
                .child(
                    v_flex()
                        .gap_2()
                        .child(div().text_size(px(14.)).child(format!(
                            "\"{filename}\" is too large to keep in memory when Fulgur closes."
                        )))
                        .child(div().text_size(px(14.)).child(
                            "Its unsaved changes will be dropped unless you save it to disk now.",
                        )),
                )
                .footer(footer)
        });
    }

    /// Apply one large-file close warning choice and resume the plan.
    ///
    /// ### Arguments
    /// - `choice`: The button the user clicked
    /// - `target`: The tab and revision shown by the dialog
    /// - `plan`: Remaining close work
    /// - `window`: The owning window
    /// - `cx`: The application context
    fn resolve_large_file_close_warning(
        &mut self,
        choice: LargeFileCloseChoice,
        target: LargeFileCloseTarget,
        mut plan: ClosePlan,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match choice {
            LargeFileCloseChoice::Cancel => return,
            LargeFileCloseChoice::Discard => {
                if let Some(current) = self.large_file_close_target(target.tab_id, cx) {
                    plan.discarded.retain(|discarded| {
                        discarded.window_id != current.window_id
                            || discarded.tab_id != current.tab_id
                    });
                    plan.discarded.push(current);
                }
            }
            LargeFileCloseChoice::Save => {
                if let Err(e) = self.save_local_tab_blocking(target.tab_id, cx) {
                    let filename = self.tab_filename(target.tab_id, cx);
                    log::error!("Failed to save large file '{filename}' on close: {e}");
                    window.push_notification(
                        (
                            NotificationType::Error,
                            SharedString::from(format!("Failed to save '{filename}': {e}")),
                        ),
                        cx,
                    );
                    return;
                }
            }
        }
        cx.defer(move |cx| Self::drive_close_plan(plan, cx));
    }

    /// Complete a close plan after a clean revalidation: confirm or quit the
    /// application, or close the initiating window.
    ///
    /// ### Arguments
    /// - `plan`: Completed warning decisions and confirmation state
    /// - `cx`: The application context
    fn finish_close_plan(plan: ClosePlan, cx: &mut App) {
        let manager = cx.global::<WindowManager>();
        let handles = cx.windows();
        let Some((entity, handle)) =
            Self::close_plan_window_ids(&plan, cx)
                .into_iter()
                .find_map(|window_id| {
                    let entity = manager.get_window(window_id)?.upgrade()?;
                    let handle = handles
                        .iter()
                        .find(|handle| handle.window_id() == window_id)?;
                    Some((entity, *handle))
                })
        else {
            return;
        };
        let _ = handle.update(cx, |_, window, cx| {
            entity.update(cx, |this, cx| match plan.scope {
                CloseScope::Application => {
                    if this.settings.app_settings.confirm_exit
                        && !plan.exit_confirmed
                        && !this.save_failed_once
                    {
                        Self::show_application_quit_confirmation(plan, window, cx);
                    } else {
                        this.persist_and_quit_application(window, cx);
                    }
                }
                CloseScope::Window => this.finish_window_close(window, cx),
            });
        });
    }

    /// Show the generic quit confirmation while retaining preflight decisions.
    ///
    /// ### Arguments
    /// - `plan`: Completed warning decisions to revalidate after confirmation
    /// - `window`: The initiating window
    /// - `cx`: The application context
    fn show_application_quit_confirmation(plan: ClosePlan, window: &mut Window, cx: &mut App) {
        window.open_alert_dialog(cx, move |modal, _, _| {
            let plan = plan.clone();
            modal
                .title(div().text_size(px(16.)).child("Quit Fulgur"))
                .keyboard(true)
                .show_cancel(true)
                .on_ok(move |_, _window, cx| {
                    let mut plan = plan.clone();
                    plan.exit_confirmed = true;
                    cx.defer(move |cx| Self::drive_close_plan(plan, cx));
                    true
                })
                .on_cancel(move |_, _window, _cx| true)
                .child(
                    div()
                        .text_size(px(14.))
                        .child("Are you sure you want to quit Fulgur?"),
                )
                .close_button(false)
        });
    }

    /// Persist the fully revalidated application state and quit.
    ///
    /// ### Arguments
    /// - `window`: The window supplying current bounds and notifications
    /// - `cx`: The application context
    fn persist_and_quit_application(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_close_after_state_save(CloseScope::Application, window, cx) {
            cx.quit();
        }
    }

    /// Attempt to persist the session and decide whether a close may continue.
    ///
    /// A failure blocks only the first attempt, so repeating the close forces
    /// it; a successful save clears that escape again.
    ///
    /// ### Arguments
    /// - `scope`: Whether the application quits or only this window closes
    /// - `window`: The window supplying current bounds and notifications
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `true`: State was saved, or the user repeated the close after a failure
    /// - `false`: The first save failure must block the close and notify the user
    pub(crate) fn can_close_after_state_save(
        &mut self,
        scope: CloseScope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // Another window may have closed while warnings were open; the last
        // window keeps its session so it is restored on the next launch.
        let keeps_this_window =
            scope == CloseScope::Application || cx.global::<WindowManager>().window_count() <= 1;
        let save_result = if keeps_this_window {
            self.save_state(cx, window)
        } else {
            self.save_state_without_this_window(cx)
        };
        let Err(e) = save_result else {
            self.save_failed_once = false;
            return true;
        };
        let (event, retry_hint) = match scope {
            CloseScope::Application => ("quit", "Quit again to force-quit"),
            CloseScope::Window => ("window close", "Close again to force-close"),
        };
        log::error!("Failed to save app state on {event}: {e}");
        if self.save_failed_once {
            log::warn!("Save failed again, allowing a forced {event}");
            return true;
        }
        self.save_failed_once = true;
        window.push_notification(
            (
                NotificationType::Error,
                SharedString::from(format!(
                    "Failed to save application state: {e}. {retry_hint}."
                )),
            ),
            cx,
        );
        false
    }
}
