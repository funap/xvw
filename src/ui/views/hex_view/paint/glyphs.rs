use super::super::layout::*;
use super::super::row_cache::*;
use super::super::types::*;
use crate::core::radix::{ByteGroupSize, DisplayRadix};
use gpui_kit::*;

const TEXT_INPUT_CURSOR_WIDTH: Pixels = px(1.5);
const TEXT_INPUT_CURSOR_HEIGHT_RATIO: f32 = 0.85;

#[inline]
pub fn paint_border_box(window: &mut Window, bounds: Bounds<Pixels>, border_width: Pixels, color: Hsla) {
    let outline = outline(bounds, color, BorderStyle::Solid).border_widths(border_width);
    window.paint_quad(outline);
}

#[inline]
pub(super) fn clip_column_mask(scrollable: Bounds<Pixels>, col: Bounds<Pixels>) -> Bounds<Pixels> {
    let left = scrollable.origin.x.max(col.origin.x);
    let top = scrollable.origin.y.max(col.origin.y);
    let right = (scrollable.origin.x + scrollable.size.width).min(col.origin.x + col.size.width);
    let bottom = (scrollable.origin.y + scrollable.size.height).min(col.origin.y + col.size.height);
    Bounds::new(point(left, top), size((right - left).max(px(0.0)), (bottom - top).max(px(0.0))))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn paint_horizontal_fade(
    window: &mut Window,
    start_x: Pixels,
    top_y: Pixels,
    height: Pixels,
    steps: usize,
    step_dx: f32,
    strip_w: f32,
    bg: Hsla,
    left_edge: bool,
) {
    let denom = if left_edge { steps as f32 } else { (steps + 1) as f32 };
    for step in 0..steps {
        let x = start_x + px(step as f32 * step_dx);
        let alpha = if left_edge { 1.0 - (step as f32 / denom) } else { (step + 1) as f32 / denom };
        window.paint_quad(fill(Bounds::new(point(x, top_y), size(px(strip_w), height)), bg.opacity(alpha * 0.95)));
    }
}

/// Paints all coalesced column edge fade gradients accumulated across rows in the current frame.
pub fn flush_column_fade_runs(fade_runs: &ColumnFadeRuns, bg_color_theme: Hsla, window: &mut Window) {
    for edge in FadeEdgeKind::ALL {
        let runs = fade_runs.runs_for(edge);
        if runs.is_empty() {
            continue;
        }
        let (steps, step_dx, strip_w, left_edge) = edge.gradient_spec();
        for run in runs {
            let height = px(run.bottom_y - run.top_y);
            let mask_bounds = Bounds::new(point(px(run.mask_left), px(run.top_y)), size(px(run.mask_width), height));
            window.with_content_mask(Some(ContentMask { bounds: mask_bounds }), |window| {
                paint_horizontal_fade(
                    window,
                    px(run.fade_start_x),
                    px(run.top_y),
                    height,
                    steps,
                    step_dx,
                    strip_w,
                    bg_color_theme,
                    left_edge,
                );
            });
        }
    }
}

/// Coalesces consecutive adjacent non-transparent color spans into minimal inclusive index runs.
#[derive(Default)]
pub(super) struct ColorRunTracker {
    active: Option<(usize, usize, Hsla)>,
}

impl ColorRunTracker {
    #[inline]
    pub(super) fn push(&mut self, idx: usize, opt_color: Option<Hsla>, mut on_run: impl FnMut(usize, usize, Hsla)) {
        let color = opt_color.filter(|c| c.a > 0.0);
        match (self.active, color) {
            (Some((start, _, prev_c)), Some(c)) if prev_c == c => {
                self.active = Some((start, idx, prev_c));
            }
            (Some((start, end, prev_c)), next) => {
                on_run(start, end, prev_c);
                self.active = next.map(|c| (idx, idx, c));
            }
            (None, Some(c)) => {
                self.active = Some((idx, idx, c));
            }
            (None, None) => {}
        }
    }

    #[inline]
    pub(super) fn finish(self, mut on_run: impl FnMut(usize, usize, Hsla)) {
        if let Some((start, end, c)) = self.active {
            on_run(start, end, c);
        }
    }
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
pub(super) fn paint_insert_cursor_at(window: &mut Window, bounds: Bounds<Pixels>, cursor_x: Pixels, color: Hsla) {
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
pub(super) fn paint_underscore_cursor_at(window: &mut Window, bounds: Bounds<Pixels>, cursor_x: Pixels, width: Pixels, color: Hsla) {
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
pub(super) struct HexInsertCursorParams {
    pub(super) radix: DisplayRadix,
    pub(super) group_size: ByteGroupSize,
    pub(super) is_big_endian: bool,
    pub(super) selection_active: bool,
    pub(super) has_pending_nibble: bool,
    pub(super) origin_x: Pixels,
    pub(super) cell_width: Pixels,
}

/// Returns the x-coordinate for the insertion boundary represented by the
/// cursor within a rendered hex group and the width of its byte slot.
pub(super) fn hex_insert_cursor_geometry(group: HexGroupInfo, cursor_in_row: usize, params: HexInsertCursorParams) -> Option<(Pixels, Pixels)> {
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
pub(super) fn hex_insert_cursor_end_geometry(group: HexGroupInfo, origin_x: Pixels, cell_width: Pixels, group_size: ByteGroupSize) -> (Pixels, Pixels) {
    let expected_len = group_size.byte_count();
    let group_len = group.chunk_end.saturating_sub(group.chunk_start);
    let data_end_slot = group.start_slot.saturating_add(group_len).min(expected_len);
    let slot_width = (group.text_end.saturating_sub(group.text_start) as f32 / expected_len.max(1) as f32).max(1.0);
    (
        origin_x + px((group.text_start as f32 + data_end_slot as f32 * slot_width) * f32::from(cell_width)),
        px(slot_width * f32::from(cell_width)),
    )
}

/// Precomputes centered per-cell Hex glyph coordinates from a batched shaped line.
pub fn build_cached_hex_glyphs(shaped: &ShapedLine, groups: &[HexGroupInfo], cell_width: Pixels) -> CachedHexGlyphs {
    let natural_width = shaped.width;
    let cell_width_f = f32::from(cell_width);
    let mut glyphs_out = Vec::with_capacity(shaped.text.len());
    let mut group_idx = 0;
    let mut glyphs = shaped
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter().map(move |glyph| (run.font_id, glyph)))
        .peekable();

    while let Some((font_id, glyph)) = glyphs.next() {
        let natural_end = glyphs.peek().map(|(_, next)| next.position.x).unwrap_or(natural_width);
        let glyph_width = f32::from(natural_end - glyph.position.x).max(0.0);

        while group_idx < groups.len() && glyph.index >= groups[group_idx].text_end {
            group_idx += 1;
        }
        if group_idx < groups.len() && groups[group_idx].text_start <= glyph.index && glyph.index < groups[group_idx].text_end {
            let centered_offset = centered_glyph_offset(cell_width_f, glyph_width);
            let x_offset = hex_grid_x(glyph.index, cell_width) + px(centered_offset);
            glyphs_out.push(CachedHexGlyph {
                font_id,
                glyph_id: glyph.id,
                text_index: glyph.index,
                group_idx,
                x_offset,
                glyph_width,
                is_emoji: glyph.is_emoji,
            });
        }
    }

    CachedHexGlyphs {
        cell_width,
        font_size: shaped.font_size,
        ascent: shaped.ascent,
        descent: shaped.descent,
        glyphs: glyphs_out,
    }
}

/// Precomputes centered per-cell ASCII glyph coordinates from a batched shaped line.
pub fn build_cached_ascii_glyphs(shaped: &ShapedLine, cells: &[CachedAsciiCell], cell_spans: &[(usize, usize)]) -> CachedAsciiGlyphs {
    let natural_width = shaped.width;
    let mut glyphs_out = Vec::with_capacity(cells.len());
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

        while entry_idx < cell_spans.len() && glyph.index >= cell_spans[entry_idx].1 {
            entry_idx += 1;
        }

        if entry_idx < cell_spans.len() && cell_spans[entry_idx].0 <= glyph.index && glyph.index < cell_spans[entry_idx].1 {
            let cell_idx = cells[entry_idx].cell_idx;
            glyphs_out.push(CachedAsciiGlyph {
                font_id,
                glyph_id: glyph.id,
                x_offset: px(cell_idx as f32 * ASCII_CELL_WIDTH + centered_offset),
                cell_index: entry_idx,
                is_emoji: glyph.is_emoji,
            });
        }
    }

    CachedAsciiGlyphs {
        font_size: shaped.font_size,
        ascent: shaped.ascent,
        descent: shaped.descent,
        glyphs: glyphs_out,
    }
}

/// Paints precomputed centered Hex glyphs without allocating a per-row paint layer or iterator.
pub fn paint_centered_hex_glyphs(
    cached: &CachedHexGlyphs,
    group_colors: &[Hsla],
    pending_edit_colors: Option<(usize, Hsla, Hsla)>,
    cell_width: Pixels,
    origin: Point<Pixels>,
    line_height: Pixels,
    window: &mut Window,
) {
    let same_cell_width = cached.cell_width == cell_width;
    let cell_width_f = f32::from(cell_width);
    let baseline_y = (line_height - cached.ascent - cached.descent) / 2.0 + cached.ascent;
    let base_y = origin.y + baseline_y;

    for glyph in &cached.glyphs {
        let mut color = group_colors.get(glyph.group_idx).copied();
        if let Some((p_char, typed_color, placeholder_color)) = pending_edit_colors {
            if glyph.text_index == p_char {
                color = Some(typed_color);
            } else if glyph.text_index == p_char + 1 {
                color = Some(placeholder_color);
            }
        }
        if let Some(color) = color {
            let x_offset = if same_cell_width {
                glyph.x_offset
            } else {
                hex_grid_x(glyph.text_index, cell_width) + px(centered_glyph_offset(cell_width_f, glyph.glyph_width))
            };
            let glyph_origin = point(origin.x + x_offset, base_y);
            if glyph.is_emoji {
                let _ = window.paint_emoji(glyph_origin, glyph.font_id, glyph.glyph_id, cached.font_size);
            } else {
                let _ = window.paint_glyph(glyph_origin, glyph.font_id, glyph.glyph_id, cached.font_size, color);
            }
        }
    }
}

/// Paints precomputed centered ASCII glyphs from cached cell colors.
pub fn paint_centered_ascii_glyphs(cached: &CachedAsciiGlyphs, cell_colors: &[Hsla], origin: Point<Pixels>, line_height: Pixels, window: &mut Window) {
    let baseline_y = (line_height - cached.ascent - cached.descent) / 2.0 + cached.ascent;
    let base_y = origin.y + baseline_y;

    for glyph in &cached.glyphs {
        let Some(&color) = cell_colors.get(glyph.cell_index) else {
            continue;
        };
        let glyph_origin = point(origin.x + glyph.x_offset, base_y);
        if glyph.is_emoji {
            let _ = window.paint_emoji(glyph_origin, glyph.font_id, glyph.glyph_id, cached.font_size);
        } else {
            let _ = window.paint_glyph(glyph_origin, glyph.font_id, glyph.glyph_id, cached.font_size, color);
        }
    }
}
