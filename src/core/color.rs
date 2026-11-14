//! Pure RGB/RGBA color representation independent of GUI frameworks.

use serde::{Deserialize, Serialize};

/// A 32-bit RGBA color representation used throughout the core editor engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct RgbaColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl RgbaColor {
    /// Creates a new RGBA color with explicit components.
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Creates an opaque RGB color with alpha set to 255.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Creates a transparent color.
    pub const fn transparent() -> Self {
        Self { r: 0, g: 0, b: 0, a: 0 }
    }

    /// Parses a color from a hexadecimal string (e.g. "#RRGGBB", "RRGGBB", "#RRGGBBAA", or "RRGGBBAA").
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        if !hex.is_ascii() {
            return None;
        }
        match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Self::rgb(r, g, b))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Self::new(r, g, b, a))
            }
            _ => None,
        }
    }

    /// Returns the color formatted as a hex string `#RRGGBBAA`.
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
    }

    /// Converts normalized HSLA float values (0.0..=1.0) to an RGBA color.
    pub fn from_hsla_f32(h: f32, s: f32, l: f32, a: f32) -> Self {
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let h_prime = ((h % 1.0 + 1.0) % 1.0) * 6.0;
        let x = c * (1.0 - ((h_prime % 2.0) - 1.0).abs());
        let m = l - c / 2.0;
        let (r1, g1, b1) = if (0.0..1.0).contains(&h_prime) {
            (c, x, 0.0)
        } else if (1.0..2.0).contains(&h_prime) {
            (x, c, 0.0)
        } else if (2.0..3.0).contains(&h_prime) {
            (0.0, c, x)
        } else if (3.0..4.0).contains(&h_prime) {
            (0.0, x, c)
        } else if (4.0..5.0).contains(&h_prime) {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };
        Self {
            r: ((r1 + m).clamp(0.0, 1.0) * 255.0).round() as u8,
            g: ((g1 + m).clamp(0.0, 1.0) * 255.0).round() as u8,
            b: ((b1 + m).clamp(0.0, 1.0) * 255.0).round() as u8,
            a: (a.clamp(0.0, 1.0) * 255.0).round() as u8,
        }
    }

    /// Returns the hue in degrees (0.0..=360.0).
    pub fn hue(&self) -> f32 {
        let r = self.r as f32 / 255.0;
        let g = self.g as f32 / 255.0;
        let b = self.b as f32 / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        if delta == 0.0 {
            return 0.0;
        }
        let mut h = if max == r {
            (g - b) / delta
        } else if max == g {
            2.0 + (b - r) / delta
        } else {
            4.0 + (r - g) / delta
        };
        h *= 60.0;
        if h < 0.0 {
            h += 360.0;
        }
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgba_creation() {
        let c = RgbaColor::new(10, 20, 30, 40);
        assert_eq!(c.r, 10);
        assert_eq!(c.g, 20);
        assert_eq!(c.b, 30);
        assert_eq!(c.a, 40);

        let rgb = RgbaColor::rgb(255, 128, 0);
        assert_eq!(rgb.a, 255);
    }

    #[test]
    fn test_from_hex() {
        assert_eq!(RgbaColor::from_hex("#ff0000"), Some(RgbaColor::rgb(255, 0, 0)));
        assert_eq!(RgbaColor::from_hex("#00FF00"), Some(RgbaColor::rgb(0, 255, 0)));
        assert_eq!(RgbaColor::from_hex("00ff0080"), Some(RgbaColor::new(0, 255, 0, 128)));
        assert_eq!(RgbaColor::from_hex("invalid"), None);
        // Multibyte string safety tests (must not panic, including '#' + byte lengths 6/8)
        assert_eq!(RgbaColor::from_hex("あいう"), None);
        assert_eq!(RgbaColor::from_hex("#あ"), None);
        assert_eq!(RgbaColor::from_hex("#あいう"), None);
        assert_eq!(RgbaColor::from_hex("#あa45"), None);
        assert_eq!(RgbaColor::from_hex("#12あ456"), None);
        assert_eq!(RgbaColor::from_hex("＃FF0000"), None);
        assert_eq!(RgbaColor::from_hex(""), None);
    }

    #[test]
    fn test_hsla_conversions_and_hue() {
        // Red
        let red = RgbaColor::from_hsla_f32(0.0, 1.0, 0.5, 1.0);
        assert_eq!(red, RgbaColor::rgb(255, 0, 0));
        assert_eq!(red.hue().round(), 0.0);

        // Green
        let green = RgbaColor::from_hsla_f32(120.0 / 360.0, 1.0, 0.5, 1.0);
        assert_eq!(green, RgbaColor::rgb(0, 255, 0));
        assert_eq!(green.hue().round(), 120.0);

        // Blue
        let blue = RgbaColor::from_hsla_f32(240.0 / 360.0, 1.0, 0.5, 1.0);
        assert_eq!(blue, RgbaColor::rgb(0, 0, 255));
        assert_eq!(blue.hue().round(), 240.0);

        // Black and White
        let black = RgbaColor::from_hsla_f32(0.0, 0.0, 0.0, 1.0);
        assert_eq!(black, RgbaColor::rgb(0, 0, 0));
        let white = RgbaColor::from_hsla_f32(0.0, 0.0, 1.0, 1.0);
        assert_eq!(white, RgbaColor::rgb(255, 255, 255));
    }
}
