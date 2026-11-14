use gpui_kit::component::{
    AxisExt, Sizable,
    input::{InputEvent, InputState, NumberInput, NumberInputEvent, StepAction},
    setting::{NumberFieldOptions, SettingField},
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{App, AppContext as _, Context, Entity, SharedString, Styled, Subscription};
use std::rc::Rc;

type SetValFn = Rc<dyn Fn(f64, &mut App)>;

struct NumberInputState {
    input: Entity<InputState>,
    _subs: Vec<Subscription>,
}

/// Convert a slider `f64` value to a `f32` font size.
///
/// The slider API always provides `f64`; font sizes in the UI range (8–24) are
/// well within `f32` range, so the narrowing cast is safe at this boundary.
#[allow(clippy::cast_possible_truncation)]
pub(super) fn slider_val_to_font_size(val: f64) -> f32 {
    val as f32
}

/// Convert a slider `f64` value to a `usize` tab size.
///
/// The slider API always provides `f64`; tab sizes in the UI range (2–12) are
/// non-negative integers well within `usize` range, so the cast is safe here.
#[allow(clippy::cast_possible_truncation)]
#[allow(clippy::cast_sign_loss)]
pub(super) fn slider_val_to_tab_size(val: f64) -> usize {
    val as usize
}

/// Create a number input setting field that correctly propagates both step-button
/// clicks and manual text edits to the entity.
///
/// gpui-component's built-in `SettingField::number_input` suppresses
/// `InputEvent::Change` when step buttons are clicked (via `InputState::set_value`
/// which sets `emit_events = false`), so the entity setter is never called for
/// step interactions. This helper replicates the field while explicitly calling the
/// setter on each step.
///
/// ### Arguments
/// - `state_key`: Unique key for `use_keyed_state`
/// - `options`: Step / min / max configuration
/// - `get_val`: Reads the current value from the application state
/// - `set_val`: Writes an updated value into the application state
///
/// ### Returns
/// - `SettingField<SharedString>`: A correctly-wired number input field
pub(super) fn make_number_field(
    state_key: SharedString,
    options: &NumberFieldOptions,
    get_val: impl Fn(&App) -> f64 + 'static,
    set_val: impl Fn(f64, &mut App) + 'static,
) -> SettingField<SharedString> {
    let options = options.clone();
    let get_val = Rc::new(get_val);
    let set_val: SetValFn = Rc::new(set_val);

    SettingField::render(move |options_render, window, cx| {
        let current_value = (get_val)(cx);
        let min = options.min;
        let max = options.max;
        let step = options.step;
        let set_val_step = set_val.clone();
        let set_val_change = set_val.clone();

        let state = window.use_keyed_state(
            state_key.clone(),
            cx,
            |window, cx: &mut Context<NumberInputState>| {
                let input = cx
                    .new(|cx| InputState::new(window, cx).default_value(current_value.to_string()));
                let subs = vec![
                    cx.subscribe_in(&input, window, {
                        let setter = set_val_step.clone();
                        move |_, inp, event: &NumberInputEvent, window, cx| {
                            let NumberInputEvent::Step(action) = event;
                            inp.update(cx, |inp: &mut InputState, cx| {
                                if let Ok(v) = inp.value().parse::<f64>() {
                                    let new_v = (if *action == StepAction::Increment {
                                        v + step
                                    } else {
                                        v - step
                                    })
                                    .clamp(min, max);
                                    inp.set_value(
                                        SharedString::from(new_v.to_string()),
                                        window,
                                        cx,
                                    );
                                    setter(new_v, cx);
                                }
                            });
                        }
                    }),
                    cx.subscribe_in(&input, window, {
                        move |_, inp, event: &InputEvent, _, cx| {
                            if let InputEvent::Change = event {
                                inp.update(cx, |inp: &mut InputState, cx| {
                                    if let Ok(v) = inp.value().parse::<f64>() {
                                        set_val_change(v.clamp(min, max), cx);
                                    }
                                });
                            }
                        }
                    }),
                ];
                NumberInputState { input, _subs: subs }
            },
        );

        let size = options_render.size();
        let is_horizontal = options_render.layout().is_horizontal();
        let input_entity = state.read(cx).input.clone();
        NumberInput::new(&input_entity).with_size(size).map(|this| {
            if is_horizontal {
                this.w_32()
            } else {
                this.w_full()
            }
        })
    })
}
