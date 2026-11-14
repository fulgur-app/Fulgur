use gpui_kit::component::{Sizable, h_flex, spinner::Spinner};
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window, div,
    prelude::FluentBuilder, px,
};

use crate::fulgur::ui::icons::CustomIcon;

use super::RemotePathBrowser;

/// Title component for the remote path browser dialog.
///
/// Observes the browser entity and re-renders whenever its loading state
/// changes, showing a spinner aligned to the right while a listing is in
/// flight.
pub struct BrowserDialogTitle {
    browser: Entity<RemotePathBrowser>,
    #[allow(dead_code, reason = "RAII guard: keeps the subscription alive")]
    browser_subscription: Subscription,
}

impl BrowserDialogTitle {
    /// Create a title that tracks the loading state of a remote browser.
    ///
    /// ### Arguments
    /// - `browser`: Remote path browser entity to observe.
    /// - `cx`: Component context.
    ///
    /// ### Returns
    /// - `BrowserDialogTitle`: Initialized title component.
    pub fn new(browser: Entity<RemotePathBrowser>, cx: &mut Context<Self>) -> Self {
        let browser_subscription = cx.observe(&browser, |_, _, cx| {
            cx.notify();
        });
        Self {
            browser,
            browser_subscription,
        }
    }
}

impl Render for BrowserDialogTitle {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_loading = self.browser.read(cx).is_loading;
        h_flex()
            .w_full()
            .items_center()
            .child(div().text_size(px(16.)).child("Browse remote files..."))
            .when(is_loading, |this| {
                this.child(
                    div()
                        .ml_auto()
                        .child(Spinner::new().icon(CustomIcon::LoaderCircle).small()),
                )
            })
    }
}
