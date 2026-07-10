use crate::fulgur::Fulgur;

use gpui::{
    App, AppContext, Context, Entity, EntityInputHandler, EventEmitter, Hsla, Subscription,
    WeakEntity, Window,
};
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
    /// - `bar`: The color picker bar holding the inputs
    ///
    /// ### Returns
    /// - `&Entity<InputState>`: The input entity for this format
    fn input(self, bar: &ColorPickerBar) -> &Entity<InputState> {
        match self {
            ColorInput::Hex => &bar.hex_input,
            ColorInput::Oklch => &bar.oklch_input,
            ColorInput::Hsla => &bar.hsla_input,
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

/// The color picker bar, rendered as its own entity
pub(crate) struct ColorPickerBar {
    pub(super) fulgur: WeakEntity<Fulgur>,
    pub(super) show_color_picker: bool,
    pub(super) color_picker_state: Entity<ColorPickerState>,
    pub(super) hex_input: Entity<InputState>,
    pub(super) oklch_input: Entity<InputState>,
    pub(super) hsla_input: Entity<InputState>,
    pub(super) cached_hex: String,
    pub(super) cached_oklch: String,
    pub(super) cached_hsla: String,
    _color_picker_subscription: Subscription,
    _hex_input_subscription: Subscription,
    _oklch_input_subscription: Subscription,
    _hsla_input_subscription: Subscription,
}

/// Typed events emitted by the color picker bar toward the owning `Fulgur` window
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ColorPickerBarEvent {
    Closed,
    ToggleHighlightColors,
}

impl EventEmitter<ColorPickerBarEvent> for ColorPickerBar {}

impl ColorPickerBar {
    /// Create a new color picker bar view owning the picker and its three text inputs
    ///
    /// ### Arguments
    /// - `fulgur`: Weak handle to the owning window entity the bar reads the active editor from
    /// - `window`: The window context
    /// - `cx`: The color picker bar context
    ///
    /// ### Returns
    /// - `ColorPickerBar`: The new color picker bar view initialized to white
    pub(crate) fn new(
        fulgur: WeakEntity<Fulgur>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
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
            |this: &mut Self, _, event: &ColorPickerEvent, window, cx| {
                if let ColorPickerEvent::Change(Some(color)) = event {
                    let color = *color;
                    let hex = gpui_component::Colorize::to_hex(&color);
                    let oklch = format_oklch(color);
                    let hsla_str = format_hsla(color);
                    this.cached_hex.clone_from(&hex);
                    this.cached_oklch.clone_from(&oklch);
                    this.cached_hsla.clone_from(&hsla_str);
                    this.hex_input.update(cx, |state, cx| {
                        state.set_value(hex, window, cx);
                    });
                    this.oklch_input.update(cx, |state, cx| {
                        state.set_value(oklch, window, cx);
                    });
                    this.hsla_input.update(cx, |state, cx| {
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
            |this: &mut Self, _, event: &InputEvent, window, cx| {
                this.handle_input_event(ColorInput::Hex, event, window, cx);
            },
        );
        let oklch_input_subscription = cx.subscribe_in(
            &oklch_input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| {
                this.handle_input_event(ColorInput::Oklch, event, window, cx);
            },
        );
        let hsla_input_subscription = cx.subscribe_in(
            &hsla_input,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| {
                this.handle_input_event(ColorInput::Hsla, event, window, cx);
            },
        );
        Self {
            fulgur,
            show_color_picker: false,
            color_picker_state,
            hex_input,
            oklch_input,
            hsla_input,
            cached_hex: initial_hex,
            cached_oklch: initial_oklch,
            cached_hsla: initial_hsla,
            _color_picker_subscription: color_picker_subscription,
            _hex_input_subscription: hex_input_subscription,
            _oklch_input_subscription: oklch_input_subscription,
            _hsla_input_subscription: hsla_input_subscription,
        }
    }

    /// Whether the color picker bar is currently shown
    ///
    /// ### Returns
    /// - `bool`: True if the bar is visible
    pub(crate) fn is_visible(&self) -> bool {
        self.show_color_picker
    }

    /// Toggle the color picker bar visibility
    ///
    /// ### Arguments
    /// - `cx`: The color picker bar context
    pub(crate) fn toggle(&mut self, cx: &mut Context<Self>) {
        self.show_color_picker = !self.show_color_picker;
        cx.notify();
    }

    /// Hide the color picker bar and tell the owning window to unmount it
    ///
    /// ### Arguments
    /// - `cx`: The color picker bar context
    pub(super) fn close(&mut self, cx: &mut Context<Self>) {
        self.show_color_picker = false;
        cx.emit(ColorPickerBarEvent::Closed);
        cx.notify();
    }

    /// Get the active editor's content entity from the owning window
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(Entity<InputState>)`: The active editor tab's content
    /// - `None`: If the window is gone or the active tab is not an editor
    fn active_editor_content(&self, cx: &App) -> Option<Entity<InputState>> {
        let fulgur = self.fulgur.upgrade()?;
        fulgur
            .read(cx)
            .get_active_editor_tab()
            .map(|editor_tab| editor_tab.content.clone())
    }

    /// Insert a value at the cursor position in the active editor tab, replacing the current selection if any.
    ///
    /// ### Arguments
    /// - `value`: The string to insert
    /// - `window`: The window context
    /// - `cx`: The color picker bar context
    pub(super) fn insert_color_value(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(content) = self.active_editor_content(cx) {
            content.update(cx, |input_state, cx| {
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

    /// Handle a change event from one of the color text inputs.
    ///
    /// ### Arguments
    /// - `source`: The input format that emitted the event
    /// - `event`: The input event to handle
    /// - `window`: The window context
    /// - `cx`: The color picker bar context
    fn handle_input_event(
        &mut self,
        source: ColorInput,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = source.input(self).read(cx).value();
        let Some(color) = source.parse(&value) else {
            return;
        };
        match event {
            InputEvent::Change => self.apply_input_color(source, color, false, window, cx),
            InputEvent::PressEnter { .. } | InputEvent::Blur => {
                self.apply_input_color(source, color, true, window, cx);
            }
            InputEvent::Focus => {}
        }
    }

    /// Apply a color parsed from one of the text inputs.
    ///
    /// ### Arguments
    /// - `source`: The input format the color originated from
    /// - `color`: The parsed color to apply
    /// - `sync_siblings`: Whether to rewrite the other two inputs
    /// - `window`: The window context
    /// - `cx`: The color picker bar context
    fn apply_input_color(
        &mut self,
        source: ColorInput,
        color: Hsla,
        sync_siblings: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.color_picker_state.update(cx, |state, cx| {
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
            target.input(self).update(cx, |state, cx| {
                state.set_value(formatted, window, cx);
            });
        }
    }
}

impl Fulgur {
    /// Dispatch a color picker bar event to the matching window-level handler
    ///
    /// ### Arguments
    /// - `event`: The color picker bar event to handle
    /// - `_window`: The window context
    /// - `cx`: The application context
    pub(crate) fn on_color_picker_bar_event(
        &mut self,
        event: ColorPickerBarEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ColorPickerBarEvent::Closed => cx.notify(),
            ColorPickerBarEvent::ToggleHighlightColors => {
                self.settings.editor_settings.highlight_colors =
                    !self.settings.editor_settings.highlight_colors;
                let _ = self.update_and_propagate_settings(cx);
            }
        }
    }

    /// Toggle the color picker bar visibility.
    ///
    /// ### Arguments
    /// - `_window`: The window context
    /// - `cx`: The application context
    pub fn toggle_color_picker(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.color_picker_bar.update(cx, ColorPickerBar::toggle);
        cx.notify();
    }
}
