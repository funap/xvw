use super::color::ensure_contrast;
use crate::core::radix::HexCase;
use crate::core::structure::{IndexedField, ParseResult};
use gpui::Font;
use gpui_kit::component::theme::Theme;
use gpui_kit::*;
use std::borrow::Cow;
use std::collections::HashSet;

pub const CHEVRON_ICON_SIZE: f32 = 14.0;
pub const CHEVRON_ICON_GAP: f32 = 4.0;

/// Sanitizes a string to guarantee single-line presentation without newlines or carriage returns.
#[inline]
pub fn clean_single_line(s: &str) -> Cow<'_, str> {
    if s.contains('\n') || s.contains('\r') {
        Cow::Owned(s.replace("\r\n", " ").replace(['\n', '\r'], " "))
    } else {
        Cow::Borrowed(s)
    }
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

pub(super) fn span_x_from_glyph_positions(glyphs: impl IntoIterator<Item = (usize, f32)>, line_width: f32, byte_start: usize, byte_end: usize) -> (f32, f32) {
    if byte_start >= byte_end {
        return (0.0, 0.0);
    }
    let mut span_start: Option<f32> = None;
    let mut span_end: Option<f32> = None;
    for (index, x) in glyphs {
        if span_start.is_none() {
            if index >= byte_start && index < byte_end {
                span_start = Some(x);
            }
        } else if index >= byte_end {
            span_end = Some(x);
            break;
        }
    }
    let Some(start_x) = span_start else {
        return (0.0, 0.0);
    };
    let end_x = span_end.unwrap_or(line_width);
    (start_x, (end_x - start_x).max(0.0))
}

pub(super) fn shaped_byte_span_x(shaped: &ShapedLine, byte_start: usize, byte_end: usize) -> (f32, f32) {
    let glyphs = shaped
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter().map(|glyph| (glyph.index, f32::from(glyph.position.x))));
    span_x_from_glyph_positions(glyphs, f32::from(shaped.width), byte_start, byte_end)
}

/// Builds continuation runs for fields spanning across multiple 16-byte rows.
pub fn build_continuation_field_runs(
    field_id: &str,
    relative_offset: usize,
    font: Font,
    palette: &DescriptionPalette,
    hex_case: HexCase,
) -> (String, Vec<TextRun>) {
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
    text.push_str(&format!(" (+{})", hex_case.format_prefixed(relative_offset)));
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

/// Resolves whether a row should be rendered as a structure header row or a collapsed container row.
///
/// While asynchronous structure parsing or index finalization is in progress (or the snapshot is live),
/// structure header rows and collapsed container banners are suppressed to match `LayoutEngine`'s
/// fixed-row layout.
pub fn resolve_structure_row_flags(
    parse_result: Option<&ParseResult>,
    is_parsing_structure: bool,
    raw_containers: &[IndexedField],
    collapsed_structs: &HashSet<String>,
    offset: usize,
    chunk_len: usize,
    total_size: usize,
) -> (bool, bool) {
    let Some(parse_res) = parse_result else {
        return (false, false);
    };
    if is_parsing_structure || parse_res.is_live() {
        return (false, false);
    }

    let is_structure_header_row = chunk_len == 0;
    let is_collapsed_struct_row = chunk_len > 0
        && raw_containers.iter().any(|c| {
            !c.is_instance
                && c.offset == offset
                && (c.size == chunk_len || chunk_len == c.size.min(total_size.saturating_sub(offset)))
                && (collapsed_structs.contains(&c.collapse_key) || collapsed_structs.contains(&c.id))
        });
    (is_structure_header_row, is_collapsed_struct_row)
}
