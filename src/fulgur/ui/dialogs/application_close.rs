use gpui_kit::component::{
    WindowExt, button::Button, button::ButtonVariant, button::ButtonVariants, h_flex,
    notification::NotificationType, v_flex,
};
use gpui_kit::{
    App, Context, InteractiveElement, ParentElement, SharedString, Styled, Window, WindowId, div,
    px,
};

use crate::fulgur::Fulgur;
use crate::fulgur::ui::tabs::editor_tab::{ContentRevision, TabLocation};
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::window_manager::WindowManager;

/// Stable identity and content revision for one application-wide close warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LargeFileCloseTarget {
    window_id: WindowId,
    tab_id: TabId,
    revision: ContentRevision,
}

/// State carried through an application-wide sequence of close warnings.
#[derive(Debug, Clone)]
pub(crate) struct ApplicationClosePlan {
    initiator_window_id: WindowId,
    remaining: Vec<LargeFileCloseTarget>,
    discarded: Vec<LargeFileCloseTarget>,
    exit_confirmed: bool,
}

impl ApplicationClosePlan {
    /// Create an application-close plan for the window that received Quit.
    ///
    /// ### Arguments
    /// - `initiator_window_id`: Stable id of the window that received Quit
    ///
    /// ### Returns
    /// - `ApplicationClosePlan`: An empty plan ready for application-wide collection
    pub(crate) fn new(initiator_window_id: WindowId) -> Self {
        Self {
            initiator_window_id,
            remaining: Vec::new(),
            discarded: Vec::new(),
            exit_confirmed: false,
        }
    }
}

/// The user's decision for one application-wide large-file warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApplicationCloseChoice {
    Save,
    Discard,
    Cancel,
}

impl Fulgur {
    /// Describe one tab if it currently needs a large-file close warning.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable id of the tab to inspect
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(LargeFileCloseTarget)`: Stable window/tab identity and current revision
    /// - `None`: The tab is absent or does not need a warning
    fn application_close_target(&self, tab_id: TabId, cx: &App) -> Option<LargeFileCloseTarget> {
        let tab = self.tab_entity_of(tab_id, cx)?;
        let tab = tab.read(cx);
        let editor_tab = tab.as_editor()?;
        if matches!(editor_tab.location, TabLocation::Local(_))
            && editor_tab.content_too_large_to_persist(cx)
            && editor_tab.content_differs_from_original(cx)
        {
            Some(LargeFileCloseTarget {
                window_id: self.window_id,
                tab_id,
                revision: editor_tab.content_revision(cx),
            })
        } else {
            None
        }
    }

    /// Collect every large modified local tab in this window.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Vec<LargeFileCloseTarget>`: Warning targets in tab order
    fn application_close_targets(&self, cx: &App) -> Vec<LargeFileCloseTarget> {
        self.tabs
            .iter()
            .filter_map(|tab| self.application_close_target(tab.read(cx).id(), cx))
            .collect()
    }

    /// Continue an application-wide quit preflight outside any entity update.
    ///
    /// The plan is revalidated whenever its current queue is exhausted. A
    /// discarded revision is exempted only while that exact content remains.
    ///
    /// ### Arguments
    /// - `plan`: Close decisions and warning targets collected so far
    /// - `cx`: The application context
    pub(crate) fn drive_application_close_plan(mut plan: ApplicationClosePlan, cx: &mut App) {
        while let Some(candidate) = plan.remaining.first().copied() {
            plan.remaining.remove(0);
            let Some((entity, handle, target)) =
                Self::resolve_application_close_target(candidate.window_id, candidate.tab_id, cx)
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
                        this.show_application_large_file_close_dialog(
                            target,
                            dispatched_plan,
                            window,
                            cx,
                        );
                    });
                    window.activate_window();
                })
                .is_ok()
            {
                return;
            }
        }

        let unresolved = Self::collect_application_close_targets(plan.initiator_window_id, cx)
            .into_iter()
            .filter(|target| !plan.discarded.contains(target))
            .collect::<Vec<_>>();
        if !unresolved.is_empty() {
            plan.remaining = unresolved;
            Self::drive_application_close_plan(plan, cx);
            return;
        }

        Self::finish_application_close_preflight(plan, cx);
    }

    /// Collect warning targets from every live Fulgur window.
    ///
    /// ### Arguments
    /// - `initiator_window_id`: Window to place first in the prompt order
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Vec<LargeFileCloseTarget>`: All current warning targets, grouped by window
    fn collect_application_close_targets(
        initiator_window_id: WindowId,
        cx: &App,
    ) -> Vec<LargeFileCloseTarget> {
        let manager = cx.global::<WindowManager>();
        let mut window_ids = manager.get_all_window_ids();
        window_ids.sort_by_key(|window_id| (*window_id != initiator_window_id, window_id.as_u64()));
        let live_window_ids = cx
            .windows()
            .into_iter()
            .map(|handle| handle.window_id())
            .collect::<Vec<_>>();
        window_ids
            .into_iter()
            .filter(|window_id| live_window_ids.contains(window_id))
            .filter_map(|window_id| manager.get_window(window_id)?.upgrade())
            .flat_map(|entity| entity.read(cx).application_close_targets(cx))
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
    fn resolve_application_close_target(
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
        let target = entity.read(cx).application_close_target(tab_id, cx)?;
        let handle = cx
            .windows()
            .into_iter()
            .find(|handle| handle.window_id() == window_id)?;
        Some((entity, handle, target))
    }

    /// Show one application-wide warning in the window that owns the tab.
    ///
    /// ### Arguments
    /// - `target`: Stable window/tab identity and current content revision
    /// - `plan`: Remaining application-close work
    /// - `window`: The owning window
    /// - `cx`: The application context
    fn show_application_large_file_close_dialog(
        &mut self,
        target: LargeFileCloseTarget,
        plan: ApplicationClosePlan,
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
                          choice: ApplicationCloseChoice| {
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
                                    this.resolve_application_large_file_close_warning(
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
                        ApplicationCloseChoice::Cancel,
                    ))
                    .child(make_button(
                        "large-file-close-discard",
                        "Discard",
                        ButtonVariant::Danger,
                        ApplicationCloseChoice::Discard,
                    ))
                    .child(make_button(
                        "large-file-close-save",
                        "Save",
                        ButtonVariant::Primary,
                        ApplicationCloseChoice::Save,
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

    /// Apply one application-wide warning choice and resume the preflight.
    ///
    /// ### Arguments
    /// - `choice`: The button the user clicked
    /// - `target`: The tab and revision shown by the dialog
    /// - `plan`: Remaining application-close work
    /// - `window`: The owning window
    /// - `cx`: The application context
    fn resolve_application_large_file_close_warning(
        &mut self,
        choice: ApplicationCloseChoice,
        target: LargeFileCloseTarget,
        mut plan: ApplicationClosePlan,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match choice {
            ApplicationCloseChoice::Cancel => return,
            ApplicationCloseChoice::Discard => {
                if let Some(current) = self.application_close_target(target.tab_id, cx) {
                    plan.discarded.retain(|discarded| {
                        discarded.window_id != current.window_id
                            || discarded.tab_id != current.tab_id
                    });
                    plan.discarded.push(current);
                }
            }
            ApplicationCloseChoice::Save => {
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
        cx.defer(move |cx| Self::drive_application_close_plan(plan, cx));
    }

    /// Complete or confirm an application quit after a clean revalidation.
    ///
    /// ### Arguments
    /// - `plan`: Completed warning decisions and confirmation state
    /// - `cx`: The application context
    fn finish_application_close_preflight(plan: ApplicationClosePlan, cx: &mut App) {
        let manager = cx.global::<WindowManager>();
        let mut window_ids = manager.get_all_window_ids();
        window_ids
            .sort_by_key(|window_id| (*window_id != plan.initiator_window_id, window_id.as_u64()));
        let handles = cx.windows();
        let Some((entity, handle)) = window_ids.into_iter().find_map(|window_id| {
            let entity = manager.get_window(window_id)?.upgrade()?;
            let handle = handles
                .iter()
                .find(|handle| handle.window_id() == window_id)?;
            Some((entity, *handle))
        }) else {
            return;
        };
        let _ = handle.update(cx, |_, window, cx| {
            entity.update(cx, |this, cx| {
                if this.settings.app_settings.confirm_exit && !plan.exit_confirmed {
                    Self::show_application_quit_confirmation(plan, window, cx);
                } else {
                    this.persist_and_quit_application(window, cx);
                }
            });
        });
    }

    /// Show the generic quit confirmation while retaining preflight decisions.
    ///
    /// ### Arguments
    /// - `plan`: Completed warning decisions to revalidate after confirmation
    /// - `window`: The initiating window
    /// - `cx`: The application context
    fn show_application_quit_confirmation(
        plan: ApplicationClosePlan,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.open_alert_dialog(cx, move |modal, _, _| {
            let plan = plan.clone();
            modal
                .title(div().text_size(px(16.)).child("Quit Fulgur"))
                .keyboard(true)
                .show_cancel(true)
                .on_ok(move |_, _window, cx| {
                    let mut plan = plan.clone();
                    plan.exit_confirmed = true;
                    cx.defer(move |cx| Self::drive_application_close_plan(plan, cx));
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
    fn persist_and_quit_application(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(e) = self.save_state(cx, window) {
            log::error!("Failed to save app state on quit: {e}");
            window.push_notification(
                (
                    NotificationType::Error,
                    SharedString::from(format!(
                        "Failed to save application state: {e}. Try again or close the app to quit without saving."
                    )),
                ),
                cx,
            );
            return;
        }
        cx.quit();
    }
}
