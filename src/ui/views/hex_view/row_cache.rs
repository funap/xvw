use crate::core::encoding::Encoding;
use crate::core::radix::{ByteGroupSize, DisplayRadix};
use crate::core::structure::IndexedField;
use crate::ui::views::hex_view::types::HexTextSource;
use gpui_kit::*;
use std::collections::HashMap;

/// Cached ASCII cell layout data for one character slot in a row.
#[derive(Clone, Copy, Debug)]
pub struct CachedAsciiCell {
    pub cell_idx: usize,
    pub text_byte_start: usize,
    pub text_byte_end: usize,
    pub byte: u8,
    pub is_printable: bool,
}

/// Cached description column layout and shaped text for one row.
#[derive(Clone, Debug)]
pub struct CachedRowDesc {
    pub indent_level: usize,
    pub indent_px: f32,
    pub chevron_icon_path: Option<SharedString>,
    pub text_offset_px: f32,
    pub desc_text_width: f32,
    pub shaped_base_desc: Option<ShapedLine>,
    pub leaf_field_ranges: Vec<(usize, usize)>,
    pub container_structs: Vec<IndexedField>,
    pub continuation_leaf: Option<IndexedField>,
    pub leaf_fields: Vec<IndexedField>,
}

/// Cached structure indexing results for one row (used for byte tints and active underlines).
#[derive(Clone, Debug)]
pub struct CachedRowStructureFields {
    pub is_structure_header_row: bool,
    pub is_collapsed_struct_row: bool,
    pub first_leaf: Option<IndexedField>,
    pub is_uniform_row_leaf: bool,
    pub row_leaf_fields: Vec<Option<IndexedField>>,
}

/// Complete cached layout and shaped text for a single painted row.
#[derive(Clone, Debug)]
pub struct CachedRow {
    pub physical_address: usize,
    pub shaped_addr: Option<ShapedLine>,
    pub hex_source: HexTextSource,
    pub shaped_hex: Option<ShapedLine>,
    pub ascii_cells: Vec<CachedAsciiCell>,
    pub shaped_ascii: Option<ShapedLine>,
    pub structure_fields: Option<CachedRowStructureFields>,
    pub desc: Option<CachedRowDesc>,
}

/// Frame-to-frame row render cache for HexView to minimize text shaping and layout allocations.
#[derive(Default)]
pub struct RowRenderCache {
    generation: usize,
    layout_version: usize,
    collapsed_ptr: usize,
    parse_result_ptr: usize,
    font_size: Pixels,
    font_family: SharedString,
    radix: DisplayRadix,
    group_size: ByteGroupSize,
    is_big_endian: bool,
    encoding: Encoding,
    theme_bg: Hsla,
    theme_fg: Hsla,
    rows: HashMap<usize, CachedRow>,
}

impl RowRenderCache {
    /// Validates the cache against current document, structure, font, and theme properties.
    /// Clears cached rows if any configuration has changed.
    #[allow(clippy::too_many_arguments)]
    pub fn validate(
        &mut self,
        generation: usize,
        layout_version: usize,
        collapsed_ptr: usize,
        parse_result_ptr: usize,
        font_size: Pixels,
        font_family: &SharedString,
        radix: DisplayRadix,
        group_size: ByteGroupSize,
        is_big_endian: bool,
        encoding: Encoding,
        theme_bg: Hsla,
        theme_fg: Hsla,
    ) {
        if self.generation != generation
            || self.layout_version != layout_version
            || self.collapsed_ptr != collapsed_ptr
            || self.parse_result_ptr != parse_result_ptr
            || self.font_size != font_size
            || &self.font_family != font_family
            || self.radix != radix
            || self.group_size != group_size
            || self.is_big_endian != is_big_endian
            || self.encoding != encoding
            || self.theme_bg != theme_bg
            || self.theme_fg != theme_fg
        {
            self.generation = generation;
            self.layout_version = layout_version;
            self.collapsed_ptr = collapsed_ptr;
            self.parse_result_ptr = parse_result_ptr;
            self.font_size = font_size;
            self.font_family = font_family.clone();
            self.radix = radix;
            self.group_size = group_size;
            self.is_big_endian = is_big_endian;
            self.encoding = encoding;
            self.theme_bg = theme_bg;
            self.theme_fg = theme_fg;
            self.rows.clear();
        }
    }

    #[inline]
    pub fn get(&self, row_idx: usize) -> Option<&CachedRow> {
        self.rows.get(&row_idx)
    }

    #[inline]
    pub fn insert(&mut self, row_idx: usize, row: CachedRow) {
        if self.rows.len() > 512 {
            self.rows.clear();
        }
        self.rows.insert(row_idx, row);
    }

    #[allow(dead_code)]
    #[inline]
    pub fn clear(&mut self) {
        self.rows.clear();
    }
}
