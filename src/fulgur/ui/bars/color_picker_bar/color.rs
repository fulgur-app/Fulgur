use gpui::{Hsla, hsla};

/// Convert an HSLA color to `OkLCH` (Lightness, Chroma, Hue) components.
///
/// ### Arguments
/// - `color`: The HSLA color to convert
///
/// ### Returns
/// `(f32, f32, f32)` where L is 0..1, C is chroma, and H is hue in degrees 0..360.
fn hsla_to_oklch(color: Hsla) -> (f32, f32, f32) {
    #[allow(clippy::excessive_precision, clippy::unreadable_literal)]
    const LMS: [[f32; 3]; 3] = [
        [0.4122214708, 0.5363325363, 0.0514459929],
        [0.2119034982, 0.6806995451, 0.1073969566],
        [0.0883024619, 0.2817188376, 0.6299787005],
    ];
    #[allow(clippy::excessive_precision, clippy::unreadable_literal)]
    const OKLAB: [[f32; 3]; 3] = [
        [0.2104542553, 0.7936177850, -0.0040720468],
        [1.9779984951, -2.4285922050, 0.4505937099],
        [0.0259040371, 0.7827717662, -0.8086757660],
    ];
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
    let l = LMS[0][0] * lr + LMS[0][1] * lg + LMS[0][2] * lb;
    let m = LMS[1][0] * lr + LMS[1][1] * lg + LMS[1][2] * lb;
    let s = LMS[2][0] * lr + LMS[2][1] * lg + LMS[2][2] * lb;
    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();
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
    #[allow(clippy::excessive_precision, clippy::unreadable_literal)]
    const INV_OKLAB: [[f32; 3]; 3] = [
        [1.0000000000, 0.3963377774, 0.2158037573],
        [1.0000000000, -0.1055613458, -0.0638541728],
        [1.0000000000, -0.0894841775, -1.2914855480],
    ];
    #[allow(clippy::excessive_precision, clippy::unreadable_literal)]
    const INV_LMS: [[f32; 3]; 3] = [
        [4.0767416621, -3.3077115913, 0.2309699292],
        [-1.2684380046, 2.6097574011, -0.3413193965],
        [-0.0041960863, -0.7034186147, 1.7076147010],
    ];
    let h_rad = h.to_radians();
    let ok_a = c * h_rad.cos();
    let ok_b = c * h_rad.sin();
    let l_ = INV_OKLAB[0][0] * l + INV_OKLAB[0][1] * ok_a + INV_OKLAB[0][2] * ok_b;
    let m_ = INV_OKLAB[1][0] * l + INV_OKLAB[1][1] * ok_a + INV_OKLAB[1][2] * ok_b;
    let s_ = INV_OKLAB[2][0] * l + INV_OKLAB[2][1] * ok_a + INV_OKLAB[2][2] * ok_b;
    let lms_l = l_ * l_ * l_;
    let lms_m = m_ * m_ * m_;
    let lms_s = s_ * s_ * s_;
    let lin_r = INV_LMS[0][0] * lms_l + INV_LMS[0][1] * lms_m + INV_LMS[0][2] * lms_s;
    let lin_g = INV_LMS[1][0] * lms_l + INV_LMS[1][1] * lms_m + INV_LMS[1][2] * lms_s;
    let lin_b = INV_LMS[2][0] * lms_l + INV_LMS[2][1] * lms_m + INV_LMS[2][2] * lms_s;
    let from_linear = |c: f32| -> f32 {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.003_130_8 {
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
pub(super) fn format_oklch(color: Hsla) -> String {
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
pub(super) fn format_hsla(color: Hsla) -> String {
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
pub(super) fn parse_hex(s: &str) -> Option<Hsla> {
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
pub(super) fn parse_oklch(s: &str) -> Option<Hsla> {
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
pub(super) fn parse_hsla(s: &str) -> Option<Hsla> {
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
