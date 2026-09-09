use super::layout::*;
use super::types::*;
use crate::core::bookmark::BookmarkItem;
use crate::core::document::Document;
use crate::core::editor::LineMap;
use crate::core::encoding::Encoding;
use crate::core::radix::{ByteGroupSize, DisplayRadix, digit_count, is_group_zero};
use crate::core::structure::{IndexedField, ParseResult};
use crate::ui::style::BookmarkColorExt;
use gpui_kit::component::ActiveTheme;
use gpui_kit::*;
use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Range;

const TEXT_INPUT_CURSOR_WIDTH: Pixels = px(1.5);
const TEXT_INPUT_CURSOR_HEIGHT_RATIO: f32 = 0.85;

#[inline]
pub fn paint_border_box(window: &mut Window, bounds: Bounds<Pixels>, border_width: Pixels, color: Hsla) {
    let outline = outline(bounds, color, BorderStyle::Solid).border_widths(border_width);
    window.paint_quad(outline);
}

#[inline]
pub fn paint_cursor_border(window: &mut Window, bounds: Bounds<Pixels>, color: Hsla) {
    let padding_x = px(CURSOR_PADDING_X);
    let padding_y = px(CURSOR_PADDING_Y);
    let padded_bounds = Bounds::new(
        point(bounds.origin.x - padding_x, bounds.origin.y - padding_y),
        size(bounds.size.width + padding_x + padding_x, bounds.size.height + padding_y + padding_y),
    );
    paint_border_box(window, padded_bounds, px(CURSOR_BORDER_WIDTH), color);
}

#[inline]
fn paint_insert_cursor_at(window: &mut Window, bounds: Bounds<Pixels>, cursor_x: Pixels, color: Hsla) {
    if bounds.size.height <= px(0.0) {
        return;
    }

    // Match gpui-kit's default TextInput cursor: a 1.5px vertical
    // caret centered in the line and sized to 85% of its line height.
    let cursor_height = px(ROW_HEIGHT * TEXT_INPUT_CURSOR_HEIGHT_RATIO);
    let cursor_y = bounds.top() + (bounds.size.height - cursor_height) / 2.0;
    window.paint_quad(fill(
        Bounds::new(point(cursor_x, cursor_y), size(TEXT_INPUT_CURSOR_WIDTH, cursor_height)),
        color,
    ));
}

#[inline]
fn paint_underscore_cursor_at(window: &mut Window, bounds: Bounds<Pixels>, cursor_x: Pixels, width: Pixels, color: Hsla) {
    if bounds.size.height <= px(0.0) || width <= px(0.0) {
        return;
    }

    const UNDERSCORE_HEIGHT: f32 = 1.5;
    // Align the underscore's lower edge with the bottom edge of the
    // overwrite-mode cursor box.
    let underscore_y = bounds.bottom() - px(UNDERSCORE_HEIGHT);
    window.paint_quad(fill(Bounds::new(point(cursor_x, underscore_y), size(width, px(UNDERSCORE_HEIGHT))), color));
}

#[derive(Clone, Copy)]
struct HexInsertCursorParams {
    radix: DisplayRadix,
    group_size: ByteGroupSize,
    is_big_endian: bool,
    selection_active: bool,
    has_pending_nibble: bool,
    origin_x: Pixels,
    cell_width: Pixels,
}

/// Returns the x-coordinate for the insertion boundary represented by the
/// cursor within a rendered hex group and the width of its byte slot.
fn hex_insert_cursor_geometry(group: HexGroupInfo, cursor_in_row: usize, params: HexInsertCursorParams) -> Option<(Pixels, Pixels)> {
    if cursor_in_row < group.chunk_start || cursor_in_row >= group.chunk_end {
        return None;
    }

    let group_len = group.chunk_end.saturating_sub(group.chunk_start);
    let byte_index = cursor_in_row - group.chunk_start;
    let expected_len = params.group_size.byte_count();
    // While extending a selection, the caret represents the selection's
    // active end and should advance through the visible byte slots just like
    // a text editor. A collapsed caret still follows the rendered byte so a
    // click in a little-endian group remains on the byte that was clicked.
    let display_index = if params.selection_active {
        group.start_slot + byte_index
    } else if group.start_slot == 0 && cursor_in_row == group.chunk_start {
        0
    } else if group.start_slot == 0 && group_len == expected_len && !params.is_big_endian {
        group_len.saturating_sub(byte_index + 1)
    } else {
        group.start_slot + byte_index
    };
    let slot_width = match params.radix {
        DisplayRadix::Hexadecimal => 2.0,
        DisplayRadix::Binary => 8.0,
        // Decimal and octal groups do not have a fixed two-character byte
        // cell, but their fixed-width group still provides a stable insertion
        // column.
        DisplayRadix::Decimal | DisplayRadix::Octal => (group.text_end.saturating_sub(group.text_start) as f32 / expected_len.max(1) as f32).max(1.0),
    };
    let nibble_offset = if params.radix == DisplayRadix::Hexadecimal && params.has_pending_nibble {
        1.0
    } else {
        0.0
    };
    Some((
        params.origin_x + px((group.text_start as f32 + display_index as f32 * slot_width + nibble_offset) * f32::from(params.cell_width)),
        px(slot_width * f32::from(params.cell_width)),
    ))
}

/// Returns the x-coordinate immediately after the final data byte in a group
/// and the width of the next byte slot.
fn hex_insert_cursor_end_geometry(group: HexGroupInfo, origin_x: Pixels, cell_width: Pixels, group_size: ByteGroupSize) -> (Pixels, Pixels) {
    let expected_len = group_size.byte_count();
    let group_len = group.chunk_end.saturating_sub(group.chunk_start);
    let data_end_slot = group.start_slot.saturating_add(group_len).min(expected_len);
    let slot_width = (group.text_end.saturating_sub(group.text_start) as f32 / expected_len.max(1) as f32).max(1.0);
    (
        origin_x + px((group.text_start as f32 + data_end_slot as f32 * slot_width) * f32::from(cell_width)),
        px(slot_width * f32::from(cell_width)),
    )
}

pub fn row_highlights(highlights: &[(Range<usize>, Hsla)], max_len: usize, offset: usize, next_offset: usize) -> &[(Range<usize>, Hsla)] {
    if highlights.is_empty() {
        return &[];
    }
    let start_search = offset.saturating_sub(max_len);
    let search_start = highlights.partition_point(|(r, _)| r.start < start_search);
    let search_end = highlights.partition_point(|(r, _)| r.start < next_offset);
    &highlights[search_start..search_end]
}

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

/// Paint every glyph centered in its fixed Hex cell while reusing the line
/// shaped for the row. This keeps shaping batched and avoids a per-row glyph
/// position allocation.
#[allow(clippy::too_many_arguments)]
pub fn paint_centered_hex_glyphs(
    shaped: &ShapedLine,
    groups: &[HexGroupInfo],
    group_colors: &[Hsla],
    pending_edit_colors: Option<(usize, Hsla, Hsla)>,
    cell_width: Pixels,
    origin: Point<Pixels>,
    line_height: Pixels,
    window: &mut Window,
) {
    let natural_width = shaped.width;
    let cell_width_f = f32::from(cell_width);

    let text_width = hex_grid_width(shaped.text.len(), cell_width);
    let line_bounds = Bounds::new(origin, size(text_width, line_height));
    window.paint_layer(line_bounds, |window| {
        let baseline_offset = point(px(0.0), (line_height - shaped.ascent - shaped.descent) / 2.0 + shaped.ascent);
        let mut group_idx = 0;
        let mut glyphs = shaped
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter().map(move |glyph| (run.font_id, glyph)))
            .peekable();

        while let Some((font_id, glyph)) = glyphs.next() {
            let natural_end = glyphs.peek().map(|(_, next)| next.position.x).unwrap_or(natural_width);
            let glyph_width = f32::from(natural_end - glyph.position.x).max(0.0);
            let centered_offset = centered_glyph_offset(cell_width_f, glyph_width);
            let glyph_origin = point(origin.x + hex_grid_x(glyph.index, cell_width) + px(centered_offset), origin.y) + baseline_offset;

            while group_idx < groups.len() && glyph.index >= groups[group_idx].text_end {
                group_idx += 1;
            }
            if group_idx < groups.len() && groups[group_idx].text_start <= glyph.index && glyph.index < groups[group_idx].text_end {
                let mut color = group_colors.get(group_idx).copied();
                if let Some((p_char, typed_color, placeholder_color)) = pending_edit_colors {
                    if glyph.index == p_char {
                        color = Some(typed_color);
                    } else if glyph.index == p_char + 1 {
                        color = Some(placeholder_color);
                    }
                }
                if let Some(color) = color {
                    if glyph.is_emoji {
                        let _ = window.paint_emoji(glyph_origin, font_id, glyph.id, shaped.font_size);
                    } else {
                        let _ = window.paint_glyph(glyph_origin, font_id, glyph.id, shaped.font_size, color);
                    }
                }
            }
        }
    });
}

/// Paint every glyph centered in its fixed ASCII cell while reusing the line
/// shaped for the row. This keeps shaping batched and avoids per-character
/// string allocations and shaping passes.
pub fn paint_centered_ascii_glyphs(shaped: &ShapedLine, entries: &[AsciiCellEntry], origin: Point<Pixels>, line_height: Pixels, window: &mut Window) {
    let natural_width = shaped.width;
    let baseline_offset = point(px(0.0), (line_height - shaped.ascent - shaped.descent) / 2.0 + shaped.ascent);
    let mut entry_idx = 0;
    let mut glyphs = shaped
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter().map(move |glyph| (run.font_id, glyph)))
        .peekable();

    while let Some((font_id, glyph)) = glyphs.next() {
        let natural_end = glyphs.peek().map(|(_, next)| next.position.x).unwrap_or(natural_width);
        let glyph_width = f32::from(natural_end - glyph.position.x).max(0.0);
        let centered_offset = centered_glyph_offset(ASCII_CELL_WIDTH, glyph_width);

        while entry_idx < entries.len() && glyph.index >= entries[entry_idx].text_byte_end {
            entry_idx += 1;
        }

        if entry_idx < entries.len() && entries[entry_idx].text_byte_start <= glyph.index && glyph.index < entries[entry_idx].text_byte_end {
            let cell_idx = entries[entry_idx].cell_idx;
            let glyph_origin = point(origin.x + px(cell_idx as f32 * ASCII_CELL_WIDTH + centered_offset), origin.y) + baseline_offset;
            let color = entries[entry_idx].color;
            if glyph.is_emoji {
                let _ = window.paint_emoji(glyph_origin, font_id, glyph.id, shaped.font_size);
            } else {
                let _ = window.paint_glyph(glyph_origin, font_id, glyph.id, shaped.font_size, color);
            }
        }
    }
}

#[inline]
pub fn format_offset_08(offset: usize) -> SharedString {
    if offset > 0xFFFF_FFFF {
        SharedString::from(format!("{:016x}", offset))
    } else {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut buf = [b'0'; 8];
        let mut val = offset;
        for i in (0..8).rev() {
            buf[i] = DIGITS[val & 0xf];
            val >>= 4;
        }
        SharedString::from(std::str::from_utf8(&buf).expect("valid ascii utf8").to_string())
    }
}

pub struct RowPaintParams<'a> {
    pub row_idx: usize,
    pub bounds: Bounds<Pixels>,
    pub top_visible_row: usize,
    pub doc: &'a Document,
    pub line_starts: &'a LineMap,
    pub parse_result: Option<&'a ParseResult>,
    pub collapsed_structs: &'a HashSet<String>,
    pub encoding: Encoding,
    pub radix: DisplayRadix,
    pub group_size: ByteGroupSize,
    pub is_big_endian: bool,
    pub cursor_offset: usize,
    pub insert_cursor_offset: usize,
    pub min_sel: usize,
    pub max_sel: usize,
    pub highlights: &'a [(Range<usize>, Hsla)],
    pub bookmark_items: &'a [BookmarkItem],
    pub max_highlight_len: usize,
    pub show_offset: bool,
    pub show_ascii: bool,
    pub ascii_col_width: f32,
    pub ascii_scroll_x: f32,
    pub is_focused: bool,
    pub insert_mode: bool,
    pub active_column: EditColumn,
    pub cursor_visible: bool,
    pub pending_hex_digit: Option<(usize, u8)>,
    pub outer_scroll_x: f32,
    pub hex_scroll_x: f32,
    pub desc_scroll_x: f32,
    pub comment_scroll_x: f32,
    pub address_col_width: f32,
    pub hex_col_width: f32,
    pub hex_cell_width: Pixels,
    pub desc_col_width: f32,
    pub comment_col_width: f32,
    pub font_family: SharedString,
    pub font_size: Pixels,
}

pub fn paint_hex_row(params: RowPaintParams, window: &mut Window, cx: &mut App) {
    let (offset, next_offset) = if params.row_idx < params.line_starts.len() {
        let offset = match params.line_starts.get(params.row_idx) {
            Some(o) => o,
            None => return,
        };
        let next_offset = if params.row_idx + 1 < params.line_starts.len() {
            params.line_starts.get(params.row_idx + 1).unwrap_or(params.doc.buffer.len())
        } else {
            params.doc.buffer.len()
        };
        (offset, next_offset)
    } else if params.row_idx == params.line_starts.len() {
        let buf_len = params.doc.buffer.len();
        (buf_len, buf_len)
    } else {
        return;
    };

    let chunk_len = next_offset - offset;
    let chunk = params.doc.buffer.get_range(offset, chunk_len);

    let total_size = params.doc.buffer.len();
    let is_eof = params.cursor_offset == total_size;
    let last_line_idx = params.line_starts.len().saturating_sub(1);
    let last_line_start = params.line_starts.get(last_line_idx).unwrap_or(0);
    let bytes_per_row = params.line_starts.max_bytes_per_row();
    let is_eof_on_new_line = total_size > last_line_start && (total_size - last_line_start) >= bytes_per_row;
    let is_eof_on_this_row =
        is_eof && ((is_eof_on_new_line && params.row_idx == params.line_starts.len()) || (!is_eof_on_new_line && params.row_idx == last_line_idx));

    let is_struct_mode = params.parse_result.is_some();

    let active_row_highlights = row_highlights(params.highlights, params.max_highlight_len, offset, next_offset);
    let (selection_bg, caret_color, muted_color, fg_color, accent_fg_color, border_color, _sidebar_bg, bg_color_theme) = {
        let theme = cx.theme();
        (
            if params.is_focused {
                theme.selection.opacity(0.70)
            } else {
                theme.selection.opacity(0.45)
            },
            theme.caret,
            theme.muted_foreground,
            theme.foreground,
            theme.accent_foreground,
            theme.border,
            theme.sidebar,
            theme.background,
        )
    };
    let line_height = px(ROW_HEIGHT);
    let font = font(params.font_family);
    let insert_cursor_active = params.insert_mode && params.is_focused && window.is_window_active();
    let insert_selection_active = params.min_sel <= params.max_sel;

    // Check for Folded Bookmark or Unbookmarked Region Row
    let fold_summary = params
        .doc
        .fold_bookmark_summary_at(offset)
        .map(|s| (s.end_offset, Some(s.color), s.comment, s.is_unbookmarked));

    if let Some((fold_end, color, comment, is_unbookmarked)) = fold_summary {
        let (offset_w, gap) = if is_struct_mode {
            (params.address_col_width, SECTION_GAP)
        } else {
            (if params.show_offset { OFFSET_WIDTH } else { 0.0 }, SECTION_GAP)
        };

        let fold_start_addr = params.doc.offset_to_address(offset);
        let fold_end_addr = params.doc.offset_to_address(fold_end);

        if is_struct_mode || params.show_offset {
            let addr_str = format_offset_08(fold_start_addr);
            let run = TextRun {
                len: addr_str.len(),
                font: font.clone(),
                color: muted_color.opacity(0.8),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window.text_system().shape_line(addr_str, params.font_size, &[run], None);
            let addr_pos = point(params.bounds.left() + px(8.0), params.bounds.top() + px(2.0));
            let _ = shaped.paint(addr_pos, line_height, TextAlign::Left, None, window, cx);

            let base_x = params.bounds.left() + px(8.0);
            let div1_x = base_x + px(offset_w + (gap / 2.0));
            window.paint_quad(fill(
                Bounds::new(point(div1_x, params.bounds.top()), size(px(1.0), px(ROW_HEIGHT))),
                border_color.opacity(0.4),
            ));
        }

        let base_x = params.bounds.left() + px(8.0);
        let bar_start_x = base_x + px(offset_w + gap);
        let bar_end_x = params.bounds.right() - px(VERTICAL_SCROLLBAR_WIDTH + 8.0);
        let bar_width = (bar_end_x - bar_start_x).max(px(0.0));

        let fold_size = fold_end.saturating_sub(offset);
        let fold_label = if is_unbookmarked {
            format!(
                "── Unbookmarked: 0x{:08X} - 0x{:08X} (0x{:X} / {} bytes) ──",
                fold_start_addr, fold_end_addr, fold_size, fold_size
            )
        } else if !comment.is_empty() {
            format!(
                "── {}: 0x{:08X} - 0x{:08X} (0x{:X} / {} bytes) ──",
                comment, fold_start_addr, fold_end_addr, fold_size, fold_size
            )
        } else {
            format!(
                "── 0x{:08X} - 0x{:08X} (0x{:X} / {} bytes) ──",
                fold_start_addr, fold_end_addr, fold_size, fold_size
            )
        };
        let (fill_bg, border_tint, text_tint) = if is_unbookmarked {
            (muted_color.opacity(0.12), border_color.opacity(0.4), muted_color)
        } else if let Some(c) = color {
            (c.to_hsla().opacity(0.18), c.to_badge_hsla().opacity(0.5), c.to_badge_hsla())
        } else {
            (muted_color.opacity(0.12), border_color.opacity(0.4), muted_color)
        };

        let run = TextRun {
            len: fold_label.len(),
            font: font.clone(),
            color: text_tint,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped_label = window
            .text_system()
            .shape_line(SharedString::from(fold_label), params.font_size * 0.9, &[run], None);

        let bar_bounds = Bounds::new(point(bar_start_x, params.bounds.top() + px(2.0)), size(bar_width, px(ROW_HEIGHT - 4.0)));
        window.paint_quad(fill(bar_bounds, fill_bg));
        let outline = outline(bar_bounds, border_tint, BorderStyle::Solid).border_widths(px(1.0));
        window.paint_quad(outline);

        let text_x = bar_start_x + px(12.0);
        let text_pos = point(text_x, params.bounds.top() + px(2.0));
        let _ = shaped_label.paint(text_pos, line_height, TextAlign::Left, None, window, cx);
        return;
    }

    // Check for Address Gap Separator Row
    let gap_info = if chunk_len == 0 {
        params.doc.address_map.gap_before_offset(offset)
    } else {
        None
    };

    if let Some((gap_start, gap_end)) = gap_info {
        let (offset_w, gap) = if is_struct_mode {
            (params.address_col_width, SECTION_GAP)
        } else {
            (if params.show_offset { OFFSET_WIDTH } else { 0.0 }, SECTION_GAP)
        };

        if is_struct_mode || params.show_offset {
            let addr_str = SharedString::from("--------");
            let run = TextRun {
                len: addr_str.len(),
                font: font.clone(),
                color: muted_color.opacity(0.6),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window.text_system().shape_line(addr_str, params.font_size, &[run], None);
            let addr_pos = point(params.bounds.left() + px(8.0), params.bounds.top() + px(2.0));
            let _ = shaped.paint(addr_pos, line_height, TextAlign::Left, None, window, cx);

            let base_x = params.bounds.left() + px(8.0);
            let div1_x = base_x + px(offset_w + (gap / 2.0));
            window.paint_quad(fill(
                Bounds::new(point(div1_x, params.bounds.top()), size(px(1.0), px(ROW_HEIGHT))),
                border_color.opacity(0.4),
            ));
        }

        let base_x = params.bounds.left() + px(8.0);
        let bar_start_x = base_x + px(offset_w + gap);
        let bar_end_x = params.bounds.right() - px(VERTICAL_SCROLLBAR_WIDTH + 8.0);
        let bar_width = (bar_end_x - bar_start_x).max(px(0.0));

        let gap_size = gap_end.saturating_sub(gap_start);
        let gap_label = format!(
            "── Address Gap: 0x{:08X} - 0x{:08X} (0x{:X} / {} bytes unmapped) ──",
            gap_start, gap_end, gap_size, gap_size
        );
        let run = TextRun {
            len: gap_label.len(),
            font: font.clone(),
            color: accent_fg_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped_label = window
            .text_system()
            .shape_line(SharedString::from(gap_label), params.font_size * 0.9, &[run], None);

        let bar_bounds = Bounds::new(point(bar_start_x, params.bounds.top() + px(2.0)), size(bar_width, px(ROW_HEIGHT - 4.0)));
        let theme = cx.theme();
        window.paint_quad(fill(bar_bounds, theme.accent.opacity(0.12)));
        let outline = outline(bar_bounds, border_color.opacity(0.4), BorderStyle::Solid).border_widths(px(1.0));
        window.paint_quad(outline);

        let text_x = bar_start_x + px(12.0);
        let text_pos = point(text_x, params.bounds.top() + px(2.0));
        let _ = shaped_label.paint(text_pos, line_height, TextAlign::Left, None, window, cx);
        return;
    }

    let physical_address = params.doc.offset_to_address(offset);

    // 1. Draw Left Columns (Address OR Offset)
    let (offset_w, gap) = if is_struct_mode {
        let addr_str = format_offset_08(physical_address);
        let run = TextRun {
            len: addr_str.len(),
            font: font.clone(),
            color: muted_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = window.text_system().shape_line(addr_str, params.font_size, &[run], None);
        let addr_pos = point(params.bounds.left() + px(8.0), params.bounds.top() + px(2.0));
        let _ = shaped.paint(addr_pos, line_height, TextAlign::Left, None, window, cx);
        (params.address_col_width, SECTION_GAP)
    } else {
        if params.show_offset {
            let offset_str = format_offset_08(physical_address);
            let run = TextRun {
                len: offset_str.len(),
                font: font.clone(),
                color: muted_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window.text_system().shape_line(offset_str, params.font_size, &[run], None);
            let offset_pos = point(params.bounds.left() + px(8.0), params.bounds.top() + px(2.0));
            let _ = shaped.paint(offset_pos, line_height, TextAlign::Left, None, window, cx);
        }
        (if params.show_offset { OFFSET_WIDTH } else { 0.0 }, SECTION_GAP)
    };

    let base_x = params.bounds.left() + px(8.0);
    let hex_start_x = base_x + px(offset_w + gap) - px(params.outer_scroll_x);
    let hex_end_x = hex_start_x + px(params.hex_col_width);
    let (comment_start_x, ascii_width) = if is_struct_mode {
        let desc_start_x = hex_end_x + px(gap);
        let comment_start_x = desc_start_x + px(params.desc_col_width + gap);
        (comment_start_x, 0.0)
    } else {
        let ascii_w = if params.show_ascii { params.ascii_col_width } else { 0.0 };
        let comment_start_x = if params.show_ascii {
            hex_end_x + px(gap + ascii_w + gap)
        } else {
            hex_end_x + px(gap)
        };
        (comment_start_x, ascii_w)
    };

    // Vertical Column Divider Borders (matching header splitters exactly)
    let border_line_color = border_color.opacity(0.4);
    if is_struct_mode || params.show_offset {
        let div1_x = base_x + px(offset_w + (gap / 2.0));
        window.paint_quad(fill(
            Bounds::new(point(div1_x, params.bounds.top()), size(px(1.0), px(ROW_HEIGHT))),
            border_line_color,
        ));
    }
    let scrollable_mask_bounds = {
        let left = base_x + px(offset_w + gap);
        let right = params.bounds.right() - px(VERTICAL_SCROLLBAR_WIDTH);
        Bounds::new(point(left, params.bounds.top()), size((right - left).max(px(0.0)), px(ROW_HEIGHT)))
    };
    window.with_content_mask(
        Some(ContentMask {
            bounds: scrollable_mask_bounds,
        }),
        |window| {
            let div2_x = hex_start_x + px(params.hex_col_width + (gap / 2.0));
            window.paint_quad(fill(
                Bounds::new(point(div2_x, params.bounds.top()), size(px(1.0), px(ROW_HEIGHT))),
                border_line_color,
            ));
            if is_struct_mode {
                let desc_start_x = hex_end_x + px(gap);
                let div3_x = desc_start_x + px(params.desc_col_width + (gap / 2.0));
                let div4_x = comment_start_x + px(params.comment_col_width + (gap / 2.0));
                window.paint_quad(fill(
                    Bounds::new(point(div3_x, params.bounds.top()), size(px(1.0), px(ROW_HEIGHT))),
                    border_line_color,
                ));
                window.paint_quad(fill(
                    Bounds::new(point(div4_x, params.bounds.top()), size(px(1.0), px(ROW_HEIGHT))),
                    border_line_color,
                ));
            } else {
                if params.show_ascii {
                    let div3_x = hex_end_x + px(gap + ascii_width + (gap / 2.0));
                    window.paint_quad(fill(
                        Bounds::new(point(div3_x, params.bounds.top()), size(px(1.0), px(ROW_HEIGHT))),
                        border_line_color,
                    ));
                }
                let div4_x = comment_start_x + px(params.comment_col_width + (gap / 2.0));
                window.paint_quad(fill(
                    Bounds::new(point(div4_x, params.bounds.top()), size(px(1.0), px(ROW_HEIGHT))),
                    border_line_color,
                ));
            }
        },
    );

    // Build and shape the exact text stream before painting any geometry.
    // Group geometry uses the fixed cell grid; glyphs are centered in that
    // grid during the text pass.
    let mut hex_source = build_hex_text_source(chunk, offset, params.radix, params.group_size, params.is_big_endian);
    let mut hex_text = hex_source.text.to_string();
    let mut pending_byte_char_start: Option<usize> = None;

    if let Some((pending_pos, pending_digit)) = params.pending_hex_digit
        && params.active_column == EditColumn::Hex
    {
        if pending_pos >= offset && pending_pos < next_offset {
            let chunk_pos = pending_pos - offset;
            if let Some(group) = hex_source.groups.iter().find(|g| g.chunk_start <= chunk_pos && chunk_pos < g.chunk_end) {
                let byte_index = chunk_pos - group.chunk_start;
                let group_len = group.chunk_end - group.chunk_start;
                let visual_byte = if !params.is_big_endian && group_len > 1 {
                    group_len.saturating_sub(byte_index + 1)
                } else {
                    byte_index
                };
                let char_start = group.text_start + visual_byte * 2;
                if char_start + 1 < hex_text.len() {
                    let digit_char = char::from_digit(pending_digit as u32, 16).unwrap_or('?');
                    let mut bytes = hex_text.into_bytes();
                    bytes[char_start] = digit_char as u8;
                    bytes[char_start + 1] = b'_';
                    hex_text = String::from_utf8(bytes).expect("valid utf8");
                    pending_byte_char_start = Some(char_start);
                }
            }
        } else if is_eof_on_this_row && pending_pos == total_size {
            let digit_char = char::from_digit(pending_digit as u32, 16).unwrap_or('?');
            let group_bytes = params.group_size.byte_count();
            if hex_source.groups.is_empty() {
                hex_text = format!("{}_", digit_char);
                hex_source.groups.push(HexGroupInfo {
                    chunk_start: 0,
                    chunk_end: 0,
                    start_slot: 0,
                    text_start: 0,
                    text_end: 2,
                });
                pending_byte_char_start = Some(0);
            } else {
                let last_group = *hex_source.groups.last().unwrap();
                let last_group_len = last_group.chunk_end - last_group.chunk_start;
                let is_complete = (last_group.start_slot + last_group_len) == group_bytes;
                if is_complete {
                    hex_text.push(' ');
                    let text_start = hex_text.len();
                    hex_text.push(digit_char);
                    hex_text.push('_');
                    let text_end = hex_text.len();
                    hex_source.groups.push(HexGroupInfo {
                        chunk_start: chunk_len,
                        chunk_end: chunk_len,
                        start_slot: 0,
                        text_start,
                        text_end,
                    });
                    pending_byte_char_start = Some(text_start);
                } else {
                    let visual_slot = last_group.start_slot + last_group_len;
                    let char_start = last_group.text_start + visual_slot * 2;
                    if char_start + 1 < hex_text.len() {
                        let mut bytes = hex_text.into_bytes();
                        bytes[char_start] = digit_char as u8;
                        bytes[char_start + 1] = b'_';
                        hex_text = String::from_utf8(bytes).expect("valid utf8");
                        pending_byte_char_start = Some(char_start);
                    }
                }
            }
        }
    }

    let mut hex_runs: Vec<TextRun> = Vec::with_capacity(hex_source.groups.len() * 2);
    let mut group_visuals: Vec<(Option<Hsla>, bool, bool)> = Vec::with_capacity(hex_source.groups.len());
    let mut group_text_colors: Vec<Hsla> = Vec::with_capacity(hex_source.groups.len());

    for (group_idx, group) in hex_source.groups.iter().enumerate() {
        let item_start_offset = offset + group.chunk_start;
        let item_end_offset = offset + group.chunk_end;
        let item_slice = if group.chunk_end <= chunk.len() && group.chunk_start <= group.chunk_end {
            &chunk[group.chunk_start..group.chunk_end]
        } else {
            &[]
        };
        let is_cursor = if group.chunk_start == group.chunk_end {
            is_eof_on_this_row
        } else {
            params.cursor_offset >= item_start_offset && params.cursor_offset < item_end_offset
        };
        let is_zero = is_group_zero(item_slice);
        let is_selected = if params.min_sel <= params.max_sel {
            item_start_offset <= params.max_sel && item_end_offset > params.min_sel
        } else {
            false
        };

        let current_hl_color = highlight_color_for_range(item_start_offset, item_end_offset, active_row_highlights);
        let has_hl = current_hl_color.is_some();
        let text_color = if (is_cursor && params.is_focused) || is_selected || has_hl {
            fg_color
        } else if is_zero {
            muted_color.opacity(0.5)
        } else {
            fg_color
        };

        group_visuals.push((current_hl_color, is_selected, is_cursor));
        group_text_colors.push(text_color);

        if group_idx > 0 {
            hex_runs.push(TextRun {
                len: 1,
                font: font.clone(),
                color: hsla(0.0, 0.0, 0.0, 0.0),
                background_color: None,
                underline: None,
                strikethrough: None,
            });
        }
        hex_runs.push(TextRun {
            len: group.text_end - group.text_start,
            font: font.clone(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    let shaped_hex = window
        .text_system()
        .shape_line(SharedString::from(hex_text.clone()), params.font_size, &hex_runs, None);
    let pending_edit_colors = pending_byte_char_start.map(|char_start| (char_start, caret_color, muted_color.opacity(0.6)));
    let text_origin_x = hex_start_x - px(params.hex_scroll_x);
    let total_data_width = f32::from(hex_grid_width(hex_text.len(), params.hex_cell_width));

    // 2. Background Quads Pass for Data Items (with clipping mask)
    let hex_mask_bounds = Bounds::new(point(hex_start_x, params.bounds.top()), size(px(params.hex_col_width), px(ROW_HEIGHT)));

    window.with_content_mask(
        Some(ContentMask {
            bounds: scrollable_mask_bounds,
        }),
        |window| {
            window.with_content_mask(Some(ContentMask { bounds: hex_mask_bounds }), |window| {
                for (item_idx, group) in hex_source.groups.iter().enumerate() {
                    let item_start_offset = offset + group.chunk_start;
                    let (hl_color, is_selected, is_cursor) = group_visuals[item_idx];
                    let (group_start_x, group_end_x) = hex_group_x(*group, text_origin_x, params.hex_cell_width);
                    let previous_group_end_x = if item_idx > 0 {
                        hex_group_x(hex_source.groups[item_idx - 1], text_origin_x, params.hex_cell_width).1
                    } else {
                        group_start_x
                    };
                    let has_next = item_idx + 1 < hex_source.groups.len();
                    let next_group_start_x = if has_next {
                        hex_group_x(hex_source.groups[item_idx + 1], text_origin_x, params.hex_cell_width).0
                    } else {
                        group_end_x
                    };

                    // 1. Highlight (bookmark) background quad
                    if let Some(color) = hl_color
                        && color.a > 0.0
                    {
                        let prev_has_hl = item_idx > 0 && group_visuals[item_idx - 1].0.is_some();
                        let next_has_hl = has_next && group_visuals[item_idx + 1].0.is_some();
                        let hl_start_x = if prev_has_hl {
                            pixel_midpoint(previous_group_end_x, group_start_x)
                        } else {
                            group_start_x
                        };
                        let hl_end_x = if next_has_hl {
                            pixel_midpoint(group_end_x, next_group_start_x)
                        } else {
                            group_end_x
                        };
                        let hl_fill_bounds = Bounds::new(
                            point(hl_start_x, params.bounds.top() + px(1.0)),
                            size(hl_end_x - hl_start_x, px(ROW_HEIGHT - 2.0)),
                        );
                        window.paint_quad(fill(hl_fill_bounds, color));
                    }

                    // 2. Translucent selection quad (overlaid on top of highlight)
                    if is_selected && selection_bg.a > 0.0 {
                        let prev_is_sel = item_idx > 0 && group_visuals[item_idx - 1].1;
                        let next_is_sel = has_next && group_visuals[item_idx + 1].1;
                        let sel_start_x = if prev_is_sel {
                            pixel_midpoint(previous_group_end_x, group_start_x)
                        } else {
                            group_start_x
                        };
                        let sel_end_x = if next_is_sel {
                            pixel_midpoint(group_end_x, next_group_start_x)
                        } else {
                            group_end_x
                        };
                        let sel_fill_bounds = Bounds::new(
                            point(sel_start_x, params.bounds.top() + px(1.0)),
                            size(sel_end_x - sel_start_x, px(ROW_HEIGHT - 2.0)),
                        );
                        window.paint_quad(fill(sel_fill_bounds, selection_bg));
                    }

                    // 3. Cursor border / underscore
                    if is_cursor && !params.insert_mode {
                        let cursor_border_color = if params.is_focused {
                            caret_color
                        } else {
                            darken_cursor_color(muted_color).opacity(0.8)
                        };

                        if params.radix == DisplayRadix::Hexadecimal {
                            let byte_index = params.cursor_offset.saturating_sub(item_start_offset);
                            let group_len = group.chunk_end.saturating_sub(group.chunk_start);
                            let visual_byte = if group_len == 0 {
                                0
                            } else if !params.is_big_endian && group_len > 1 {
                                group_len.saturating_sub(byte_index + 1)
                            } else {
                                byte_index
                            };
                            let byte_char_start = group.text_start + visual_byte * 2;
                            let byte_start_x = text_origin_x + hex_grid_x(byte_char_start, params.hex_cell_width);
                            let byte_width = hex_grid_x(2, params.hex_cell_width);
                            let byte_box_bounds = Bounds::new(point(byte_start_x, params.bounds.top() + px(1.0)), size(byte_width, px(ROW_HEIGHT - 2.0)));

                            if params.active_column == EditColumn::Hex {
                                paint_cursor_border(window, byte_box_bounds, cursor_border_color);
                            } else {
                                paint_underscore_cursor_at(window, byte_box_bounds, byte_start_x, byte_width, cursor_border_color);
                            }
                        } else {
                            let has_bg = hl_color.is_some() || is_selected;
                            let prev_has_bg = item_idx > 0 && (group_visuals[item_idx - 1].0.is_some() || group_visuals[item_idx - 1].1);
                            let next_has_bg = has_next && (group_visuals[item_idx + 1].0.is_some() || group_visuals[item_idx + 1].1);
                            let (cursor_start_x, cursor_end_x) = if has_bg {
                                (
                                    if prev_has_bg {
                                        pixel_midpoint(previous_group_end_x, group_start_x)
                                    } else {
                                        group_start_x
                                    },
                                    if next_has_bg {
                                        pixel_midpoint(group_end_x, next_group_start_x)
                                    } else {
                                        group_end_x
                                    },
                                )
                            } else {
                                (group_start_x, group_end_x)
                            };
                            let cursor_width = cursor_end_x - cursor_start_x;
                            let item_box_bounds = Bounds::new(point(cursor_start_x, params.bounds.top() + px(1.0)), size(cursor_width, px(ROW_HEIGHT - 2.0)));
                            if params.active_column == EditColumn::Hex {
                                paint_cursor_border(window, item_box_bounds, cursor_border_color);
                            } else {
                                paint_underscore_cursor_at(window, item_box_bounds, cursor_start_x, cursor_width, cursor_border_color);
                            }
                        }
                    }
                }

                if is_eof_on_this_row && !params.insert_mode && pending_byte_char_start.is_none() {
                    let cursor_border_color = if params.is_focused {
                        caret_color
                    } else {
                        darken_cursor_color(muted_color).opacity(0.8)
                    };
                    let (eof_start_x, eof_width) = if hex_source.groups.is_empty() {
                        (text_origin_x, hex_grid_x(2, params.hex_cell_width))
                    } else {
                        let last_group = *hex_source.groups.last().unwrap();
                        let group_bytes = params.group_size.byte_count();
                        let last_group_len = last_group.chunk_end - last_group.chunk_start;
                        let is_complete = (last_group.start_slot + last_group_len) == group_bytes;
                        let eof_char_start = if is_complete {
                            last_group.text_end + 1
                        } else {
                            last_group.text_start + (last_group.start_slot + last_group_len) * 2
                        };
                        (
                            text_origin_x + hex_grid_x(eof_char_start, params.hex_cell_width),
                            hex_grid_x(2, params.hex_cell_width),
                        )
                    };
                    let byte_box_bounds = Bounds::new(point(eof_start_x, params.bounds.top() + px(1.0)), size(eof_width, px(ROW_HEIGHT - 2.0)));
                    if params.active_column == EditColumn::Hex {
                        paint_cursor_border(window, byte_box_bounds, cursor_border_color);
                    } else {
                        paint_underscore_cursor_at(window, byte_box_bounds, eof_start_x, eof_width, cursor_border_color);
                    }
                }

                // 3. Text Pass for Data Items (same shaped line as the geometry pass)
                if !hex_source.text.is_empty() {
                    paint_centered_hex_glyphs(
                        &shaped_hex,
                        &hex_source.groups,
                        &group_text_colors,
                        pending_edit_colors,
                        params.hex_cell_width,
                        point(text_origin_x, params.bounds.top() + px(2.0)),
                        line_height,
                        window,
                    );
                }

                if insert_cursor_active {
                    let has_pending_nibble = params.pending_hex_digit.is_some_and(|(offset, _)| offset == params.insert_cursor_offset);
                    let insert_cursor_params = HexInsertCursorParams {
                        radix: params.radix,
                        group_size: params.group_size,
                        is_big_endian: params.is_big_endian,
                        selection_active: insert_selection_active,
                        has_pending_nibble,
                        origin_x: text_origin_x,
                        cell_width: params.hex_cell_width,
                    };
                    for (item_idx, group) in hex_source.groups.iter().enumerate() {
                        let cursor_border_color = caret_color;
                        let (group_start_x, group_end_x) = hex_group_x(*group, text_origin_x, params.hex_cell_width);
                        let item_box_bounds = Bounds::new(
                            point(group_start_x, params.bounds.top() + px(1.0)),
                            size(group_end_x - group_start_x, px(ROW_HEIGHT - 2.0)),
                        );
                        let item_start_offset = offset + group.chunk_start;
                        let item_end_offset = offset + group.chunk_end;
                        let cursor_geometry = if params.insert_cursor_offset >= item_start_offset && params.insert_cursor_offset < item_end_offset {
                            hex_insert_cursor_geometry(*group, params.insert_cursor_offset.saturating_sub(offset), insert_cursor_params)
                        } else if params.insert_cursor_offset == params.doc.buffer.len()
                            && item_idx + 1 == hex_source.groups.len()
                            && item_end_offset == next_offset
                            && next_offset == params.doc.buffer.len()
                        {
                            Some(hex_insert_cursor_end_geometry(*group, text_origin_x, params.hex_cell_width, params.group_size))
                        } else {
                            None
                        };

                        if let Some((cursor_x, cursor_width)) = cursor_geometry {
                            match params.active_column {
                                EditColumn::Hex if params.cursor_visible => {
                                    paint_insert_cursor_at(window, item_box_bounds, cursor_x, cursor_border_color);
                                }
                                EditColumn::Ascii => {
                                    paint_underscore_cursor_at(window, item_box_bounds, cursor_x, cursor_width, cursor_border_color);
                                }
                                EditColumn::Hex => {}
                            }
                        }
                    }

                    if hex_source.groups.is_empty() && params.insert_cursor_offset == offset {
                        let cursor_border_color = caret_color;
                        let item_box_bounds = Bounds::new(
                            point(text_origin_x, params.bounds.top() + px(1.0)),
                            size(params.hex_cell_width, px(ROW_HEIGHT - 2.0)),
                        );
                        let empty_slot_width =
                            px(digit_count(params.radix, params.group_size) as f32 / params.group_size.byte_count() as f32 * f32::from(params.hex_cell_width));
                        match params.active_column {
                            EditColumn::Hex if params.cursor_visible => {
                                paint_insert_cursor_at(window, item_box_bounds, text_origin_x, cursor_border_color);
                            }
                            EditColumn::Ascii => {
                                paint_underscore_cursor_at(window, item_box_bounds, text_origin_x, empty_slot_width, cursor_border_color);
                            }
                            EditColumn::Hex => {}
                        }
                    }
                }

                // Left-edge subtle gradient fade when hex_scroll_x > 1.0
                if params.hex_scroll_x > 1.0 {
                    let bg = bg_color_theme;
                    for step in 0..5 {
                        let x = hex_start_x + px(step as f32 * 3.2);
                        let alpha = 1.0 - (step as f32 / 5.0);
                        window.paint_quad(fill(
                            Bounds::new(point(x, params.bounds.top()), size(px(3.4), px(ROW_HEIGHT))),
                            bg.opacity(alpha * 0.95),
                        ));
                    }
                }

                // Right-edge subtle gradient fade when row data overflows hex_col_width
                if params.hex_scroll_x + params.hex_col_width < total_data_width - 1.0 {
                    let fade_w = 22.0;
                    let fade_start = hex_end_x - px(fade_w);
                    let bg = bg_color_theme;
                    for step in 0..6 {
                        let x = fade_start + px(step as f32 * 3.6);
                        let alpha = (step + 1) as f32 / 7.0;
                        window.paint_quad(fill(
                            Bounds::new(point(x, params.bounds.top()), size(px(3.8), px(ROW_HEIGHT))),
                            bg.opacity(alpha * 0.95),
                        ));
                    }
                }
            });
        },
    );

    // 3. ASCII Column (when not in structure definition mode and ASCII view is enabled)
    if !is_struct_mode && params.show_ascii {
        window.with_content_mask(
            Some(ContentMask {
                bounds: scrollable_mask_bounds,
            }),
            |window| {
                let ascii_start_x = hex_end_x + px(gap);
                let ascii_content_start_x = ascii_start_x - px(params.ascii_scroll_x);
                let ascii_mask_bounds = Bounds::new(point(ascii_start_x, params.bounds.top()), size(px(ascii_width), px(ROW_HEIGHT)));
                window.with_content_mask(Some(ContentMask { bounds: ascii_mask_bounds }), |window| {
                    let char_map = build_ascii_char_map(params.encoding, params.doc.buffer.data(), offset, chunk.len());
                    let group_bytes = params.group_size.byte_count();

                    // 1. Background Quads Pass for ASCII Cells
                    for (j, _) in chunk.iter().enumerate() {
                        let byte_pos = offset + j;
                        let in_selected_group = if params.min_sel <= params.max_sel {
                            let group_start = (byte_pos / group_bytes) * group_bytes;
                            let group_end = group_start + group_bytes;
                            group_start <= params.max_sel && group_end > params.min_sel
                        } else {
                            false
                        };

                        let current_hl_color = highlight_color_for_range(byte_pos, byte_pos.saturating_add(1), active_row_highlights);

                        let ascii_item_bounds = Bounds::new(
                            point(ascii_content_start_x + px(j as f32 * ASCII_CELL_WIDTH), params.bounds.top() + px(1.0)),
                            size(px(ASCII_CELL_WIDTH), px(ROW_HEIGHT - 2.0)),
                        );

                        // Highlight (bookmark) quad
                        if let Some(hl_color) = current_hl_color
                            && hl_color.a > 0.0
                        {
                            window.paint_quad(fill(ascii_item_bounds, hl_color));
                        }

                        // Translucent selection quad (overlaid on top of highlight)
                        if in_selected_group && selection_bg.a > 0.0 {
                            window.paint_quad(fill(ascii_item_bounds, selection_bg));
                        }
                    }

                    // 2. Cursor Border Pass for ASCII Column / Groups
                    if params.active_column == EditColumn::Ascii {
                        let char_range = params.encoding.char_range_at(params.doc.buffer.data(), params.cursor_offset);
                        if char_range.start < next_offset && char_range.end > offset && !params.insert_mode {
                            let cell_start = char_range.start.saturating_sub(offset);
                            let cell_end = (char_range.end.saturating_sub(offset)).min(chunk.len());
                            if cell_end > cell_start {
                                let cursor_border_color = if params.is_focused {
                                    caret_color
                                } else {
                                    darken_cursor_color(muted_color).opacity(0.8)
                                };
                                let char_start_x = ascii_content_start_x + px(cell_start as f32 * ASCII_CELL_WIDTH);
                                let char_width = px((cell_end - cell_start) as f32 * ASCII_CELL_WIDTH);
                                let ascii_char_bounds = Bounds::new(point(char_start_x, params.bounds.top() + px(1.0)), size(char_width, px(ROW_HEIGHT - 2.0)));
                                paint_cursor_border(window, ascii_char_bounds, cursor_border_color);
                            }
                        } else if is_eof_on_this_row && !params.insert_mode {
                            let cursor_border_color = if params.is_focused {
                                caret_color
                            } else {
                                darken_cursor_color(muted_color).opacity(0.8)
                            };
                            let ascii_x = ascii_content_start_x + px(chunk_len as f32 * ASCII_CELL_WIDTH);
                            let ascii_box_bounds = Bounds::new(point(ascii_x, params.bounds.top() + px(1.0)), size(px(ASCII_CELL_WIDTH), px(ROW_HEIGHT - 2.0)));
                            paint_cursor_border(window, ascii_box_bounds, cursor_border_color);
                        }
                    } else {
                        for group in hex_source.groups.iter() {
                            let item_start_offset = offset + group.chunk_start;
                            let item_end_offset = offset + group.chunk_end;
                            let is_cursor = params.cursor_offset >= item_start_offset && params.cursor_offset < item_end_offset;

                            if is_cursor && !params.insert_mode {
                                let cursor_border_color = if params.is_focused {
                                    caret_color
                                } else {
                                    darken_cursor_color(muted_color).opacity(0.8)
                                };
                                let group_start_x = ascii_content_start_x + px(group.chunk_start as f32 * ASCII_CELL_WIDTH);
                                let group_width = px((group.chunk_end - group.chunk_start) as f32 * ASCII_CELL_WIDTH);
                                let ascii_group_bounds =
                                    Bounds::new(point(group_start_x, params.bounds.top() + px(1.0)), size(group_width, px(ROW_HEIGHT - 2.0)));
                                paint_underscore_cursor_at(window, ascii_group_bounds, group_start_x, group_width, cursor_border_color);
                            }
                        }
                        if is_eof_on_this_row && !params.insert_mode {
                            let cursor_border_color = if params.is_focused {
                                caret_color
                            } else {
                                darken_cursor_color(muted_color).opacity(0.8)
                            };
                            let ascii_x = ascii_content_start_x + px(chunk_len as f32 * ASCII_CELL_WIDTH);
                            let ascii_box_bounds = Bounds::new(point(ascii_x, params.bounds.top() + px(1.0)), size(px(ASCII_CELL_WIDTH), px(ROW_HEIGHT - 2.0)));
                            paint_underscore_cursor_at(window, ascii_box_bounds, ascii_x, px(ASCII_CELL_WIDTH), cursor_border_color);
                        }
                    }

                    // Paint each glyph centered in its fixed byte cell using batched line shaping.
                    let mut ascii_text = String::with_capacity(chunk.len());
                    let mut ascii_runs: Vec<TextRun> = Vec::with_capacity(chunk.len());
                    let mut ascii_entries: Vec<AsciiCellEntry> = Vec::with_capacity(chunk.len());

                    for (j, opt) in char_map.into_iter().enumerate() {
                        let item = if let Some(item) = opt { item } else { continue };
                        let c = item.character();
                        let byte_pos = offset + j;
                        let current_hl_color = highlight_color_for_range(byte_pos, byte_pos.saturating_add(1), active_row_highlights);
                        let has_hl = current_hl_color.is_some();
                        let in_selected_group = if params.min_sel <= params.max_sel {
                            let group_start = (byte_pos / group_bytes) * group_bytes;
                            let group_end = group_start + group_bytes;
                            group_start <= params.max_sel && group_end > params.min_sel
                        } else {
                            false
                        };
                        let text_color = if !item.is_printable() {
                            if has_hl || in_selected_group {
                                fg_color.opacity(0.65)
                            } else {
                                muted_color.opacity(0.4)
                            }
                        } else {
                            fg_color
                        };

                        let text_byte_start = ascii_text.len();
                        ascii_text.push(c);
                        let text_byte_end = ascii_text.len();

                        ascii_runs.push(TextRun {
                            len: text_byte_end - text_byte_start,
                            font: font.clone(),
                            color: text_color,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        });

                        ascii_entries.push(AsciiCellEntry {
                            cell_idx: j,
                            text_byte_start,
                            text_byte_end,
                            color: text_color,
                        });
                    }

                    if !ascii_text.is_empty() {
                        let shaped = window
                            .text_system()
                            .shape_line(SharedString::from(ascii_text), params.font_size, &ascii_runs, None);
                        paint_centered_ascii_glyphs(
                            &shaped,
                            &ascii_entries,
                            point(ascii_content_start_x, params.bounds.top() + px(2.0)),
                            line_height,
                            window,
                        );
                    }

                    if insert_cursor_active {
                        let cursor_x = if params.insert_cursor_offset >= offset && params.insert_cursor_offset < next_offset {
                            Some(ascii_content_start_x + px((params.insert_cursor_offset - offset) as f32 * ASCII_CELL_WIDTH))
                        } else if params.insert_cursor_offset == params.doc.buffer.len()
                            && next_offset == params.doc.buffer.len()
                            && params.insert_cursor_offset == next_offset
                        {
                            Some(ascii_content_start_x + px(chunk_len as f32 * ASCII_CELL_WIDTH))
                        } else if chunk_len == 0 && params.insert_cursor_offset == offset {
                            Some(ascii_content_start_x)
                        } else {
                            None
                        };

                        if let Some(cursor_x) = cursor_x {
                            let cursor_border_color = caret_color;
                            let ascii_cursor_bounds =
                                Bounds::new(point(cursor_x, params.bounds.top() + px(1.0)), size(px(ASCII_CELL_WIDTH), px(ROW_HEIGHT - 2.0)));
                            match params.active_column {
                                EditColumn::Ascii if params.cursor_visible => {
                                    paint_insert_cursor_at(window, ascii_cursor_bounds, cursor_x, cursor_border_color);
                                }
                                EditColumn::Hex => {
                                    paint_underscore_cursor_at(window, ascii_cursor_bounds, cursor_x, px(ASCII_CELL_WIDTH), cursor_border_color);
                                }
                                EditColumn::Ascii => {}
                            }
                        }
                    }

                    // Edge fades when the ASCII row is horizontally scrolled or clipped.
                    if params.ascii_scroll_x > 1.0 {
                        let bg = bg_color_theme;
                        for step in 0..5 {
                            let x = ascii_start_x + px(step as f32 * 3.2);
                            let alpha = 1.0 - (step as f32 / 5.0);
                            window.paint_quad(fill(
                                Bounds::new(point(x, params.bounds.top()), size(px(3.4), px(ROW_HEIGHT))),
                                bg.opacity(alpha * 0.95),
                            ));
                        }
                    }

                    if params.ascii_scroll_x + ascii_width < chunk.len() as f32 * ASCII_CELL_WIDTH - 1.0 {
                        let fade_w = 22.0;
                        let fade_start = ascii_start_x + px(ascii_width - fade_w);
                        let bg = bg_color_theme;
                        for step in 0..6 {
                            let x = fade_start + px(step as f32 * 3.6);
                            let alpha = (step + 1) as f32 / 7.0;
                            window.paint_quad(fill(
                                Bounds::new(point(x, params.bounds.top()), size(px(3.8), px(ROW_HEIGHT))),
                                bg.opacity(alpha * 0.95),
                            ));
                        }
                    }
                });
            },
        );
    }

    // 4. Description Column (when structure definition is present)
    if let Some(parse_res) = params.parse_result {
        let desc_start_x = hex_end_x + px(SECTION_GAP);
        let desc_end_x = desc_start_x + px(params.desc_col_width);

        let is_structure_header_row = chunk_len == 0;
        let query_len = chunk_len.max(1);
        let mut container_structs: Cow<'_, [IndexedField]> = if parse_res.is_live() {
            Cow::Owned(parse_res.find_live_container_structs_starting_at(offset, query_len))
        } else {
            Cow::Borrowed(parse_res.find_container_structs_starting_at(offset, query_len))
        };

        // A structure header gets its own zero-byte row. On the following
        // data row, keep only collapsed containers visible so the parent is
        // not printed twice.
        let has_header_before = !is_structure_header_row
            && params.row_idx > 0
            && params.line_starts.get(params.row_idx - 1) == Some(offset)
            && parse_res.has_structure_header_at(offset, params.collapsed_structs);
        if has_header_before {
            container_structs = Cow::Owned(
                container_structs
                    .iter()
                    .filter(|container| params.collapsed_structs.contains(&container.id))
                    .cloned()
                    .collect(),
            );
        }

        if is_structure_header_row {
            container_structs = Cow::Owned(
                container_structs
                    .iter()
                    .filter(|container| !container.is_instance && container.size > 0)
                    .cloned()
                    .collect(),
            );
        }

        if is_structure_header_row && container_structs.len() > 1 {
            let mut same_offset_rows = 0;
            let mut previous_row = params.row_idx;
            while previous_row > 0 && params.line_starts.get(previous_row - 1) == Some(offset) {
                same_offset_rows += 1;
                previous_row -= 1;
            }
            if let Some(container) = container_structs.get(same_offset_rows).cloned() {
                container_structs = Cow::Owned(vec![container]);
            }
        }

        let leaf_fields: Cow<'_, [IndexedField]> = if is_structure_header_row {
            Cow::Owned(Vec::new())
        } else if parse_res.is_live() {
            Cow::Owned(parse_res.find_live_leaf_fields_starting_at(offset, chunk_len))
        } else {
            Cow::Borrowed(parse_res.find_leaf_fields_starting_at(offset, chunk_len))
        };
        let active_ranges = if parse_res.is_live() {
            Vec::new()
        } else {
            parse_res.find_active_struct_ranges(offset, query_len)
        };

        let is_collapsed = container_structs.first().map(|c| params.collapsed_structs.contains(&c.id)).unwrap_or(false);

        let struct_depth = active_ranges.len().saturating_sub(1);
        let indent_level = if !container_structs.is_empty() {
            active_ranges
                .iter()
                .find(|r| container_structs.first().map(|c| c.id == r.id).unwrap_or(false))
                .map(|r| r.depth)
                .or_else(|| container_structs.first().map(|container| container.depth))
                .unwrap_or(struct_depth)
        } else if !active_ranges.is_empty() {
            active_ranges.len()
        } else {
            leaf_fields.first().map(|field| field.depth).unwrap_or(0)
        };
        let indent_px = indent_level as f32 * 14.0;

        let mut desc_parts = Vec::new();
        if let Some(container) = container_structs.first() {
            let icon = if is_collapsed { "▶" } else { "▼" };
            let label = container.format_container_label();
            if is_collapsed {
                desc_parts.push(format!("{} {} ({} bytes)", icon, label, container.size));
            } else {
                desc_parts.push(format!("{} {}", icon, label));
            }
        }
        if !is_collapsed {
            for f in leaf_fields.iter() {
                desc_parts.push(f.format_expression().to_string());
            }
        }

        if !desc_parts.is_empty() {
            let expr_shared = SharedString::from(desc_parts.join("  "));
            let text_color = if !container_structs.is_empty() { accent_fg_color } else { fg_color };
            let run = TextRun {
                len: expr_shared.len(),
                font: font.clone(),
                color: text_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped_expr = window.text_system().shape_line(expr_shared, params.font_size, &[run], None);
            let desc_mask_bounds = Bounds::new(point(desc_start_x, params.bounds.top()), size(px(params.desc_col_width), px(ROW_HEIGHT)));
            let desc_text_width = f32::from(shaped_expr.width) + indent_px + 8.0;

            window.with_content_mask(
                Some(ContentMask {
                    bounds: scrollable_mask_bounds,
                }),
                |window| {
                    window.with_content_mask(Some(ContentMask { bounds: desc_mask_bounds }), |window| {
                        let _ = shaped_expr.paint(
                            point(desc_start_x - px(params.desc_scroll_x) + px(indent_px), params.bounds.top() + px(2.0)),
                            line_height,
                            TextAlign::Left,
                            None,
                            window,
                            cx,
                        );

                        // Left fade
                        if params.desc_scroll_x > 1.0 {
                            let bg = bg_color_theme;
                            for step in 0..5 {
                                let x = desc_start_x + px(step as f32 * 3.2);
                                let alpha = 1.0 - (step as f32 / 5.0);
                                window.paint_quad(fill(
                                    Bounds::new(point(x, params.bounds.top()), size(px(3.4), px(ROW_HEIGHT))),
                                    bg.opacity(alpha * 0.95),
                                ));
                            }
                        }

                        // Right fade
                        if params.desc_scroll_x + params.desc_col_width < desc_text_width - 1.0 {
                            let fade_w = 20.0;
                            let fade_start = desc_end_x - px(fade_w);
                            let bg = bg_color_theme;
                            for step in 0..5 {
                                let x = fade_start + px(step as f32 * 4.0);
                                let alpha = (step + 1) as f32 / 6.0;
                                window.paint_quad(fill(
                                    Bounds::new(point(x, params.bounds.top()), size(px(4.2), px(ROW_HEIGHT))),
                                    bg.opacity(alpha * 0.95),
                                ));
                            }
                        }
                    });
                },
            );
        }
    }

    // 5. Bookmark Comments Column
    let row_bookmark_comments: Vec<(Hsla, SharedString)> = params
        .bookmark_items
        .iter()
        .filter(|h| {
            if h.comment.trim().is_empty() {
                return false;
            }
            let h_start_row = crate::core::editor::Editor::find_line_index(h.offset, params.line_starts);
            let h_end_offset = h.offset.saturating_add(h.size);
            let h_last_byte = h_end_offset.saturating_sub(1).max(h.offset);
            let h_end_row = crate::core::editor::Editor::find_line_index(h_last_byte, params.line_starts);
            let display_row = h_start_row.max(params.top_visible_row);
            params.row_idx == display_row && params.row_idx <= h_end_row
        })
        .map(|h| (h.color.to_badge_hsla(), SharedString::from(h.comment.trim().to_string())))
        .collect();

    if !row_bookmark_comments.is_empty() {
        let comment_mask_bounds = Bounds::new(point(comment_start_x, params.bounds.top()), size(px(params.comment_col_width), px(ROW_HEIGHT)));
        let comment_end_x = comment_start_x + px(params.comment_col_width);

        let dot_size = 8.0;
        let dot_radius = 4.0;
        let dot_margin_right = 5.0;
        let item_spacing = 14.0;

        let mut shaped_items = Vec::new();
        let mut total_content_width = 4.0;

        for (badge_color, comment_shared) in &row_bookmark_comments {
            let run = TextRun {
                len: comment_shared.len(),
                font: font.clone(),
                color: muted_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped_comment = window.text_system().shape_line(comment_shared.clone(), params.font_size, &[run], None);
            let text_w = f32::from(shaped_comment.width);
            shaped_items.push((*badge_color, shaped_comment, text_w));
            total_content_width += dot_size + dot_margin_right + text_w + item_spacing;
        }
        let comment_text_width = total_content_width;

        window.with_content_mask(
            Some(ContentMask {
                bounds: scrollable_mask_bounds,
            }),
            |window| {
                window.with_content_mask(Some(ContentMask { bounds: comment_mask_bounds }), |window| {
                    let mut cur_x = comment_start_x - px(params.comment_scroll_x) + px(4.0);
                    let dot_y = params.bounds.top() + px((ROW_HEIGHT - dot_size) / 2.0);

                    for (badge_color, shaped_comment, text_w) in shaped_items {
                        // Highlight colored circle dot
                        let dot_bounds = Bounds::new(point(cur_x, dot_y), size(px(dot_size), px(dot_size)));
                        let mut dot_quad = fill(dot_bounds, badge_color);
                        dot_quad.corner_radii = Corners {
                            top_left: px(dot_radius),
                            top_right: px(dot_radius),
                            bottom_left: px(dot_radius),
                            bottom_right: px(dot_radius),
                        };
                        window.paint_quad(dot_quad);

                        // Comment text
                        let text_x = cur_x + px(dot_size + dot_margin_right);
                        let _ = shaped_comment.paint(point(text_x, params.bounds.top() + px(2.0)), line_height, TextAlign::Left, None, window, cx);

                        cur_x = text_x + px(text_w + item_spacing);
                    }

                    // Left fade
                    if params.comment_scroll_x > 1.0 {
                        let bg = bg_color_theme;
                        for step in 0..5 {
                            let x = comment_start_x + px(step as f32 * 3.2);
                            let alpha = 1.0 - (step as f32 / 5.0);
                            window.paint_quad(fill(
                                Bounds::new(point(x, params.bounds.top()), size(px(3.4), px(ROW_HEIGHT))),
                                bg.opacity(alpha * 0.95),
                            ));
                        }
                    }

                    // Right fade
                    if params.comment_scroll_x + params.comment_col_width < comment_text_width - 1.0 {
                        let fade_w = 20.0;
                        let fade_start = comment_end_x - px(fade_w);
                        let bg = bg_color_theme;
                        for step in 0..5 {
                            let x = fade_start + px(step as f32 * 4.0);
                            let alpha = (step + 1) as f32 / 6.0;
                            window.paint_quad(fill(
                                Bounds::new(point(x, params.bounds.top()), size(px(4.2), px(ROW_HEIGHT))),
                                bg.opacity(alpha * 0.95),
                            ));
                        }
                    }
                });
            },
        );
    }
}

pub use crate::ui::scrollbar::paint_scrollbar;

#[cfg(test)]
mod tests {
    use super::{ByteGroupSize, DisplayRadix, HexGroupInfo, HexInsertCursorParams, hex_insert_cursor_geometry};
    use gpui_kit::px;

    #[test]
    fn insert_cursor_advances_through_four_byte_group() {
        let group = HexGroupInfo {
            chunk_start: 0,
            chunk_end: 4,
            start_slot: 0,
            text_start: 0,
            text_end: 8,
        };

        let positions: Vec<f32> = (0..4)
            .map(|cursor| {
                let (x, _) = hex_insert_cursor_geometry(
                    group,
                    cursor,
                    HexInsertCursorParams {
                        radix: DisplayRadix::Hexadecimal,
                        group_size: ByteGroupSize::Four,
                        is_big_endian: false,
                        selection_active: true,
                        has_pending_nibble: false,
                        origin_x: px(0.0),
                        cell_width: px(1.0),
                    },
                )
                .expect("cursor is inside the group");
                f32::from(x)
            })
            .collect();

        assert_eq!(positions, vec![0.0, 2.0, 4.0, 6.0]);

        // When has_pending_nibble is true, cursor advances by 1 character
        let (x_nibble1, _) = hex_insert_cursor_geometry(
            group,
            0,
            HexInsertCursorParams {
                radix: DisplayRadix::Hexadecimal,
                group_size: ByteGroupSize::Four,
                is_big_endian: false,
                selection_active: true,
                has_pending_nibble: true,
                origin_x: px(0.0),
                cell_width: px(1.0),
            },
        )
        .expect("cursor is inside the group");
        assert_eq!(f32::from(x_nibble1), 1.0);
    }
}
