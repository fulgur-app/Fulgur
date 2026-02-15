use std::sync::mpsc::Receiver;

/// Collect all available events from an optional receiver
///
/// This helper drains all pending events from a channel using `try_recv()`.
/// It's used for both file watch events and SSE events to avoid code duplication.
///
/// ### Arguments
/// - `receiver`: Optional reference to a receiver channel
///
/// ### Returns
/// - `Vec<T>`: Vector containing all available events, or empty vec if receiver is None
pub fn collect_events<T>(receiver: &Option<Receiver<T>>) -> Vec<T> {
    if let Some(rx) = receiver {
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        events
    } else {
        Vec::new()
    }
}

/// Macro to simplify action handler registration
///
/// This macro reduces boilerplate for simple action handlers that just call a method.
/// Instead of writing:
/// ```ignore
/// .on_action(cx.listener(|this, _action: &ActionType, window, cx| {
///     this.method(window, cx);
/// }))
/// ```
/// You can write:
/// ```ignore
/// register_action!(div, cx, ActionType => method)
/// ```
#[macro_export]
macro_rules! register_action {
    // Simple action → method call (uses window, cx)
    ($div:expr, $cx:expr, $action:ty => $method:ident) => {
        $div = $div.on_action($cx.listener(|this, _: &$action, window, cx| {
            this.$method(window, cx);
        }));
    };
    // Simple action → method call (uses only cx, no window)
    ($div:expr, $cx:expr, $action:ty => $method:ident(cx_only)) => {
        $div = $div.on_action($cx.listener(|this, _: &$action, _window, cx| {
            this.$method(cx);
        }));
    };
    // Action with parameter → method call with extracted param
    ($div:expr, $cx:expr, $action:ty => $method:ident($param:ident)) => {
        $div = $div.on_action($cx.listener(|this, action: &$action, window, cx| {
            this.$method(window, cx, action.$param.clone());
        }));
    };
    // Action with tuple struct .0 → method call with extracted param (window, cx, param)
    ($div:expr, $cx:expr, $action:ty => $method:ident(.0)) => {
        $div = $div.on_action($cx.listener(|this, action: &$action, window, cx| {
            this.$method(window, cx, action.0.clone());
        }));
    };
    // Action with tuple struct .0 → method call with extracted param (param, cx) - no window
    ($div:expr, $cx:expr, $action:ty => $method:ident(.0, no_window)) => {
        $div = $div.on_action($cx.listener(|this, action: &$action, _window, cx| {
            this.$method(action.0.clone(), cx);
        }));
    };
    // Action with action reference passed to method (action, window, cx)
    ($div:expr, $cx:expr, $action:ty => $method:ident(&action)) => {
        $div = $div.on_action($cx.listener(|this, action: &$action, window, cx| {
            this.$method(action, window, cx);
        }));
    };
    // Static function call with no parameters
    ($div:expr, $cx:expr, $action:ty => call_no_args $func:path) => {
        $div = $div.on_action($cx.listener(|_, _: &$action, _window, _cx| {
            $func();
        }));
    };
    // Static function call with (window, cx) parameters
    ($div:expr, $cx:expr, $action:ty => call $func:path) => {
        $div = $div.on_action($cx.listener(|_, _: &$action, window, cx| {
            $func(window, cx);
        }));
    };
}
