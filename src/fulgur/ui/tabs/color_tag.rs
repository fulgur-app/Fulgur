use gpui::{App, Hsla};
use gpui_component::ActiveTheme;

/// A theme-relative color a user can assign to a tab as a visual tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorTag {
    Red,
    RedLight,
    Green,
    GreenLight,
    Blue,
    BlueLight,
    Yellow,
    YellowLight,
    Magenta,
    MagentaLight,
    Cyan,
    CyanLight,
    Accent,
    Primary,
}

impl ColorTag {
    /// List every selectable color tag, in the order shown in the menu.
    ///
    /// ### Returns
    /// - `[ColorTag; 14]`: All color tag variants
    #[must_use]
    pub fn all() -> [ColorTag; 14] {
        [
            ColorTag::Red,
            ColorTag::RedLight,
            ColorTag::Green,
            ColorTag::GreenLight,
            ColorTag::Blue,
            ColorTag::BlueLight,
            ColorTag::Yellow,
            ColorTag::YellowLight,
            ColorTag::Magenta,
            ColorTag::MagentaLight,
            ColorTag::Cyan,
            ColorTag::CyanLight,
            ColorTag::Accent,
            ColorTag::Primary,
        ]
    }

    /// Human-readable name shown in the menu.
    ///
    /// ### Returns
    /// - `&'static str`: The display label of the color tag
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ColorTag::Red => "Red",
            ColorTag::RedLight => "Light Red",
            ColorTag::Green => "Green",
            ColorTag::GreenLight => "Light Green",
            ColorTag::Blue => "Blue",
            ColorTag::BlueLight => "Light Blue",
            ColorTag::Yellow => "Yellow",
            ColorTag::YellowLight => "Light Yellow",
            ColorTag::Magenta => "Magenta",
            ColorTag::MagentaLight => "Light Magenta",
            ColorTag::Cyan => "Cyan",
            ColorTag::CyanLight => "Light Cyan",
            ColorTag::Accent => "Accent",
            ColorTag::Primary => "Primary",
        }
    }

    /// Stable key used to persist the color tag in the state file.
    ///
    /// Decoupled from [`ColorTag::label`] so the display text can change without
    /// invalidating saved state.
    ///
    /// ### Returns
    /// - `&'static str`: The persistence key of the color tag
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            ColorTag::Red => "red",
            ColorTag::RedLight => "red_light",
            ColorTag::Green => "green",
            ColorTag::GreenLight => "green_light",
            ColorTag::Blue => "blue",
            ColorTag::BlueLight => "blue_light",
            ColorTag::Yellow => "yellow",
            ColorTag::YellowLight => "yellow_light",
            ColorTag::Magenta => "magenta",
            ColorTag::MagentaLight => "magenta_light",
            ColorTag::Cyan => "cyan",
            ColorTag::CyanLight => "cyan_light",
            ColorTag::Accent => "accent",
            ColorTag::Primary => "primary",
        }
    }

    /// Parse a persisted key back into a color tag.
    ///
    /// ### Arguments
    /// - `key`: The persistence key previously produced by [`ColorTag::key`]
    ///
    /// ### Returns
    /// - `Some(ColorTag)`: The matching color tag
    /// - `None`: If the key is unknown (for example from a newer version)
    #[must_use]
    pub fn from_key(key: &str) -> Option<ColorTag> {
        match key {
            "red" => Some(ColorTag::Red),
            "red_light" => Some(ColorTag::RedLight),
            "green" => Some(ColorTag::Green),
            "green_light" => Some(ColorTag::GreenLight),
            "blue" => Some(ColorTag::Blue),
            "blue_light" => Some(ColorTag::BlueLight),
            "yellow" => Some(ColorTag::Yellow),
            "yellow_light" => Some(ColorTag::YellowLight),
            "magenta" => Some(ColorTag::Magenta),
            "magenta_light" => Some(ColorTag::MagentaLight),
            "cyan" => Some(ColorTag::Cyan),
            "cyan_light" => Some(ColorTag::CyanLight),
            "accent" => Some(ColorTag::Accent),
            "primary" => Some(ColorTag::Primary),
            _ => None,
        }
    }

    /// Resolve the color tag against the active theme.
    ///
    /// ### Arguments
    /// - `cx`: The application context, used to read the active theme
    ///
    /// ### Returns
    /// - `Hsla`: The theme color the tag currently maps to
    #[must_use]
    pub fn to_hsla(self, cx: &App) -> Hsla {
        let theme = cx.theme();
        match self {
            ColorTag::Red => theme.red,
            ColorTag::RedLight => theme.red_light,
            ColorTag::Green => theme.green,
            ColorTag::GreenLight => theme.green_light,
            ColorTag::Blue => theme.blue,
            ColorTag::BlueLight => theme.blue_light,
            ColorTag::Yellow => theme.yellow,
            ColorTag::YellowLight => theme.yellow_light,
            ColorTag::Magenta => theme.magenta,
            ColorTag::MagentaLight => theme.magenta_light,
            ColorTag::Cyan => theme.cyan,
            ColorTag::CyanLight => theme.cyan_light,
            ColorTag::Accent => theme.accent,
            ColorTag::Primary => theme.primary,
        }
    }
}
