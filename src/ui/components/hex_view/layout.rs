use super::types::*;
use crate::core::encoding::Encoding;
use crate::core::radix::{ByteGroupSize, DisplayRadix, format_group};
use gpui_kit::*;
use std::ops::Range;

const HEX_METRIC_CHARS: &str = "0123456789abcdef.";

pub fn ascii_byte_index_from_world_x(world_x: f32, column: ColumnLayout, scroll_x: f32) -> usize {
    let content_x = (world_x - column.start + scroll_x).max(0.0);
    (content_x / ASCII_CELL_WIDTH) as usize
}

pub fn build_ascii_char_map(encoding: Encoding, buffer: &[u8], row_offset: usize, row_len: usize) -> Vec<Option<AsciiChar>> {
    let mut map = vec![None; row_len];
    let row_end = row_offset + row_len;
    let mut local_offset = 0;

    if encoding.is_multibyte() && row_offset > 0 {
        let max_back = 64.min(row_offset);
        let window_start = row_offset - max_back;
        let mut sync_start = window_start;

        // Look for an unambiguous ASCII synchronization point (< 0x40) in the back window
        for pos in (window_start..row_offset).rev() {
            if buffer[pos] < 0x40 {
                sync_start = pos;
                break;
            }
        }

        // Scan forward from sync_start to row_offset to find exact character boundaries
        let mut scan_pos = sync_start;
        while scan_pos < row_offset {
            if let Some((_, len)) = encoding.decode_char_at(buffer, scan_pos) {
                let next_pos = scan_pos + len.max(1);
                if next_pos > row_offset {
                    // This character crosses the row_offset boundary.
                    let skip = next_pos - row_offset;
                    local_offset = skip.min(row_len);
                    break;
                }
                scan_pos = next_pos;
            } else {
                scan_pos += 1;
            }
        }
    }

    while local_offset < row_len {
        let absolute_offset = row_offset + local_offset;
        if absolute_offset >= buffer.len() {
            break;
        }
        if let Some((character, byte_len)) = encoding.decode_char_at(buffer, absolute_offset) {
            let byte_len = byte_len.max(1);
            let display_offset = if encoding.is_multibyte() && absolute_offset.saturating_add(byte_len) > row_end {
                row_len - 1
            } else {
                local_offset
            };
            map[display_offset] = Some(AsciiChar::Printable(character, byte_len));
            local_offset += byte_len;
        } else {
            map[local_offset] = Some(AsciiChar::NonPrintable);
            local_offset += 1;
        }
    }

    map
}

#[inline]
pub fn can_chain_to_outer(target: HorizontalScrollTarget, residual_delta: f32) -> bool {
    !matches!(target, HorizontalScrollTarget::View) && residual_delta.abs() > 0.01
}

pub fn make_hex_view_layout(input: LayoutInput) -> HexViewLayout {
    let fixed_width = input.fixed_column_width + input.section_gap;

    let hex = ColumnLayout {
        start: fixed_width,
        width: input.hex_col_width,
        inner_max: input.hex_inner_max.max(0.0),
    };

    let (ascii, description, comment_start) = if input.is_struct_mode {
        let description_start = hex.end() + input.section_gap;
        let description = ColumnLayout {
            start: description_start,
            width: input.desc_col_width,
            inner_max: input.desc_inner_max.max(0.0),
        };
        (None, Some(description), description.end() + input.section_gap)
    } else {
        let ascii = if input.show_ascii {
            let ascii_start = hex.end() + input.section_gap;
            Some(ColumnLayout {
                start: ascii_start,
                width: input.ascii_col_width,
                inner_max: input.ascii_inner_max.max(0.0),
            })
        } else {
            None
        };
        let comment_start = ascii
            .map(|column| column.end() + input.section_gap)
            .unwrap_or_else(|| hex.end() + input.section_gap);
        (ascii, None, comment_start)
    };

    let comment = ColumnLayout {
        start: comment_start,
        width: input.comment_col_width,
        inner_max: input.comment_inner_max.max(0.0),
    };
    let content_width = (comment.end() - fixed_width).max(0.0);
    let viewport_width = (input.bounds_width - input.content_padding - input.scrollbar_width - fixed_width).max(0.0);

    HexViewLayout {
        fixed_width,
        content_width,
        viewport_width,
        outer_max: (content_width - viewport_width).max(0.0),
        hex,
        ascii,
        description,
        comment,
    }
}

/// Build the exact text stream used by the batched Hex renderer and retain the
/// byte/text ranges needed to translate shaped coordinates back to bytes.
pub fn build_hex_text_source(chunk: &[u8], line_offset: usize, radix: DisplayRadix, group_size: ByteGroupSize, is_big_endian: bool) -> HexTextSource {
    let chunk = if chunk.len() > 1024 { &chunk[..1024] } else { chunk };
    let group_bytes = group_size.byte_count();
    let mut text = String::with_capacity(chunk.len().saturating_mul(3));
    let mut groups = Vec::with_capacity(chunk.len().div_ceil(group_bytes));
    let mut chunk_idx = 0;

    while chunk_idx < chunk.len() {
        let item_start_offset = line_offset + chunk_idx;
        let start_slot = item_start_offset % group_bytes;
        let item_slice_len = (chunk.len() - chunk_idx).min(group_bytes - start_slot);

        if !text.is_empty() {
            text.push(' ');
        }
        let text_start = text.len();
        text.push_str(&format_group(
            &chunk[chunk_idx..chunk_idx + item_slice_len],
            start_slot,
            radix,
            group_size,
            is_big_endian,
        ));
        let text_end = text.len();

        groups.push(HexGroupInfo {
            chunk_start: chunk_idx,
            chunk_end: chunk_idx + item_slice_len,
            start_slot,
            text_start,
            text_end,
        });
        chunk_idx += item_slice_len;
    }

    HexTextSource {
        text: SharedString::from(text),
        groups,
    }
}

/// Measure the widest glyph used by the formatted Hex stream.
///
/// This is intentionally a small, cached probe rather than a per-row
/// measurement. The extra pixel gives centered glyphs a little breathing
/// room for rounding and side bearings in proportional fonts.
pub fn measure_hex_cell_width(window: &Window, font: Font, font_size: Pixels) -> Pixels {
    let mut max_advance: f32 = 0.0;

    for character in HEX_METRIC_CHARS.chars() {
        let text = SharedString::from(character.to_string());
        let run = TextRun {
            len: text.len(),
            font: font.clone(),
            color: hsla(0.0, 0.0, 0.0, 0.0),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = window.text_system().shape_line(text, font_size, &[run], None);
        max_advance = max_advance.max(f32::from(shaped.width));
    }

    px((max_advance + 1.01).max(1.0))
}

#[inline]
pub fn hex_grid_x(text_index: usize, cell_width: Pixels) -> Pixels {
    px(text_index as f32 * f32::from(cell_width))
}

#[inline]
pub fn hex_grid_width(text_len: usize, cell_width: Pixels) -> Pixels {
    hex_grid_x(text_len, cell_width)
}

#[inline]
pub fn centered_glyph_offset(cell_width: f32, glyph_width: f32) -> f32 {
    ((cell_width - glyph_width) / 2.0).max(0.0)
}

pub fn bounded_auto_fit_range(total_size: usize, visible_start: usize, visible_end: usize) -> Range<usize> {
    let visible_start = visible_start.min(total_size);
    let visible_end = visible_end.clamp(visible_start, total_size);
    if total_size <= AUTO_FIT_SCAN_BYTES {
        return 0..total_size;
    }

    let visible_len = visible_end - visible_start;
    if visible_len >= AUTO_FIT_SCAN_BYTES {
        let start = visible_start.min(total_size - AUTO_FIT_SCAN_BYTES);
        return start..start + AUTO_FIT_SCAN_BYTES;
    }

    let extra = AUTO_FIT_SCAN_BYTES - visible_len;
    let centered_start = visible_start.saturating_sub(extra / 2);
    let start = centered_start.min(total_size - AUTO_FIT_SCAN_BYTES);
    start..start + AUTO_FIT_SCAN_BYTES
}

pub fn weighted_text_width(text: &str, char_w: f32) -> f32 {
    text.chars()
        .take(AUTO_FIT_MAX_TEXT_CHARS)
        .map(|character| if character.is_ascii() { char_w } else { char_w * 1.8 })
        .sum()
}

pub fn hex_group_x(group: HexGroupInfo, origin_x: Pixels, cell_width: Pixels) -> (Pixels, Pixels) {
    (
        origin_x + hex_grid_x(group.text_start, cell_width),
        origin_x + hex_grid_x(group.text_end, cell_width),
    )
}

/// Number of margin rows to display above the target when scrolling into view.
/// This places the target at the 5th line from the top of the viewport.
pub const SCROLL_REVEAL_TOP_MARGIN: usize = 4;

/// Computes the new top scroll row to reveal target range (rows `start_row..=end_row`) in the viewport.
/// If the target is already fully visible within the current viewport, returns `None`.
/// If the target is outside the current viewport, returns `Some(new_scroll_top)` placing `start_row`
/// with a 4-row top margin (`SCROLL_REVEAL_TOP_MARGIN`).
pub fn calculate_scroll_top_for_range(current_scroll_top: usize, visible_rows: usize, total_rows: usize, start_row: usize, end_row: usize) -> Option<usize> {
    let (start_row, end_row) = (start_row.min(end_row), start_row.max(end_row));
    let visible_rows = visible_rows.max(1);
    let top_row = current_scroll_top;
    let bottom_row = top_row + visible_rows.saturating_sub(1);
    let max_offset = total_rows.saturating_sub(visible_rows);

    if start_row >= top_row && end_row <= bottom_row {
        return None;
    }

    let target_top = start_row.saturating_sub(SCROLL_REVEAL_TOP_MARGIN).min(max_offset);

    if target_top != current_scroll_top { Some(target_top) } else { None }
}
