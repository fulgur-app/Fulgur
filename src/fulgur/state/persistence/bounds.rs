use serde::{Deserialize, Serialize};

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
    pub fn from_gpui_bounds(bounds: gpui::WindowBounds, display_id: Option<u32>) -> Self {
        use gpui::WindowBounds;
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
    /// - `gpui::WindowBounds`: The GPUI window bounds
    pub fn to_gpui_bounds(&self) -> gpui::WindowBounds {
        use gpui::{Bounds, WindowBounds, point, px, size};
        let bounds = Bounds {
            origin: point(px(self.x), px(self.y)),
            size: size(px(self.width), px(self.height)),
        };
        match self.state.as_str() {
            "Maximized" => WindowBounds::Maximized(bounds),
            "Fullscreen" => WindowBounds::Fullscreen(bounds),
            _ => WindowBounds::Windowed(bounds), // Default to Windowed for unknown states
        }
    }
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
        bounds: &gpui::WindowBounds,
        expected_x: f32,
        expected_y: f32,
        expected_width: f32,
        expected_height: f32,
    ) {
        use gpui::WindowBounds;
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
        use gpui::{Bounds, WindowBounds, point, px, size};
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
        use gpui::{Bounds, WindowBounds, point, px, size};
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
        use gpui::{Bounds, WindowBounds, point, px, size};
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
        use gpui::WindowBounds;
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
        use gpui::WindowBounds;
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
}
