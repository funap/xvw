use crate::core::encoding::Encoding;
use crate::core::radix::{ByteGroupSize, DisplayRadix, HexCase};
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

/// Small lookup cache for WCAG contrast adjustments to avoid repeating `powf` luminance loops across cells.
#[derive(Default)]
pub struct HighlightContrastCache {
    last: Option<([u32; 4], [u32; 4], Hsla)>,
    entries: Vec<([u32; 4], [u32; 4], Hsla)>,
}

impl HighlightContrastCache {
    const MAX_ENTRIES: usize = 64;

    #[inline]
    fn hsla_key(c: Hsla) -> [u32; 4] {
        [c.h.to_bits(), c.s.to_bits(), c.l.to_bits(), c.a.to_bits()]
    }

    #[inline]
    pub fn get_or_compute(&mut self, fg: Hsla, bg: Hsla, compute: impl FnOnce(Hsla, Hsla) -> Hsla) -> Hsla {
        let fg_key = Self::hsla_key(fg);
        let bg_key = Self::hsla_key(bg);
        if let Some((last_fg, last_bg, last_color)) = self.last
            && last_fg == fg_key
            && last_bg == bg_key
        {
            return last_color;
        }
        for &(entry_fg, entry_bg, entry_color) in &self.entries {
            if entry_fg == fg_key && entry_bg == bg_key {
                self.last = Some((entry_fg, entry_bg, entry_color));
                return entry_color;
            }
        }
        let adjusted = compute(fg, bg);
        if self.entries.len() >= Self::MAX_ENTRIES {
            self.entries.remove(0);
        }
        self.entries.push((fg_key, bg_key, adjusted));
        self.last = Some((fg_key, bg_key, adjusted));
        adjusted
    }
}

/// Frame-to-frame row render cache for HexView to minimize text shaping and layout allocations.
#[derive(Default)]
pub struct RowRenderCache {
    generation: usize,
    layout_version: usize,
    bytes_per_row: usize,
    collapsed_ptr: usize,
    parse_result_ptr: usize,
    font_size: Pixels,
    font_family: SharedString,
    radix: DisplayRadix,
    group_size: ByteGroupSize,
    is_big_endian: bool,
    hex_case: HexCase,
    encoding: Encoding,
    theme_bg: Hsla,
    theme_fg: Hsla,
    rows: HashMap<usize, CachedRow>,
    contrast_cache: HighlightContrastCache,
}

impl RowRenderCache {
    /// Validates the cache against current document, structure, font, and theme properties.
    /// Clears cached rows if any configuration has changed.
    #[allow(clippy::too_many_arguments)]
    pub fn validate(
        &mut self,
        generation: usize,
        layout_version: usize,
        bytes_per_row: usize,
        collapsed_ptr: usize,
        parse_result_ptr: usize,
        font_size: Pixels,
        font_family: &SharedString,
        radix: DisplayRadix,
        group_size: ByteGroupSize,
        is_big_endian: bool,
        hex_case: HexCase,
        encoding: Encoding,
        theme_bg: Hsla,
        theme_fg: Hsla,
    ) {
        if self.generation != generation
            || self.layout_version != layout_version
            || self.bytes_per_row != bytes_per_row
            || self.collapsed_ptr != collapsed_ptr
            || self.parse_result_ptr != parse_result_ptr
            || self.font_size != font_size
            || &self.font_family != font_family
            || self.radix != radix
            || self.group_size != group_size
            || self.is_big_endian != is_big_endian
            || self.hex_case != hex_case
            || self.encoding != encoding
            || self.theme_bg != theme_bg
            || self.theme_fg != theme_fg
        {
            self.generation = generation;
            self.layout_version = layout_version;
            self.bytes_per_row = bytes_per_row;
            self.collapsed_ptr = collapsed_ptr;
            self.parse_result_ptr = parse_result_ptr;
            self.font_size = font_size;
            self.font_family = font_family.clone();
            self.radix = radix;
            self.group_size = group_size;
            self.is_big_endian = is_big_endian;
            self.hex_case = hex_case;
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
    pub fn row_and_contrast_mut(&mut self, row_idx: usize) -> Option<(&CachedRow, &mut HighlightContrastCache)> {
        let contrast_cache = &mut self.contrast_cache;
        self.rows.get(&row_idx).map(|row| (row, contrast_cache))
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

#[cfg(test)]
mod tests {
    use super::HighlightContrastCache;
    use gpui_kit::hsla;

    #[test]
    fn highlight_contrast_cache_hits_and_bounds_capacity() {
        let mut cache = HighlightContrastCache::default();
        let mut calls = 0usize;
        let fg = hsla(0.0, 0.0, 0.9, 1.0);
        let bg1 = hsla(0.1, 0.5, 0.5, 1.0);
        let bg2 = hsla(0.6, 0.5, 0.3, 1.0);

        let r1 = cache.get_or_compute(fg, bg1, |c, _| {
            calls += 1;
            hsla(c.h, c.s, 0.1, 1.0)
        });
        let r1_again = cache.get_or_compute(fg, bg1, |_, _| {
            calls += 1;
            hsla(0.0, 0.0, 0.0, 1.0)
        });
        assert_eq!(r1, r1_again);
        assert_eq!(calls, 1);

        let r2 = cache.get_or_compute(fg, bg2, |c, _| {
            calls += 1;
            hsla(c.h, c.s, 0.8, 1.0)
        });
        assert_eq!(calls, 2);

        // Switching back to bg1 hits `entries` even though `last` holds `bg2`.
        let r1_from_entries = cache.get_or_compute(fg, bg1, |_, _| {
            calls += 1;
            hsla(0.0, 0.0, 0.0, 1.0)
        });
        assert_eq!(r1_from_entries, r1);
        assert_eq!(calls, 2);
        assert_ne!(r1, r2);

        for i in 0..80 {
            let bg = hsla(i as f32 / 100.0, 0.5, 0.4, 1.0);
            let _ = cache.get_or_compute(fg, bg, |_, b| b);
        }
        assert!(cache.entries.len() <= HighlightContrastCache::MAX_ENTRIES);
    }
}
