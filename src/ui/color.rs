use gpui_kit::{Hsla, Rgba};

impl From<crate::core::color::RgbaColor> for Hsla {
    fn from(c: crate::core::color::RgbaColor) -> Self {
        let rf = c.r as f32 / 255.0;
        let gf = c.g as f32 / 255.0;
        let bf = c.b as f32 / 255.0;
        let af = c.a as f32 / 255.0;
        Rgba { r: rf, g: gf, b: bf, a: af }.into()
    }
}

impl From<Hsla> for crate::core::color::RgbaColor {
    fn from(hsla: Hsla) -> Self {
        let rgba = hsla.to_rgb();
        Self {
            r: (rgba.r.clamp(0.0, 1.0) * 255.0).round() as u8,
            g: (rgba.g.clamp(0.0, 1.0) * 255.0).round() as u8,
            b: (rgba.b.clamp(0.0, 1.0) * 255.0).round() as u8,
            a: (rgba.a.clamp(0.0, 1.0) * 255.0).round() as u8,
        }
    }
}

pub trait BookmarkColorExt {
    fn to_hsla(self) -> Hsla;
    fn to_badge_hsla(self) -> Hsla;
}

impl BookmarkColorExt for crate::core::bookmark::BookmarkColor {
    fn to_hsla(self) -> Hsla {
        self.to_rgba().into()
    }

    fn to_badge_hsla(self) -> Hsla {
        self.to_badge_rgba().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::color::RgbaColor;

    #[test]
    fn test_rgba_color_roundtrip() {
        let orig = RgbaColor {
            r: 100,
            g: 150,
            b: 200,
            a: 255,
        };
        let hsla: Hsla = orig.into();
        let back: RgbaColor = hsla.into();
        assert_eq!(orig.r, back.r);
        assert_eq!(orig.g, back.g);
        assert_eq!(orig.b, back.b);
        assert_eq!(orig.a, back.a);
    }
}
