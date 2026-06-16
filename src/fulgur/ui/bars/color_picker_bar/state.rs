use crate::fulgur::Fulgur;

use gpui::{AppContext, Context, Entity, Hsla, Subscription, Window};
use gpui_component::{
    color_picker::{ColorPickerEvent, ColorPickerState},
    input::{InputEvent, InputState},
};

use super::color::{format_hsla, format_oklch, parse_hex, parse_hsla, parse_oklch};

/// The three color text-input formats handled by the color picker bar.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ColorInput {
    Hex,
    Oklch,
    Hsla,
}

impl ColorInput {
    /// All formats, in display order.
    const ALL: [ColorInput; 3] = [ColorInput::Hex, ColorInput::Oklch, ColorInput::Hsla];

    /// Borrow the input state entity backing this format.
    ///
    /// ### Arguments
    /// - `state`: The color picker bar state holding the inputs
    ///
    /// ### Returns
    /// - `&Entity<InputState>`: The input entity for this format
    fn input(self, state: &ColorPickerBarState) -> &Entity<InputState> {
        match self {
            ColorInput::Hex => &state.hex_input,
            ColorInput::Oklch => &state.oklch_input,
            ColorInput::Hsla => &state.hsla_input,
        }
    }

    /// Format a color into the textual representation for this format.
    ///
    /// ### Arguments
    /// - `color`: The color to format
    ///
    /// ### Returns
    /// - `String`: The formatted color string
    fn format(self, color: Hsla) -> String {
        match self {
            ColorInput::Hex => gpui_component::Colorize::to_hex(&color),
            ColorInput::Oklch => format_oklch(color),
            ColorInput::Hsla => format_hsla(color),
        }
    }

    /// Parse a textual color in this format into a color.
    ///
    /// ### Arguments
    /// - `value`: The string to parse
    ///
    /// ### Returns
    /// - `Some(Hsla)`: The parsed color
    /// - `None`: If the string is not valid for this format
    fn parse(self, value: &str) -> Option<Hsla> {
        match self {
            ColorInput::Hex => parse_hex(value),
            ColorInput::Oklch => parse_oklch(value),
            ColorInput::Hsla => parse_hsla(value),
        }
    }
}

/// State for the color picker bar.
pub struct ColorPickerBarState {
    pub color_picker_state: Entity<ColorPickerState>,
    pub show_color_picker: bool,
    pub hex_input: Entity<InputState>,
    pub oklch_input: Entity<InputState>,
    pub hsla_input: Entity<InputState>,
    pub(super) cached_hex: String,
    pub(super) cached_oklch: String,
    pub(super) cached_hsla: String,
    #[allow(dead_code, reason = "RAII guard: keeps the subscription alive")]
    color_picker_subscription: Subscription,
    #[allow(dead_code, reason = "RAII guard: keeps the subscription alive")]
    hex_input_subscription: Subscription,
    #[allow(dead_code, reason = "RAII guard: keeps the subscription alive")]
    oklch_input_subscription: Subscription,
    #[allow(dead_code, reason = "RAII guard: keeps the subscription alive")]
    hsla_input_subscription: Subscription,
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
        let initial_hex = gpui_component::Colorize::to_hex(&initial).clone();
        let initial_oklch = format_oklch(initial);
        let initial_hsla = format_hsla(initial);
        let hex_input = cx.new(|cx| InputState::new(window, cx).default_value(initial_hex.clone()));
        let oklch_input =
            cx.new(|cx| InputState::new(window, cx).default_value(initial_oklch.clone()));
        let hsla_input =
            cx.new(|cx| InputState::new(window, cx).default_value(initial_hsla.clone()));
        let color_picker_subscription = cx.subscribe_in(
            &color_picker_state,
            window,
            |this: &mut Fulgur, _, event: &ColorPickerEvent, window, cx| {
                if let ColorPickerEvent::Change(Some(color)) = event {
                    let color = *color;
                    let hex = gpui_component::Colorize::to_hex(&color);
                    let oklch = format_oklch(color);
                    let hsla_str = format_hsla(color);
                    this.color_picker_bar_state.cached_hex.clone_from(&hex);
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
        // On Change: update the color picker preview only.
        // On PressEnter / Blur: parse and sync all three inputs + the color picker.
        let hex_input_subscription = cx.subscribe_in(
            &hex_input,
            window,
            |this: &mut Fulgur, _, event: &InputEvent, window, cx| {
                Self::handle_input_event(this, ColorInput::Hex, event, window, cx);
            },
        );
        let oklch_input_subscription = cx.subscribe_in(
            &oklch_input,
            window,
            |this: &mut Fulgur, _, event: &InputEvent, window, cx| {
                Self::handle_input_event(this, ColorInput::Oklch, event, window, cx);
            },
        );
        let hsla_input_subscription = cx.subscribe_in(
            &hsla_input,
            window,
            |this: &mut Fulgur, _, event: &InputEvent, window, cx| {
                Self::handle_input_event(this, ColorInput::Hsla, event, window, cx);
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
            color_picker_subscription,
            hex_input_subscription,
            oklch_input_subscription,
            hsla_input_subscription,
        }
    }

    /// Handle a change event from one of the color text inputs.
    ///
    /// ### Arguments
    /// - `this`: The owning `Fulgur` instance
    /// - `source`: The input format that emitted the event
    /// - `event`: The input event to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    fn handle_input_event(
        this: &mut Fulgur,
        source: ColorInput,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Fulgur>,
    ) {
        let value = source.input(&this.color_picker_bar_state).read(cx).value();
        let Some(color) = source.parse(&value) else {
            return;
        };
        match event {
            InputEvent::Change => Self::apply_input_color(this, source, color, false, window, cx),
            InputEvent::PressEnter { .. } | InputEvent::Blur => {
                Self::apply_input_color(this, source, color, true, window, cx);
            }
            InputEvent::Focus => {}
        }
    }

    /// Apply a color parsed from one of the text inputs.
    ///
    /// ### Arguments
    /// - `this`: The owning `Fulgur` instance
    /// - `source`: The input format the color originated from
    /// - `color`: The parsed color to apply
    /// - `sync_siblings`: Whether to rewrite the other two inputs
    /// - `window`: The window context
    /// - `cx`: The application context
    fn apply_input_color(
        this: &mut Fulgur,
        source: ColorInput,
        color: Hsla,
        sync_siblings: bool,
        window: &mut Window,
        cx: &mut Context<Fulgur>,
    ) {
        this.color_picker_bar_state
            .color_picker_state
            .update(cx, |state, cx| {
                state.set_value(color, window, cx);
            });
        if !sync_siblings {
            return;
        }
        for target in ColorInput::ALL {
            if target == source {
                continue;
            }
            let formatted = target.format(color);
            target
                .input(&this.color_picker_bar_state)
                .update(cx, |state, cx| {
                    state.set_value(formatted, window, cx);
                });
        }
    }
}
