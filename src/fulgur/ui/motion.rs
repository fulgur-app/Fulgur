//! Application motion tokens and the helpers that apply them.

use std::time::Duration;

use gpui_kit::base::motion::{Easing, MotionReveal, Presence, PresenceSample, Transition};
use gpui_kit::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div};

/// Timing for small affordances that fade within an existing surface without
/// changing the layout around them.
pub const AFFORDANCE_DURATION: Duration = Duration::from_millis(120);

/// Timing for bars and panels that claim space in, or hand space back to, the
/// main window layout.
pub const SURFACE_DURATION: Duration = Duration::from_millis(160);

/// Build the transition used by small affordances.
///
/// ### Returns
/// - `Transition`: A decelerating transition lasting [`AFFORDANCE_DURATION`]
pub fn affordance() -> Transition {
    Transition::new(AFFORDANCE_DURATION).easing(Easing::EaseOut)
}

/// Build the transition used by surfaces that resize the main layout.
///
/// ### Returns
/// - `Transition`: A decelerating transition lasting [`SURFACE_DURATION`]
pub fn surface() -> Transition {
    Transition::new(SURFACE_DURATION).easing(Easing::EaseOut)
}

/// Sample the presence of a surface for the current frame.
///
/// ### Arguments
/// - `id`: Stable identity of the surface, used to key the transition state
/// - `present`: Whether the surface should be shown this frame
/// - `transition`: The timing policy to apply
/// - `window`: The window being rendered
/// - `cx`: The application context
///
/// ### Returns
/// - `PresenceSample`: The current phase and eased `0.0..=1.0` progress
pub fn presence(
    id: &'static str,
    present: bool,
    transition: Transition,
    window: &mut Window,
    cx: &mut App,
) -> PresenceSample {
    Presence::new(id, present)
        .transition(transition)
        .sample(window, cx)
}

/// Wrap a full width bar so it grows into the window layout when shown and
/// collapses out of it when hidden.
///
/// ### Arguments
/// - `id`: Stable identity of the bar, used to key both the transition and the reveal
/// - `present`: Whether the bar should be shown this frame
/// - `child`: Builds the bar, called only while the bar is on screen
/// - `window`: The window being rendered
/// - `cx`: The application context
///
/// ### Returns
/// - `Some(AnyElement)`: The wrapped bar, while it is entering, present or collapsing
/// - `None`: The bar is fully hidden and should not be rendered at all
pub fn revealed_bar(
    id: &'static str,
    present: bool,
    child: impl FnOnce() -> AnyElement,
    window: &mut Window,
    cx: &mut App,
) -> Option<AnyElement> {
    let sample = presence(id, present, surface(), window, cx);
    if !sample.should_render() {
        return None;
    }
    let progress = sample.progress;
    Some(
        div()
            .flex_none()
            .opacity(progress)
            .child(MotionReveal::new(id, progress, child()))
            .into_any_element(),
    )
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::{AFFORDANCE_DURATION, SURFACE_DURATION, presence, revealed_bar, surface};
    use gpui_kit::base::motion::PresencePhase;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Context, Entity, IntoElement, ParentElement, Render, TestAppContext,
        VisualTestContext, Window, WindowOptions, div,
    };
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    /// What one rendered frame observed about the surface under test.
    #[derive(Clone, Copy, Debug)]
    struct Frame {
        phase: PresencePhase,
        progress: f32,
        bar_rendered: bool,
    }

    /// A view that samples the motion helpers once per frame and records the result.
    struct MotionProbe {
        present: bool,
        frames: Rc<RefCell<Vec<Frame>>>,
    }

    impl Render for MotionProbe {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let sample = presence("probe", self.present, surface(), window, cx);
            let bar = revealed_bar(
                "probe-bar",
                self.present,
                || div().into_any_element(),
                window,
                cx,
            );
            self.frames.borrow_mut().push(Frame {
                phase: sample.phase,
                progress: sample.progress,
                bar_rendered: bar.is_some(),
            });
            div().children(bar)
        }
    }

    /// Open a window hosting a probe, with motion either reduced or fully enabled.
    ///
    /// ### Arguments
    /// - `cx`: The test application context
    /// - `reduce_motion`: Whether the platform reports a reduced motion preference
    ///
    /// ### Returns
    /// - `(Entity<MotionProbe>, Rc<RefCell<Vec<Frame>>>, VisualTestContext)`: The probe, the
    ///   frames it has recorded so far and the visual context driving it
    fn setup_probe(
        cx: &mut TestAppContext,
        reduce_motion: bool,
    ) -> (
        Entity<MotionProbe>,
        Rc<RefCell<Vec<Frame>>>,
        VisualTestContext,
    ) {
        let frames: Rc<RefCell<Vec<Frame>>> = Rc::new(RefCell::new(Vec::new()));
        let probe_slot: RefCell<Option<Entity<MotionProbe>>> = RefCell::new(None);
        let window = cx
            .update(|cx| {
                gpui_kit::init(cx);
                cx.set_reduce_motion(reduce_motion);
                cx.open_window(WindowOptions::default(), |_, cx| {
                    let probe = cx.new(|_| MotionProbe {
                        present: false,
                        frames: Rc::clone(&frames),
                    });
                    *probe_slot.borrow_mut() = Some(probe.clone());
                    probe
                })
            })
            .expect("failed to open test window");
        let visual_cx = VisualTestContext::from_window(window.into(), cx);
        visual_cx.run_until_parked();
        let probe = probe_slot
            .into_inner()
            .expect("failed to capture the probe entity");
        frames.borrow_mut().clear();
        (probe, frames, visual_cx)
    }

    /// Set the probe's target presence and draw one frame.
    ///
    /// ### Arguments
    /// - `probe`: The probe under test
    /// - `present`: The new target presence
    /// - `visual_cx`: The visual context driving the window
    fn set_present(probe: &Entity<MotionProbe>, present: bool, visual_cx: &mut VisualTestContext) {
        visual_cx.update(|window, cx| {
            probe.update(cx, |probe, cx| {
                probe.present = present;
                cx.notify();
            });
            window.render_frame(cx);
        });
    }

    /// Advance the test clock and draw one frame.
    ///
    /// ### Arguments
    /// - `elapsed`: How far to move the clock forward
    /// - `cx`: The test application context owning the clock
    /// - `visual_cx`: The visual context driving the window
    fn advance(elapsed: Duration, cx: &TestAppContext, visual_cx: &mut VisualTestContext) {
        cx.executor().advance_clock(elapsed);
        visual_cx.update(gpui_kit::test::TestWindowExt::render_frame);
    }

    /// Read the most recently recorded frame.
    ///
    /// ### Arguments
    /// - `frames`: The recording the probe appends to
    ///
    /// ### Returns
    /// - `Frame`: The last frame the probe rendered
    fn last(frames: &Rc<RefCell<Vec<Frame>>>) -> Frame {
        *frames
            .borrow()
            .last()
            .expect("the probe rendered no frames")
    }

    #[gpui_kit::test]
    fn test_reduced_motion_shows_and_hides_a_bar_instantly(cx: &mut TestAppContext) {
        let (probe, frames, mut visual_cx) = setup_probe(cx, true);

        set_present(&probe, true, &mut visual_cx);
        let shown = last(&frames);
        assert_eq!(shown.phase, PresencePhase::Present);
        assert!((shown.progress - 1.0).abs() < f32::EPSILON);
        assert!(shown.bar_rendered);

        set_present(&probe, false, &mut visual_cx);
        let hidden = last(&frames);
        assert_eq!(hidden.phase, PresencePhase::Absent);
        assert!(!hidden.bar_rendered);
    }

    #[gpui_kit::test]
    fn test_a_bar_reveals_over_the_surface_duration(cx: &mut TestAppContext) {
        let (probe, frames, mut visual_cx) = setup_probe(cx, false);

        set_present(&probe, true, &mut visual_cx);
        advance(SURFACE_DURATION / 2, cx, &mut visual_cx);
        let midway = last(&frames);
        assert_eq!(midway.phase, PresencePhase::Entering);
        assert!(
            midway.progress > 0.0 && midway.progress < 1.0,
            "expected a partial reveal, got {}",
            midway.progress
        );
        assert!(midway.bar_rendered);

        advance(SURFACE_DURATION, cx, &mut visual_cx);
        let settled = last(&frames);
        assert_eq!(settled.phase, PresencePhase::Present);
        assert!((settled.progress - 1.0).abs() < f32::EPSILON);
    }

    #[gpui_kit::test]
    fn test_a_hidden_bar_keeps_rendering_while_it_collapses(cx: &mut TestAppContext) {
        let (probe, frames, mut visual_cx) = setup_probe(cx, false);
        set_present(&probe, true, &mut visual_cx);
        advance(SURFACE_DURATION * 2, cx, &mut visual_cx);

        set_present(&probe, false, &mut visual_cx);
        advance(SURFACE_DURATION / 2, cx, &mut visual_cx);
        let collapsing = last(&frames);
        assert_eq!(collapsing.phase, PresencePhase::Exiting);
        assert!(
            collapsing.progress > 0.0 && collapsing.progress < 1.0,
            "expected a partial collapse, got {}",
            collapsing.progress
        );
        assert!(
            collapsing.bar_rendered,
            "a collapsing bar must stay mounted until its transition ends"
        );

        advance(SURFACE_DURATION, cx, &mut visual_cx);
        let gone = last(&frames);
        assert_eq!(gone.phase, PresencePhase::Absent);
        assert!(!gone.bar_rendered);
    }

    #[gpui_kit::test]
    fn test_reversing_mid_transition_resumes_instead_of_snapping(cx: &mut TestAppContext) {
        let (probe, frames, mut visual_cx) = setup_probe(cx, false);

        set_present(&probe, true, &mut visual_cx);
        advance(SURFACE_DURATION / 2, cx, &mut visual_cx);
        let interrupted_at = last(&frames).progress;

        set_present(&probe, false, &mut visual_cx);
        let reversed = last(&frames);
        assert_eq!(reversed.phase, PresencePhase::Exiting);
        assert!(
            reversed.progress > 0.0 && reversed.progress <= interrupted_at,
            "reversal should resume from {interrupted_at}, got {}",
            reversed.progress
        );
        assert!(reversed.bar_rendered);
    }

    #[gpui_kit::test]
    fn test_affordances_settle_sooner_than_surfaces(cx: &mut TestAppContext) {
        assert!(AFFORDANCE_DURATION < SURFACE_DURATION);
        let _ = cx;
    }
}
