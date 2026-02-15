use gpui::*;
use gpui_component::{Icon, WindowExt, h_flex, link::Link};

use crate::fulgur::ui::icons::CustomIcon;

/// Show the about dialog
///
/// ### Arguments
/// - `window`: The window context
/// - `cx`: The application context
pub fn about(window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, |modal, _window, _cx| {
        modal
            .alert()
            .keyboard(true)
            .title(div().text_center().child("Fulgur"))
            .child(
                gpui_component::v_flex()
                    .gap_4()
                    .items_center()
                    .child(img("assets/icon_square.png").w(px(200.0)).h(px(200.0)))
                    .child(format!("Version {}", env!("CARGO_PKG_VERSION")))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(Icon::new(CustomIcon::Globe))
                            .child(
                                Link::new("website-link")
                                    .href("https://fulgur.app")
                                    .child("https://fulgur.app"),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(Icon::new(CustomIcon::GitHub))
                            .child(
                                Link::new("github-link")
                                    .href("https://github.com/fulgur-app/Fulgur")
                                    .child("https://github.com/fulgur-app/Fulgur"),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(Icon::new(CustomIcon::File))
                            .child(
                                Link::new("license-link")
                                    .href("http://www.apache.org/licenses/LICENSE-2.0")
                                    .child("http://www.apache.org/licenses/LICENSE-2.0"),
                            ),
                    ),
            )
    });
}
