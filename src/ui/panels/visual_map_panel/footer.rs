//! Status footer rendering for hovered pixel details, address offsets, and buffer stats.

use super::legend::ByteCategoryExt;
use crate::core::editor::Editor;
use crate::core::visual_map::{ByteCategory, HoveredPixel, VisualMapColorMode as ColorMode};
use crate::ui::appearance::Appearance;
use gpui_kit::component::{StyledExt, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

/// Renders the footer bar showing hovered cell information or general buffer stats.
#[allow(clippy::too_many_arguments)]
pub fn render_footer(
    hovered_info: Option<HoveredPixel>,
    editor: Option<&Entity<Editor>>,
    buffer_len: usize,
    total_rows: usize,
    cols: usize,
    pixel_size: usize,
    header_offset: usize,
    color_mode: ColorMode,
    is_big_endian: bool,
    entropy_window: usize,
    theme: &gpui_kit::component::Theme,
    cx: &App,
) -> AnyElement {
    let border_color = theme.border;
    let muted_color = theme.muted_foreground;
    let font_family = cx.global::<Appearance>().font_family.clone();

    let (left, right): (AnyElement, AnyElement) = match hovered_info {
        Some(HoveredPixel::SubByte {
            offset,
            sub_idx,
            bit_val,
            mode,
        }) => {
            let display_addr = editor.map(|ed| ed.read(cx).offset_to_address(offset)).unwrap_or(offset);
            let (swatch_color, val_desc, bit_pos_str, mode_name) = match mode {
                ColorMode::Mono1bpp => {
                    let col = if bit_val == 1 { rgb(0xFFFFFF) } else { rgb(0x000000) };
                    let name = if bit_val == 1 { "1 (White)" } else { "0 (Black)" };
                    (col, name.to_string(), format!("bit {}", 7 - sub_idx), "1BPP Mono")
                }
                ColorMode::Indexed2bpp => {
                    let shade = bit_val * 85;
                    let col = rgb(u32::from_be_bytes([0, shade, shade, shade]));
                    let shade_name = match bit_val {
                        0 => "Black",
                        1 => "Dark",
                        2 => "Light",
                        3 => "White",
                        _ => "",
                    };
                    (
                        col,
                        format!("{}/3 ({})", bit_val, shade_name),
                        format!("bits {}..{}", 6 - sub_idx * 2, 7 - sub_idx * 2),
                        "2BPP Gray",
                    )
                }
                ColorMode::Indexed4bpp => {
                    let [b, g, r, _] = crate::core::visual_map::cga_16_bgra_lut()[(bit_val & 0x0F) as usize];
                    let col = rgb(u32::from_be_bytes([0, r, g, b]));
                    let name = crate::core::visual_map::cga_color_name(bit_val);
                    let nibble = if sub_idx == 0 { "high nibble [4..7]" } else { "low nibble [0..3]" };
                    (col, format!("{}/15 ({})", bit_val, name), nibble.to_string(), "4BPP CGA")
                }
                _ => (rgb(0x000000), String::new(), String::new(), ""),
            };

            let byte_val = editor
                .and_then(|ed| {
                    let doc = ed.read(cx).document.read().ok()?;
                    doc.buffer.get_range(offset, 1).first().copied()
                })
                .unwrap_or(0);

            let left = h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .font_family(font_family.clone())
                        .text_color(theme.foreground)
                        .child(format!("0x{:08X}", display_addr)),
                )
                .child(div().text_color(muted_color).child("|"))
                .child(
                    div()
                        .font_family(font_family.clone())
                        .text_color(theme.foreground)
                        .child(format!("0x{:02X}", byte_val)),
                )
                .child(
                    div()
                        .px_1()
                        .py_0p5()
                        .rounded_sm()
                        .bg(theme.muted.opacity(0.4))
                        .font_family(font_family.clone())
                        .text_color(theme.foreground)
                        .child(bit_pos_str),
                )
                .into_any_element();

            let right = h_flex()
                .gap_1p5()
                .items_center()
                .child(div().w_3().h_3().rounded_sm().bg(swatch_color).border_1().border_color(theme.border))
                .child(div().font_family(font_family).text_color(theme.foreground).child(val_desc))
                .child(
                    div()
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .bg(theme.accent.opacity(0.2))
                        .text_color(theme.accent)
                        .font_medium()
                        .child(mode_name),
                )
                .into_any_element();

            (left, right)
        }
        Some(HoveredPixel::PlanarTile {
            offset,
            tile_idx,
            in_tile_x,
            in_tile_y,
            color_idx,
            bp0,
            bp1,
            bp2,
            bp3,
        }) => {
            let display_addr = editor.map(|ed| ed.read(cx).offset_to_address(offset)).unwrap_or(offset);
            let [b, g, r, _] = crate::core::visual_map::cga_16_bgra_lut()[(color_idx & 0x0F) as usize];
            let swatch_color = rgb(u32::from_be_bytes([0, r, g, b]));
            let name = crate::core::visual_map::cga_color_name(color_idx);

            let left = h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .font_family(font_family.clone())
                        .text_color(theme.foreground)
                        .child(format!("0x{:08X}", display_addr)),
                )
                .child(div().text_color(muted_color).child("|"))
                .child(
                    div()
                        .font_family(font_family.clone())
                        .text_color(theme.foreground)
                        .child(format!("Tile #{tile_idx} ({in_tile_x},{in_tile_y})")),
                )
                .child(
                    div()
                        .px_1()
                        .py_0p5()
                        .rounded_sm()
                        .bg(theme.muted.opacity(0.4))
                        .font_family(font_family.clone())
                        .text_color(theme.foreground)
                        .child(format!("BP:[{bp0:02X} {bp1:02X} {bp2:02X} {bp3:02X}]")),
                )
                .into_any_element();

            let right = h_flex()
                .gap_1p5()
                .items_center()
                .child(div().w_3().h_3().rounded_sm().bg(swatch_color).border_1().border_color(theme.border))
                .child(
                    div()
                        .font_family(font_family)
                        .text_color(theme.foreground)
                        .child(format!("{color_idx}/15 ({name})")),
                )
                .child(
                    div()
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .bg(theme.accent.opacity(0.2))
                        .text_color(theme.accent)
                        .font_medium()
                        .child("4BPP Planar"),
                )
                .into_any_element();

            (left, right)
        }
        Some(HoveredPixel::Rgb16 { offset, raw_val, b0, b1, mode }) => {
            let display_addr = editor.map(|ed| ed.read(cx).offset_to_address(offset)).unwrap_or(offset);
            let (r, g, b, r_bits, g_bits, b_bits, mode_name) = match mode {
                ColorMode::Rgb565 => {
                    let (r8, g8, b8) = crate::core::visual_map::rgb565_to_rgb888(raw_val);
                    let r5 = (raw_val >> 11) & 0x1F;
                    let g6 = (raw_val >> 5) & 0x3F;
                    let b5 = raw_val & 0x1F;
                    (r8, g8, b8, r5, g6, b5, "RGB 565")
                }
                ColorMode::Rgb555 => {
                    let (r8, g8, b8) = crate::core::visual_map::rgb555_to_rgb888(raw_val);
                    let r5 = (raw_val >> 10) & 0x1F;
                    let g5 = (raw_val >> 5) & 0x1F;
                    let b5 = raw_val & 0x1F;
                    (r8, g8, b8, r5, g5, b5, "RGB 555")
                }
                _ => (0, 0, 0, 0, 0, 0, "RGB"),
            };
            let swatch_color = rgb(u32::from_be_bytes([0, r, g, b]));

            let left = h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .font_family(font_family.clone())
                        .text_color(theme.foreground)
                        .child(format!("0x{:08X}", display_addr)),
                )
                .child(div().text_color(muted_color).child("|"))
                .child(
                    div()
                        .font_family(font_family.clone())
                        .text_color(theme.foreground)
                        .child(format!("0x{:04X}", raw_val)),
                )
                .child(
                    div()
                        .px_1()
                        .py_0p5()
                        .rounded_sm()
                        .bg(theme.muted.opacity(0.4))
                        .font_family(font_family.clone())
                        .text_color(theme.foreground)
                        .child(format!("[{:02X} {:02X}]", b0, b1)),
                )
                .into_any_element();

            let right = h_flex()
                .gap_1p5()
                .items_center()
                .child(div().w_3().h_3().rounded_sm().bg(swatch_color).border_1().border_color(theme.border))
                .child(
                    div()
                        .font_family(font_family)
                        .text_color(theme.foreground)
                        .child(format!("RGB({}, {}, {})", r, g, b)),
                )
                .child(
                    div()
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .bg(theme.muted.opacity(0.3))
                        .text_color(muted_color)
                        .child(format!("R:{} G:{} B:{}", r_bits, g_bits, b_bits)),
                )
                .child(
                    div()
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .bg(theme.accent.opacity(0.2))
                        .text_color(theme.accent)
                        .font_medium()
                        .child(mode_name),
                )
                .into_any_element();

            (left, right)
        }
        Some(HoveredPixel::Rgb24 { offset, b0, b1, b2, mode }) => {
            let display_addr = editor.map(|ed| ed.read(cx).offset_to_address(offset)).unwrap_or(offset);
            let (r, g, b, mode_name) = match mode {
                ColorMode::Rgb888 => (b0, b1, b2, "RGB 888"),
                ColorMode::Bgr888 => (b2, b1, b0, "BGR 888"),
                _ => (0, 0, 0, "RGB 24"),
            };
            let swatch_color = rgb(u32::from_be_bytes([0, r, g, b]));
            let raw_val = u32::from_be_bytes([0, b0, b1, b2]);

            let left = h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .font_family(font_family.clone())
                        .text_color(theme.foreground)
                        .child(format!("0x{:08X}", display_addr)),
                )
                .child(div().text_color(muted_color).child("|"))
                .child(
                    div()
                        .font_family(font_family.clone())
                        .text_color(theme.foreground)
                        .child(format!("0x{:06X}", raw_val)),
                )
                .child(
                    div()
                        .px_1()
                        .py_0p5()
                        .rounded_sm()
                        .bg(theme.muted.opacity(0.4))
                        .font_family(font_family.clone())
                        .text_color(theme.foreground)
                        .child(format!("[{:02X} {:02X} {:02X}]", b0, b1, b2)),
                )
                .into_any_element();

            let right = h_flex()
                .gap_1p5()
                .items_center()
                .child(div().w_3().h_3().rounded_sm().bg(swatch_color).border_1().border_color(theme.border))
                .child(
                    div()
                        .font_family(font_family)
                        .text_color(theme.foreground)
                        .child(format!("RGB({}, {}, {})", r, g, b)),
                )
                .child(
                    div()
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .bg(theme.accent.opacity(0.2))
                        .text_color(theme.accent)
                        .font_medium()
                        .child(mode_name),
                )
                .into_any_element();

            (left, right)
        }
        Some(HoveredPixel::Rgb32 { offset, b0, b1, b2, b3, mode }) => {
            let display_addr = editor.map(|ed| ed.read(cx).offset_to_address(offset)).unwrap_or(offset);
            let (r, g, b, a, mode_name) = match mode {
                ColorMode::Rgba => (b0, b1, b2, b3, "RGBA"),
                ColorMode::Argb => (b1, b2, b3, b0, "ARGB"),
                ColorMode::Bgra => (b2, b1, b0, b3, "BGRA"),
                _ => (0, 0, 0, 255, "RGBA 32"),
            };
            let swatch_color = rgba(u32::from_be_bytes([r, g, b, a]));
            let raw_val = u32::from_be_bytes([b0, b1, b2, b3]);

            let left = h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .font_family(font_family.clone())
                        .text_color(theme.foreground)
                        .child(format!("0x{:08X}", display_addr)),
                )
                .child(div().text_color(muted_color).child("|"))
                .child(
                    div()
                        .font_family(font_family.clone())
                        .text_color(theme.foreground)
                        .child(format!("0x{:08X}", raw_val)),
                )
                .child(
                    div()
                        .px_1()
                        .py_0p5()
                        .rounded_sm()
                        .bg(theme.muted.opacity(0.4))
                        .font_family(font_family.clone())
                        .text_color(theme.foreground)
                        .child(format!("[{:02X} {:02X} {:02X} {:02X}]", b0, b1, b2, b3)),
                )
                .into_any_element();

            let right = h_flex()
                .gap_1p5()
                .items_center()
                .child(div().w_3().h_3().rounded_sm().bg(swatch_color).border_1().border_color(theme.border))
                .child(
                    div()
                        .font_family(font_family)
                        .text_color(theme.foreground)
                        .child(format!("RGBA({}, {}, {}, {})", r, g, b, a)),
                )
                .child(
                    div()
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .bg(theme.accent.opacity(0.2))
                        .text_color(theme.accent)
                        .font_medium()
                        .child(mode_name),
                )
                .into_any_element();

            (left, right)
        }
        Some(HoveredPixel::Byte(offset, byte)) => {
            let display_addr = editor.map(|ed| ed.read(cx).offset_to_address(offset)).unwrap_or(offset);

            let left = h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .font_family(font_family.clone())
                        .text_color(theme.foreground)
                        .child(format!("0x{:08X}", display_addr)),
                )
                .child(div().text_color(muted_color).child("|"))
                .child(
                    div()
                        .font_family(font_family.clone())
                        .text_color(theme.foreground)
                        .child(format!("0x{:02X} ({})", byte, byte)),
                )
                .child(
                    div()
                        .px_1()
                        .py_0p5()
                        .rounded_sm()
                        .bg(theme.muted.opacity(0.4))
                        .font_family(font_family.clone())
                        .text_color(theme.foreground)
                        .child(if color_mode == ColorMode::Vga256 {
                            format!("Color #{}", byte)
                        } else if (32..=126).contains(&byte) {
                            format!("'{}'", byte as char)
                        } else if byte == 0 {
                            "NUL".to_string()
                        } else {
                            format!("0x{:02X}", byte)
                        }),
                )
                .into_any_element();

            let right = if color_mode == ColorMode::Vga256 {
                let [b, g, r, _] = crate::core::visual_map::vga256_bgra_lut()[byte as usize];
                let swatch_color = rgb(u32::from_be_bytes([0, r, g, b]));
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .child(div().w_3().h_3().rounded_sm().bg(swatch_color).border_1().border_color(theme.border))
                    .child(
                        div()
                            .font_family(font_family)
                            .text_color(theme.foreground)
                            .child(format!("RGB({}, {}, {})", r, g, b)),
                    )
                    .child(
                        div()
                            .px_1p5()
                            .py_0p5()
                            .rounded_sm()
                            .bg(theme.accent.opacity(0.2))
                            .text_color(theme.accent)
                            .font_medium()
                            .child("8BPP VGA"),
                    )
                    .into_any_element()
            } else {
                let cat = ByteCategory::of(byte);
                let entropy_info = editor.and_then(|ed| {
                    let doc = ed.read(cx).document.read().ok()?;
                    let h = crate::core::entropy::shannon_entropy_at(doc.buffer.data(), offset, entropy_window);
                    let norm = (h / 8.0) as f32;
                    let idx = crate::core::entropy::normalized_to_lut_index(norm);
                    let [r, g, b, _] = crate::core::entropy::entropy_lut()[idx];
                    let color: Hsla = rgb(u32::from_be_bytes([0, r, g, b])).into();
                    Some((h, norm, color))
                });

                h_flex()
                    .gap_1p5()
                    .items_center()
                    .children(entropy_info.map(|(h, norm, color)| {
                        let label = crate::core::entropy::entropy_level_label(h);
                        div()
                            .px_1p5()
                            .py_0p5()
                            .rounded_sm()
                            .bg(color.opacity(0.2))
                            .text_color(color)
                            .font_medium()
                            .child(format!("H: {:.2} ({:.0}%) {}", h, norm * 100.0, label))
                    }))
                    .child(
                        div()
                            .px_1p5()
                            .py_0p5()
                            .rounded_sm()
                            .bg(cat.color(theme).opacity(0.2))
                            .text_color(cat.color(theme))
                            .font_medium()
                            .child(cat.label()),
                    )
                    .into_any_element()
            };

            (left, right)
        }
        None => {
            let cursor_str = editor.map(|ed| {
                let cur = ed.read(cx).cursor_address();
                format!("Cursor: 0x{:08X}", cur)
            });

            let extra_spec = if color_mode == ColorMode::Entropy {
                format!(" | Win: {}B", entropy_window)
            } else if color_mode.is_rgb_16() {
                format!(" | {}", if is_big_endian { "BE" } else { "LE" })
            } else {
                String::new()
            };

            let left = h_flex()
                .gap_2()
                .items_center()
                .text_color(muted_color)
                .child(div().child(crate::core::format::format_size_friendly(buffer_len)))
                .child(div().child("|"))
                .child(div().child(format!("{} rows", crate::core::format::format_with_commas(total_rows))))
                .children(cursor_str.map(|c| {
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().child("|"))
                        .child(div().font_family(font_family.clone()).child(c))
                }))
                .into_any_element();

            let offset_spec = if header_offset > 0 {
                format!(" | +{}B", header_offset)
            } else {
                String::new()
            };

            let right = div()
                .text_color(muted_color)
                .child(format!("{} cols @ x{}{}{}", cols, pixel_size, extra_spec, offset_spec))
                .into_any_element();

            (left, right)
        }
    };

    h_flex()
        .w_full()
        .justify_between()
        .items_center()
        .p_2()
        .border_t_1()
        .border_color(border_color)
        .text_xs()
        .child(left)
        .child(right)
        .into_any_element()
}
