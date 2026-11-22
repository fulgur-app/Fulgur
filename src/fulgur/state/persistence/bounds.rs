use gpui_kit::{App, Bounds, DisplayId, Pixels, WindowBounds};
use serde::{Deserialize, Serialize};

/// Smallest overlap, in pixels along each axis, between a restored window and its
/// display for the saved origin to be kept: enough of the window to grab and drag it.
const MIN_REACHABLE_EXTENT: f32 = 64.0;

/// Serialized window bounds that can be saved to JSON
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SerializedWindowBounds {
    /// Window state: "Windowed", "Maximized", or "Fullscreen"
    pub state: String,
    /// X position of window origin in pixels
    pub x: f32,
    /// Y position of window origin in pixels
    pub y: f32,
    /// Window width in pixels
    pub width: f32,
    /// Window height in pixels
    pub height: f32,
    /// Display ID (monitor) where the window was located
    #[serde(default)]
    pub display_id: Option<u32>,
}

impl Default for SerializedWindowBounds {
    /// Default values for serialized window bounds
    ///
    /// ### Returns
    /// - `SerializedWindowBounds`: The default serialized window bounds
    fn default() -> Self {
        Self {
            state: "Windowed".to_string(),
            x: 100.0,
            y: 100.0,
            width: 1200.0,
            height: 800.0,
            display_id: None,
        }
    }
}

impl SerializedWindowBounds {
    /// Convert GPUI `WindowBounds` to `SerializedWindowBounds`
    ///
    /// ### Arguments
    /// - `bounds`: The GPUI `WindowBounds` to convert
    /// - `display_id`: Optional display ID (monitor) for the window
    ///
    /// ### Returns
    /// - `SerializedWindowBounds`: The serialized window bounds
    #[must_use]
    pub fn from_gpui_bounds(bounds: WindowBounds, display_id: Option<u32>) -> Self {
        match bounds {
            WindowBounds::Windowed(rect) => Self {
                state: "Windowed".to_string(),
                x: rect.origin.x.into(),
                y: rect.origin.y.into(),
                width: rect.size.width.into(),
                height: rect.size.height.into(),
                display_id,
            },
            WindowBounds::Maximized(rect) => Self {
                state: "Maximized".to_string(),
                x: rect.origin.x.into(),
                y: rect.origin.y.into(),
                width: rect.size.width.into(),
                height: rect.size.height.into(),
                display_id,
            },
            WindowBounds::Fullscreen(rect) => Self {
                state: "Fullscreen".to_string(),
                x: rect.origin.x.into(),
                y: rect.origin.y.into(),
                width: rect.size.width.into(),
                height: rect.size.height.into(),
                display_id,
            },
        }
    }

    /// Convert `SerializedWindowBounds` to GPUI `WindowBounds`
    ///
    /// ### Returns
    /// - `gpui_kit::WindowBounds`: The GPUI window bounds
    #[must_use]
    pub fn to_gpui_bounds(&self) -> WindowBounds {
        self.with_window_state(self.rect())
    }

    /// Resolve the bounds and the display a restored window opens on
    ///
    /// ### Arguments
    /// - `cx`: The application context, used to list the connected displays
    ///
    /// ### Returns
    /// - `(WindowBounds, Option<DisplayId>)`: The bounds to open the window with,
    ///   and the saved display when it is still connected (`None` lets the
    ///   platform pick the primary display)
    #[must_use]
    pub fn restore_placement(&self, cx: &App) -> (WindowBounds, Option<DisplayId>) {
        let saved_display = self.display_id.and_then(|saved_id| {
            cx.displays()
                .into_iter()
                .find(|display| u64::from(display.id()) == u64::from(saved_id))
        });
        let saved_display_lost = self.display_id.is_some() && saved_display.is_none();
        let display_id = saved_display.as_ref().map(|display| display.id());
        let window_bounds = match saved_display.or_else(|| cx.primary_display()) {
            Some(display) => self.fit_to_display(
                saved_display_lost,
                display.bounds(),
                display.visible_bounds(),
            ),
            None => self.to_gpui_bounds(),
        };
        (window_bounds, display_id)
    }

    /// Fit the saved rectangle onto the display the window will open on
    ///
    /// ### Arguments
    /// - `saved_display_lost`: Whether the display the window was saved on is gone,
    ///   which makes the saved origin meaningless on the target display
    /// - `display_bounds`: Bounds of the target display
    /// - `visible_bounds`: Usable area of the target display, without dock or taskbar
    ///
    /// ### Returns
    /// - `WindowBounds`: The saved bounds when they are reachable on the display,
    ///   otherwise the saved size, clamped to the usable area, centered in it
    fn fit_to_display(
        &self,
        saved_display_lost: bool,
        display_bounds: Bounds<Pixels>,
        visible_bounds: Bounds<Pixels>,
    ) -> WindowBounds {
        let rect = self.rect();
        if !saved_display_lost && is_reachable_on(&rect, &display_bounds) {
            return self.with_window_state(rect);
        }
        let centered =
            Bounds::centered_at(visible_bounds.center(), rect.size.min(&visible_bounds.size));
        self.with_window_state(centered)
    }

    /// Build the saved window rectangle
    ///
    /// ### Returns
    /// - `Bounds<Pixels>`: The saved origin and size
    fn rect(&self) -> Bounds<Pixels> {
        use gpui_kit::{point, px, size};
        Bounds {
            origin: point(px(self.x), px(self.y)),
            size: size(px(self.width), px(self.height)),
        }
    }

    /// Wrap a rectangle in the saved window state
    ///
    /// ### Arguments
    /// - `rect`: The window rectangle (the restore rectangle when maximized or fullscreen)
    ///
    /// ### Returns
    /// - `WindowBounds`: The rectangle in the saved state, `Windowed` for unknown states
    fn with_window_state(&self, rect: Bounds<Pixels>) -> WindowBounds {
        match self.state.as_str() {
            "Maximized" => WindowBounds::Maximized(rect),
            "Fullscreen" => WindowBounds::Fullscreen(rect),
            _ => WindowBounds::Windowed(rect),
        }
    }
}

/// Whether enough of a window overlaps a display to be grabbed and moved
///
/// ### Arguments
/// - `rect`: The window rectangle
/// - `display_bounds`: Bounds of the display, in the same coordinate space
///
/// ### Returns
/// - `bool`: `true` when the overlap reaches `MIN_REACHABLE_EXTENT` along both axes
fn is_reachable_on(rect: &Bounds<Pixels>, display_bounds: &Bounds<Pixels>) -> bool {
    if !rect.intersects(display_bounds) {
        return false;
    }
    let overlap = rect.intersect(display_bounds).size;
    let min_width = MIN_REACHABLE_EXTENT.min(f32::from(rect.size.width));
    let min_height = MIN_REACHABLE_EXTENT.min(f32::from(rect.size.height));
    f32::from(overlap.width) >= min_width && f32::from(overlap.height) >= min_height
}

#[cfg(test)]
mod tests {
    use super::SerializedWindowBounds;

    /// Assert the geometry values of a GPUI window bounds rectangle.
    ///
    /// ### Parameters
    /// - `bounds`: The GPUI window bounds to inspect.
    /// - `expected_x`: Expected x origin in pixels.
    /// - `expected_y`: Expected y origin in pixels.
    /// - `expected_width`: Expected width in pixels.
    /// - `expected_height`: Expected height in pixels.
    fn assert_gpui_bounds_geometry(
        bounds: &gpui_kit::WindowBounds,
        expected_x: f32,
        expected_y: f32,
        expected_width: f32,
        expected_height: f32,
    ) {
        use gpui_kit::WindowBounds;
        let rect = match bounds {
            WindowBounds::Windowed(rect)
            | WindowBounds::Maximized(rect)
            | WindowBounds::Fullscreen(rect) => rect,
        };
        assert!((f32::from(rect.origin.x) - expected_x).abs() < f32::EPSILON);
        assert!((f32::from(rect.origin.y) - expected_y).abs() < f32::EPSILON);
        assert!((f32::from(rect.size.width) - expected_width).abs() < f32::EPSILON);
        assert!((f32::from(rect.size.height) - expected_height).abs() < f32::EPSILON);
    }

    #[test]
    fn test_serialized_window_bounds_from_gpui_windowed_preserves_geometry_and_display() {
        use gpui_kit::{Bounds, WindowBounds, point, px, size};
        let gpui_bounds = WindowBounds::Windowed(Bounds {
            origin: point(px(120.0), px(80.0)),
            size: size(px(1440.0), px(900.0)),
        });
        let serialized = SerializedWindowBounds::from_gpui_bounds(gpui_bounds, Some(7));
        assert_eq!(serialized.state, "Windowed");
        assert!((serialized.x - 120.0_f32).abs() < f32::EPSILON);
        assert!((serialized.y - 80.0_f32).abs() < f32::EPSILON);
        assert!((serialized.width - 1440.0_f32).abs() < f32::EPSILON);
        assert!((serialized.height - 900.0_f32).abs() < f32::EPSILON);
        assert_eq!(serialized.display_id, Some(7));
    }

    #[test]
    fn test_serialized_window_bounds_from_gpui_maximized_preserves_geometry_and_display() {
        use gpui_kit::{Bounds, WindowBounds, point, px, size};
        let gpui_bounds = WindowBounds::Maximized(Bounds {
            origin: point(px(0.0), px(0.0)),
            size: size(px(1920.0), px(1080.0)),
        });
        let serialized = SerializedWindowBounds::from_gpui_bounds(gpui_bounds, Some(2));
        assert_eq!(serialized.state, "Maximized");
        assert!((serialized.x - 0.0_f32).abs() < f32::EPSILON);
        assert!((serialized.y - 0.0_f32).abs() < f32::EPSILON);
        assert!((serialized.width - 1920.0_f32).abs() < f32::EPSILON);
        assert!((serialized.height - 1080.0_f32).abs() < f32::EPSILON);
        assert_eq!(serialized.display_id, Some(2));
    }

    #[test]
    fn test_serialized_window_bounds_from_gpui_fullscreen_preserves_geometry_and_display() {
        use gpui_kit::{Bounds, WindowBounds, point, px, size};
        let gpui_bounds = WindowBounds::Fullscreen(Bounds {
            origin: point(px(10.0), px(20.0)),
            size: size(px(2560.0), px(1440.0)),
        });
        let serialized = SerializedWindowBounds::from_gpui_bounds(gpui_bounds, None);
        assert_eq!(serialized.state, "Fullscreen");
        assert!((serialized.x - 10.0_f32).abs() < f32::EPSILON);
        assert!((serialized.y - 20.0_f32).abs() < f32::EPSILON);
        assert!((serialized.width - 2560.0_f32).abs() < f32::EPSILON);
        assert!((serialized.height - 1440.0_f32).abs() < f32::EPSILON);
        assert_eq!(serialized.display_id, None);
    }

    #[test]
    fn test_serialized_window_bounds_to_gpui_bounds_preserves_geometry_for_each_state() {
        use gpui_kit::WindowBounds;
        let cases = [
            (
                SerializedWindowBounds {
                    state: "Windowed".to_string(),
                    x: 11.0,
                    y: 22.0,
                    width: 1280.0,
                    height: 720.0,
                    display_id: Some(1),
                },
                "Windowed",
            ),
            (
                SerializedWindowBounds {
                    state: "Maximized".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 1920.0,
                    height: 1080.0,
                    display_id: Some(2),
                },
                "Maximized",
            ),
            (
                SerializedWindowBounds {
                    state: "Fullscreen".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 2560.0,
                    height: 1440.0,
                    display_id: None,
                },
                "Fullscreen",
            ),
        ];
        for (serialized, expected_state) in cases {
            let gpui_bounds = serialized.to_gpui_bounds();
            match (expected_state, &gpui_bounds) {
                ("Windowed", WindowBounds::Windowed(_))
                | ("Maximized", WindowBounds::Maximized(_))
                | ("Fullscreen", WindowBounds::Fullscreen(_)) => {}
                _ => panic!("unexpected WindowBounds variant for state {expected_state}"),
            }
            assert_gpui_bounds_geometry(
                &gpui_bounds,
                serialized.x,
                serialized.y,
                serialized.width,
                serialized.height,
            );
        }
    }

    #[test]
    fn test_serialized_window_bounds_to_gpui_bounds_unknown_state_defaults_to_windowed() {
        use gpui_kit::WindowBounds;
        let serialized = SerializedWindowBounds {
            state: "UnknownState".to_string(),
            x: 40.0,
            y: 50.0,
            width: 900.0,
            height: 700.0,
            display_id: None,
        };
        let gpui_bounds = serialized.to_gpui_bounds();
        assert!(
            matches!(gpui_bounds, WindowBounds::Windowed(_)),
            "unknown window state should default to Windowed bounds"
        );
        assert_gpui_bounds_geometry(
            &gpui_bounds,
            serialized.x,
            serialized.y,
            serialized.width,
            serialized.height,
        );
    }

    /// Build windowed bounds saved on a display.
    ///
    /// ### Arguments
    /// - `x`: Saved x origin in pixels.
    /// - `y`: Saved y origin in pixels.
    /// - `width`: Saved width in pixels.
    /// - `height`: Saved height in pixels.
    /// - `display_id`: The display the window was saved on.
    ///
    /// ### Returns
    /// - `SerializedWindowBounds`: The saved windowed bounds.
    fn windowed(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        display_id: Option<u32>,
    ) -> SerializedWindowBounds {
        SerializedWindowBounds {
            state: "Windowed".to_string(),
            x,
            y,
            width,
            height,
            display_id,
        }
    }

    /// Build the bounds of a 1920x1080 display at the given origin.
    ///
    /// ### Arguments
    /// - `x`: The display x origin in pixels.
    /// - `y`: The display y origin in pixels.
    ///
    /// ### Returns
    /// - `Bounds<Pixels>`: The display bounds.
    fn full_hd_display_at(x: f32, y: f32) -> gpui_kit::Bounds<gpui_kit::Pixels> {
        use gpui_kit::{Bounds, point, px, size};
        Bounds {
            origin: point(px(x), px(y)),
            size: size(px(1920.0), px(1080.0)),
        }
    }

    #[test]
    fn fit_to_display_keeps_bounds_that_are_reachable_on_their_display() {
        let display = full_hd_display_at(0.0, 0.0);
        let saved = windowed(1800.0, 900.0, 1200.0, 800.0, Some(1));
        let fitted = saved.fit_to_display(false, display, display);
        assert_gpui_bounds_geometry(&fitted, 1800.0, 900.0, 1200.0, 800.0);
    }

    #[test]
    fn fit_to_display_centers_a_window_that_lies_off_its_display() {
        let display = full_hd_display_at(0.0, 0.0);
        let saved = windowed(2600.0, 200.0, 1200.0, 800.0, Some(1));
        let fitted = saved.fit_to_display(false, display, display);
        assert_gpui_bounds_geometry(&fitted, 360.0, 140.0, 1200.0, 800.0);
    }

    #[test]
    fn fit_to_display_centers_a_window_with_only_a_sliver_on_screen() {
        let display = full_hd_display_at(0.0, 0.0);
        let saved = windowed(1900.0, 100.0, 1200.0, 800.0, Some(1));
        let fitted = saved.fit_to_display(false, display, display);
        assert_gpui_bounds_geometry(&fitted, 360.0, 140.0, 1200.0, 800.0);
    }

    #[test]
    fn fit_to_display_drops_the_origin_of_a_window_whose_display_is_gone() {
        let display = full_hd_display_at(0.0, 0.0);
        let saved = windowed(100.0, 100.0, 1200.0, 800.0, Some(2));
        let fitted = saved.fit_to_display(true, display, display);
        assert_gpui_bounds_geometry(&fitted, 360.0, 140.0, 1200.0, 800.0);
    }

    #[test]
    fn fit_to_display_clamps_a_window_larger_than_the_usable_area() {
        use gpui_kit::{Bounds, point, px, size};
        let display = full_hd_display_at(0.0, 0.0);
        let visible = Bounds {
            origin: point(px(0.0), px(25.0)),
            size: size(px(1920.0), px(1000.0)),
        };
        let saved = windowed(5000.0, 0.0, 2560.0, 1440.0, Some(1));
        let fitted = saved.fit_to_display(false, display, visible);
        assert_gpui_bounds_geometry(&fitted, 0.0, 25.0, 1920.0, 1000.0);
    }

    #[test]
    fn fit_to_display_checks_reachability_in_the_display_coordinate_space() {
        let display = full_hd_display_at(-1920.0, 0.0);
        let saved = windowed(-1500.0, 200.0, 1200.0, 800.0, Some(3));
        let fitted = saved.fit_to_display(false, display, display);
        assert_gpui_bounds_geometry(&fitted, -1500.0, 200.0, 1200.0, 800.0);
    }

    #[test]
    fn fit_to_display_keeps_the_maximized_state_when_recentering() {
        use gpui_kit::WindowBounds;
        let display = full_hd_display_at(0.0, 0.0);
        let mut saved = windowed(4000.0, 4000.0, 1200.0, 800.0, Some(1));
        saved.state = "Maximized".to_string();
        let fitted = saved.fit_to_display(false, display, display);
        assert!(matches!(fitted, WindowBounds::Maximized(_)));
        assert_gpui_bounds_geometry(&fitted, 360.0, 140.0, 1200.0, 800.0);
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
    fn restore_placement_recenters_a_window_saved_on_a_disconnected_display(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let second_display = gpui_kit::DisplayId::new(2);
        cx.simulate_display_added(second_display, None);
        let saved = windowed(100.0, 100.0, 1200.0, 800.0, Some(2));

        let (_, display_id) = cx.update(|cx| saved.restore_placement(cx));
        assert_eq!(display_id, Some(second_display));

        cx.simulate_display_removed(second_display);
        let (bounds, display_id) = cx.update(|cx| saved.restore_placement(cx));
        assert_eq!(
            display_id, None,
            "a disconnected display must not be requested"
        );
        assert_gpui_bounds_geometry(&bounds, 360.0, 140.0, 1200.0, 800.0);
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
    fn restore_placement_keeps_bounds_on_a_connected_display(cx: &mut gpui_kit::TestAppContext) {
        let saved = windowed(100.0, 100.0, 1200.0, 800.0, Some(1));
        let (bounds, display_id) = cx.update(|cx| saved.restore_placement(cx));
        assert_eq!(display_id.map(u64::from), Some(1));
        assert_gpui_bounds_geometry(&bounds, 100.0, 100.0, 1200.0, 800.0);
    }
}
