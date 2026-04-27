use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use gpui::{
    App, ElementId, IntoElement, ParentElement, SharedString, Styled, WeakEntity, Window, div,
};
use gpui_component::{
    Sizable, WindowExt, button::Button, h_flex, notification::Notification, spinner::Spinner,
};

use crate::fulgur::ui::icons::CustomIcon;

/// Grace period before a long-running operation indicator becomes visible.
const PROGRESS_NOTIFICATION_SHOW_AFTER: Duration = Duration::from_millis(300);

/// Polling interval used to detect operation completion and dismiss the
/// notification.
const PROGRESS_NOTIFICATION_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Stable name fragment used to construct unique notification ids.
const PROGRESS_NOTIFICATION_KEY: &str = "fulgur-progress";

/// Marker type used so progress notifications share a `TypeId` namespace
/// distinct from app status toasts (success/info/error).
struct ProgressKind;

/// Monotonic id used as the `ElementId` integer for each notification so
/// concurrent indicators do not replace each other.
static NEXT_PROGRESS_ID: AtomicU64 = AtomicU64::new(1);

/// Closure invoked once when the user clicks the Cancel button.
pub type CancelCallback = Box<dyn FnOnce(&mut Window, &mut App) + 'static>;

/// RAII handle to a long-running operation indicator.
pub struct ProgressNotification {
    completed: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
}

impl ProgressNotification {
    /// Cancel flag the worker thread can poll before publishing results.
    ///
    /// ### Returns
    /// - `Arc<AtomicBool>`: Becomes `true` after the user clicks Cancel.
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }
}

impl Drop for ProgressNotification {
    fn drop(&mut self) {
        self.completed.store(true, Ordering::Release);
    }
}

/// Start a long-running operation indicator.
///
/// ### Arguments
/// - `window`: Target window for the notification.
/// - `cx`: Application context.
/// - `label`: Description shown next to the spinner.
/// - `on_cancel`: Optional callback invoked once when the user clicks Cancel.
///   Should remove any in-flight request entry so the eventual worker
///   completion is discarded by the caller's freshness check.
///
/// ### Returns
/// - `ProgressNotification`: RAII handle whose drop dismisses the
///   notification.
pub fn start_progress(
    window: &mut Window,
    cx: &mut App,
    label: SharedString,
    on_cancel: Option<CancelCallback>,
) -> ProgressNotification {
    let completed = Arc::new(AtomicBool::new(false));
    let cancelled = Arc::new(AtomicBool::new(false));
    let progress_id = NEXT_PROGRESS_ID.fetch_add(1, Ordering::Relaxed);

    let completed_for_task = Arc::clone(&completed);
    let cancelled_for_task = Arc::clone(&cancelled);
    let cancel_slot: Rc<RefCell<Option<CancelCallback>>> = Rc::new(RefCell::new(on_cancel));

    window
        .spawn(cx, async move |async_cx| {
            async_cx
                .background_executor()
                .timer(PROGRESS_NOTIFICATION_SHOW_AFTER)
                .await;
            if completed_for_task.load(Ordering::Acquire) {
                return;
            }

            let element_id =
                ElementId::NamedInteger(SharedString::from(PROGRESS_NOTIFICATION_KEY), progress_id);
            let has_cancel = cancel_slot.borrow().is_some();
            let label_for_content = label;
            let cancelled_for_action = Arc::clone(&cancelled_for_task);
            let completed_for_action = Arc::clone(&completed_for_task);
            let cancel_slot_for_action = Rc::clone(&cancel_slot);

            let entity: Option<WeakEntity<Notification>> = match async_cx.update(|window, cx| {
                let mut note = Notification::new()
                    .autohide(false)
                    .id1::<ProgressKind>(element_id)
                    .content({
                        let label = label_for_content;
                        move |_, _, _| {
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(Spinner::new().icon(CustomIcon::LoaderCircle).small())
                                .child(div().text_sm().child(label.clone()))
                                .into_any_element()
                        }
                    });

                if has_cancel {
                    let cancelled = cancelled_for_action;
                    let completed = completed_for_action;
                    let cancel_slot = cancel_slot_for_action;
                    note = note.action(move |_, _, cx| {
                        let cancelled = Arc::clone(&cancelled);
                        let completed = Arc::clone(&completed);
                        let cancel_slot = Rc::clone(&cancel_slot);
                        Button::new("progress-cancel")
                            .label("Cancel")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cancelled.store(true, Ordering::Release);
                                completed.store(true, Ordering::Release);
                                let maybe_cb = cancel_slot.borrow_mut().take();
                                if let Some(cb) = maybe_cb {
                                    cb(window, cx);
                                }
                                this.dismiss(window, cx);
                            }))
                    });
                }

                window.push_notification(note, cx);
                window.notifications(cx).last().map(|e| e.downgrade())
            }) {
                Ok(entity) => entity,
                Err(_) => return,
            };

            loop {
                async_cx
                    .background_executor()
                    .timer(PROGRESS_NOTIFICATION_POLL_INTERVAL)
                    .await;
                if completed_for_task.load(Ordering::Acquire) {
                    let _ = async_cx.update(|window, cx| {
                        if let Some(entity) = entity.as_ref().and_then(|e| e.upgrade()) {
                            entity.update(cx, |note, cx| note.dismiss(window, cx));
                        }
                    });
                    return;
                }
            }
        })
        .detach();

    ProgressNotification {
        completed,
        cancelled,
    }
}
