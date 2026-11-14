use crate::fulgur::Fulgur;
use crate::fulgur::ui::dialogs::external_link::show_external_link_dialog;
use crate::fulgur::utils::markdown_links::{
    MarkdownLinkTarget, describe_markdown_link, resolve_markdown_link,
};
use gpui_kit::component::{WindowExt, notification::NotificationType, tooltip::Tooltip};
use gpui_kit::{AnyView, App, ClickEvent, Context, MouseButton, SharedString, Window};
use std::path::PathBuf;

impl Fulgur {
    /// Build the link activation handler for a markdown preview.
    ///
    /// ### Arguments
    /// - `base_dir`: The directory of the previewed file, used to resolve
    ///   relative links. `None` for a buffer that has never been saved.
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl Fn(...)`: A handler that hands web links to the system, asks
    ///   before opening any other scheme, refuses network shares, and opens
    ///   local files in a tab of this window.
    pub(super) fn markdown_link_handler(
        base_dir: Option<PathBuf>,
        cx: &Context<Self>,
    ) -> impl Fn(&SharedString, &ClickEvent, &mut Window, &mut App) + use<> {
        let view = cx.entity().downgrade();
        move |target, event, window, cx| {
            if !is_link_activation(event) {
                return;
            }
            match resolve_markdown_link(target, base_dir.as_deref()) {
                Some(MarkdownLinkTarget::Web(url)) => {
                    log::debug!("Opening markdown preview link {url} in the system handler");
                    cx.open_url(&url);
                }
                Some(MarkdownLinkTarget::OtherScheme(url)) => {
                    show_external_link_dialog(url, window, cx);
                }
                Some(MarkdownLinkTarget::NetworkFile(path)) => {
                    log::warn!(
                        "Refusing markdown preview link to a network share: {}",
                        path.display()
                    );
                    window.push_notification(
                        (
                            NotificationType::Warning,
                            SharedString::from(format!(
                                "Links to network shares are not opened from the preview: {}",
                                path.display()
                            )),
                        ),
                        cx,
                    );
                }
                Some(MarkdownLinkTarget::LocalFile(path)) => {
                    if !path.is_file() {
                        log::warn!(
                            "Markdown preview link points at a missing file: {}",
                            path.display()
                        );
                        window.push_notification(
                            (
                                NotificationType::Warning,
                                SharedString::from(format!(
                                    "Cannot open '{}': the file does not exist",
                                    path.display()
                                )),
                            ),
                            cx,
                        );
                        return;
                    }
                    view.update(cx, |this, cx| this.do_open_file(window, cx, path))
                        .ok();
                }
                // An anchor has no scroll target in the preview, and an
                // unresolvable reference has nowhere to go.
                Some(MarkdownLinkTarget::Anchor) | None => {}
            }
        }
    }

    /// Build the hover tooltip of a markdown preview's links.
    ///
    /// ### Arguments
    /// - `base_dir`: The directory of the previewed file, used to resolve
    ///   relative links. `None` for a buffer that has never been saved.
    ///
    /// ### Returns
    /// - `impl Fn(...)`: A builder showing where a link leads, resolved the
    ///   same way `markdown_link_handler` resolves a click.
    pub(super) fn markdown_link_tooltip(
        base_dir: Option<PathBuf>,
    ) -> impl Fn(&SharedString, &mut Window, &mut App) -> AnyView + Send + Sync + 'static {
        move |target, window, cx| {
            Tooltip::new(describe_markdown_link(target, base_dir.as_deref())).build(window, cx)
        }
    }
}

/// Tell whether a click on a preview link should activate it, mirroring the
/// default handler of gpui-base's `TextView`.
///
/// ### Arguments
/// - `event`: The click that hit the link
///
/// ### Returns
/// - `bool`: `true` for a left or middle mouse click, a keyboard activation,
///   or a touch tap. `false` for any other mouse button or a touch long press,
///   which are secondary activations.
fn is_link_activation(event: &ClickEvent) -> bool {
    match event {
        ClickEvent::Mouse(click) => {
            matches!(click.up.button, MouseButton::Left | MouseButton::Middle)
        }
        ClickEvent::Keyboard(_) => true,
        ClickEvent::Touch(tap) => !tap.long_press,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{KeyboardClickEvent, MouseClickEvent, MouseUpEvent, TouchClickEvent};

    fn mouse_click(button: MouseButton) -> ClickEvent {
        ClickEvent::Mouse(MouseClickEvent {
            up: MouseUpEvent {
                button,
                ..MouseUpEvent::default()
            },
            ..MouseClickEvent::default()
        })
    }

    #[test]
    fn left_and_middle_clicks_activate_links() {
        assert!(is_link_activation(&mouse_click(MouseButton::Left)));
        assert!(is_link_activation(&mouse_click(MouseButton::Middle)));
    }

    #[test]
    fn right_click_does_not_activate_links() {
        assert!(!is_link_activation(&mouse_click(MouseButton::Right)));
    }

    #[test]
    fn keyboard_activation_opens_links() {
        assert!(is_link_activation(&ClickEvent::Keyboard(
            KeyboardClickEvent::default()
        )));
    }

    #[test]
    fn touch_tap_activates_but_long_press_does_not() {
        let tap = TouchClickEvent::default();
        assert!(is_link_activation(&ClickEvent::Touch(tap.clone())));
        assert!(!is_link_activation(&ClickEvent::Touch(TouchClickEvent {
            long_press: true,
            ..tap
        })));
    }
}
