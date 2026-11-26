use super::super::types::*;
use crate::core::radix::{byte_category, group_category};
use gpui_kit::*;
use std::ops::Range;

/// Return the smallest matching highlight for the specified range.
#[inline]
pub fn highlight_color_for_range(item_start: usize, item_end: usize, active_highlights: &[(Range<usize>, Hsla)]) -> Option<Hsla> {
    let mut smallest_len = usize::MAX;
    let mut color = None;
    for (range, highlight_color) in active_highlights {
        if range.start < item_end && range.end > item_start {
            let len = range.end.saturating_sub(range.start);
            if len <= smallest_len {
                smallest_len = len;
                color = Some(*highlight_color);
            }
        }
    }
    color
}

#[inline]
pub fn pixel_midpoint(left: Pixels, right: Pixels) -> Pixels {
    px((f32::from(left) + f32::from(right)) * 0.5)
}

#[inline]
pub fn darken_cursor_color(color: Hsla) -> Hsla {
    Hsla {
        l: (color.l * 0.72).clamp(0.0, 1.0),
        a: color.a.max(0.9),
        ..color
    }
}

/// Computes the selection background color with appropriate alpha for focused/unfocused
/// states and dark/light theme modes.
#[inline]
pub fn selection_background_color(selection_color: Hsla, is_focused: bool, is_dark: bool) -> Hsla {
    let alpha = if is_focused {
        if is_dark { 0.55 } else { 0.40 }
    } else if is_dark {
        0.28
    } else {
        0.20
    };
    selection_color.alpha(alpha)
}

/// Computes the text color for a hex group item using a supplied contrast resolver.
#[inline]
pub(super) fn hex_item_text_color_with(
    is_cursor_focused: bool,
    is_selected: bool,
    hl_color: Option<Hsla>,
    item_slice: &[u8],
    palette: &ByteCategoryPalette,
    fg_color: Hsla,
    resolve_contrast: impl FnOnce(Hsla, Hsla) -> Hsla,
) -> Hsla {
    if let Some(hl) = hl_color {
        resolve_contrast(fg_color, hl)
    } else if is_cursor_focused || is_selected {
        fg_color
    } else {
        palette.color_for(group_category(item_slice))
    }
}

/// Computes the text color for a hex group item taking into account cursor focus,
/// active selection, highlights, and category palette.
/// When a highlight background is present, `ensure_contrast` guarantees legibility.
#[cfg(test)]
#[inline]
pub fn hex_item_text_color(
    is_cursor_focused: bool,
    is_selected: bool,
    hl_color: Option<Hsla>,
    item_slice: &[u8],
    palette: &ByteCategoryPalette,
    fg_color: Hsla,
) -> Hsla {
    hex_item_text_color_with(is_cursor_focused, is_selected, hl_color, item_slice, palette, fg_color, |fg, bg| {
        ensure_contrast(fg, bg, 4.5)
    })
}

/// Computes the text color for an ASCII character cell using a supplied contrast resolver.
#[inline]
#[allow(clippy::too_many_arguments)]
pub(super) fn ascii_item_text_color_with(
    in_selected_group: bool,
    hl_color: Option<Hsla>,
    byte: u8,
    is_printable: bool,
    palette: &ByteCategoryPalette,
    fg_color: Hsla,
    muted_color: Hsla,
    resolve_contrast: impl FnOnce(Hsla, Hsla) -> Hsla,
) -> Hsla {
    if let Some(hl) = hl_color {
        let base = if !is_printable { muted_color } else { fg_color };
        resolve_contrast(base, hl)
    } else if in_selected_group {
        if !is_printable { muted_color } else { fg_color }
    } else {
        let cat = byte_category(byte);
        let color = palette.color_for(Some(cat));
        if is_printable {
            color
        } else if color == fg_color || color == muted_color {
            muted_color
        } else {
            color
        }
    }
}

/// Computes the text color for an ASCII character cell taking into account selection,
/// highlights, printable status, and category palette.
/// When a highlight background is present, `ensure_contrast` guarantees legibility.
#[cfg(test)]
#[inline]
pub fn ascii_item_text_color(
    in_selected_group: bool,
    hl_color: Option<Hsla>,
    byte: u8,
    is_printable: bool,
    palette: &ByteCategoryPalette,
    fg_color: Hsla,
    muted_color: Hsla,
) -> Hsla {
    ascii_item_text_color_with(in_selected_group, hl_color, byte, is_printable, palette, fg_color, muted_color, |fg, bg| {
        ensure_contrast(fg, bg, 4.5)
    })
}

/// Precomputes centered per-cell Hex glyph coordinates from a batched shaped line.

#[inline]
fn channel_to_linear(c: f32) -> f32 {
    if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// Calculates relative luminance according to WCAG 2.1 specs.
#[inline]
pub fn rel_luminance(c: Hsla) -> f32 {
    let rgba = Rgba::from(c);
    0.2126 * channel_to_linear(rgba.r) + 0.7152 * channel_to_linear(rgba.g) + 0.0722 * channel_to_linear(rgba.b)
}

/// Calculates contrast ratio between two precomputed relative luminances according to WCAG 2.1 specs.
#[inline]
fn contrast_from_luminances(l1: f32, l2: f32) -> f32 {
    let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

/// Calculates contrast ratio between two colors according to WCAG 2.1 specs.
#[cfg(test)]
#[inline]
pub fn contrast_ratio(c1: Hsla, c2: Hsla) -> f32 {
    contrast_from_luminances(rel_luminance(c1), rel_luminance(c2))
}

/// Adjusts color lightness if necessary to guarantee a minimum WCAG contrast ratio against `background`.
pub fn ensure_contrast(color: Hsla, background: Hsla, min_ratio: f32) -> Hsla {
    let mut solid_color = color;
    solid_color.a = 1.0;
    let bg_lum = rel_luminance(background);
    if contrast_from_luminances(rel_luminance(solid_color), bg_lum) >= min_ratio {
        return solid_color;
    }
    let is_dark_bg = bg_lum < 0.2;

    for _ in 0..25 {
        if is_dark_bg {
            solid_color.l = (solid_color.l + 0.03).min(1.0);
        } else {
            solid_color.l = (solid_color.l - 0.03).max(0.0);
        }
        if contrast_from_luminances(rel_luminance(solid_color), bg_lum) >= min_ratio {
            break;
        }
    }
    solid_color
}
