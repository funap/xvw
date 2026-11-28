use super::super::layout::*;
use super::super::row_cache::*;
use super::super::types::*;
use super::color::{ascii_item_text_color_with, darken_cursor_color, ensure_contrast, hex_item_text_color_with, highlight_color_for_range, pixel_midpoint};
use super::description::{
    CHEVRON_ICON_GAP, CHEVRON_ICON_SIZE, DescriptionPalette, build_continuation_field_runs, build_leaf_field_runs, build_structure_header_runs,
    calculate_description_indent_level, resolve_structure_row_flags, shaped_byte_span_x,
};
use super::glyphs::{
    ColorRunTracker, HexInsertCursorParams, build_cached_ascii_glyphs, build_cached_hex_glyphs, clip_column_mask, hex_insert_cursor_end_geometry,
    hex_insert_cursor_geometry, paint_border_box, paint_centered_ascii_glyphs, paint_centered_hex_glyphs, paint_cursor_border, paint_insert_cursor_at,
    paint_underscore_cursor_at,
};
use crate::core::document::Document;
use crate::core::editor::LineMap;
use crate::core::encoding::Encoding;
use crate::core::radix::{ByteGroupSize, DisplayRadix, HexCase, digit_count};
use crate::core::structure::{IndexedField, ParseResult};
use crate::ui::color::BookmarkColorExt;
use crate::ui::icon::IconName;
use gpui::TransformationMatrix;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::IconNamed;
use gpui_kit::*;
use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Range;

/// Returns the byte range `[offset, next_offset)` covered by `row_idx`, including any trailing EOF cursor row.
#[inline]
pub fn row_byte_range(line_starts: &LineMap, row_idx: usize, total_size: usize) -> Option<(usize, usize)> {
    if row_idx < line_starts.len() {
        let offset = line_starts.get(row_idx)?;
        let next_offset = if row_idx + 1 < line_starts.len() {
            line_starts.get(row_idx + 1).unwrap_or(total_size)
        } else {
            total_size
        };
        Some((offset, next_offset))
    } else if row_idx == line_starts.len() {
        Some((total_size, total_size))
    } else {
        None
    }
}

#[inline]
pub fn format_offset_08(offset: usize, hex_case: HexCase) -> SharedString {
    let digits = hex_case.digits();
    let mut buf = [b'0'; 16];
    let width = if offset > 0xFFFF_FFFF { 16 } else { 8 };
    let mut val = offset;
    for i in (16 - width..16).rev() {
        buf[i] = digits[val & 0xf];
        val >>= 4;
    }
    let slice = std::str::from_utf8(&buf[16 - width..]).expect("valid ascii utf8");
    SharedString::from(std::sync::Arc::<str>::from(slice))
}

#[inline]
fn is_eof_on_row(active_cursor_offset: usize, row_idx: usize, line_starts: &LineMap, total_size: usize) -> bool {
    if active_cursor_offset != total_size {
        return false;
    }
    let last_line_idx = line_starts.len().saturating_sub(1);
    let last_line_start = line_starts.get(last_line_idx).unwrap_or(0);
    let bytes_per_row = line_starts.max_bytes_per_row();
    let is_eof_on_new_line = total_size > last_line_start && (total_size - last_line_start) >= bytes_per_row;
    (is_eof_on_new_line && row_idx == line_starts.len()) || (!is_eof_on_new_line && row_idx == last_line_idx)
}

/// Returns whether the row at `row_idx` (with byte range `[offset, next_offset)`) contains the active cursor position.
#[cfg(test)]
pub fn is_cursor_row(active_cursor_offset: usize, row_idx: usize, offset: usize, next_offset: usize, line_starts: &LineMap, total_size: usize) -> bool {
    (active_cursor_offset >= offset && active_cursor_offset < next_offset) || is_eof_on_row(active_cursor_offset, row_idx, line_starts, total_size)
}

pub struct RowPaintParams<'a> {
    pub row_idx: usize,
    pub bounds: Bounds<Pixels>,
    pub doc: &'a Document,
    pub line_starts: &'a LineMap,
    pub parse_result: Option<&'a ParseResult>,
    pub is_parsing_structure: bool,
    pub collapsed_structs: &'a HashSet<String>,
    pub encoding: Encoding,
    pub radix: DisplayRadix,
    pub group_size: ByteGroupSize,
    pub is_big_endian: bool,
    pub hex_case: HexCase,
    pub category_palette: ByteCategoryPalette,
    pub cursor_offset: usize,
    pub insert_cursor_offset: usize,
    pub min_sel: usize,
    pub max_sel: usize,
    pub highlights: &'a [(Range<usize>, Hsla)],
    pub bookmark_comments: &'a [RowCommentRef<'a>],
    pub show_offset: bool,
    pub show_ascii: bool,
    pub ascii_col_width: f32,
    pub ascii_scroll_x: f32,
    pub is_focused: bool,
    pub insert_mode: bool,
    pub active_column: EditColumn,
    pub ascii_cursor_char_range: Option<Range<usize>>,
    pub cursor_visible: bool,
    pub pending_hex_digit: Option<(usize, u8)>,
    pub low_nibble_offset: Option<usize>,
    pub outer_scroll_x: f32,
    pub hex_scroll_x: f32,
    pub desc_scroll_x: f32,
    pub comment_scroll_x: f32,
    pub address_col_width: f32,
    pub hex_col_width: f32,
    pub hex_cell_width: Pixels,
    pub desc_col_width: f32,
    pub comment_col_width: f32,
    pub font: &'a Font,
    pub font_size: Pixels,
    pub desc_palette: DescriptionPalette,
    pub active_leaf_field: Option<&'a IndexedField>,
    pub has_folded_bookmarks: bool,
    pub has_address_gaps: bool,
    pub theme_colors: &'a RowThemeColors,
    pub row_cache: &'a mut RowRenderCache,
    pub scratch: &'a mut PaintRowScratch,
}

pub fn build_cached_row(params: &RowPaintParams, offset: usize, next_offset: usize, window: &mut Window) -> CachedRow {
    let chunk_len = next_offset - offset;
    let chunk = params.doc.buffer.get_range(offset, chunk_len);
    let total_size = params.doc.buffer.len();
    let is_struct_mode = params.parse_result.is_some();
    let is_live_structure_parse = params.is_parsing_structure || params.parse_result.is_some_and(ParseResult::is_live);
    let font = params.font;

    let physical_address = params.doc.offset_to_address(offset);
    let query_len = chunk_len.max(1);

    let raw_containers: Cow<'_, [IndexedField]> = if !is_live_structure_parse && let Some(parse_res) = params.parse_result {
        Cow::Borrowed(parse_res.find_container_structs_starting_at(offset, query_len))
    } else {
        Cow::Borrowed(&[])
    };

    let (is_structure_header_row, is_collapsed_struct_row) = resolve_structure_row_flags(
        params.parse_result,
        params.is_parsing_structure,
        &raw_containers,
        params.collapsed_structs,
        offset,
        chunk_len,
        total_size,
    );

    let row_leaves: Cow<'_, [IndexedField]> = if is_structure_header_row || is_collapsed_struct_row {
        Cow::Borrowed(&[])
    } else if let Some(parse_res) = params.parse_result {
        if parse_res.is_live() {
            Cow::Owned(parse_res.find_live_leaf_fields_starting_at(offset, chunk_len))
        } else {
            Cow::Borrowed(parse_res.find_leaf_fields_starting_at(offset, chunk_len))
        }
    } else {
        Cow::Borrowed(&[])
    };

    let first_leaf = if !is_structure_header_row && !is_collapsed_struct_row && chunk_len > 0 {
        params.parse_result.and_then(|pr| pr.query_deepest_leaf_at(offset))
    } else {
        None
    };

    let structure_fields = if is_struct_mode {
        Some(CachedRowStructureFields {
            is_structure_header_row,
            is_collapsed_struct_row,
        })
    } else {
        None
    };

    // Address column
    let shaped_addr = if is_struct_mode || params.show_offset {
        let addr_str = format_offset_08(physical_address, params.hex_case);
        let addr_color = params.theme_colors.muted_color;
        let run = TextRun {
            len: addr_str.len(),
            font: font.clone(),
            color: addr_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        Some(window.text_system().shape_line(addr_str, params.font_size, &[run], None))
    } else {
        None
    };

    // Hex column
    let hex_chunk = if is_structure_header_row || is_collapsed_struct_row { &[] } else { chunk };
    let hex_source = build_hex_text_source(hex_chunk, offset, params.radix, params.group_size, params.is_big_endian, params.hex_case);
    let hex_glyphs = if !hex_source.text.is_empty() {
        let run = TextRun {
            len: hex_source.text.len(),
            font: font.clone(),
            color: params.theme_colors.fg_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = window.text_system().shape_line(hex_source.text.clone(), params.font_size, &[run], None);
        Some(build_cached_hex_glyphs(&shaped, &hex_source.groups, params.hex_cell_width))
    } else {
        None
    };

    // ASCII column
    let mut ascii_cells = Vec::with_capacity(chunk.len());
    let ascii_glyphs = if params.show_ascii && chunk_len > 0 && !is_collapsed_struct_row {
        let mut ascii_text = String::with_capacity(chunk.len());
        let mut cell_spans = Vec::with_capacity(chunk.len());
        for_each_ascii_char(params.encoding, params.doc.buffer.data(), offset, chunk.len(), |j, item| {
            let c = item.character();
            let text_byte_start = ascii_text.len();
            ascii_text.push(c);
            let text_byte_end = ascii_text.len();
            cell_spans.push((text_byte_start, text_byte_end));
            let byte = chunk.get(j).copied().unwrap_or(0);
            ascii_cells.push(CachedAsciiCell {
                cell_idx: j,
                byte,
                is_printable: item.is_printable(),
            });
        });
        if !ascii_text.is_empty() {
            let run = TextRun {
                len: ascii_text.len(),
                font: font.clone(),
                color: params.theme_colors.fg_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window.text_system().shape_line(SharedString::from(ascii_text), params.font_size, &[run], None);
            Some(build_cached_ascii_glyphs(&shaped, &ascii_cells, &cell_spans))
        } else {
            None
        }
    } else {
        None
    };

    // Description column
    let desc = if let Some(parse_res) = params.parse_result {
        let container_struct: Option<&IndexedField> = if is_structure_header_row || is_collapsed_struct_row {
            let matches_container = |c: &&IndexedField| {
                if c.is_instance {
                    return false;
                }
                if is_collapsed_struct_row
                    && !(c.offset == offset && (params.collapsed_structs.contains(&c.collapse_key) || params.collapsed_structs.contains(&c.id)))
                {
                    return false;
                }
                if is_structure_header_row && c.size == 0 {
                    return false;
                }
                true
            };
            if is_structure_header_row {
                let mut matching = raw_containers.iter().filter(matches_container);
                let first = matching.next();
                if let Some(second) = matching.next() {
                    let mut same_offset_rows = 0;
                    let mut previous_row = params.row_idx;
                    while previous_row > 0 && params.line_starts.get(previous_row - 1) == Some(offset) {
                        same_offset_rows += 1;
                        previous_row -= 1;
                    }
                    match same_offset_rows {
                        0 => first,
                        1 => Some(second),
                        n => matching.nth(n - 2).or(first),
                    }
                } else {
                    first
                }
            } else {
                raw_containers.iter().find(matches_container)
            }
        } else {
            None
        };

        let is_collapsed = is_collapsed_struct_row
            || container_struct.is_some_and(|c| params.collapsed_structs.contains(&c.collapse_key) || params.collapsed_structs.contains(&c.id));

        let continuation_leaf = if !is_structure_header_row && !is_collapsed_struct_row && container_struct.is_none() && row_leaves.is_empty() {
            first_leaf.as_deref()
        } else {
            None
        };

        let active_range_count = if continuation_leaf.is_none() && container_struct.is_none() && row_leaves.is_empty() && !parse_res.is_live() {
            parse_res.count_active_struct_ranges(offset, query_len)
        } else {
            0
        };

        let indent_level = calculate_description_indent_level(continuation_leaf, container_struct, row_leaves.first(), active_range_count);
        let indent_px = indent_level as f32 * 14.0;
        let has_chevron = container_struct.is_some();
        let chevron_icon_path = if has_chevron {
            if is_collapsed {
                Some(IconName::ChevronRight.path())
            } else {
                Some(IconName::ChevronDown.path())
            }
        } else {
            None
        };
        let chevron_width = if has_chevron { CHEVRON_ICON_SIZE + CHEVRON_ICON_GAP } else { 0.0 };

        let mut desc_runs = Vec::new();
        let mut desc_full_text = String::new();
        let mut leaf_byte_ranges: Vec<(usize, usize, usize, usize)> = Vec::new();
        if let Some(container) = container_struct {
            let (hdr_text, hdr_runs) = build_structure_header_runs(container, is_collapsed, font.clone(), &params.desc_palette);
            desc_full_text.push_str(&hdr_text);
            desc_runs.extend(hdr_runs);
        }
        if let Some(leaf) = continuation_leaf {
            let rel_offset = offset.saturating_sub(leaf.offset);
            let (c_text, c_runs) = build_continuation_field_runs(&leaf.id, rel_offset, font.clone(), &params.desc_palette, params.hex_case);
            desc_full_text.push_str(&c_text);
            desc_runs.extend(c_runs);
        } else if !is_collapsed {
            let include_doc = row_leaves.len() == 1 && container_struct.is_none();
            leaf_byte_ranges.reserve(row_leaves.len());
            for f in row_leaves.iter() {
                if !desc_full_text.is_empty() {
                    desc_full_text.push_str("  ");
                    desc_runs.push(TextRun {
                        len: 2,
                        font: font.clone(),
                        color: params.desc_palette.punctuation,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
                let byte_start = desc_full_text.len();
                let (leaf_text, leaf_runs) = build_leaf_field_runs(f, font.clone(), &params.desc_palette, include_doc, false);
                desc_full_text.push_str(&leaf_text);
                let byte_end = desc_full_text.len();
                leaf_byte_ranges.push((f.offset, f.size, byte_start, byte_end));
                desc_runs.extend(leaf_runs);
            }
        }

        if !desc_full_text.is_empty() {
            if desc_full_text.contains('\n') || desc_full_text.contains('\r') {
                let mut sanitized = String::with_capacity(desc_full_text.len());
                for ch in desc_full_text.chars() {
                    if ch == '\n' || ch == '\r' {
                        sanitized.push(' ');
                    } else {
                        sanitized.push(ch);
                    }
                }
                desc_full_text = sanitized;
            }

            let text_offset_px = indent_px + chevron_width;
            let expr_shared = SharedString::from(desc_full_text);
            let shaped_base_desc = window.text_system().shape_line(expr_shared, params.font_size, &desc_runs, None);
            let desc_text_width = f32::from(shaped_base_desc.width) + text_offset_px + 8.0;
            let leaf_field_spans = leaf_byte_ranges
                .into_iter()
                .map(|(f_offset, f_size, byte_start, byte_end)| {
                    let (start_x, width_px) = shaped_byte_span_x(&shaped_base_desc, byte_start, byte_end);
                    (f_offset, f_size, start_x, width_px)
                })
                .collect();

            Some(CachedRowDesc {
                indent_level,
                indent_px,
                chevron_icon_path,
                text_offset_px,
                desc_text_width,
                shaped_base_desc: Some(shaped_base_desc),
                leaf_field_spans,
            })
        } else {
            None
        }
    } else {
        None
    };

    CachedRow {
        offset,
        next_offset,
        physical_address,
        shaped_addr,
        hex_source,
        hex_glyphs,
        ascii_cells,
        ascii_glyphs,
        structure_fields,
        desc,
    }
}

struct BannerRowSpec<'a> {
    addr_text: &'a str,
    addr_color: Hsla,
    show_left_col: bool,
    offset_w: f32,
    gap: f32,
    divider_color: Hsla,
    label: &'a str,
    fill_bg: Hsla,
    border_tint: Hsla,
    label_color: Hsla,
}

fn paint_special_banner_row(
    spec: BannerRowSpec<'_>,
    bounds: Bounds<Pixels>,
    font: &Font,
    font_size: Pixels,
    row_cache: &mut RowRenderCache,
    window: &mut Window,
    cx: &mut App,
) {
    let line_height = px(ROW_HEIGHT);
    let base_x = bounds.left() + px(8.0);
    if spec.show_left_col {
        let (shaped, _) = row_cache.get_or_shape_special(spec.addr_text, spec.addr_color, font, font_size, window);
        let addr_pos = point(base_x, bounds.top() + px(2.0));
        let _ = shaped.paint(addr_pos, line_height, TextAlign::Left, None, window, cx);

        let div1_x = base_x + px(spec.offset_w + (spec.gap / 2.0));
        window.paint_quad(fill(
            Bounds::new(point(div1_x, bounds.top()), size(px(1.0), px(ROW_HEIGHT))),
            spec.divider_color,
        ));
    }

    let bar_start_x = base_x + px(spec.offset_w + spec.gap);
    let bar_end_x = bounds.right() - px(VERTICAL_SCROLLBAR_WIDTH + 8.0);
    let bar_width = (bar_end_x - bar_start_x).max(px(0.0));
    let bar_bounds = Bounds::new(point(bar_start_x, bounds.top() + px(2.0)), size(bar_width, px(ROW_HEIGHT - 4.0)));
    window.paint_quad(fill(bar_bounds, spec.fill_bg));
    paint_border_box(window, bar_bounds, px(1.0), spec.border_tint);

    let (shaped_label, _) = row_cache.get_or_shape_special(spec.label, spec.label_color, font, font_size * 0.9, window);
    let text_pos = point(bar_start_x + px(12.0), bounds.top() + px(2.0));
    let _ = shaped_label.paint(text_pos, line_height, TextAlign::Left, None, window, cx);
}

#[allow(clippy::too_many_arguments)]
fn paint_row_address(
    is_cursor_row: bool,
    physical_address: usize,
    addr_color: Hsla,
    hex_case: HexCase,
    font: &Font,
    font_size: Pixels,
    line_height: Pixels,
    addr_pos: Point<Pixels>,
    cached_addr: Option<&ShapedLine>,
    cursor_addr_cache: &mut CursorAddrShapeCache,
    window: &mut Window,
    cx: &mut App,
) {
    if is_cursor_row {
        let shaped = cursor_addr_cache.get_or_shape(physical_address, addr_color, font, font_size, |addr| format_offset_08(addr, hex_case), window);
        let _ = shaped.paint(addr_pos, line_height, TextAlign::Left, None, window, cx);
    } else if let Some(shaped) = cached_addr {
        let _ = shaped.paint(addr_pos, line_height, TextAlign::Left, None, window, cx);
    }
}

pub fn paint_hex_row(params: RowPaintParams, window: &mut Window, cx: &mut App) {
    let total_size = params.doc.buffer.len();
    let Some((offset, next_offset)) = row_byte_range(params.line_starts, params.row_idx, total_size) else {
        return;
    };

    let chunk_len = next_offset - offset;
    let chunk = params.doc.buffer.get_range(offset, chunk_len);
    let active_cursor_offset = if params.insert_mode {
        params.insert_cursor_offset
    } else {
        params.cursor_offset
    };
    let is_eof_on_this_row = is_eof_on_row(active_cursor_offset, params.row_idx, params.line_starts, total_size);
    let is_cursor_row = (active_cursor_offset >= offset && active_cursor_offset < next_offset) || is_eof_on_this_row;

    let is_struct_mode = params.parse_result.is_some();

    let active_row_highlights = params.highlights;
    let desc_palette = params.desc_palette;
    let selection_bg = params.theme_colors.selection_bg;
    let caret_color = params.theme_colors.caret_color;
    let muted_color = params.theme_colors.muted_color;
    let fg_color = params.theme_colors.fg_color;
    let accent_fg_color = params.theme_colors.accent_fg_color;
    let border_color = params.theme_colors.border_color;
    let border_line_color = params.theme_colors.border_line_color;

    let line_height = px(ROW_HEIGHT);
    let font = params.font;
    let insert_cursor_active = params.insert_mode && params.is_focused && window.is_window_active();
    let insert_selection_active = params.min_sel <= params.max_sel;

    let active_leaf_field = params.active_leaf_field;

    // Check for Folded Bookmark or Unbookmarked Region Row
    let fold_summary = if params.has_folded_bookmarks {
        params
            .doc
            .fold_bookmark_summary_at(offset)
            .map(|s| (s.end_offset, Some(s.color), s.comment, s.is_unbookmarked))
    } else {
        None
    };

    if let Some((fold_end, color, comment, is_unbookmarked)) = fold_summary {
        let (offset_w, gap) = if is_struct_mode {
            (params.address_col_width, SECTION_GAP)
        } else {
            (if params.show_offset { OFFSET_WIDTH } else { 0.0 }, SECTION_GAP)
        };

        let fold_start_addr = params.doc.offset_to_address(offset);
        let fold_end_addr = params.doc.offset_to_address(fold_end);
        let show_left_col = is_struct_mode || params.show_offset;
        let is_cursor_in_fold = active_cursor_offset >= offset && active_cursor_offset < fold_end;
        let addr_color = if is_cursor_in_fold { fg_color } else { muted_color };
        let addr_str = if show_left_col {
            format_offset_08(fold_start_addr, params.hex_case)
        } else {
            SharedString::default()
        };

        let fold_size = fold_end.saturating_sub(offset);
        let fold_label = if is_unbookmarked {
            format!(
                "── Unbookmarked: {} - {} ({} / {} bytes) ──",
                params.hex_case.format_prefixed_pad(fold_start_addr, 8),
                params.hex_case.format_prefixed_pad(fold_end_addr, 8),
                params.hex_case.format_prefixed(fold_size),
                fold_size
            )
        } else if !comment.is_empty() {
            let clean_comment = if comment.contains('\n') || comment.contains('\r') {
                comment.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("")
            } else {
                comment.as_str()
            };
            format!(
                "── {}: {} - {} ({} / {} bytes) ──",
                clean_comment,
                params.hex_case.format_prefixed_pad(fold_start_addr, 8),
                params.hex_case.format_prefixed_pad(fold_end_addr, 8),
                params.hex_case.format_prefixed(fold_size),
                fold_size
            )
        } else {
            format!(
                "── {} - {} ({} / {} bytes) ──",
                params.hex_case.format_prefixed_pad(fold_start_addr, 8),
                params.hex_case.format_prefixed_pad(fold_end_addr, 8),
                params.hex_case.format_prefixed(fold_size),
                fold_size
            )
        };
        let theme = cx.theme();
        let (fill_bg, border_tint, text_tint) = if is_unbookmarked {
            (theme.secondary, border_color, muted_color)
        } else if let Some(c) = color {
            (theme.secondary, c.to_badge_hsla(), c.to_badge_hsla())
        } else {
            (theme.secondary, border_color, muted_color)
        };

        paint_special_banner_row(
            BannerRowSpec {
                addr_text: addr_str.as_ref(),
                addr_color,
                show_left_col,
                offset_w,
                gap,
                divider_color: border_color,
                label: &fold_label,
                fill_bg,
                border_tint,
                label_color: text_tint,
            },
            params.bounds,
            font,
            params.font_size,
            params.row_cache,
            window,
            cx,
        );
        return;
    }

    // Check for Address Gap Separator Row
    let gap_info = if params.has_address_gaps && chunk_len == 0 {
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

        let gap_size = gap_end.saturating_sub(gap_start);
        let gap_label = format!(
            "── Address Gap: {} - {} ({} / {} bytes unmapped) ──",
            params.hex_case.format_prefixed_pad(gap_start, 8),
            params.hex_case.format_prefixed_pad(gap_end, 8),
            params.hex_case.format_prefixed(gap_size),
            gap_size
        );
        let theme = cx.theme();
        paint_special_banner_row(
            BannerRowSpec {
                addr_text: "--------",
                addr_color: muted_color,
                show_left_col: is_struct_mode || params.show_offset,
                offset_w,
                gap,
                divider_color: border_color,
                label: &gap_label,
                fill_bg: theme.secondary,
                border_tint: border_color,
                label_color: accent_fg_color,
            },
            params.bounds,
            font,
            params.font_size,
            params.row_cache,
            window,
            cx,
        );
        return;
    }

    if params.row_cache.get_for_range(params.row_idx, offset, next_offset).is_none() {
        let cached_row = build_cached_row(&params, offset, next_offset, window);
        params.row_cache.insert(params.row_idx, cached_row);
    }
    let (cached, caches) = params.row_cache.row_and_caches_mut(params.row_idx).expect("cached row inserted");
    let contrast_cache = caches.contrast;
    let comment_shapes = caches.comment_shapes;
    let cursor_addr_cache = caches.cursor_addr;

    let physical_address = cached.physical_address;
    let (is_structure_header_row, is_collapsed_struct_row) = cached
        .structure_fields
        .map_or((false, false), |sf| (sf.is_structure_header_row, sf.is_collapsed_struct_row));
    let row_active_underline: Option<(Range<usize>, Hsla)> = if is_struct_mode && !is_structure_header_row && !is_collapsed_struct_row {
        active_leaf_field
            .filter(|af| af.offset < next_offset && af.offset + af.size > offset)
            .map(|af| (af.offset..af.offset + af.size, af.color.map(Hsla::from).unwrap_or(desc_palette.type_name)))
    } else {
        None
    };

    // 1. Draw Left Columns (Address OR Offset)
    let addr_color = if is_cursor_row { fg_color } else { muted_color };
    let (offset_w, gap) = if is_struct_mode {
        (params.address_col_width, SECTION_GAP)
    } else {
        (if params.show_offset { OFFSET_WIDTH } else { 0.0 }, SECTION_GAP)
    };
    if is_struct_mode || params.show_offset {
        let addr_pos = point(params.bounds.left() + px(8.0), params.bounds.top() + px(2.0));
        paint_row_address(
            is_cursor_row,
            physical_address,
            addr_color,
            params.hex_case,
            font,
            params.font_size,
            line_height,
            addr_pos,
            cached.shaped_addr.as_ref(),
            cursor_addr_cache,
            window,
            cx,
        );
    }

    let base_x = params.bounds.left() + px(8.0);
    let hex_start_x = base_x + px(offset_w + gap) - px(params.outer_scroll_x);
    let hex_end_x = hex_start_x + px(params.hex_col_width);
    let ascii_width = if params.show_ascii { params.ascii_col_width } else { 0.0 };
    let ascii_start_x = hex_end_x + px(gap);
    let ascii_end_x = if params.show_ascii { ascii_start_x + px(ascii_width) } else { hex_end_x };
    let desc_start_x = if params.show_ascii { ascii_end_x + px(gap) } else { hex_end_x + px(gap) };
    let desc_end_x = if is_struct_mode {
        desc_start_x + px(params.desc_col_width)
    } else {
        ascii_end_x
    };
    let comment_start_x = if is_struct_mode {
        desc_end_x + px(gap)
    } else if params.show_ascii {
        ascii_end_x + px(gap)
    } else {
        hex_end_x + px(gap)
    };

    // Vertical Column Divider Borders (matching header splitters exactly)
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
            if is_structure_header_row || is_collapsed_struct_row {
                let bar_start_x = hex_start_x;
                let bar_end_x = params.bounds.right() - px(VERTICAL_SCROLLBAR_WIDTH + 8.0);
                let bar_width = (bar_end_x - bar_start_x).max(px(0.0));
                let banner_bounds = Bounds::new(point(bar_start_x, params.bounds.top() + px(1.0)), size(bar_width, px(ROW_HEIGHT - 2.0)));
                window.paint_quad(fill(banner_bounds, desc_palette.active_bg));

                let pip_bounds = Bounds::new(point(bar_start_x + px(2.0), params.bounds.top() + px(3.0)), size(px(3.0), px(ROW_HEIGHT - 6.0)));
                window.paint_quad(fill(pip_bounds, desc_palette.type_name));

                let line_y = params.bounds.top() + px(ROW_HEIGHT / 2.0);
                let leader_end_x = desc_start_x - px(8.0);
                if leader_end_x > bar_start_x + px(16.0) {
                    let leader_bounds = Bounds::new(point(bar_start_x + px(10.0), line_y), size(leader_end_x - bar_start_x - px(10.0), px(1.0)));
                    window.paint_quad(fill(leader_bounds, desc_palette.tree_guide));
                }
            }

            let div2_x = hex_start_x + px(params.hex_col_width + (gap / 2.0));
            window.paint_quad(fill(
                Bounds::new(point(div2_x, params.bounds.top()), size(px(1.0), px(ROW_HEIGHT))),
                border_line_color,
            ));
            if params.show_ascii {
                let div_ascii_x = ascii_end_x + px(gap / 2.0);
                window.paint_quad(fill(
                    Bounds::new(point(div_ascii_x, params.bounds.top()), size(px(1.0), px(ROW_HEIGHT))),
                    border_line_color,
                ));
            }
            if is_struct_mode {
                let div_desc_x = desc_end_x + px(gap / 2.0);
                window.paint_quad(fill(
                    Bounds::new(point(div_desc_x, params.bounds.top()), size(px(1.0), px(ROW_HEIGHT))),
                    border_line_color,
                ));
            }
            let div_comment_x = comment_start_x + px(params.comment_col_width + (gap / 2.0));
            window.paint_quad(fill(
                Bounds::new(point(div_comment_x, params.bounds.top()), size(px(1.0), px(ROW_HEIGHT))),
                border_line_color,
            ));
        },
    );

    let mut dyn_hex_glyphs: Option<CachedHexGlyphs> = None;
    let (hex_source_ref, pending_edit_colors) = if let Some((pending_pos, pending_digit)) = params.pending_hex_digit
        && params.active_column == EditColumn::Hex
        && ((pending_pos >= offset && pending_pos < next_offset) || (is_eof_on_this_row && pending_pos == total_size))
    {
        let hex_chunk = if is_structure_header_row || is_collapsed_struct_row { &[] } else { chunk };
        let mut dyn_source = build_hex_text_source(hex_chunk, offset, params.radix, params.group_size, params.is_big_endian, params.hex_case);
        let mut hex_shared_text = dyn_source.text.clone();
        let mut pending_byte_char_start: Option<usize> = None;
        if pending_pos >= offset && pending_pos < next_offset {
            let chunk_pos = pending_pos - offset;
            if let Some(group) = dyn_source.groups.iter().find(|g| g.chunk_start <= chunk_pos && chunk_pos < g.chunk_end) {
                let byte_index = chunk_pos - group.chunk_start;
                let group_len = group.chunk_end - group.chunk_start;
                let visual_byte = if !params.is_big_endian && group_len > 1 {
                    group_len.saturating_sub(byte_index + 1)
                } else {
                    byte_index
                };
                let char_start = group.text_start + visual_byte * 2;
                if char_start + 1 < hex_shared_text.len() {
                    let digit_char = if (pending_digit as usize) < 16 {
                        params.hex_case.digits()[pending_digit as usize] as char
                    } else {
                        '?'
                    };
                    let mut bytes = hex_shared_text.to_string().into_bytes();
                    bytes[char_start] = digit_char as u8;
                    bytes[char_start + 1] = b'_';
                    hex_shared_text = SharedString::from(String::from_utf8(bytes).expect("valid utf8"));
                    pending_byte_char_start = Some(char_start);
                }
            }
        } else if is_eof_on_this_row && pending_pos == total_size {
            let digit_char = if (pending_digit as usize) < 16 {
                params.hex_case.digits()[pending_digit as usize] as char
            } else {
                '?'
            };
            let group_bytes = params.group_size.byte_count();
            if dyn_source.groups.is_empty() {
                hex_shared_text = SharedString::from(format!("{}_", digit_char));
                dyn_source.groups.push(HexGroupInfo {
                    chunk_start: 0,
                    chunk_end: 0,
                    start_slot: 0,
                    text_start: 0,
                    text_end: 2,
                });
                pending_byte_char_start = Some(0);
            } else {
                let last_group = *dyn_source.groups.last().unwrap();
                let last_group_len = last_group.chunk_end - last_group.chunk_start;
                let is_complete = (last_group.start_slot + last_group_len) == group_bytes;
                if is_complete {
                    let mut hex_text = hex_shared_text.to_string();
                    hex_text.push(' ');
                    let text_start = hex_text.len();
                    hex_text.push(digit_char);
                    hex_text.push('_');
                    let text_end = hex_text.len();
                    hex_shared_text = SharedString::from(hex_text);
                    dyn_source.groups.push(HexGroupInfo {
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
                    if char_start + 1 < hex_shared_text.len() {
                        let mut bytes = hex_shared_text.to_string().into_bytes();
                        bytes[char_start] = digit_char as u8;
                        bytes[char_start + 1] = b'_';
                        hex_shared_text = SharedString::from(String::from_utf8(bytes).expect("valid utf8"));
                        pending_byte_char_start = Some(char_start);
                    }
                }
            }
        }
        let dyn_shaped = if !hex_shared_text.is_empty() {
            let runs = [TextRun {
                len: hex_shared_text.len(),
                font: font.clone(),
                color: fg_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            }];
            Some(window.text_system().shape_line(hex_shared_text, params.font_size, &runs, None))
        } else {
            None
        };
        dyn_hex_glyphs = dyn_shaped
            .as_ref()
            .map(|shaped| build_cached_hex_glyphs(shaped, &dyn_source.groups, params.hex_cell_width));
        (
            Cow::Owned(dyn_source),
            pending_byte_char_start.map(|char_start| (char_start, caret_color, muted_color)),
        )
    } else {
        (Cow::Borrowed(&cached.hex_source), None)
    };
    let hex_glyphs_ref = dyn_hex_glyphs.as_ref().or(cached.hex_glyphs.as_ref());

    let pending_byte_char_start = pending_edit_colors.map(|(p, _, _)| p);

    params.scratch.group_visuals.clear();
    params.scratch.group_text_colors.clear();

    for group in &hex_source_ref.groups {
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
        let is_selected = if params.min_sel <= params.max_sel {
            item_start_offset <= params.max_sel && item_end_offset > params.min_sel
        } else {
            false
        };

        let current_hl_color = highlight_color_for_range(item_start_offset, item_end_offset, active_row_highlights);
        let text_color = hex_item_text_color_with(
            is_cursor && params.is_focused,
            is_selected,
            current_hl_color,
            item_slice,
            &params.category_palette,
            fg_color,
            |fg, bg| contrast_cache.get_or_compute(fg, bg, |f, b| ensure_contrast(f, b, 4.5)),
        );

        params.scratch.group_visuals.push((current_hl_color, is_selected, is_cursor));
        params.scratch.group_text_colors.push(text_color);
    }

    let group_visuals = params.scratch.group_visuals.as_slice();
    let group_text_colors = params.scratch.group_text_colors.as_slice();

    let text_origin_x = hex_start_x - px(params.hex_scroll_x);
    let total_data_width = f32::from(hex_grid_width(hex_source_ref.text.len(), params.hex_cell_width));

    let row_top_y = f32::from(params.bounds.top());
    let row_bottom_y = f32::from(params.bounds.bottom());

    // 2. Background Quads Pass for Data Items (with clipped single content mask)
    let hex_mask_bounds = clip_column_mask(
        scrollable_mask_bounds,
        Bounds::new(point(hex_start_x, params.bounds.top()), size(px(params.hex_col_width), px(ROW_HEIGHT))),
    );

    if hex_mask_bounds.size.width > px(0.0) {
        window.with_content_mask(Some(ContentMask { bounds: hex_mask_bounds }), |window| {
            // 2a. Structure field active underline
            if let Some((ref af_range, field_hsla)) = row_active_underline {
                for group in &hex_source_ref.groups {
                    let item_start_offset = offset + group.chunk_start;
                    if af_range.contains(&item_start_offset) {
                        let (group_start_x, group_end_x) = hex_group_x(*group, text_origin_x, params.hex_cell_width);
                        let underline_bounds = Bounds::new(
                            point(group_start_x, params.bounds.top() + px(ROW_HEIGHT - 2.0)),
                            size(group_end_x - group_start_x, px(2.0)),
                        );
                        window.paint_quad(fill(underline_bounds, field_hsla));
                    }
                }
            }

            // 2b. Coalesced highlight (bookmark) background quads
            let mut paint_hl_run = |start: usize, end: usize, color: Hsla| {
                let start_x = hex_group_x(hex_source_ref.groups[start], text_origin_x, params.hex_cell_width).0;
                let end_x = hex_group_x(hex_source_ref.groups[end], text_origin_x, params.hex_cell_width).1;
                let hl_start_x = if start > 0 && group_visuals[start - 1].0.is_some() {
                    let prev_end_x = hex_group_x(hex_source_ref.groups[start - 1], text_origin_x, params.hex_cell_width).1;
                    pixel_midpoint(prev_end_x, start_x)
                } else {
                    start_x
                };
                let hl_end_x = if end + 1 < hex_source_ref.groups.len() && group_visuals[end + 1].0.is_some() {
                    let next_start_x = hex_group_x(hex_source_ref.groups[end + 1], text_origin_x, params.hex_cell_width).0;
                    pixel_midpoint(end_x, next_start_x)
                } else {
                    end_x
                };
                let hl_fill_bounds = Bounds::new(
                    point(hl_start_x, params.bounds.top() + px(1.0)),
                    size(hl_end_x - hl_start_x, px(ROW_HEIGHT - 2.0)),
                );
                window.paint_quad(fill(hl_fill_bounds, color));
            };
            let mut hl_tracker = ColorRunTracker::default();
            for (item_idx, &(hl_color, _, _)) in group_visuals.iter().enumerate() {
                hl_tracker.push(item_idx, hl_color, &mut paint_hl_run);
            }
            hl_tracker.finish(&mut paint_hl_run);

            // 2c. Coalesced translucent selection quad (overlaid on top of highlight)
            if selection_bg.a > 0.0 {
                let mut sel_start: Option<usize> = None;
                for item_idx in 0..=group_visuals.len() {
                    let is_selected = group_visuals.get(item_idx).is_some_and(|v| v.1);
                    match (sel_start, is_selected) {
                        (None, true) => sel_start = Some(item_idx),
                        (Some(start), false) => {
                            let start_x = hex_group_x(hex_source_ref.groups[start], text_origin_x, params.hex_cell_width).0;
                            let end_x = hex_group_x(hex_source_ref.groups[item_idx - 1], text_origin_x, params.hex_cell_width).1;
                            let sel_fill_bounds = Bounds::new(point(start_x, params.bounds.top() + px(1.0)), size(end_x - start_x, px(ROW_HEIGHT - 2.0)));
                            window.paint_quad(fill(sel_fill_bounds, selection_bg));
                            sel_start = None;
                        }
                        _ => {}
                    }
                }
            }

            // 2d. Cursor Pass (only evaluated on the cursor row)
            if is_cursor_row && !params.insert_mode && pending_byte_char_start.is_none() {
                let cursor_border_color = if params.is_focused { caret_color } else { darken_cursor_color(muted_color) };
                for (item_idx, group) in hex_source_ref.groups.iter().enumerate() {
                    let (hl_color, is_selected, is_cursor) = group_visuals[item_idx];
                    if !is_cursor {
                        continue;
                    }
                    let item_start_offset = offset + group.chunk_start;
                    let (group_start_x, group_end_x) = hex_group_x(*group, text_origin_x, params.hex_cell_width);

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
                            if params.low_nibble_offset == Some(params.cursor_offset) {
                                paint_underscore_cursor_at(
                                    window,
                                    byte_box_bounds,
                                    byte_start_x + params.hex_cell_width,
                                    params.hex_cell_width,
                                    cursor_border_color,
                                );
                            }
                        } else {
                            paint_underscore_cursor_at(window, byte_box_bounds, byte_start_x, byte_width, cursor_border_color);
                        }
                    } else {
                        let has_bg = hl_color.is_some() || is_selected;
                        let prev_has_bg = item_idx > 0 && (group_visuals[item_idx - 1].0.is_some() || group_visuals[item_idx - 1].1);
                        let next_has_bg =
                            item_idx + 1 < hex_source_ref.groups.len() && (group_visuals[item_idx + 1].0.is_some() || group_visuals[item_idx + 1].1);
                        let (cursor_start_x, cursor_end_x) = if has_bg {
                            (
                                if prev_has_bg {
                                    let prev_end_x = hex_group_x(hex_source_ref.groups[item_idx - 1], text_origin_x, params.hex_cell_width).1;
                                    pixel_midpoint(prev_end_x, group_start_x)
                                } else {
                                    group_start_x
                                },
                                if next_has_bg {
                                    let next_start_x = hex_group_x(hex_source_ref.groups[item_idx + 1], text_origin_x, params.hex_cell_width).0;
                                    pixel_midpoint(group_end_x, next_start_x)
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

                if is_eof_on_this_row {
                    let (eof_start_x, eof_width) = if hex_source_ref.groups.is_empty() {
                        (text_origin_x, hex_grid_x(2, params.hex_cell_width))
                    } else {
                        let last_group = *hex_source_ref.groups.last().unwrap();
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
            }

            // 3. Text Pass for Data Items
            if let Some(hex_glyphs) = hex_glyphs_ref {
                paint_centered_hex_glyphs(
                    hex_glyphs,
                    group_text_colors,
                    pending_edit_colors,
                    params.hex_cell_width,
                    point(text_origin_x, params.bounds.top() + px(2.0)),
                    line_height,
                    window,
                );
            }

            if insert_cursor_active && is_cursor_row {
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
                for (item_idx, group) in hex_source_ref.groups.iter().enumerate() {
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
                        && item_idx + 1 == hex_source_ref.groups.len()
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

                if hex_source_ref.groups.is_empty() && params.insert_cursor_offset == offset {
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
        });

        // Coalesce left/right gradient fades across contiguous rows
        if params.hex_scroll_x > 1.0 {
            params
                .scratch
                .fade_runs
                .push(FadeEdgeKind::HexLeft, params.row_idx, hex_start_x, hex_mask_bounds, row_top_y, row_bottom_y);
        }
        if params.hex_scroll_x + params.hex_col_width < total_data_width - 1.0 {
            params.scratch.fade_runs.push(
                FadeEdgeKind::HexRight,
                params.row_idx,
                hex_end_x - px(22.0),
                hex_mask_bounds,
                row_top_y,
                row_bottom_y,
            );
        }
    }

    // 3. ASCII Column (when ASCII view is enabled and row has bytes)
    if params.show_ascii && chunk_len > 0 && !is_collapsed_struct_row {
        let ascii_mask_bounds = clip_column_mask(
            scrollable_mask_bounds,
            Bounds::new(point(ascii_start_x, params.bounds.top()), size(px(ascii_width), px(ROW_HEIGHT))),
        );
        if ascii_mask_bounds.size.width > px(0.0) {
            let ascii_content_start_x = ascii_start_x - px(params.ascii_scroll_x);
            window.with_content_mask(Some(ContentMask { bounds: ascii_mask_bounds }), |window| {
                let group_bytes = params.group_size.byte_count();
                let has_ascii_glyphs = cached.ascii_glyphs.is_some();
                let ascii_cell_colors = &mut params.scratch.ascii_cell_colors;
                ascii_cell_colors.clear();
                let mut cell_cursor = 0;
                let cells = &cached.ascii_cells;
                let (sel_start_j, sel_end_j) = if params.min_sel <= params.max_sel {
                    let sel_byte_start = (params.min_sel / group_bytes) * group_bytes;
                    let sel_byte_end = (params.max_sel / group_bytes).saturating_add(1).saturating_mul(group_bytes);
                    if sel_byte_start < next_offset && sel_byte_end > offset {
                        (sel_byte_start.saturating_sub(offset), sel_byte_end.saturating_sub(offset).min(chunk.len()))
                    } else {
                        (0, 0)
                    }
                } else {
                    (0, 0)
                };

                // 1. Coalesced Background Quads Pass & Glyph Color Collection for ASCII Cells
                let mut paint_ascii_hl = |start: usize, end: usize, color: Hsla| {
                    let hl_bounds = Bounds::new(
                        point(ascii_content_start_x + px(start as f32 * ASCII_CELL_WIDTH), params.bounds.top() + px(1.0)),
                        size(px((end - start + 1) as f32 * ASCII_CELL_WIDTH), px(ROW_HEIGHT - 2.0)),
                    );
                    window.paint_quad(fill(hl_bounds, color));
                };
                let mut hl_tracker = ColorRunTracker::default();
                for j in 0..chunk.len() {
                    let byte_pos = offset + j;
                    let in_selected_group = j >= sel_start_j && j < sel_end_j;
                    let current_hl_color = highlight_color_for_range(byte_pos, byte_pos.saturating_add(1), active_row_highlights);
                    hl_tracker.push(j, current_hl_color, &mut paint_ascii_hl);

                    if has_ascii_glyphs {
                        while cells.get(cell_cursor).is_some_and(|c| c.cell_idx < j) {
                            cell_cursor += 1;
                        }
                        if let Some(cell) = cells.get(cell_cursor).filter(|c| c.cell_idx == j) {
                            let text_color = ascii_item_text_color_with(
                                in_selected_group,
                                current_hl_color,
                                cell.byte,
                                cell.is_printable,
                                &params.category_palette,
                                fg_color,
                                muted_color,
                                |fg, bg| contrast_cache.get_or_compute(fg, bg, |f, b| ensure_contrast(f, b, 4.5)),
                            );
                            ascii_cell_colors.push(text_color);
                            cell_cursor += 1;
                        }
                    }
                }
                hl_tracker.finish(&mut paint_ascii_hl);

                // Structure field active underline in ASCII
                if let Some((ref af_range, field_hsla)) = row_active_underline {
                    let ul_start = af_range.start.saturating_sub(offset);
                    let ul_end = af_range.end.saturating_sub(offset).min(chunk.len());
                    if ul_end > ul_start {
                        let underline_bounds = Bounds::new(
                            point(
                                ascii_content_start_x + px(ul_start as f32 * ASCII_CELL_WIDTH),
                                params.bounds.top() + px(ROW_HEIGHT - 2.0),
                            ),
                            size(px((ul_end - ul_start) as f32 * ASCII_CELL_WIDTH), px(2.0)),
                        );
                        window.paint_quad(fill(underline_bounds, field_hsla));
                    }
                }

                // Translucent selection quad (overlaid on top of highlight & underline)
                if sel_end_j > sel_start_j && selection_bg.a > 0.0 {
                    let sel_bounds = Bounds::new(
                        point(ascii_content_start_x + px(sel_start_j as f32 * ASCII_CELL_WIDTH), params.bounds.top() + px(1.0)),
                        size(px((sel_end_j - sel_start_j) as f32 * ASCII_CELL_WIDTH), px(ROW_HEIGHT - 2.0)),
                    );
                    window.paint_quad(fill(sel_bounds, selection_bg));
                }

                // 2. Cursor Border Pass for ASCII Column / Groups (only evaluated on the cursor row)
                if is_cursor_row && !params.insert_mode {
                    if params.active_column == EditColumn::Ascii {
                        if let Some(char_range) = params.ascii_cursor_char_range.as_ref().filter(|cr| cr.start < next_offset && cr.end > offset) {
                            let cell_start = char_range.start.saturating_sub(offset);
                            let cell_end = (char_range.end.saturating_sub(offset)).min(chunk.len());
                            if cell_end > cell_start {
                                let cursor_border_color = if params.is_focused { caret_color } else { darken_cursor_color(muted_color) };
                                let char_start_x = ascii_content_start_x + px(cell_start as f32 * ASCII_CELL_WIDTH);
                                let char_width = px((cell_end - cell_start) as f32 * ASCII_CELL_WIDTH);
                                let ascii_char_bounds = Bounds::new(point(char_start_x, params.bounds.top() + px(1.0)), size(char_width, px(ROW_HEIGHT - 2.0)));
                                paint_cursor_border(window, ascii_char_bounds, cursor_border_color);
                            }
                        } else if is_eof_on_this_row {
                            let cursor_border_color = if params.is_focused { caret_color } else { darken_cursor_color(muted_color) };
                            let ascii_x = ascii_content_start_x + px(chunk_len as f32 * ASCII_CELL_WIDTH);
                            let ascii_box_bounds = Bounds::new(point(ascii_x, params.bounds.top() + px(1.0)), size(px(ASCII_CELL_WIDTH), px(ROW_HEIGHT - 2.0)));
                            paint_cursor_border(window, ascii_box_bounds, cursor_border_color);
                        }
                    } else {
                        for group in hex_source_ref.groups.iter() {
                            let item_start_offset = offset + group.chunk_start;
                            let item_end_offset = offset + group.chunk_end;
                            if params.cursor_offset >= item_start_offset && params.cursor_offset < item_end_offset {
                                let cursor_border_color = if params.is_focused { caret_color } else { darken_cursor_color(muted_color) };
                                let group_start_x = ascii_content_start_x + px(group.chunk_start as f32 * ASCII_CELL_WIDTH);
                                let group_width = px((group.chunk_end - group.chunk_start) as f32 * ASCII_CELL_WIDTH);
                                let ascii_group_bounds =
                                    Bounds::new(point(group_start_x, params.bounds.top() + px(1.0)), size(group_width, px(ROW_HEIGHT - 2.0)));
                                paint_underscore_cursor_at(window, ascii_group_bounds, group_start_x, group_width, cursor_border_color);
                            }
                        }
                        if is_eof_on_this_row {
                            let cursor_border_color = if params.is_focused { caret_color } else { darken_cursor_color(muted_color) };
                            let ascii_x = ascii_content_start_x + px(chunk_len as f32 * ASCII_CELL_WIDTH);
                            let ascii_box_bounds = Bounds::new(point(ascii_x, params.bounds.top() + px(1.0)), size(px(ASCII_CELL_WIDTH), px(ROW_HEIGHT - 2.0)));
                            paint_underscore_cursor_at(window, ascii_box_bounds, ascii_x, px(ASCII_CELL_WIDTH), cursor_border_color);
                        }
                    }
                }

                // Paint each precomputed glyph centered in its fixed byte cell
                if let Some(ref ascii_glyphs) = cached.ascii_glyphs {
                    paint_centered_ascii_glyphs(
                        ascii_glyphs,
                        ascii_cell_colors,
                        point(ascii_content_start_x, params.bounds.top() + px(2.0)),
                        line_height,
                        window,
                    );
                }

                if insert_cursor_active && is_cursor_row {
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
                        let ascii_cursor_bounds = Bounds::new(point(cursor_x, params.bounds.top() + px(1.0)), size(px(ASCII_CELL_WIDTH), px(ROW_HEIGHT - 2.0)));
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
            });

            // Edge fades when the ASCII row is horizontally scrolled or clipped.
            if params.ascii_scroll_x > 1.0 {
                params.scratch.fade_runs.push(
                    FadeEdgeKind::AsciiLeft,
                    params.row_idx,
                    ascii_start_x,
                    ascii_mask_bounds,
                    row_top_y,
                    row_bottom_y,
                );
            }
            if params.ascii_scroll_x + ascii_width < chunk.len() as f32 * ASCII_CELL_WIDTH - 1.0 {
                params.scratch.fade_runs.push(
                    FadeEdgeKind::AsciiRight,
                    params.row_idx,
                    ascii_start_x + px(ascii_width - 22.0),
                    ascii_mask_bounds,
                    row_top_y,
                    row_bottom_y,
                );
            }
        }
    }

    // 4. Description Column (when structure definition is present)
    if let Some(ref desc) = cached.desc {
        let desc_mask_bounds = clip_column_mask(
            scrollable_mask_bounds,
            Bounds::new(point(desc_start_x, params.bounds.top()), size(px(params.desc_col_width), px(ROW_HEIGHT))),
        );
        if desc_mask_bounds.size.width > px(0.0) {
            window.with_content_mask(Some(ContentMask { bounds: desc_mask_bounds }), |window| {
                // Vertical tree guide lines for indentation depth
                if desc.indent_level > 0 {
                    for lvl in 1..=desc.indent_level {
                        let guide_x = desc_start_x - px(params.desc_scroll_x) + px((lvl - 1) as f32 * 14.0 + 5.0);
                        if guide_x >= desc_start_x && guide_x < desc_end_x {
                            window.paint_quad(fill(
                                Bounds::new(point(guide_x, params.bounds.top()), size(px(1.0), px(ROW_HEIGHT))),
                                desc_palette.tree_guide,
                            ));
                        }
                    }
                }

                // Paint Lucide chevron icon if container header
                if let Some(ref icon_path) = desc.chevron_icon_path {
                    let icon_x = desc_start_x - px(params.desc_scroll_x) + px(desc.indent_px);
                    let icon_y = params.bounds.top() + px((ROW_HEIGHT - CHEVRON_ICON_SIZE) / 2.0);
                    let icon_bounds = Bounds::new(point(icon_x, icon_y), size(px(CHEVRON_ICON_SIZE), px(CHEVRON_ICON_SIZE)));
                    let _ = window.paint_svg(icon_bounds, icon_path.clone(), None, TransformationMatrix::unit(), desc_palette.header_icon, cx);
                }

                let text_pos = point(desc_start_x - px(params.desc_scroll_x) + px(desc.text_offset_px), params.bounds.top() + px(2.0));
                if let Some(af) = active_leaf_field {
                    for &(f_offset, f_size, span_start_x, span_width_px) in &desc.leaf_field_spans {
                        if f_offset == af.offset && f_size == af.size && span_width_px > 0.0 {
                            window.paint_quad(fill(
                                Bounds::new(point(text_pos.x + px(span_start_x), text_pos.y), size(px(span_width_px), line_height)),
                                desc_palette.active_bg,
                            ));
                        }
                    }
                }
                if let Some(ref shaped) = desc.shaped_base_desc {
                    let _ = shaped.paint(text_pos, line_height, TextAlign::Left, None, window, cx);
                }
            });

            if params.desc_scroll_x > 1.0 {
                params
                    .scratch
                    .fade_runs
                    .push(FadeEdgeKind::DescLeft, params.row_idx, desc_start_x, desc_mask_bounds, row_top_y, row_bottom_y);
            }
            if params.desc_scroll_x + params.desc_col_width < desc.desc_text_width - 1.0 {
                params.scratch.fade_runs.push(
                    FadeEdgeKind::DescRight,
                    params.row_idx,
                    desc_end_x - px(20.0),
                    desc_mask_bounds,
                    row_top_y,
                    row_bottom_y,
                );
            }
        }
    }

    // 5. Bookmark Comments Column
    if !params.bookmark_comments.is_empty() {
        let comment_mask_bounds = clip_column_mask(
            scrollable_mask_bounds,
            Bounds::new(point(comment_start_x, params.bounds.top()), size(px(params.comment_col_width), px(ROW_HEIGHT))),
        );
        if comment_mask_bounds.size.width > px(0.0) {
            let comment_end_x = comment_start_x + px(params.comment_col_width);

            let dot_size = 8.0;
            let dot_radius = 4.0;
            let dot_margin_right = 5.0;
            let item_spacing = 14.0;

            let mut total_content_width = 4.0;
            for item in params.bookmark_comments {
                let text_w = comment_shapes.ensure_shaped_width(item.clean_text, muted_color, font, params.font_size, window);
                total_content_width += dot_size + dot_margin_right + text_w + item_spacing;
            }
            let comment_text_width = total_content_width;

            window.with_content_mask(Some(ContentMask { bounds: comment_mask_bounds }), |window| {
                let mut cur_x = comment_start_x - px(params.comment_scroll_x) + px(4.0);
                let dot_y = params.bounds.top() + px((ROW_HEIGHT - dot_size) / 2.0);

                for item in params.bookmark_comments {
                    let Some((shaped_comment, text_w)) = comment_shapes.get(item.clean_text, muted_color, params.font_size) else {
                        continue;
                    };
                    let dot_bounds = Bounds::new(point(cur_x, dot_y), size(px(dot_size), px(dot_size)));
                    let mut dot_quad = fill(dot_bounds, item.badge_color);
                    dot_quad.corner_radii = Corners {
                        top_left: px(dot_radius),
                        top_right: px(dot_radius),
                        bottom_left: px(dot_radius),
                        bottom_right: px(dot_radius),
                    };
                    window.paint_quad(dot_quad);

                    let text_x = cur_x + px(dot_size + dot_margin_right);
                    let _ = shaped_comment.paint(point(text_x, params.bounds.top() + px(2.0)), line_height, TextAlign::Left, None, window, cx);

                    cur_x = text_x + px(text_w + item_spacing);
                }
            });

            if params.comment_scroll_x > 1.0 {
                params.scratch.fade_runs.push(
                    FadeEdgeKind::CommentLeft,
                    params.row_idx,
                    comment_start_x,
                    comment_mask_bounds,
                    row_top_y,
                    row_bottom_y,
                );
            }
            if params.comment_scroll_x + params.comment_col_width < comment_text_width - 1.0 {
                params.scratch.fade_runs.push(
                    FadeEdgeKind::CommentRight,
                    params.row_idx,
                    comment_end_x - px(20.0),
                    comment_mask_bounds,
                    row_top_y,
                    row_bottom_y,
                );
            }
        }
    }
}
