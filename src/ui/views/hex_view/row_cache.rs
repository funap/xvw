use crate::core::bookmark::BookmarkItem;
use crate::core::encoding::Encoding;
use crate::core::radix::{ByteGroupSize, DisplayRadix, HexCase};
use crate::core::structure::IndexedField;
use crate::ui::views::hex_view::types::HexTextSource;
use gpui_kit::*;
use std::cell::Cell;
use std::collections::HashMap;
use std::ops::Range;

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

struct CachedRowEntry {
    row: CachedRow,
    last_used: Cell<u64>,
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
    rows: HashMap<usize, CachedRowEntry>,
    access_tick: Cell<u64>,
    contrast_cache: HighlightContrastCache,
}

impl RowRenderCache {
    const MAX_CACHED_ROWS: usize = 512;
    const EVICT_TO_ROWS: usize = 384;

    #[inline]
    fn next_tick(&self) -> u64 {
        let next = self.access_tick.get().wrapping_add(1);
        self.access_tick.set(next);
        next
    }

    fn evict_stale_rows(&mut self) {
        if self.rows.len() < Self::MAX_CACHED_ROWS {
            return;
        }
        let evict_count = self.rows.len().saturating_sub(Self::EVICT_TO_ROWS);
        if evict_count == 0 {
            return;
        }
        let mut ticks: Vec<u64> = self.rows.values().map(|entry| entry.last_used.get()).collect();
        let (_, nth, _) = ticks.select_nth_unstable(evict_count - 1);
        let cutoff = *nth;
        self.rows.retain(|_, entry| entry.last_used.get() > cutoff);
    }
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
        let entry = self.rows.get(&row_idx)?;
        entry.last_used.set(self.next_tick());
        Some(&entry.row)
    }

    #[inline]
    pub fn row_and_contrast_mut(&mut self, row_idx: usize) -> Option<(&CachedRow, &mut HighlightContrastCache)> {
        let tick = self.next_tick();
        let contrast_cache = &mut self.contrast_cache;
        self.rows.get(&row_idx).map(|entry| {
            entry.last_used.set(tick);
            (&entry.row, contrast_cache)
        })
    }

    #[inline]
    pub fn insert(&mut self, row_idx: usize, row: CachedRow) {
        if !self.rows.contains_key(&row_idx) {
            self.evict_stale_rows();
        }
        let tick = self.next_tick();
        self.rows.insert(
            row_idx,
            CachedRowEntry {
                row,
                last_used: Cell::new(tick),
            },
        );
    }

    #[allow(dead_code)]
    #[inline]
    pub fn clear(&mut self) {
        self.rows.clear();
    }
}

/// Merges sorted bookmark highlights and search highlights by start offset, matching `BookmarkItem` precedence on exact ties.
fn merge_sorted_highlights(bookmark_items: &[BookmarkItem], highlights: &[(Range<usize>, Hsla)], out: &mut Vec<(Range<usize>, Hsla)>) {
    if highlights.is_empty() {
        out.reserve(bookmark_items.len());
        for b in bookmark_items {
            out.push((b.range(), b.rgba_color().into()));
        }
        return;
    }

    out.reserve(bookmark_items.len() + highlights.len());
    let mut b_iter = bookmark_items.iter().peekable();
    let mut h_iter = highlights.iter().peekable();

    while let (Some(&b), Some((hr, hc))) = (b_iter.peek(), h_iter.peek()) {
        let br = b.range();
        if br.start < hr.start {
            out.push((br, b.rgba_color().into()));
            b_iter.next();
        } else if hr.start < br.start {
            out.push((hr.clone(), *hc));
            h_iter.next();
        } else if br == *hr {
            out.push((br, b.rgba_color().into()));
            b_iter.next();
            h_iter.next();
        } else if br.end <= hr.end {
            out.push((br, b.rgba_color().into()));
            b_iter.next();
        } else {
            out.push((hr.clone(), *hc));
            h_iter.next();
        }
    }
    for b in b_iter {
        out.push((b.range(), b.rgba_color().into()));
    }
    for (hr, hc) in h_iter {
        out.push((hr.clone(), *hc));
    }
}

/// Caches merged highlight ranges and maintains block-level max-end bounds for fast viewport queries.
#[derive(Default)]
pub struct EffectiveHighlightsCache {
    initialized: bool,
    bookmark_version: u64,
    highlights_version: u64,
    items: Vec<(Range<usize>, Hsla)>,
    block_max_end: Vec<usize>,
}

impl EffectiveHighlightsCache {
    const BLOCK_SIZE: usize = 64;

    pub fn ensure_updated(&mut self, bookmark_version: u64, bookmark_items: &[BookmarkItem], highlights_version: u64, highlights: &[(Range<usize>, Hsla)]) {
        if self.initialized && self.bookmark_version == bookmark_version && self.highlights_version == highlights_version {
            return;
        }
        self.initialized = true;
        self.bookmark_version = bookmark_version;
        self.highlights_version = highlights_version;
        self.items.clear();
        self.block_max_end.clear();

        merge_sorted_highlights(bookmark_items, highlights, &mut self.items);
        for chunk in self.items.chunks(Self::BLOCK_SIZE) {
            let max_end = chunk.iter().map(|(r, _)| r.end).max().unwrap_or(0);
            self.block_max_end.push(max_end);
        }
    }

    pub fn collect_viewport(&self, vp_start: usize, vp_end: usize, out: &mut Vec<(Range<usize>, Hsla)>) {
        out.clear();
        if self.items.is_empty() || vp_end <= vp_start {
            return;
        }
        let end_idx = self.items.partition_point(|(r, _)| r.start < vp_end);
        if end_idx == 0 {
            return;
        }
        let block_count = end_idx.div_ceil(Self::BLOCK_SIZE);
        for (block_idx, &block_max) in self.block_max_end[..block_count].iter().enumerate() {
            if block_max <= vp_start {
                continue;
            }
            let start = block_idx * Self::BLOCK_SIZE;
            let end = (start + Self::BLOCK_SIZE).min(end_idx);
            for (range, color) in &self.items[start..end] {
                if range.end > vp_start {
                    out.push((range.clone(), *color));
                }
            }
        }
    }
}

/// Incremental sweep cursor yielding the active highlights for consecutive rows within a viewport.
pub struct HighlightSweep<'a> {
    viewport: &'a [(Range<usize>, Hsla)],
    cursor: usize,
    active: Vec<(Range<usize>, Hsla)>,
}

impl<'a> HighlightSweep<'a> {
    pub fn new(viewport: &'a [(Range<usize>, Hsla)]) -> Self {
        Self {
            viewport,
            cursor: 0,
            active: Vec::new(),
        }
    }

    #[inline]
    pub fn advance_to(&mut self, offset: usize, next_offset: usize) -> &[(Range<usize>, Hsla)] {
        self.active.retain(|(r, _)| r.end > offset);
        while let Some((r, c)) = self.viewport.get(self.cursor) {
            if r.start >= next_offset {
                break;
            }
            if r.end > offset {
                self.active.push((r.clone(), *c));
            }
            self.cursor += 1;
        }
        &self.active
    }
}

#[cfg(test)]
mod tests {
    use super::{CachedRow, EffectiveHighlightsCache, HighlightContrastCache, HighlightSweep, RowRenderCache};
    use crate::core::bookmark::{BookmarkColor, BookmarkItem};
    use crate::ui::views::hex_view::types::HexTextSource;
    use gpui_kit::{Hsla, SharedString, hsla};

    fn sample_cached_row(addr: usize) -> CachedRow {
        CachedRow {
            physical_address: addr,
            shaped_addr: None,
            hex_source: HexTextSource {
                text: SharedString::default(),
                groups: Vec::new(),
            },
            shaped_hex: None,
            ascii_cells: Vec::new(),
            shaped_ascii: None,
            structure_fields: None,
            desc: None,
        }
    }

    #[test]
    fn row_render_cache_evicts_oldest_entries_preserving_hot_rows() {
        let mut cache = RowRenderCache::default();
        for idx in 0..RowRenderCache::MAX_CACHED_ROWS {
            cache.insert(idx, sample_cached_row(idx * 16));
        }
        assert_eq!(cache.rows.len(), RowRenderCache::MAX_CACHED_ROWS);

        // Mark row 0 as recently accessed right before adding new rows.
        assert_eq!(cache.get(0).map(|r| r.physical_address), Some(0));

        for idx in RowRenderCache::MAX_CACHED_ROWS..560 {
            cache.insert(idx, sample_cached_row(idx * 16));
        }

        assert!(cache.rows.len() < RowRenderCache::MAX_CACHED_ROWS);
        assert!(cache.get(0).is_some(), "recently accessed row should survive eviction");
        assert!(cache.get(559).is_some(), "newly inserted row should remain cached");
        assert!(cache.get(1).is_none(), "oldest cold row should be evicted");
    }

    #[test]
    fn effective_highlights_cache_and_sweep_match_brute_force() {
        let mut bookmarks: Vec<BookmarkItem> = (0..200).map(|i| BookmarkItem::new(i * 32 + 4, 4, BookmarkColor::Orange, "")).collect();
        // Prepend a giant spanning bookmark across the entire range (tests block max-end skip correctness)
        bookmarks.insert(0, BookmarkItem::new(0, 20_000, BookmarkColor::Blue, ""));
        let search_hl: Vec<(std::ops::Range<usize>, Hsla)> = vec![(0..4, hsla(0.2, 0.5, 0.5, 1.0)), (4_000..4_008, hsla(0.5, 0.5, 0.5, 1.0))];

        let mut cache = EffectiveHighlightsCache::default();
        cache.ensure_updated(1, &bookmarks, 1, &search_hl);

        let vp_start = 3_968;
        let vp_end = 4_064;
        let mut viewport = Vec::new();
        cache.collect_viewport(vp_start, vp_end, &mut viewport);

        // Verify viewport results match brute-force intersection against cache.items
        let expected_vp: Vec<_> = cache.items.iter().filter(|(r, _)| r.start < vp_end && r.end > vp_start).cloned().collect();
        assert_eq!(viewport, expected_vp);

        // Sweep across monotonic rows including zero-byte structure header rows [4000, 4000) and trailing EOF [4064, 4064)
        let rows = [(3_968, 3_984), (3_984, 4_000), (4_000, 4_000), (4_000, 4_016), (4_016, 4_032), (4_064, 4_064)];
        let mut sweep = HighlightSweep::new(&viewport);
        for &(offset, next_offset) in &rows {
            let active = sweep.advance_to(offset, next_offset);
            let expected_row: Vec<_> = cache.items.iter().filter(|(r, _)| r.start < next_offset && r.end > offset).cloned().collect();
            assert_eq!(active, expected_row.as_slice(), "mismatch on row {offset}..{next_offset}");
        }

        // Calling again with same versions must preserve cached items without rebuilding
        let ptr_before = cache.items.as_ptr();
        cache.ensure_updated(1, &[], 1, &[]);
        assert_eq!(cache.items.as_ptr(), ptr_before);
    }

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
