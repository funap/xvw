use super::layout::*;
use super::row_cache::*;
use super::types::*;
use crate::core::bookmark::BookmarkItem;
use crate::core::document::Document;
use crate::core::editor::LineMap;
use crate::core::encoding::Encoding;
use crate::core::radix::{ByteGroupSize, DisplayRadix, byte_category, digit_count, group_category};
use crate::core::structure::{IndexedField, ParseResult};
use crate::ui::color::BookmarkColorExt;
use crate::ui::icon::IconName;
use gpui::TransformationMatrix;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::IconNamed;
use gpui_kit::component::theme::Theme;
use gpui_kit::*;
use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Range;

pub const CHEVRON_ICON_SIZE: f32 = 14.0;
pub const CHEVRON_ICON_GAP: f32 = 4.0;

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

/// Computes the text color for a hex group item taking into account cursor focus,
/// active selection, highlights, and category palette.
#[inline]
pub fn hex_item_text_color(is_cursor_focused: bool, is_selected: bool, has_hl: bool, item_slice: &[u8], palette: &ByteCategoryPalette, fg_color: Hsla) -> Hsla {
    if is_cursor_focused || is_selected || has_hl {
        fg_color
    } else {
        palette.color_for(group_category(item_slice))
    }
}

/// Computes the text color for an ASCII character cell taking into account selection,
/// highlights, printable status, and category palette.
#[inline]
pub fn ascii_item_text_color(
    in_selected_group: bool,
    has_hl: bool,
    byte: u8,
    is_printable: bool,
    palette: &ByteCategoryPalette,
    fg_color: Hsla,
    muted_color: Hsla,
) -> Hsla {
    if has_hl || in_selected_group {
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

/// Returns whether the row at `row_idx` (with byte range `[offset, next_offset)`) contains the active cursor position.
pub fn is_cursor_row(active_cursor_offset: usize, row_idx: usize, offset: usize, next_offset: usize, line_starts: &LineMap, total_size: usize) -> bool {
    let is_eof = active_cursor_offset == total_size;
    let last_line_idx = line_starts.len().saturating_sub(1);
    let last_line_start = line_starts.get(last_line_idx).unwrap_or(0);
    let bytes_per_row = line_starts.max_bytes_per_row();
    let is_eof_on_new_line = total_size > last_line_start && (total_size - last_line_start) >= bytes_per_row;
    let is_eof_on_this_row = is_eof && ((is_eof_on_new_line && row_idx == line_starts.len()) || (!is_eof_on_new_line && row_idx == last_line_idx));

    (active_cursor_offset >= offset && active_cursor_offset < next_offset) || is_eof_on_this_row
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
    pub category_palette: ByteCategoryPalette,
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
    pub desc_palette: DescriptionPalette,
    pub active_leaf_field: Option<&'a IndexedField>,
    pub theme_colors: &'a RowThemeColors,
    pub row_cache: &'a mut RowRenderCache,
}

/// Sanitizes a string to guarantee single-line presentation without newlines or carriage returns.
#[inline]
pub fn clean_single_line(s: &str) -> Cow<'_, str> {
    if s.contains('\n') || s.contains('\r') {
        Cow::Owned(s.replace("\r\n", " ").replace(['\n', '\r'], " "))
    } else {
        Cow::Borrowed(s)
    }
}

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

/// Calculates contrast ratio between two colors according to WCAG 2.1 specs.
#[inline]
pub fn contrast_ratio(c1: Hsla, c2: Hsla) -> f32 {
    let l1 = rel_luminance(c1);
    let l2 = rel_luminance(c2);
    let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

/// Adjusts color lightness if necessary to guarantee a minimum WCAG contrast ratio against `background`.
pub fn ensure_contrast(color: Hsla, background: Hsla, min_ratio: f32) -> Hsla {
    let mut solid_color = color;
    solid_color.a = 1.0;
    let ratio = contrast_ratio(solid_color, background);
    if ratio >= min_ratio {
        return solid_color;
    }
    let bg_lum = rel_luminance(background);
    let is_dark_bg = bg_lum < 0.2;

    for _ in 0..25 {
        if is_dark_bg {
            solid_color.l = (solid_color.l + 0.03).min(1.0);
        } else {
            solid_color.l = (solid_color.l - 0.03).max(0.0);
        }
        if contrast_ratio(solid_color, background) >= min_ratio {
            break;
        }
    }
    solid_color
}

/// Color palette for the structure description column, designed for high legibility
/// across both light and dark themes using theme tokens and WCAG contrast guarantees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DescriptionPalette {
    pub field_id: Hsla,
    pub type_name: Hsla,
    pub punctuation: Hsla,
    pub number_value: Hsla,
    pub string_value: Hsla,
    pub bytes_value: Hsla,
    pub enum_label: Hsla,
    pub comment: Hsla,
    pub error: Hsla,
    pub header_icon: Hsla,
    pub continuation: Hsla,
    pub size_badge: Hsla,
    pub tree_guide: Hsla,
    pub active_bg: Hsla,
}

impl DescriptionPalette {
    pub fn from_theme(theme: &Theme) -> Self {
        let bg = theme.background;
        let punct_base = theme.muted_foreground;
        Self {
            field_id: ensure_contrast(theme.foreground, bg, 4.5),
            type_name: ensure_contrast(theme.blue, bg, 4.5),
            punctuation: ensure_contrast(punct_base, bg, 3.0),
            number_value: ensure_contrast(theme.magenta, bg, 4.5),
            string_value: ensure_contrast(theme.green, bg, 4.5),
            bytes_value: ensure_contrast(theme.cyan, bg, 4.0),
            enum_label: ensure_contrast(theme.yellow, bg, 4.5),
            comment: ensure_contrast(theme.muted_foreground, bg, 4.0),
            error: ensure_contrast(theme.red, bg, 4.5),
            header_icon: ensure_contrast(theme.accent_foreground, bg, 4.5),
            continuation: ensure_contrast(theme.muted_foreground, bg, 3.5),
            size_badge: ensure_contrast(theme.muted_foreground, bg, 3.5),
            tree_guide: ensure_contrast(theme.border, bg, 1.8),
            active_bg: theme.secondary,
        }
    }
}

/// Builds formatted text and syntax-highlighted `TextRun`s for a container structure header.
pub fn build_structure_header_runs(container: &IndexedField, is_collapsed: bool, font: Font, palette: &DescriptionPalette) -> (String, Vec<TextRun>) {
    let mut text = String::with_capacity(64);
    let mut runs = Vec::new();

    // 1. Field Name (id)
    let id_start = text.len();
    text.push_str(&clean_single_line(&container.id));
    runs.push(TextRun {
        len: text.len() - id_start,
        font: font.clone(),
        color: palette.field_id,
        background_color: None,
        underline: None,
        strikethrough: None,
    });

    // 2. Type name (": type")
    if !container.type_name.is_empty() && container.type_name != container.id {
        let punct_start = text.len();
        text.push_str(": ");
        runs.push(TextRun {
            len: text.len() - punct_start,
            font: font.clone(),
            color: palette.punctuation,
            background_color: None,
            underline: None,
            strikethrough: None,
        });

        let type_start = text.len();
        text.push_str(&clean_single_line(&container.type_name));
        runs.push(TextRun {
            len: text.len() - type_start,
            font: font.clone(),
            color: palette.type_name,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    // 3. Size & status badge (" (30B)" or " (30B, collapsed)")
    let size_start = text.len();
    if is_collapsed {
        text.push_str(&format!(" ({}B, collapsed)", container.size));
    } else {
        text.push_str(&format!(" ({}B)", container.size));
    }
    runs.push(TextRun {
        len: text.len() - size_start,
        font: font.clone(),
        color: palette.size_badge,
        background_color: None,
        underline: None,
        strikethrough: None,
    });

    (text, runs)
}

/// Builds formatted text and syntax-highlighted `TextRun`s for a leaf field.
pub fn build_leaf_field_runs(field: &IndexedField, font: Font, palette: &DescriptionPalette, include_comment: bool, is_active: bool) -> (String, Vec<TextRun>) {
    let mut text = String::with_capacity(64);
    let mut runs = Vec::new();
    let bg_color = if is_active { Some(palette.active_bg) } else { None };

    // 1. Color pip ("▪ ")
    let pip_color = field.color.map(Hsla::from).unwrap_or(palette.field_id);
    text.push_str("▪ ");
    runs.push(TextRun {
        len: "▪ ".len(),
        font: font.clone(),
        color: pip_color,
        background_color: bg_color,
        underline: None,
        strikethrough: None,
    });

    // 2. Field ID
    let id_start = text.len();
    text.push_str(&clean_single_line(&field.id));
    runs.push(TextRun {
        len: text.len() - id_start,
        font: font.clone(),
        color: palette.field_id,
        background_color: bg_color,
        underline: None,
        strikethrough: None,
    });

    // 3. Optional Type name
    if !field.type_name.is_empty() && field.type_name != field.id {
        let punct_start = text.len();
        text.push_str(": ");
        runs.push(TextRun {
            len: text.len() - punct_start,
            font: font.clone(),
            color: palette.punctuation,
            background_color: bg_color,
            underline: None,
            strikethrough: None,
        });

        let type_start = text.len();
        text.push_str(&clean_single_line(&field.type_name));
        runs.push(TextRun {
            len: text.len() - type_start,
            font: font.clone(),
            color: palette.type_name,
            background_color: bg_color,
            underline: None,
            strikethrough: None,
        });
    }

    // 4. Operator " = "
    let op_start = text.len();
    text.push_str(" = ");
    runs.push(TextRun {
        len: text.len() - op_start,
        font: font.clone(),
        color: palette.punctuation,
        background_color: bg_color,
        underline: None,
        strikethrough: None,
    });

    // 5. Value
    let val_start = text.len();
    let val_str = if !field.formatted_value.is_empty() {
        &field.formatted_value
    } else {
        field.format_expression()
    };
    let val_clean = clean_single_line(val_str);
    text.push_str(&val_clean);
    let val_color = if field.validation_failed {
        palette.error
    } else if val_clean.starts_with('"') {
        palette.string_value
    } else if val_clean.starts_with('[') {
        palette.bytes_value
    } else {
        palette.number_value
    };
    runs.push(TextRun {
        len: text.len() - val_start,
        font: font.clone(),
        color: val_color,
        background_color: bg_color,
        underline: None,
        strikethrough: None,
    });

    // 6. Enum label
    if let Some(enum_label) = &field.enum_label {
        let punct_start = text.len();
        text.push_str(" (");
        runs.push(TextRun {
            len: text.len() - punct_start,
            font: font.clone(),
            color: palette.punctuation,
            background_color: bg_color,
            underline: None,
            strikethrough: None,
        });

        let enum_start = text.len();
        text.push_str(&clean_single_line(enum_label));
        runs.push(TextRun {
            len: text.len() - enum_start,
            font: font.clone(),
            color: palette.enum_label,
            background_color: bg_color,
            underline: None,
            strikethrough: None,
        });

        let punct_close = text.len();
        text.push(')');
        runs.push(TextRun {
            len: text.len() - punct_close,
            font: font.clone(),
            color: palette.punctuation,
            background_color: bg_color,
            underline: None,
            strikethrough: None,
        });
    }

    // 7. Doc comment (if enabled and present)
    if include_comment
        && let Some(desc) = &field.description
        && !desc.is_empty()
        && let Some(first_line) = desc.lines().map(str::trim).find(|line| !line.is_empty())
    {
        let punct_start = text.len();
        text.push_str(" // ");
        runs.push(TextRun {
            len: text.len() - punct_start,
            font: font.clone(),
            color: palette.punctuation,
            background_color: bg_color,
            underline: None,
            strikethrough: None,
        });

        let doc_start = text.len();
        text.push_str(&clean_single_line(first_line));
        runs.push(TextRun {
            len: text.len() - doc_start,
            font: font.clone(),
            color: palette.comment,
            background_color: bg_color,
            underline: None,
            strikethrough: None,
        });
    }

    (text, runs)
}

/// Builds continuation runs for fields spanning across multiple 16-byte rows.
pub fn build_continuation_field_runs(field_id: &str, relative_offset: usize, font: Font, palette: &DescriptionPalette) -> (String, Vec<TextRun>) {
    let mut text = String::with_capacity(32);
    let mut runs = Vec::new();

    let prefix_start = text.len();
    text.push_str("↳ ");
    runs.push(TextRun {
        len: text.len() - prefix_start,
        font: font.clone(),
        color: palette.continuation,
        background_color: None,
        underline: None,
        strikethrough: None,
    });

    let id_start = text.len();
    text.push_str(&clean_single_line(field_id));
    runs.push(TextRun {
        len: text.len() - id_start,
        font: font.clone(),
        color: palette.field_id,
        background_color: None,
        underline: None,
        strikethrough: None,
    });

    let off_start = text.len();
    text.push_str(&format!(" (+0x{:X})", relative_offset));
    runs.push(TextRun {
        len: text.len() - off_start,
        font: font.clone(),
        color: palette.continuation,
        background_color: None,
        underline: None,
        strikethrough: None,
    });

    (text, runs)
}

/// Computes the tree indentation depth for a structure description row.
///
/// Indentation strictly respects the tree nesting depth (`depth`) of the
/// field being rendered on this row (container, leaf, or continuation).
/// This prevents elements sharing identical identifiers (such as `body` in
/// ZIP specifications) from inheriting the shallower depth of an ancestor.
pub fn calculate_description_indent_level(
    continuation_leaf: Option<&IndexedField>,
    container: Option<&IndexedField>,
    leaf: Option<&IndexedField>,
    active_range_count: usize,
) -> usize {
    if let Some(leaf) = continuation_leaf {
        leaf.depth
    } else if let Some(container) = container {
        container.depth
    } else if let Some(leaf) = leaf {
        leaf.depth
    } else if active_range_count > 0 {
        active_range_count
    } else {
        0
    }
}

#[inline]
fn resolve_row_leaf_fields<'a>(
    parse_res: Option<&'a ParseResult>,
    offset: usize,
    chunk_len: usize,
    first_leaf: Option<&'a IndexedField>,
    row_leaves: &'a [IndexedField],
) -> Vec<Option<Cow<'a, IndexedField>>> {
    let mut result = Vec::with_capacity(chunk_len);
    for j in 0..chunk_len {
        let byte_pos = offset + j;
        let mut best: Option<&'a IndexedField> = None;

        if let Some(fl) = first_leaf
            && fl.offset <= byte_pos
            && byte_pos < fl.offset + fl.size
        {
            best = Some(fl);
        }

        for leaf in row_leaves {
            if leaf.offset <= byte_pos && byte_pos < leaf.offset + leaf.size && best.is_none_or(|b| leaf.depth > b.depth) {
                best = Some(leaf);
            }
        }

        if let Some(b) = best {
            result.push(Some(Cow::Borrowed(b)));
        } else if let Some(pr) = parse_res {
            result.push(pr.query_deepest_leaf_at(byte_pos));
        } else {
            result.push(None);
        }
    }
    result
}

pub fn build_cached_row(params: &RowPaintParams, offset: usize, next_offset: usize, window: &mut Window) -> CachedRow {
    let chunk_len = next_offset - offset;
    let chunk = params.doc.buffer.get_range(offset, chunk_len);
    let total_size = params.doc.buffer.len();
    let is_struct_mode = params.parse_result.is_some();
    let font = font(params.font_family.clone());

    let physical_address = params.doc.offset_to_address(offset);
    let is_structure_header_row = is_struct_mode && chunk_len == 0;
    let query_len = chunk_len.max(1);

    let raw_containers: Cow<'_, [IndexedField]> = if let Some(parse_res) = params.parse_result {
        if parse_res.is_live() {
            Cow::Owned(parse_res.find_live_container_structs_starting_at(offset, query_len))
        } else {
            Cow::Borrowed(parse_res.find_container_structs_starting_at(offset, query_len))
        }
    } else {
        Cow::Borrowed(&[])
    };

    let is_collapsed_struct_row = is_struct_mode
        && chunk_len > 0
        && raw_containers.iter().any(|c| {
            c.offset == offset
                && (c.size == chunk_len || chunk_len == c.size.min(total_size.saturating_sub(offset)))
                && (params.collapsed_structs.contains(&c.collapse_key) || params.collapsed_structs.contains(&c.id))
        });

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

    let is_uniform_row_leaf = first_leaf.as_ref().is_none_or(|fl| fl.offset + fl.size >= next_offset)
        && row_leaves.iter().all(|lf| {
            first_leaf
                .as_ref()
                .is_some_and(|fl| lf.offset == fl.offset && lf.size == fl.size && lf.depth <= fl.depth)
        });

    let row_leaf_fields: Vec<Option<IndexedField>> = if is_uniform_row_leaf || chunk_len == 0 {
        Vec::new()
    } else {
        resolve_row_leaf_fields(params.parse_result, offset, chunk_len, first_leaf.as_deref(), &row_leaves)
            .into_iter()
            .map(|opt| opt.map(|c| c.into_owned()))
            .collect()
    };

    let structure_fields = if is_struct_mode {
        Some(CachedRowStructureFields {
            is_structure_header_row,
            is_collapsed_struct_row,
            first_leaf: first_leaf.as_deref().cloned(),
            is_uniform_row_leaf,
            row_leaf_fields,
        })
    } else {
        None
    };

    // Address column
    let shaped_addr = if is_struct_mode || params.show_offset {
        let addr_str = format_offset_08(physical_address);
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
    let hex_source = build_hex_text_source(hex_chunk, offset, params.radix, params.group_size, params.is_big_endian);
    let shaped_hex = if !hex_source.text.is_empty() {
        let run = TextRun {
            len: hex_source.text.len(),
            font: font.clone(),
            color: params.theme_colors.fg_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        Some(window.text_system().shape_line(hex_source.text.clone(), params.font_size, &[run], None))
    } else {
        None
    };

    // ASCII column
    let mut ascii_cells = Vec::with_capacity(chunk.len());
    let shaped_ascii = if params.show_ascii && chunk_len > 0 && !is_collapsed_struct_row {
        let char_map = build_ascii_char_map(params.encoding, params.doc.buffer.data(), offset, chunk.len());
        let mut ascii_text = String::with_capacity(chunk.len());
        for (j, opt) in char_map.into_iter().enumerate() {
            let item = if let Some(item) = opt { item } else { continue };
            let c = item.character();
            let text_byte_start = ascii_text.len();
            ascii_text.push(c);
            let text_byte_end = ascii_text.len();
            let byte = chunk.get(j).copied().unwrap_or(0);
            ascii_cells.push(CachedAsciiCell {
                cell_idx: j,
                text_byte_start,
                text_byte_end,
                byte,
                is_printable: item.is_printable(),
            });
        }
        if !ascii_text.is_empty() {
            let run = TextRun {
                len: ascii_text.len(),
                font: font.clone(),
                color: params.theme_colors.fg_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            Some(window.text_system().shape_line(SharedString::from(ascii_text), params.font_size, &[run], None))
        } else {
            None
        }
    } else {
        None
    };

    // Description column
    let desc = if let Some(parse_res) = params.parse_result {
        let has_header_at_offset = raw_containers.iter().any(|header| {
            header.offset == offset
                && header.size > 0
                && !header.is_instance
                && !params.collapsed_structs.contains(&header.collapse_key)
                && !params.collapsed_structs.contains(&header.id)
        });
        let has_header_before =
            !is_structure_header_row && params.row_idx > 0 && params.line_starts.get(params.row_idx - 1) == Some(offset) && has_header_at_offset;
        let mut container_structs: Vec<IndexedField> = if has_header_before {
            raw_containers
                .iter()
                .filter(|container| params.collapsed_structs.contains(&container.collapse_key) || params.collapsed_structs.contains(&container.id))
                .cloned()
                .collect()
        } else {
            raw_containers.into_owned()
        };

        if is_collapsed_struct_row {
            container_structs.retain(|container| {
                !container.is_instance
                    && container.offset == offset
                    && (params.collapsed_structs.contains(&container.collapse_key) || params.collapsed_structs.contains(&container.id))
            });
        }

        if is_structure_header_row {
            container_structs.retain(|container| !container.is_instance && container.size > 0);
        }

        if is_structure_header_row && container_structs.len() > 1 {
            let mut same_offset_rows = 0;
            let mut previous_row = params.row_idx;
            while previous_row > 0 && params.line_starts.get(previous_row - 1) == Some(offset) {
                same_offset_rows += 1;
                previous_row -= 1;
            }
            if let Some(container) = container_structs.get(same_offset_rows).cloned() {
                container_structs = vec![container];
            }
        }

        if !is_structure_header_row && !is_collapsed_struct_row {
            container_structs.clear();
        }

        let leaf_fields: Vec<IndexedField> = row_leaves.into_owned();

        let is_collapsed = is_collapsed_struct_row
            || container_structs
                .first()
                .map(|c| params.collapsed_structs.contains(&c.collapse_key) || params.collapsed_structs.contains(&c.id))
                .unwrap_or(false);

        let continuation_leaf = if !is_structure_header_row && !is_collapsed_struct_row && container_structs.is_empty() && leaf_fields.is_empty() {
            first_leaf.as_deref().cloned()
        } else {
            None
        };

        let active_range_count = if continuation_leaf.is_none() && container_structs.is_empty() && leaf_fields.is_empty() && !parse_res.is_live() {
            parse_res.count_active_struct_ranges(offset, query_len)
        } else {
            0
        };

        let indent_level = calculate_description_indent_level(continuation_leaf.as_ref(), container_structs.first(), leaf_fields.first(), active_range_count);
        let indent_px = indent_level as f32 * 14.0;
        let has_chevron = !container_structs.is_empty();
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
        if let Some(container) = container_structs.first() {
            let (hdr_text, hdr_runs) = build_structure_header_runs(container, is_collapsed, font.clone(), &params.desc_palette);
            desc_full_text.push_str(&hdr_text);
            desc_runs.extend(hdr_runs);
        }
        if let Some(ref leaf) = continuation_leaf {
            let rel_offset = offset.saturating_sub(leaf.offset);
            let (c_text, c_runs) = build_continuation_field_runs(&leaf.id, rel_offset, font.clone(), &params.desc_palette);
            desc_full_text.push_str(&c_text);
            desc_runs.extend(c_runs);
        } else if !is_collapsed {
            let include_doc = leaf_fields.len() == 1 && container_structs.is_empty();
            for f in leaf_fields.iter() {
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
                let (leaf_text, leaf_runs) = build_leaf_field_runs(f, font.clone(), &params.desc_palette, include_doc, false);
                desc_full_text.push_str(&leaf_text);
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
            let leaf_field_ranges = leaf_fields.iter().map(|f| (f.offset, f.size)).collect();

            Some(CachedRowDesc {
                indent_level,
                indent_px,
                chevron_icon_path,
                text_offset_px,
                desc_text_width,
                shaped_base_desc: Some(shaped_base_desc),
                leaf_field_ranges,
                container_structs,
                continuation_leaf,
                leaf_fields,
            })
        } else {
            None
        }
    } else {
        None
    };

    CachedRow {
        physical_address,
        shaped_addr,
        hex_source,
        shaped_hex,
        ascii_cells,
        shaped_ascii,
        structure_fields,
        desc,
    }
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
    let active_cursor_offset = if params.insert_mode {
        params.insert_cursor_offset
    } else {
        params.cursor_offset
    };
    let is_cursor_row = is_cursor_row(active_cursor_offset, params.row_idx, offset, next_offset, params.line_starts, total_size);
    let is_eof = active_cursor_offset == total_size;
    let last_line_idx = params.line_starts.len().saturating_sub(1);
    let last_line_start = params.line_starts.get(last_line_idx).unwrap_or(0);
    let bytes_per_row = params.line_starts.max_bytes_per_row();
    let is_eof_on_new_line = total_size > last_line_start && (total_size - last_line_start) >= bytes_per_row;
    let is_eof_on_this_row =
        is_eof && ((is_eof_on_new_line && params.row_idx == params.line_starts.len()) || (!is_eof_on_new_line && params.row_idx == last_line_idx));

    let is_struct_mode = params.parse_result.is_some();

    let active_row_highlights = row_highlights(params.highlights, params.max_highlight_len, offset, next_offset);
    let desc_palette = params.desc_palette;
    let selection_bg = params.theme_colors.selection_bg;
    let caret_color = params.theme_colors.caret_color;
    let muted_color = params.theme_colors.muted_color;
    let fg_color = params.theme_colors.fg_color;
    let accent_fg_color = params.theme_colors.accent_fg_color;
    let border_color = params.theme_colors.border_color;
    let border_line_color = params.theme_colors.border_line_color;
    let bg_color_theme = params.theme_colors.bg_color_theme;

    let line_height = px(ROW_HEIGHT);
    let font = font(params.font_family.clone());
    let insert_cursor_active = params.insert_mode && params.is_focused && window.is_window_active();
    let insert_selection_active = params.min_sel <= params.max_sel;

    let active_leaf_field = params.active_leaf_field;

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
            let is_cursor_in_fold = active_cursor_offset >= offset && active_cursor_offset < fold_end;
            let addr_color = if is_cursor_in_fold { fg_color } else { muted_color };
            let addr_str = format_offset_08(fold_start_addr);
            let run = TextRun {
                len: addr_str.len(),
                font: font.clone(),
                color: addr_color,
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
                border_color,
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
            let clean_comment = if comment.contains('\n') || comment.contains('\r') {
                comment.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("")
            } else {
                comment.as_str()
            };
            format!(
                "── {}: 0x{:08X} - 0x{:08X} (0x{:X} / {} bytes) ──",
                clean_comment, fold_start_addr, fold_end_addr, fold_size, fold_size
            )
        } else {
            format!(
                "── 0x{:08X} - 0x{:08X} (0x{:X} / {} bytes) ──",
                fold_start_addr, fold_end_addr, fold_size, fold_size
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
                color: muted_color,
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
                border_color,
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
        window.paint_quad(fill(bar_bounds, theme.secondary));
        let outline = outline(bar_bounds, border_color, BorderStyle::Solid).border_widths(px(1.0));
        window.paint_quad(outline);

        let text_x = bar_start_x + px(12.0);
        let text_pos = point(text_x, params.bounds.top() + px(2.0));
        let _ = shaped_label.paint(text_pos, line_height, TextAlign::Left, None, window, cx);
        return;
    }

    if params.row_cache.get(params.row_idx).is_none() {
        let cached_row = build_cached_row(&params, offset, next_offset, window);
        params.row_cache.insert(params.row_idx, cached_row);
    }
    let cached = params.row_cache.get(params.row_idx).unwrap();

    let physical_address = cached.physical_address;
    let (is_structure_header_row, is_collapsed_struct_row, first_leaf, is_uniform_row_leaf, row_leaf_fields) = if let Some(ref sf) = cached.structure_fields {
        (
            sf.is_structure_header_row,
            sf.is_collapsed_struct_row,
            sf.first_leaf.as_ref(),
            sf.is_uniform_row_leaf,
            &sf.row_leaf_fields[..],
        )
    } else {
        (false, false, None, false, &[][..])
    };

    // 1. Draw Left Columns (Address OR Offset)
    let addr_color = if is_cursor_row { fg_color } else { muted_color };
    let (offset_w, gap) = if is_struct_mode {
        let addr_pos = point(params.bounds.left() + px(8.0), params.bounds.top() + px(2.0));
        if is_cursor_row {
            let addr_str = format_offset_08(physical_address);
            let run = TextRun {
                len: addr_str.len(),
                font: font.clone(),
                color: addr_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window.text_system().shape_line(addr_str, params.font_size, &[run], None);
            let _ = shaped.paint(addr_pos, line_height, TextAlign::Left, None, window, cx);
        } else if let Some(ref shaped) = cached.shaped_addr {
            let _ = shaped.paint(addr_pos, line_height, TextAlign::Left, None, window, cx);
        }
        (params.address_col_width, SECTION_GAP)
    } else {
        if params.show_offset {
            let offset_pos = point(params.bounds.left() + px(8.0), params.bounds.top() + px(2.0));
            if is_cursor_row {
                let offset_str = format_offset_08(physical_address);
                let run = TextRun {
                    len: offset_str.len(),
                    font: font.clone(),
                    color: addr_color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window.text_system().shape_line(offset_str, params.font_size, &[run], None);
                let _ = shaped.paint(offset_pos, line_height, TextAlign::Left, None, window, cx);
            } else if let Some(ref shaped) = cached.shaped_addr {
                let _ = shaped.paint(offset_pos, line_height, TextAlign::Left, None, window, cx);
            }
        }
        (if params.show_offset { OFFSET_WIDTH } else { 0.0 }, SECTION_GAP)
    };

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

    let (hex_source_ref, shaped_hex_ref, pending_edit_colors) = if let Some((pending_pos, pending_digit)) = params.pending_hex_digit
        && params.active_column == EditColumn::Hex
        && ((pending_pos >= offset && pending_pos < next_offset) || (is_eof_on_this_row && pending_pos == total_size))
    {
        let hex_chunk = if is_structure_header_row || is_collapsed_struct_row { &[] } else { chunk };
        let mut dyn_source = build_hex_text_source(hex_chunk, offset, params.radix, params.group_size, params.is_big_endian);
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
                    let digit_char = char::from_digit(pending_digit as u32, 16).unwrap_or('?');
                    let mut bytes = hex_shared_text.to_string().into_bytes();
                    bytes[char_start] = digit_char as u8;
                    bytes[char_start + 1] = b'_';
                    hex_shared_text = SharedString::from(String::from_utf8(bytes).expect("valid utf8"));
                    pending_byte_char_start = Some(char_start);
                }
            }
        } else if is_eof_on_this_row && pending_pos == total_size {
            let digit_char = char::from_digit(pending_digit as u32, 16).unwrap_or('?');
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
        (
            Cow::Owned(dyn_source),
            dyn_shaped,
            pending_byte_char_start.map(|char_start| (char_start, caret_color, muted_color)),
        )
    } else {
        (Cow::Borrowed(&cached.hex_source), cached.shaped_hex.clone(), None)
    };

    let pending_byte_char_start = pending_edit_colors.map(|(p, _, _)| p);

    let mut hex_runs: Vec<TextRun> = Vec::with_capacity(hex_source_ref.groups.len() * 2);
    let mut group_visuals: Vec<(Option<Hsla>, bool, bool)> = Vec::with_capacity(hex_source_ref.groups.len());
    let mut group_text_colors: Vec<Hsla> = Vec::with_capacity(hex_source_ref.groups.len());

    for (group_idx, group) in hex_source_ref.groups.iter().enumerate() {
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
        let has_hl = current_hl_color.is_some();
        let text_color = hex_item_text_color(
            is_cursor && params.is_focused,
            is_selected,
            has_hl,
            item_slice,
            &params.category_palette,
            fg_color,
        );

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

    let text_origin_x = hex_start_x - px(params.hex_scroll_x);
    let total_data_width = f32::from(hex_grid_width(hex_source_ref.text.len(), params.hex_cell_width));

    // 2. Background Quads Pass for Data Items (with clipping mask)
    let hex_mask_bounds = Bounds::new(point(hex_start_x, params.bounds.top()), size(px(params.hex_col_width), px(ROW_HEIGHT)));

    window.with_content_mask(
        Some(ContentMask {
            bounds: scrollable_mask_bounds,
        }),
        |window| {
            window.with_content_mask(Some(ContentMask { bounds: hex_mask_bounds }), |window| {
                for (item_idx, group) in hex_source_ref.groups.iter().enumerate() {
                    let item_start_offset = offset + group.chunk_start;
                    let (hl_color, is_selected, is_cursor) = group_visuals[item_idx];
                    let (group_start_x, group_end_x) = hex_group_x(*group, text_origin_x, params.hex_cell_width);
                    let previous_group_end_x = if item_idx > 0 {
                        hex_group_x(hex_source_ref.groups[item_idx - 1], text_origin_x, params.hex_cell_width).1
                    } else {
                        group_start_x
                    };
                    let has_next = item_idx + 1 < hex_source_ref.groups.len();
                    let next_group_start_x = if has_next {
                        hex_group_x(hex_source_ref.groups[item_idx + 1], text_origin_x, params.hex_cell_width).0
                    } else {
                        group_end_x
                    };

                    // 0. Structure field active underline
                    if is_struct_mode && params.parse_result.is_some() {
                        let is_in_active_field = active_leaf_field.is_some_and(|af| item_start_offset >= af.offset && item_start_offset < af.offset + af.size);

                        if is_in_active_field {
                            let field_at_item = if is_uniform_row_leaf {
                                first_leaf
                            } else {
                                row_leaf_fields.get(group.chunk_start).and_then(|f| f.as_ref())
                            };
                            if let Some(field) = field_at_item {
                                let field_hsla = field.color.map(Hsla::from).unwrap_or(desc_palette.type_name);
                                let underline_bounds = Bounds::new(
                                    point(group_start_x, params.bounds.top() + px(ROW_HEIGHT - 2.0)),
                                    size(group_end_x - group_start_x, px(2.0)),
                                );
                                window.paint_quad(fill(underline_bounds, field_hsla));
                            }
                        }
                    }

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

                    // 3. Cursor Pass
                    if is_cursor && !params.insert_mode && pending_byte_char_start.is_none() {
                        let cursor_border_color = if params.is_focused { caret_color } else { darken_cursor_color(muted_color) };

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
                    let cursor_border_color = if params.is_focused { caret_color } else { darken_cursor_color(muted_color) };
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

                // 3. Text Pass for Data Items
                if let Some(ref shaped_hex) = shaped_hex_ref {
                    paint_centered_hex_glyphs(
                        shaped_hex,
                        &hex_source_ref.groups,
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

    // 3. ASCII Column (when ASCII view is enabled and row has bytes)
    if params.show_ascii && chunk_len > 0 && !is_collapsed_struct_row {
        window.with_content_mask(
            Some(ContentMask {
                bounds: scrollable_mask_bounds,
            }),
            |window| {
                let ascii_start_x = hex_end_x + px(gap);
                let ascii_content_start_x = ascii_start_x - px(params.ascii_scroll_x);
                let ascii_mask_bounds = Bounds::new(point(ascii_start_x, params.bounds.top()), size(px(ascii_width), px(ROW_HEIGHT)));
                window.with_content_mask(Some(ContentMask { bounds: ascii_mask_bounds }), |window| {
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

                        // Structure field active underline in ASCII
                        if is_struct_mode && params.parse_result.is_some() {
                            let is_ascii_in_active = active_leaf_field.is_some_and(|af| byte_pos >= af.offset && byte_pos < af.offset + af.size);
                            if is_ascii_in_active {
                                let ascii_field = if is_uniform_row_leaf {
                                    first_leaf
                                } else {
                                    row_leaf_fields.get(j).and_then(|f| f.as_ref())
                                };
                                if let Some(field) = ascii_field {
                                    let field_hsla = field.color.map(Hsla::from).unwrap_or(desc_palette.type_name);
                                    let underline_bounds = Bounds::new(
                                        point(ascii_item_bounds.left(), params.bounds.top() + px(ROW_HEIGHT - 2.0)),
                                        size(ascii_item_bounds.size.width, px(2.0)),
                                    );
                                    window.paint_quad(fill(underline_bounds, field_hsla));
                                }
                            }
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
                                let cursor_border_color = if params.is_focused { caret_color } else { darken_cursor_color(muted_color) };
                                let char_start_x = ascii_content_start_x + px(cell_start as f32 * ASCII_CELL_WIDTH);
                                let char_width = px((cell_end - cell_start) as f32 * ASCII_CELL_WIDTH);
                                let ascii_char_bounds = Bounds::new(point(char_start_x, params.bounds.top() + px(1.0)), size(char_width, px(ROW_HEIGHT - 2.0)));
                                paint_cursor_border(window, ascii_char_bounds, cursor_border_color);
                            }
                        } else if is_eof_on_this_row && !params.insert_mode {
                            let cursor_border_color = if params.is_focused { caret_color } else { darken_cursor_color(muted_color) };
                            let ascii_x = ascii_content_start_x + px(chunk_len as f32 * ASCII_CELL_WIDTH);
                            let ascii_box_bounds = Bounds::new(point(ascii_x, params.bounds.top() + px(1.0)), size(px(ASCII_CELL_WIDTH), px(ROW_HEIGHT - 2.0)));
                            paint_cursor_border(window, ascii_box_bounds, cursor_border_color);
                        }
                    } else {
                        for group in hex_source_ref.groups.iter() {
                            let item_start_offset = offset + group.chunk_start;
                            let item_end_offset = offset + group.chunk_end;
                            let is_cursor = params.cursor_offset >= item_start_offset && params.cursor_offset < item_end_offset;

                            if is_cursor && !params.insert_mode {
                                let cursor_border_color = if params.is_focused { caret_color } else { darken_cursor_color(muted_color) };
                                let group_start_x = ascii_content_start_x + px(group.chunk_start as f32 * ASCII_CELL_WIDTH);
                                let group_width = px((group.chunk_end - group.chunk_start) as f32 * ASCII_CELL_WIDTH);
                                let ascii_group_bounds =
                                    Bounds::new(point(group_start_x, params.bounds.top() + px(1.0)), size(group_width, px(ROW_HEIGHT - 2.0)));
                                paint_underscore_cursor_at(window, ascii_group_bounds, group_start_x, group_width, cursor_border_color);
                            }
                        }
                        if is_eof_on_this_row && !params.insert_mode {
                            let cursor_border_color = if params.is_focused { caret_color } else { darken_cursor_color(muted_color) };
                            let ascii_x = ascii_content_start_x + px(chunk_len as f32 * ASCII_CELL_WIDTH);
                            let ascii_box_bounds = Bounds::new(point(ascii_x, params.bounds.top() + px(1.0)), size(px(ASCII_CELL_WIDTH), px(ROW_HEIGHT - 2.0)));
                            paint_underscore_cursor_at(window, ascii_box_bounds, ascii_x, px(ASCII_CELL_WIDTH), cursor_border_color);
                        }
                    }

                    // Paint each glyph centered in its fixed byte cell using cached shaped line
                    let mut ascii_entries: Vec<AsciiCellEntry> = Vec::with_capacity(cached.ascii_cells.len());
                    for cell in &cached.ascii_cells {
                        let byte_pos = offset + cell.cell_idx;
                        let current_hl_color = highlight_color_for_range(byte_pos, byte_pos.saturating_add(1), active_row_highlights);
                        let has_hl = current_hl_color.is_some();
                        let in_selected_group = if params.min_sel <= params.max_sel {
                            let group_start = (byte_pos / group_bytes) * group_bytes;
                            let group_end = group_start + group_bytes;
                            group_start <= params.max_sel && group_end > params.min_sel
                        } else {
                            false
                        };
                        let text_color = ascii_item_text_color(
                            in_selected_group,
                            has_hl,
                            cell.byte,
                            cell.is_printable,
                            &params.category_palette,
                            fg_color,
                            muted_color,
                        );

                        ascii_entries.push(AsciiCellEntry {
                            cell_idx: cell.cell_idx,
                            text_byte_start: cell.text_byte_start,
                            text_byte_end: cell.text_byte_end,
                            color: text_color,
                        });
                    }

                    if let Some(ref shaped) = cached.shaped_ascii {
                        paint_centered_ascii_glyphs(
                            shaped,
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
    if let Some(ref desc) = cached.desc {
        let is_active_field = active_leaf_field.is_some_and(|af| desc.leaf_field_ranges.iter().any(|&(o, s)| o == af.offset && s == af.size));
        let desc_mask_bounds = Bounds::new(point(desc_start_x, params.bounds.top()), size(px(params.desc_col_width), px(ROW_HEIGHT)));
        window.with_content_mask(
            Some(ContentMask {
                bounds: scrollable_mask_bounds,
            }),
            |window| {
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
                    if is_active_field {
                        let mut active_runs = Vec::new();
                        let mut active_full_text = String::new();
                        if let Some(container) = desc.container_structs.first() {
                            let is_collapsed = desc.chevron_icon_path.as_ref() == Some(&IconName::ChevronRight.path());
                            let (hdr_text, hdr_runs) = build_structure_header_runs(container, is_collapsed, font.clone(), &desc_palette);
                            active_full_text.push_str(&hdr_text);
                            active_runs.extend(hdr_runs);
                        }
                        if let Some(ref leaf) = desc.continuation_leaf {
                            let rel_offset = offset.saturating_sub(leaf.offset);
                            let (c_text, c_runs) = build_continuation_field_runs(&leaf.id, rel_offset, font.clone(), &desc_palette);
                            active_full_text.push_str(&c_text);
                            active_runs.extend(c_runs);
                        } else {
                            let include_doc = desc.leaf_fields.len() == 1 && desc.container_structs.is_empty();
                            for f in desc.leaf_fields.iter() {
                                if !active_full_text.is_empty() {
                                    active_full_text.push_str("  ");
                                    active_runs.push(TextRun {
                                        len: 2,
                                        font: font.clone(),
                                        color: desc_palette.punctuation,
                                        background_color: None,
                                        underline: None,
                                        strikethrough: None,
                                    });
                                }
                                let is_this_active = active_leaf_field.is_some_and(|af| af.offset == f.offset && af.size == f.size);
                                let (leaf_text, leaf_runs) = build_leaf_field_runs(f, font.clone(), &desc_palette, include_doc, is_this_active);
                                active_full_text.push_str(&leaf_text);
                                active_runs.extend(leaf_runs);
                            }
                        }

                        if !active_full_text.is_empty() {
                            if active_full_text.contains('\n') || active_full_text.contains('\r') {
                                let mut sanitized = String::with_capacity(active_full_text.len());
                                for ch in active_full_text.chars() {
                                    if ch == '\n' || ch == '\r' {
                                        sanitized.push(' ');
                                    } else {
                                        sanitized.push(ch);
                                    }
                                }
                                active_full_text = sanitized;
                            }
                            let shaped = window
                                .text_system()
                                .shape_line(SharedString::from(active_full_text), params.font_size, &active_runs, None);
                            let _ = shaped.paint(text_pos, line_height, TextAlign::Left, None, window, cx);
                        }
                    } else if let Some(ref shaped) = desc.shaped_base_desc {
                        let _ = shaped.paint(text_pos, line_height, TextAlign::Left, None, window, cx);
                    }

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
                    if params.desc_scroll_x + params.desc_col_width < desc.desc_text_width - 1.0 {
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

    // 5. Bookmark Comments Column
    let row_bookmark_comments: Vec<(Hsla, SharedString)> = params
        .bookmark_items
        .iter()
        .filter(|h| {
            let h_end_offset = h.offset.saturating_add(h.size);
            let h_last_byte = h_end_offset.saturating_sub(1).max(h.offset);
            if h.offset > next_offset || h_last_byte < offset || h.comment.trim().is_empty() {
                return false;
            }
            let h_start_row = crate::core::editor::Editor::find_line_index(h.offset, params.line_starts);
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
            let clean_comment = if comment_shared.contains('\n') || comment_shared.contains('\r') {
                let first_line = comment_shared.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
                SharedString::from(first_line.to_string())
            } else {
                comment_shared.clone()
            };
            if clean_comment.is_empty() {
                continue;
            }
            let run = TextRun {
                len: clean_comment.len(),
                font: font.clone(),
                color: muted_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped_comment = window.text_system().shape_line(clean_comment, params.font_size, &[run], None);
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
                        let dot_bounds = Bounds::new(point(cur_x, dot_y), size(px(dot_size), px(dot_size)));
                        let mut dot_quad = fill(dot_bounds, badge_color);
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

/// Paints a vertical scrollbar using HexView's standard row height on a GPUI canvas.
pub fn paint_scrollbar(
    list_bounds: gpui_kit::Bounds<gpui_kit::Pixels>,
    scroll_offset: usize,
    total_rows: usize,
    is_dragging: bool,
    is_hovered: bool,
    theme: &gpui_kit::component::theme::Theme,
    window: &mut gpui_kit::Window,
) {
    crate::ui::components::scrollbar::CanvasScrollbar::new(list_bounds, scroll_offset, total_rows, super::types::ROW_HEIGHT)
        .dragging(is_dragging)
        .hovered(is_hovered)
        .paint(theme, window);
}

#[cfg(test)]
mod tests {
    use super::{
        ByteCategoryPalette, ByteGroupSize, DisplayRadix, HexGroupInfo, HexInsertCursorParams, ascii_item_text_color, hex_insert_cursor_geometry,
        hex_item_text_color, selection_background_color,
    };
    use crate::core::radix::{ByteCategoryColors, ByteColoring, CategoryColor};
    use gpui_kit::component::theme::Theme;
    use gpui_kit::{hsla, px};

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

    #[test]
    fn test_selection_background_color() {
        let base = hsla(0.6, 0.8, 0.5, 0.3);

        // Dark mode focused
        let dark_focused = selection_background_color(base, true, true);
        assert_eq!(dark_focused.h, base.h);
        assert_eq!(dark_focused.s, base.s);
        assert_eq!(dark_focused.l, base.l);
        assert_eq!(dark_focused.a, 0.55);

        // Dark mode unfocused
        let dark_unfocused = selection_background_color(base, false, true);
        assert_eq!(dark_unfocused.a, 0.28);

        // Light mode focused
        let light_focused = selection_background_color(base, true, false);
        assert_eq!(light_focused.a, 0.40);

        // Light mode unfocused
        let light_unfocused = selection_background_color(base, false, false);
        assert_eq!(light_unfocused.a, 0.20);
    }

    #[allow(clippy::field_reassign_with_default)]
    fn test_theme() -> Theme {
        let mut theme = Theme::default();
        theme.background = hsla(0.0, 0.0, 0.0, 1.0);
        theme.foreground = hsla(0.0, 0.0, 1.0, 1.0);
        theme.muted_foreground = hsla(0.0, 0.0, 0.5, 1.0);
        theme.accent_foreground = hsla(0.0, 0.0, 0.9, 1.0);
        theme.red = hsla(0.0, 0.8, 0.5, 1.0);
        theme.green = hsla(120.0 / 360.0, 0.8, 0.5, 1.0);
        theme.blue = hsla(210.0 / 360.0, 0.8, 0.5, 1.0);
        theme.yellow = hsla(45.0 / 360.0, 0.8, 0.5, 1.0);
        theme.cyan = hsla(190.0 / 360.0, 0.8, 0.5, 1.0);
        theme.magenta = hsla(280.0 / 360.0, 0.8, 0.5, 1.0);
        theme.border = hsla(0.0, 0.0, 0.3, 1.0);
        theme
    }

    #[test]
    fn test_hex_item_text_color() {
        let theme = test_theme();
        let fg = theme.foreground;
        let muted = theme.muted_foreground;

        let dim_zero_palette = ByteCategoryPalette::from_colors(ByteColoring::DimZero.colors().unwrap(), &theme);
        let uniform_palette = ByteCategoryPalette::from_colors(ByteColoring::Uniform.colors().unwrap(), &theme);
        let dim_padding_palette = ByteCategoryPalette::from_colors(ByteColoring::DimPadding.colors().unwrap(), &theme);
        let category_palette = ByteCategoryPalette::from_colors(ByteColoring::Category.colors().unwrap(), &theme);

        // When focused cursor is on item, use fg
        assert_eq!(hex_item_text_color(true, false, false, &[0x00], &dim_zero_palette, fg), fg);

        // When selected, use fg even if dimmed
        assert_eq!(hex_item_text_color(false, true, false, &[0x00], &dim_zero_palette, fg), fg);

        // When highlighted, use fg even if dimmed
        assert_eq!(hex_item_text_color(false, false, true, &[0x00], &dim_zero_palette, fg), fg);

        // Uniform mode: always fg
        assert_eq!(hex_item_text_color(false, false, false, &[0x00], &uniform_palette, fg), fg);

        // DimZero mode: 0x00 is dimmed
        assert_eq!(hex_item_text_color(false, false, false, &[0x00], &dim_zero_palette, fg), dim_zero_palette.null);
        assert_eq!(hex_item_text_color(false, false, false, &[0x42], &dim_zero_palette, fg), fg);

        // DimPadding mode: 0x00 and 0xFF are dimmed
        assert_eq!(hex_item_text_color(false, false, false, &[0xFF], &dim_padding_palette, fg), muted);

        // Category mode: distinct colors for different categories
        let null_col = hex_item_text_color(false, false, false, &[0x00], &category_palette, fg);
        let ascii_col = hex_item_text_color(false, false, false, b"A", &category_palette, fg);
        let ws_col = hex_item_text_color(false, false, false, b" ", &category_palette, fg);
        let ctrl_col = hex_item_text_color(false, false, false, &[0x01], &category_palette, fg);
        let non_ascii_col = hex_item_text_color(false, false, false, &[0x80], &category_palette, fg);
        let max_col = hex_item_text_color(false, false, false, &[0xFF], &category_palette, fg);

        assert_eq!(null_col, category_palette.null);
        assert_eq!(ascii_col, category_palette.ascii_printable);
        assert_eq!(ws_col, category_palette.ascii_whitespace);
        assert_eq!(ctrl_col, category_palette.ascii_control);
        assert_eq!(non_ascii_col, category_palette.non_ascii);
        assert_eq!(max_col, category_palette.max);

        assert_ne!(ascii_col, fg);
        assert_ne!(ascii_col, ws_col);
        assert_ne!(ws_col, ctrl_col);
        assert_ne!(ctrl_col, non_ascii_col);
        assert_ne!(non_ascii_col, max_col);

        // Mixed group defaults to fg
        let mixed_col = hex_item_text_color(false, false, false, &[b'A', 0x00], &category_palette, fg);
        assert_eq!(mixed_col, fg);
    }

    #[test]
    fn test_ascii_item_text_color() {
        let theme = test_theme();
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let dim_zero_palette = ByteCategoryPalette::from_colors(ByteColoring::DimZero.colors().unwrap(), &theme);
        let category_palette = ByteCategoryPalette::from_colors(ByteColoring::Category.colors().unwrap(), &theme);

        // Selection / highlight takes priority
        assert_eq!(ascii_item_text_color(true, false, b'A', true, &category_palette, fg, muted), fg);
        assert_eq!(ascii_item_text_color(false, true, b'A', true, &category_palette, fg, muted), fg);

        // Normal mode (DimZero)
        assert_eq!(ascii_item_text_color(false, false, b'A', true, &dim_zero_palette, fg, muted), fg);
        assert_eq!(ascii_item_text_color(false, false, 0x00, false, &dim_zero_palette, fg, muted), muted);
        assert_eq!(ascii_item_text_color(false, false, 0x01, false, &dim_zero_palette, fg, muted), muted);

        // Category mode
        let printable_col = ascii_item_text_color(false, false, b'A', true, &category_palette, fg, muted);
        let ws_col = ascii_item_text_color(false, false, b' ', true, &category_palette, fg, muted);
        assert_eq!(printable_col, category_palette.ascii_printable);
        assert_eq!(ws_col, category_palette.ascii_whitespace);

        let unprintable_col = ascii_item_text_color(false, false, 0x01, false, &category_palette, fg, muted);
        assert_eq!(unprintable_col, category_palette.ascii_control);
    }

    #[test]
    fn test_palette_from_colors() {
        let theme = test_theme();

        let custom_colors = ByteCategoryColors {
            null: CategoryColor::Muted,
            ascii_printable: CategoryColor::Default,
            ascii_whitespace: CategoryColor::Green,
            ascii_control: CategoryColor::Red,
            non_ascii: CategoryColor::Yellow,
            max: CategoryColor::Magenta,
        };

        let custom_palette = ByteCategoryPalette::from_colors(custom_colors, &theme);
        assert_eq!(custom_palette.null, theme.muted_foreground);
        assert_eq!(custom_palette.ascii_printable, theme.foreground);
        assert_eq!(custom_palette.ascii_whitespace, theme.green);
        assert_eq!(custom_palette.ascii_control, theme.red);
        assert_eq!(custom_palette.non_ascii, theme.yellow);
        assert_eq!(custom_palette.max, theme.magenta);
        assert_eq!(custom_palette.default, theme.foreground);
    }

    #[test]
    fn test_build_structure_header_runs() {
        use super::*;
        use gpui::Font;

        let theme = test_theme();
        let font = Font::default();
        let container = IndexedField {
            id: "sections[0]".into(),
            offset: 0x1E,
            size: 30,
            depth: 1,
            is_instance: false,
            type_name: "section".into(),
            collapse_key: "0x1E:1:sections[0]".into(),
            formatted_value: String::new(),
            enum_label: None,
            description: None,
            color: None,
            validation_failed: false,
            expression: String::new(),
        };

        let palette = DescriptionPalette::from_theme(&theme);

        // Expanded
        let (text_exp, runs_exp) = build_structure_header_runs(&container, false, font.clone(), &palette);
        assert_eq!(text_exp, "sections[0]: section (30B)");
        assert_eq!(runs_exp.iter().map(|r| r.len).sum::<usize>(), text_exp.len());

        // Collapsed
        let (text_col, runs_col) = build_structure_header_runs(&container, true, font, &palette);
        assert_eq!(text_col, "sections[0]: section (30B, collapsed)");
        assert_eq!(runs_col.iter().map(|r| r.len).sum::<usize>(), text_col.len());
    }

    #[test]
    fn test_build_leaf_field_runs() {
        use super::*;
        use gpui::Font;

        let theme = test_theme();
        let font = Font::default();
        let palette = DescriptionPalette::from_theme(&theme);
        let field = IndexedField {
            id: "magic".into(),
            offset: 0,
            size: 2,
            depth: 1,
            is_instance: false,
            type_name: "u2le".into(),
            collapse_key: "0:1:magic".into(),
            formatted_value: "0x3412 (13330)".into(),
            enum_label: Some("VALID".into()),
            description: Some("magic header".into()),
            color: None,
            validation_failed: false,
            expression: "magic = 3412h (13330)".into(),
        };

        let (text, runs) = build_leaf_field_runs(&field, font.clone(), &palette, true, false);

        assert_eq!(text, "▪ magic: u2le = 0x3412 (13330) (VALID) // magic header");
        assert_eq!(runs.iter().map(|r| r.len).sum::<usize>(), text.len());
        assert!(runs[0].background_color.is_none());

        // Test active highlight
        let (_act_text, act_runs) = build_leaf_field_runs(&field, font.clone(), &palette, true, true);
        assert!(act_runs[0].background_color.is_some());

        let (cont_text, cont_runs) = build_continuation_field_runs("body", 0x10, font.clone(), &palette);
        assert_eq!(cont_text, "↳ body (+0x10)");
        assert_eq!(cont_runs.iter().map(|r| r.len).sum::<usize>(), cont_text.len());

        // Test multi-line doc comment and values with newlines are sanitized and single-line
        let multiline_field = IndexedField {
            id: "data".into(),
            offset: 0,
            size: 4,
            depth: 0,
            is_instance: false,
            type_name: "str".into(),
            collapse_key: "0:0:data".into(),
            formatted_value: "\"line1\r\nline2\"".into(),
            enum_label: Some("LABEL\r\nCONTINUED".into()),
            description: Some("\n  Doc summary line\n  Second explanation line\n".into()),
            color: None,
            validation_failed: false,
            expression: "data = \"line1\r\nline2\"".into(),
        };

        let (ml_text, ml_runs) = build_leaf_field_runs(&multiline_field, font, &palette, true, false);

        assert!(!ml_text.contains('\n'));
        assert!(!ml_text.contains('\r'));
        assert_eq!(ml_runs.iter().map(|r| r.len).sum::<usize>(), ml_text.len());
        assert!(ml_text.contains("Doc summary line"));
        assert!(!ml_text.contains("Second explanation line"));
        assert_eq!(clean_single_line("hello\r\nworld\n"), "hello world ");
    }

    #[test]
    fn test_description_palette_contrast_across_all_embedded_themes() {
        use super::*;
        use crate::assets::Assets;
        use crate::theme::EmbeddedThemes;

        let embedded = EmbeddedThemes::load_from_assets(&Assets);
        assert!(!embedded.theme_names().is_empty());

        for name in embedded.theme_names() {
            let config = embedded.get(&name).unwrap();
            let mut theme = Theme::default();
            theme.apply_config(&config);

            let palette = DescriptionPalette::from_theme(&theme);
            let bg = theme.background;

            // Verify high contrast for all textual elements across every embedded theme
            assert!(
                contrast_ratio(palette.field_id, bg) >= 4.5,
                "Theme '{}' failed contrast for field_id: ratio was {:.2}:1",
                name,
                contrast_ratio(palette.field_id, bg)
            );
            assert!(
                contrast_ratio(palette.type_name, bg) >= 4.0,
                "Theme '{}' failed contrast for type_name: ratio was {:.2}:1",
                name,
                contrast_ratio(palette.type_name, bg)
            );
            assert!(
                contrast_ratio(palette.number_value, bg) >= 4.0,
                "Theme '{}' failed contrast for number_value: ratio was {:.2}:1",
                name,
                contrast_ratio(palette.number_value, bg)
            );
            assert!(
                contrast_ratio(palette.string_value, bg) >= 4.0,
                "Theme '{}' failed contrast for string_value: ratio was {:.2}:1",
                name,
                contrast_ratio(palette.string_value, bg)
            );
            assert!(
                contrast_ratio(palette.comment, bg) >= 3.5,
                "Theme '{}' failed contrast for comment: ratio was {:.2}:1",
                name,
                contrast_ratio(palette.comment, bg)
            );
        }
    }

    #[test]
    fn test_calculate_description_indent_level_preserves_depth_for_duplicate_element_names() {
        use super::*;

        // Scenario like zip.ksy:
        // pk_section.body is at depth 1 (container)
        // extra_field.body is at depth 5 (container)
        // local_file.body is at depth 2 (leaf)
        let outer_body_container = IndexedField {
            id: "body".into(),
            offset: 4,
            size: 100,
            depth: 1,
            is_instance: false,
            type_name: "local_file".into(),
            collapse_key: "4:1:body".into(),
            formatted_value: String::new(),
            enum_label: None,
            description: None,
            color: None,
            validation_failed: false,
            expression: String::new(),
        };

        let inner_body_container = IndexedField {
            id: "body".into(),
            offset: 40,
            size: 20,
            depth: 5,
            is_instance: false,
            type_name: "extended_timestamp".into(),
            collapse_key: "40:5:body".into(),
            formatted_value: String::new(),
            enum_label: None,
            description: None,
            color: None,
            validation_failed: false,
            expression: String::new(),
        };

        let leaf_body = IndexedField {
            id: "body".into(),
            offset: 60,
            size: 40,
            depth: 2,
            is_instance: false,
            type_name: "bytes".into(),
            collapse_key: "60:2:body".into(),
            formatted_value: "[40 bytes]".into(),
            enum_label: None,
            description: None,
            color: None,
            validation_failed: false,
            expression: "body = [40 bytes]".into(),
        };

        // 1. Header row for outer body: container.depth = 1
        let outer_indent = calculate_description_indent_level(None, Some(&outer_body_container), None, 1);
        assert_eq!(outer_indent, 1, "outer body must have indent level 1");

        // 2. Header row for inner body: container.depth = 5 (even when active_ranges has 6 items or ancestor has depth 1)
        let inner_indent = calculate_description_indent_level(None, Some(&inner_body_container), None, 6);
        assert_eq!(inner_indent, 5, "inner body must have indent level 5, not be overridden to ancestor depth");

        // 3. Leaf row for payload body: leaf.depth = 2 (even when active_ranges has 3 items)
        let leaf_indent = calculate_description_indent_level(None, None, Some(&leaf_body), 3);
        assert_eq!(leaf_indent, 2, "leaf body must have indent level 2");

        // 4. Continuation row for payload body: continuation_leaf.depth = 2
        let cont_indent = calculate_description_indent_level(Some(&leaf_body), None, None, 3);
        assert_eq!(cont_indent, 2, "continuation body must preserve indent level 2");
    }
}
