//! Legend chips and descriptions for each visual map color mode.

use crate::core::visual_map::{ByteCategory, VisualMapColorMode as ColorMode};
use gpui_kit::component::{StyledExt, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

pub trait ByteCategoryExt {
    fn color(self, theme: &gpui_kit::component::Theme) -> Hsla;
}

impl ByteCategoryExt for ByteCategory {
    fn color(self, theme: &gpui_kit::component::Theme) -> Hsla {
        match self {
            ByteCategory::Null => theme.muted_foreground.opacity(0.4),
            ByteCategory::Control => theme.red.opacity(0.85),
            ByteCategory::Space => theme.blue.opacity(0.75),
            ByteCategory::Ascii => theme.green.opacity(0.9),
            ByteCategory::Extended => theme.accent.opacity(0.85),
        }
    }
}

fn legend_chip(color: Hsla, label: &'static str, border: bool, theme: &gpui_kit::component::Theme) -> AnyElement {
    let mut swatch = div().w_2().h_2().rounded_sm().bg(color);
    if border {
        swatch = swatch.border_1().border_color(theme.border);
    }
    h_flex()
        .gap_1()
        .items_center()
        .child(swatch)
        .child(div().text_color(theme.muted_foreground).child(label))
        .into_any_element()
}

fn cga_chips(theme: &gpui_kit::component::Theme) -> Vec<AnyElement> {
    let lut = crate::core::visual_map::cga_16_bgra_lut();
    (0..16)
        .map(|i| {
            let [b, g, r, _] = lut[i];
            div()
                .w_2p5()
                .h_2p5()
                .rounded_sm()
                .bg(rgb(u32::from_be_bytes([0, r, g, b])))
                .border_1()
                .border_color(theme.border)
                .into_any_element()
        })
        .collect()
}

/// Renders the color palette legend bar at the bottom of the visual map panel.
pub fn render_legend(color_mode: ColorMode, theme: &gpui_kit::component::Theme) -> Option<AnyElement> {
    let muted_color = theme.muted_foreground;
    let mut row = h_flex()
        .flex_wrap()
        .gap_2()
        .px_3()
        .py_1()
        .border_t_1()
        .border_color(theme.border)
        .bg(theme.muted.opacity(0.15))
        .text_xs()
        .items_center();

    match color_mode {
        ColorMode::DataCategory => {
            row = row
                .child(legend_chip(ByteCategory::Null.color(theme), "Null", false, theme))
                .child(legend_chip(ByteCategory::Control.color(theme), "Control", false, theme))
                .child(legend_chip(ByteCategory::Space.color(theme), "Space", false, theme))
                .child(legend_chip(ByteCategory::Ascii.color(theme), "ASCII", false, theme))
                .child(legend_chip(ByteCategory::Extended.color(theme), "Extended", false, theme));
        }
        ColorMode::Entropy => {
            let color_chip = |norm: f32| {
                let idx = crate::core::entropy::normalized_to_lut_index(norm);
                let [r, g, b, _] = crate::core::entropy::entropy_lut()[idx];
                rgb(u32::from_be_bytes([0, r, g, b]))
            };
            row = row
                .child(legend_chip(color_chip(0.0).into(), "0.0 Uniform", false, theme))
                .child(legend_chip(color_chip(0.35).into(), "Low", false, theme))
                .child(legend_chip(color_chip(0.60).into(), "4.8 Text/Code", false, theme))
                .child(legend_chip(color_chip(0.80).into(), "High", false, theme))
                .child(legend_chip(color_chip(1.0).into(), "8.0 Packed", false, theme));
        }
        ColorMode::Mono1bpp => {
            row = row
                .child(legend_chip(rgb(0x000000).into(), "0: Black", true, theme))
                .child(legend_chip(rgb(0xFFFFFF).into(), "1: White", true, theme))
                .child(div().text_color(muted_color).child("(1BPP, 8 px/B, MSB first)"));
        }
        ColorMode::Indexed2bpp => {
            row = row
                .child(legend_chip(rgb(0x000000).into(), "00: Black", true, theme))
                .child(legend_chip(rgb(0x555555).into(), "01: Dark", true, theme))
                .child(legend_chip(rgb(0xAAAAAA).into(), "10: Light", true, theme))
                .child(legend_chip(rgb(0xFFFFFF).into(), "11: White", true, theme))
                .child(div().text_color(muted_color).child("(2BPP, 4 px/B, 2b shade)"));
        }
        ColorMode::Indexed4bpp => {
            row = row
                .child(div().text_color(muted_color).font_medium().child("4BPP (CGA 16-Color, 2 px/B):"))
                .children(cga_chips(theme))
                .child(div().text_color(muted_color).child("[0: Black .. 15: Br.White]"));
        }
        ColorMode::Planar4bpp => {
            row = row
                .child(
                    div()
                        .text_color(muted_color)
                        .font_medium()
                        .child("4BPP Planar (16-Color 8×8 Tiles, 32 B/tile):"),
                )
                .children(cga_chips(theme))
                .child(div().text_color(muted_color).child("[0: Black .. 15: Br.White]"));
        }
        ColorMode::Vga256 => {
            row = row
                .child(div().text_color(muted_color).font_medium().child("8BPP (VGA 256-Color, 1 B/px):"))
                .child(
                    div()
                        .text_color(muted_color)
                        .child("0..15: Standard CGA | 16..231: 6×6×6 Color Cube | 232..255: Grayscale"),
                );
        }
        ColorMode::Rgb565 => {
            row = row
                .child(legend_chip(rgb(0xFF0000).into(), "R: 5b [11..15]", false, theme))
                .child(legend_chip(rgb(0x00FF00).into(), "G: 6b [5..10]", false, theme))
                .child(legend_chip(rgb(0x0000FF).into(), "B: 5b [0..4]", false, theme));
        }
        ColorMode::Rgb555 => {
            let x_col = theme.muted_foreground.opacity(0.4);
            row = row
                .child(legend_chip(x_col, "X: 1b [15]", false, theme))
                .child(legend_chip(rgb(0xFF0000).into(), "R: 5b [10..14]", false, theme))
                .child(legend_chip(rgb(0x00FF00).into(), "G: 5b [5..9]", false, theme))
                .child(legend_chip(rgb(0x0000FF).into(), "B: 5b [0..4]", false, theme));
        }
        ColorMode::Rgb888 => {
            row = row
                .child(legend_chip(rgb(0xFF0000).into(), "R: 8b [0]", false, theme))
                .child(legend_chip(rgb(0x00FF00).into(), "G: 8b [1]", false, theme))
                .child(legend_chip(rgb(0x0000FF).into(), "B: 8b [2]", false, theme));
        }
        ColorMode::Bgr888 => {
            row = row
                .child(legend_chip(rgb(0x0000FF).into(), "B: 8b [0]", false, theme))
                .child(legend_chip(rgb(0x00FF00).into(), "G: 8b [1]", false, theme))
                .child(legend_chip(rgb(0xFF0000).into(), "R: 8b [2]", false, theme));
        }
        ColorMode::Rgba => {
            let a_col = theme.muted_foreground.opacity(0.6);
            row = row
                .child(legend_chip(rgb(0xFF0000).into(), "R: 8b [0]", false, theme))
                .child(legend_chip(rgb(0x00FF00).into(), "G: 8b [1]", false, theme))
                .child(legend_chip(rgb(0x0000FF).into(), "B: 8b [2]", false, theme))
                .child(legend_chip(a_col, "A: 8b [3]", false, theme));
        }
        ColorMode::Argb => {
            let a_col = theme.muted_foreground.opacity(0.6);
            row = row
                .child(legend_chip(a_col, "A: 8b [0]", false, theme))
                .child(legend_chip(rgb(0xFF0000).into(), "R: 8b [1]", false, theme))
                .child(legend_chip(rgb(0x00FF00).into(), "G: 8b [2]", false, theme))
                .child(legend_chip(rgb(0x0000FF).into(), "B: 8b [3]", false, theme));
        }
        ColorMode::Bgra => {
            let a_col = theme.muted_foreground.opacity(0.6);
            row = row
                .child(legend_chip(rgb(0x0000FF).into(), "B: 8b [0]", false, theme))
                .child(legend_chip(rgb(0x00FF00).into(), "G: 8b [1]", false, theme))
                .child(legend_chip(rgb(0xFF0000).into(), "R: 8b [2]", false, theme))
                .child(legend_chip(a_col, "A: 8b [3]", false, theme));
        }
        _ => return None,
    }

    Some(row.into_any_element())
}
