use crate::fulgur::{
    Fulgur,
    ui::{
        components_utils::SEARCH_BAR_HEIGHT, copy_button::CopyButton, icons::CustomIcon,
        insert_button::InsertButton,
    },
};

use gpui::{
    Anchor, AppContext, Context, Div, Entity, EntityInputHandler, Hsla, InteractiveElement,
    IntoElement, ParentElement, SharedString, StatefulInteractiveElement, Styled, Subscription,
    Window, div, hsla,
};
use gpui_component::{
    ActiveTheme,
    color_picker::{ColorPickerEvent, ColorPickerState},
    h_flex,
    input::{Input, InputEvent, InputState},
};

use super::search_bar::{search_bar_button_factory, search_bar_toggle_button_factory};

/// Convert an HSLA color to `OkLCH` (Lightness, Chroma, Hue) components.
///
/// ### Arguments
/// - `color`: The HSLA color to convert
///
/// ### Returns
/// `(f32, f32, f32)` where L is 0..1, C is chroma, and H is hue in degrees 0..360.
fn hsla_to_oklch(color: Hsla) -> (f32, f32, f32) {
    let rgb = color.to_rgb();
    let to_linear = |c: f32| -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let lr = to_linear(rgb.r);
    let lg = to_linear(rgb.g);
    let lb = to_linear(rgb.b);
    #[allow(clippy::excessive_precision)]
    const LMS: [[f32; 3]; 3] = [
        [0.4122214708, 0.5363325363, 0.0514459929],
        [0.2119034982, 0.6806995451, 0.1073969566],
        [0.0883024619, 0.2817188376, 0.6299787005],
    ];
    let l = LMS[0][0] * lr + LMS[0][1] * lg + LMS[0][2] * lb;
    let m = LMS[1][0] * lr + LMS[1][1] * lg + LMS[1][2] * lb;
    let s = LMS[2][0] * lr + LMS[2][1] * lg + LMS[2][2] * lb;
    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();
    #[allow(clippy::excessive_precision)]
    const OKLAB: [[f32; 3]; 3] = [
        [0.2104542553, 0.7936177850, -0.0040720468],
        [1.9779984951, -2.4285922050, 0.4505937099],
        [0.0259040371, 0.7827717662, -0.8086757660],
    ];
    let ok_l = OKLAB[0][0] * l_ + OKLAB[0][1] * m_ + OKLAB[0][2] * s_;
    let ok_a = OKLAB[1][0] * l_ + OKLAB[1][1] * m_ + OKLAB[1][2] * s_;
    let ok_b = OKLAB[2][0] * l_ + OKLAB[2][1] * m_ + OKLAB[2][2] * s_;
    let chroma = (ok_a * ok_a + ok_b * ok_b).sqrt();
    let hue = ok_b.atan2(ok_a).to_degrees();
    let hue = if hue < 0.0 { hue + 360.0 } else { hue };
    (ok_l, chroma, hue)
}

/// Convert `OkLCH` (Lightness, Chroma, Hue) components back to an HSLA color.
///
/// ### Arguments
/// - `l`: Lightness in 0..1
/// - `c`: Chroma (non-negative)
/// - `h`: Hue in degrees 0..360
///
/// ### Returns
/// `Hsla`: The resulting color
fn oklch_to_hsla(l: f32, c: f32, h: f32) -> Hsla {
    let h_rad = h.to_radians();
    let ok_a = c * h_rad.cos();
    let ok_b = c * h_rad.sin();
    #[allow(clippy::excessive_precision)]
    const INV_OKLAB: [[f32; 3]; 3] = [
        [1.0000000000, 0.3963377774, 0.2158037573],
        [1.0000000000, -0.1055613458, -0.0638541728],
        [1.0000000000, -0.0894841775, -1.2914855480],
    ];
    let l_ = INV_OKLAB[0][0] * l + INV_OKLAB[0][1] * ok_a + INV_OKLAB[0][2] * ok_b;
    let m_ = INV_OKLAB[1][0] * l + INV_OKLAB[1][1] * ok_a + INV_OKLAB[1][2] * ok_b;
    let s_ = INV_OKLAB[2][0] * l + INV_OKLAB[2][1] * ok_a + INV_OKLAB[2][2] * ok_b;
    let lms_l = l_ * l_ * l_;
    let lms_m = m_ * m_ * m_;
    let lms_s = s_ * s_ * s_;
    #[allow(clippy::excessive_precision)]
    const INV_LMS: [[f32; 3]; 3] = [
        [4.0767416621, -3.3077115913, 0.2309699292],
        [-1.2684380046, 2.6097574011, -0.3413193965],
        [-0.0041960863, -0.7034186147, 1.7076147010],
    ];
    let lin_r = INV_LMS[0][0] * lms_l + INV_LMS[0][1] * lms_m + INV_LMS[0][2] * lms_s;
    let lin_g = INV_LMS[1][0] * lms_l + INV_LMS[1][1] * lms_m + INV_LMS[1][2] * lms_s;
    let lin_b = INV_LMS[2][0] * lms_l + INV_LMS[2][1] * lms_m + INV_LMS[2][2] * lms_s;
    let from_linear = |c: f32| -> f32 {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.0031308 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    };
    gpui::Rgba {
        r: from_linear(lin_r),
        g: from_linear(lin_g),
        b: from_linear(lin_b),
        a: 1.0,
    }
    .into()
}

/// Format an HSLA color as an `OkLCH` CSS string.
///
/// ### Arguments
/// - `color`: The HSLA color to format
///
/// ### Returns
/// `String`: The `OkLCH` CSS string (e.g. "oklch(0.63 0.26 29.2)")
fn format_oklch(color: Hsla) -> String {
    let (l, c, h) = hsla_to_oklch(color);
    format!("oklch({l:.2} {c:.4} {h:.1})")
}

/// Format an HSLA color as an HSLA CSS string.
///
/// ### Arguments
/// - `color`: The HSLA color to format
///
/// ### Returns
/// `String`: The HSLA CSS string (e.g. "hsla(210, 50%, 60%, 1.00)")
fn format_hsla(color: Hsla) -> String {
    format!(
        "hsla({:.0}, {:.0}%, {:.0}%, {:.2})",
        color.h * 360.0,
        color.s * 100.0,
        color.l * 100.0,
        color.a,
    )
}

/// Parse a hex color string into an HSLA color.
///
/// ### Arguments
/// - `s`: A hex string, with or without the leading `#` (e.g. "#FF0000" or "FF0000")
///
/// ### Returns
/// - `Option<Hsla>`: The parsed color
/// - `None`: If the string is not a valid HSLA string
fn parse_hex(s: &str) -> Option<Hsla> {
    gpui_component::Colorize::parse_hex(s).ok()
}

/// Parse an `OkLCH` CSS string into an HSLA color.
///
/// ### Arguments
/// - `s`: An `OkLCH` string in the form "oklch(L C H)" (e.g. "oklch(0.63 0.26 29.2)")
///
/// ### Returns
/// - `Some<Hsla>`: The parsed color
/// - `None`: If the string is not a valid HSLA string
fn parse_oklch(s: &str) -> Option<Hsla> {
    let inner = s.trim().strip_prefix("oklch(")?.strip_suffix(')')?;
    let mut parts = inner.split_whitespace();
    let l: f32 = parts.next()?.parse().ok()?;
    let c: f32 = parts.next()?.parse().ok()?;
    let h: f32 = parts.next()?.parse().ok()?;
    Some(oklch_to_hsla(l, c, h))
}

/// Parse an HSLA CSS string into an HSLA color.
///
/// ### Arguments
/// - `s`: An HSLA string in the form "hsla(H, S%, L%, A)" (e.g. "hsla(210, 50%, 60%, 1.00)")
///
/// ### Returns
/// - `Option<Hsla>`: The parsed color
/// - `None`: If the string is not a valid HSLA string
fn parse_hsla(s: &str) -> Option<Hsla> {
    let inner = s.trim().strip_prefix("hsla(")?.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() != 4 {
        return None;
    }
    let h: f32 = parts[0].trim().parse().ok()?;
    let s: f32 = parts[1].trim().trim_end_matches('%').parse().ok()?;
    let l: f32 = parts[2].trim().trim_end_matches('%').parse().ok()?;
    let a: f32 = parts[3].trim().parse().ok()?;
    Some(hsla(h / 360.0, s / 100.0, l / 100.0, a))
}

/// State for the color picker bar.
///
/// Groups the `ColorPickerState` entity, input states for each color format,
/// cached format strings (kept in sync by the color-picker subscription),
/// and the subscriptions for change events.
pub struct ColorPickerBarState {
    pub color_picker_state: Entity<ColorPickerState>,
    pub show_color_picker: bool,
    pub hex_input: Entity<InputState>,
    pub oklch_input: Entity<InputState>,
    pub hsla_input: Entity<InputState>,
    cached_hex: String,
    cached_oklch: String,
    cached_hsla: String,
    _color_picker_subscription: Subscription,
    _hex_input_subscription: Subscription,
    _oklch_input_subscription: Subscription,
    _hsla_input_subscription: Subscription,
}

impl ColorPickerBarState {
    /// Create a new `ColorPickerBarState`.
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Self`: A new `ColorPickerBarState` initialized to white
    pub fn new(window: &mut Window, cx: &mut Context<Fulgur>) -> Self {
        let color_picker_state =
            cx.new(|cx| ColorPickerState::new(window, cx).default_value(gpui::white()));
        let initial = gpui::white();
        let initial_hex = gpui_component::Colorize::to_hex(&initial).to_string();
        let initial_oklch = format_oklch(initial);
        let initial_hsla = format_hsla(initial);
        let hex_input = cx.new(|cx| InputState::new(window, cx).default_value(initial_hex.clone()));
        let oklch_input =
            cx.new(|cx| InputState::new(window, cx).default_value(initial_oklch.clone()));
        let hsla_input =
            cx.new(|cx| InputState::new(window, cx).default_value(initial_hsla.clone()));
        let _color_picker_subscription = cx.subscribe_in(
            &color_picker_state,
            window,
            |this: &mut Fulgur, _, event: &ColorPickerEvent, window, cx| {
                if let ColorPickerEvent::Change(Some(color)) = event {
                    let color = *color;
                    let hex = gpui_component::Colorize::to_hex(&color);
                    let oklch = format_oklch(color);
                    let hsla_str = format_hsla(color);
                    this.color_picker_bar_state.cached_hex = hex.to_string();
                    this.color_picker_bar_state.cached_oklch.clone_from(&oklch);
                    this.color_picker_bar_state
                        .cached_hsla
                        .clone_from(&hsla_str);
                    this.color_picker_bar_state
                        .hex_input
                        .update(cx, |state, cx| {
                            state.set_value(hex, window, cx);
                        });
                    this.color_picker_bar_state
                        .oklch_input
                        .update(cx, |state, cx| {
                            state.set_value(oklch, window, cx);
                        });
                    this.color_picker_bar_state
                        .hsla_input
                        .update(cx, |state, cx| {
                            state.set_value(hsla_str, window, cx);
                        });
                }
                cx.notify();
            },
        );
        // On Change: update the color picker preview only (no cross-input sync, which
        // would loop since InputState::set_value always emits InputEvent::Change).
        // On PressEnter / Blur: parse and sync all three inputs + the color picker.
        let _hex_input_subscription = cx.subscribe_in(
            &hex_input,
            window,
            |this: &mut Fulgur, _, event: &InputEvent, window, cx| {
                let value = this.color_picker_bar_state.hex_input.read(cx).value();
                let Some(color) = parse_hex(&value) else {
                    return;
                };
                match event {
                    InputEvent::Change => {
                        this.color_picker_bar_state
                            .color_picker_state
                            .update(cx, |state, cx| {
                                state.set_value(color, window, cx);
                            });
                    }
                    InputEvent::PressEnter { .. } | InputEvent::Blur => {
                        this.color_picker_bar_state
                            .color_picker_state
                            .update(cx, |state, cx| {
                                state.set_value(color, window, cx);
                            });
                        this.color_picker_bar_state
                            .oklch_input
                            .update(cx, |state, cx| {
                                state.set_value(format_oklch(color), window, cx);
                            });
                        this.color_picker_bar_state
                            .hsla_input
                            .update(cx, |state, cx| {
                                state.set_value(format_hsla(color), window, cx);
                            });
                    }
                    _ => {}
                }
            },
        );
        let _oklch_input_subscription = cx.subscribe_in(
            &oklch_input,
            window,
            |this: &mut Fulgur, _, event: &InputEvent, window, cx| {
                let value = this.color_picker_bar_state.oklch_input.read(cx).value();
                let Some(color) = parse_oklch(&value) else {
                    return;
                };
                match event {
                    InputEvent::Change => {
                        this.color_picker_bar_state
                            .color_picker_state
                            .update(cx, |state, cx| {
                                state.set_value(color, window, cx);
                            });
                    }
                    InputEvent::PressEnter { .. } | InputEvent::Blur => {
                        this.color_picker_bar_state
                            .color_picker_state
                            .update(cx, |state, cx| {
                                state.set_value(color, window, cx);
                            });
                        this.color_picker_bar_state
                            .hex_input
                            .update(cx, |state, cx| {
                                state.set_value(
                                    gpui_component::Colorize::to_hex(&color),
                                    window,
                                    cx,
                                );
                            });
                        this.color_picker_bar_state
                            .hsla_input
                            .update(cx, |state, cx| {
                                state.set_value(format_hsla(color), window, cx);
                            });
                    }
                    _ => {}
                }
            },
        );
        let _hsla_input_subscription = cx.subscribe_in(
            &hsla_input,
            window,
            |this: &mut Fulgur, _, event: &InputEvent, window, cx| {
                let value = this.color_picker_bar_state.hsla_input.read(cx).value();
                let Some(color) = parse_hsla(&value) else {
                    return;
                };
                match event {
                    InputEvent::Change => {
                        this.color_picker_bar_state
                            .color_picker_state
                            .update(cx, |state, cx| {
                                state.set_value(color, window, cx);
                            });
                    }
                    InputEvent::PressEnter { .. } | InputEvent::Blur => {
                        this.color_picker_bar_state
                            .color_picker_state
                            .update(cx, |state, cx| {
                                state.set_value(color, window, cx);
                            });
                        this.color_picker_bar_state
                            .hex_input
                            .update(cx, |state, cx| {
                                state.set_value(
                                    gpui_component::Colorize::to_hex(&color),
                                    window,
                                    cx,
                                );
                            });
                        this.color_picker_bar_state
                            .oklch_input
                            .update(cx, |state, cx| {
                                state.set_value(format_oklch(color), window, cx);
                            });
                    }
                    _ => {}
                }
            },
        );
        Self {
            color_picker_state,
            show_color_picker: false,
            hex_input,
            oklch_input,
            hsla_input,
            cached_hex: initial_hex,
            cached_oklch: initial_oklch,
            cached_hsla: initial_hsla,
            _color_picker_subscription,
            _hex_input_subscription,
            _oklch_input_subscription,
            _hsla_input_subscription,
        }
    }
}

impl Fulgur {
    /// Insert a value at the cursor position in the active editor tab,
    /// replacing the current selection if any.
    ///
    /// ### Arguments
    /// - `value`: The string to insert
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn insert_color_value(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.active_tab_index
            && let Some(crate::fulgur::tab::Tab::Editor(editor_tab)) = self.tabs.get_mut(index)
        {
            editor_tab.content.update(cx, |input_state, cx| {
                let selection = input_state.selected_text_range(true, window, cx);
                if selection.is_some() {
                    input_state.replace(value, window, cx);
                } else {
                    input_state.insert(value, window, cx);
                }
                cx.notify();
            });
        }
    }

    /// Toggle the color picker bar visibility.
    ///
    /// ### Arguments
    /// - `_window`: The window context
    /// - `cx`: The application context
    pub fn toggle_color_picker(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.color_picker_bar_state.show_color_picker =
            !self.color_picker_bar_state.show_color_picker;
        cx.notify();
    }

    /// Render the color picker bar.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(Div)`: The rendered color picker bar
    /// - `None`: If the color picker bar is hidden
    pub fn render_color_picker_bar(&self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.color_picker_bar_state.show_color_picker {
            return None;
        }
        let hex_value = &self.color_picker_bar_state.cached_hex;
        let oklch_value = &self.color_picker_bar_state.cached_oklch;
        let hsla_value = &self.color_picker_bar_state.cached_hsla;
        Some(
            div()
                .flex()
                .items_center()
                .justify_between()
                .bg(cx.theme().tab_bar)
                .p_0()
                .m_0()
                .w_full()
                .h(SEARCH_BAR_HEIGHT)
                .border_t_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .id("color-picker-scroll-container")
                        .flex()
                        .flex_1()
                        .overflow_x_scroll()
                        .child(
                            div()
                                .flex()
                                .flex_1()
                                .items_center()
                                .h(SEARCH_BAR_HEIGHT)
                                .child(self.render_color_picker_section(cx))
                                .child(self.render_color_value_section(
                                    "Hex",
                                    hex_value,
                                    &self.color_picker_bar_state.hex_input,
                                    cx,
                                ))
                                .child(self.render_color_value_section(
                                    "OkLCH",
                                    oklch_value,
                                    &self.color_picker_bar_state.oklch_input,
                                    cx,
                                ))
                                .child(self.render_color_value_section(
                                    "HSLA",
                                    hsla_value,
                                    &self.color_picker_bar_state.hsla_input,
                                    cx,
                                ))
                                .child(self.render_highlight_toggle_button(cx)),
                        ),
                )
                .child(self.render_color_picker_close_button(cx)),
        )
    }

    /// Render the color picker section (left part with the color picker widget).
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered color picker section
    fn render_color_picker_section(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let color_picker_state = &self.color_picker_bar_state.color_picker_state;
        h_flex()
            .items_center()
            .flex_shrink_0()
            .px_2()
            .gap_2()
            .h(SEARCH_BAR_HEIGHT)
            .child(
                gpui_component::color_picker::ColorPicker::new(color_picker_state)
                    .anchor(Anchor::BottomLeft),
            )
    }

    /// Render a color value section with an editable input and a clipboard copy button.
    ///
    /// ### Arguments
    /// - `label`: The label for the color format (e.g. "Hex", "`OkLCH`", "HSLA")
    /// - `value`: The current formatted color value string (used for the clipboard button)
    /// - `input_state`: The input state entity for this field
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Div`: The rendered color value section
    fn render_color_value_section(
        &self,
        label: &'static str,
        value: &str,
        input_state: &Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .flex_1()
            .min_w(gpui::px(330.0))
            .h(SEARCH_BAR_HEIGHT)
            .border_l_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_xs()
                    .px_2()
                    .text_color(cx.theme().muted_foreground)
                    .child(label),
            )
            .child(
                Input::new(input_state)
                    .appearance(false)
                    .bordered(false)
                    .px_0(),
            )
            .child(
                InsertButton::new(SharedString::from(format!("color-insert-{label}"))).on_click(
                    cx.listener({
                        let value = value.to_string();
                        move |this, _, window, cx| {
                            this.insert_color_value(value.clone(), window, cx);
                        }
                    }),
                ),
            )
            .child(
                CopyButton::new(SharedString::from(format!("color-copy-{label}")))
                    .value(SharedString::from(value.to_string())),
            )
    }

    /// Render the highlight colors toggle button for the color picker bar.
    ///
    /// Mirrors the "Highlight Colors" setting from the editor settings page.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Div`: The rendered toggle button
    fn render_highlight_toggle_button(&self, cx: &mut Context<Self>) -> Div {
        let highlight_colors = self.settings.editor_settings.highlight_colors;
        div()
            .flex()
            .items_center()
            .p_0()
            .m_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .child(
                search_bar_toggle_button_factory(
                    "highlight-colors-toggle",
                    "Toggle color highlighting",
                    CustomIcon::Highlighter,
                    cx.theme().border,
                    cx.theme().tab_bar,
                    cx.theme().accent,
                    highlight_colors,
                )
                .on_click(cx.listener(|this, _, _window, cx| {
                    this.settings.editor_settings.highlight_colors =
                        !this.settings.editor_settings.highlight_colors;
                    let _ = this.update_and_propagate_settings(cx);
                })),
            )
    }

    /// Render the close button for the color picker bar.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Div`: The rendered close button
    fn render_color_picker_close_button(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .p_0()
            .m_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .child(
                search_bar_button_factory(
                    "close-color-picker-button",
                    "Close",
                    CustomIcon::Close,
                    cx.theme().border,
                )
                .on_click(cx.listener(|this, _, _window, cx| {
                    this.color_picker_bar_state.show_color_picker = false;
                    cx.notify();
                })),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        format_hsla, format_oklch, hsla_to_oklch, oklch_to_hsla, parse_hex, parse_hsla, parse_oklch,
    };
    use gpui::hsla;

    #[test]
    fn test_format_oklch_produces_valid_string() {
        let color = hsla(0.0, 1.0, 0.5, 1.0); // pure red
        let result = format_oklch(color);
        assert!(result.starts_with("oklch("));
        assert!(result.ends_with(')'));
    }

    #[test]
    fn test_format_hsla_produces_valid_string() {
        let color = hsla(0.5833, 1.0, 0.5, 1.0); // ~210 degrees blue
        let result = format_hsla(color);
        assert!(result.starts_with("hsla("));
        assert!(result.ends_with(')'));
        assert!(result.contains('%'));
    }

    #[test]
    fn test_hsla_to_oklch_white() {
        let (l, c, _h) = hsla_to_oklch(gpui::white());
        assert!((l - 1.0).abs() < 0.01, "white should have lightness ~1.0");
        assert!(c < 0.01, "white should have near-zero chroma");
    }

    #[test]
    fn test_hsla_to_oklch_black() {
        let (l, c, _h) = hsla_to_oklch(gpui::black());
        assert!(l.abs() < 0.01, "black should have lightness ~0.0");
        assert!(c < 0.01, "black should have near-zero chroma");
    }

    #[test]
    fn test_parse_hex_valid() {
        let color = parse_hex("#FF0000");
        assert!(color.is_some());
    }

    #[test]
    fn test_parse_hex_invalid() {
        assert!(parse_hex("not-a-color").is_none());
    }

    #[test]
    fn test_parse_oklch_valid() {
        let color = parse_oklch("oklch(0.63 0.2600 29.2)");
        assert!(color.is_some());
    }

    #[test]
    fn test_parse_oklch_invalid() {
        assert!(parse_oklch("not-oklch").is_none());
    }

    #[test]
    fn test_parse_hsla_valid() {
        let color = parse_hsla("hsla(210, 50%, 60%, 1.00)");
        assert!(color.is_some());
    }

    #[test]
    fn test_parse_hsla_invalid() {
        assert!(parse_hsla("not-hsla").is_none());
    }

    #[test]
    fn test_oklch_roundtrip_red() {
        let original = hsla(0.0, 1.0, 0.5, 1.0);
        let (l, c, h) = hsla_to_oklch(original);
        let recovered = oklch_to_hsla(l, c, h);
        let orig_rgb = original.to_rgb();
        let rec_rgb = recovered.to_rgb();
        assert!(
            (orig_rgb.r - rec_rgb.r).abs() < 0.01,
            "red channel mismatch"
        );
        assert!(
            (orig_rgb.g - rec_rgb.g).abs() < 0.01,
            "green channel mismatch"
        );
        assert!(
            (orig_rgb.b - rec_rgb.b).abs() < 0.01,
            "blue channel mismatch"
        );
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod gpui_tests {
    use super::Fulgur;
    use crate::fulgur::{
        settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
    };
    use gpui::{
        AppContext, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext,
        Window, WindowOptions,
    };
    use gpui_component::input::Position;
    use parking_lot::Mutex;
    use std::{cell::RefCell, path::PathBuf, sync::Arc};

    struct EmptyView;

    impl Render for EmptyView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            gpui::div()
        }
    }

    fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let mut settings = Settings::new();
            settings.editor_settings.watch_files = false;
            let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
            cx.set_global(SharedAppState::new(settings, pending_files));
            cx.set_global(WindowManager::new());
        });

        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        let window = cx
            .update(|cx| {
                cx.open_window(WindowOptions::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                    *fulgur_slot.borrow_mut() = Some(fulgur);
                    cx.new(|_| EmptyView)
                })
            })
            .expect("failed to open test window");

        let visual_cx = VisualTestContext::from_window(window.into(), cx);
        visual_cx.run_until_parked();
        let fulgur = fulgur_slot
            .into_inner()
            .expect("failed to capture Fulgur entity");
        (fulgur, visual_cx)
    }

    #[gpui::test]
    fn test_color_picker_bar_hidden_by_default(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                assert!(!this.color_picker_bar_state.show_color_picker);
                assert!(this.render_color_picker_bar(cx).is_none());
            });
        });
    }

    #[gpui::test]
    fn test_toggle_color_picker_shows_bar(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.toggle_color_picker(window, cx);
                assert!(this.color_picker_bar_state.show_color_picker);
            });
        });
    }

    #[gpui::test]
    fn test_toggle_color_picker_twice_hides_bar(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.toggle_color_picker(window, cx);
                assert!(this.color_picker_bar_state.show_color_picker);

                this.toggle_color_picker(window, cx);
                assert!(!this.color_picker_bar_state.show_color_picker);
            });
        });
    }

    #[gpui::test]
    fn test_insert_color_value_inserts_at_cursor(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let editor = this
                    .get_active_editor_tab_mut()
                    .expect("expected active editor tab");
                editor.content.update(cx, |content, cx| {
                    content.set_value("color: ;", window, cx);
                    content.set_cursor_position(
                        Position {
                            line: 0,
                            character: 7,
                        },
                        window,
                        cx,
                    );
                });
                this.insert_color_value("#FF0000".to_string(), window, cx);
                let text = this
                    .get_active_editor_tab()
                    .expect("expected active editor tab")
                    .content
                    .read(cx)
                    .text()
                    .to_string();
                assert_eq!(text, "color: #FF0000;");
            });
        });
    }

    #[gpui::test]
    fn test_insert_color_value_no_active_tab_does_not_panic(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.active_tab_index = None;
                this.insert_color_value("#FF0000".to_string(), window, cx);
            });
        });
    }

    #[gpui::test]
    fn test_highlight_toggle_reflects_editor_setting(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, _cx| {
                let initial = this.settings.editor_settings.highlight_colors;
                this.settings.editor_settings.highlight_colors = !initial;
                assert_ne!(
                    this.settings.editor_settings.highlight_colors, initial,
                    "toggling highlight_colors should change the setting"
                );
                this.settings.editor_settings.highlight_colors = initial;
                assert_eq!(
                    this.settings.editor_settings.highlight_colors, initial,
                    "reverting should restore original value"
                );
            });
        });
    }
}
