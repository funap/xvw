use crate::core::bookmark::BookmarkItem;
use crate::core::editor::{Editor, LineMap};
use crate::core::encoding::Encoding;
use crate::core::radix::{ByteGroupSize, DisplayRadix, HexCase};
use crate::ui::color::BookmarkColorExt;
use crate::ui::views::hex_view::types::HexTextSource;
use gpui_kit::*;
use std::cell::Cell;
use std::collections::HashMap;
use std::ops::Range;

/// Cached ASCII cell layout data for one character slot in a row.
#[derive(Clone, Copy, Debug)]
pub struct CachedAsciiCell {
    pub cell_idx: usize,
    pub byte: u8,
    pub is_printable: bool,
}

/// Precomputed glyph placement data for centered Hex cells.
#[derive(Clone, Copy, Debug)]
pub struct CachedHexGlyph {
    pub font_id: gpui::FontId,
    pub glyph_id: gpui::GlyphId,
    pub text_index: usize,
    pub group_idx: usize,
    pub x_offset: Pixels,
    pub glyph_width: f32,
    pub is_emoji: bool,
}

/// Precomputed glyph list and metrics for one Hex row.
#[derive(Clone, Debug)]
pub struct CachedHexGlyphs {
    pub cell_width: Pixels,
    pub font_size: Pixels,
    pub ascent: Pixels,
    pub descent: Pixels,
    pub glyphs: Vec<CachedHexGlyph>,
}

/// Precomputed glyph placement data for centered ASCII cells.
#[derive(Clone, Copy, Debug)]
pub struct CachedAsciiGlyph {
    pub font_id: gpui::FontId,
    pub glyph_id: gpui::GlyphId,
    pub x_offset: Pixels,
    pub cell_index: usize,
    pub is_emoji: bool,
}

/// Precomputed glyph list and metrics for one ASCII row.
#[derive(Clone, Debug)]
pub struct CachedAsciiGlyphs {
    pub font_size: Pixels,
    pub ascent: Pixels,
    pub descent: Pixels,
    pub glyphs: Vec<CachedAsciiGlyph>,
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
    /// `(offset, size, start_x_px, width_px)` of each rendered leaf field in `shaped_base_desc`.
    pub leaf_field_spans: Vec<(usize, usize, f32, f32)>,
}

/// Cached structure indexing flags for one row.
#[derive(Clone, Copy, Debug)]
pub struct CachedRowStructureFields {
    pub is_structure_header_row: bool,
    pub is_collapsed_struct_row: bool,
}

/// Complete cached layout and shaped text for a single painted row.
#[derive(Clone, Debug)]
pub struct CachedRow {
    pub physical_address: usize,
    pub shaped_addr: Option<ShapedLine>,
    pub hex_source: HexTextSource,
    pub hex_glyphs: Option<CachedHexGlyphs>,
    pub ascii_cells: Vec<CachedAsciiCell>,
    pub ascii_glyphs: Option<CachedAsciiGlyphs>,
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

/// Returns the trimmed first non-empty line of a bookmark comment, if any.
#[inline]
pub fn clean_comment_first_line(comment: &str) -> Option<&str> {
    let trimmed = comment.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains('\n') || trimmed.contains('\r') {
        trimmed.split(['\n', '\r']).map(str::trim).find(|line| !line.is_empty())
    } else {
        Some(trimmed)
    }
}

/// Pre-filtered visible bookmark comment associated with its display row index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowCommentRef<'a> {
    pub row_idx: usize,
    pub badge_color: Hsla,
    pub clean_text: &'a str,
}

/// Collects and row-buckets visible bookmark comments for `[top_row, end_row)` once per frame.
pub fn collect_visible_comment_refs<'a>(
    bookmark_items: &'a [BookmarkItem],
    line_starts: &LineMap,
    total_size: usize,
    top_row: usize,
    end_row: usize,
    vp_start: usize,
    vp_end: usize,
) -> Vec<RowCommentRef<'a>> {
    if bookmark_items.is_empty() || end_row <= top_row {
        return Vec::new();
    }

    let end_bm = bookmark_items.partition_point(|h| h.offset <= vp_end);
    let mut visible = Vec::new();

    for h in &bookmark_items[..end_bm] {
        let Some(clean_text) = clean_comment_first_line(&h.comment) else {
            continue;
        };
        let h_end_offset = h.offset.saturating_add(h.size);
        let h_last_byte = h_end_offset.saturating_sub(1).max(h.offset);
        if h_last_byte < vp_start {
            continue;
        }
        let h_start_row = Editor::find_line_index(h.offset, line_starts);
        if h_start_row >= end_row {
            continue;
        }
        let display_row = h_start_row.max(top_row);
        let h_end_row = Editor::find_line_index(h_last_byte, line_starts);
        if display_row > h_end_row {
            continue;
        }
        if let Some((row_offset, row_next_offset)) = super::paint::row_byte_range(line_starts, display_row, total_size)
            && h.offset <= row_next_offset
            && h_last_byte >= row_offset
        {
            visible.push(RowCommentRef {
                row_idx: display_row,
                badge_color: h.color.to_badge_hsla(),
                clean_text,
            });
        }
    }

    visible.sort_by_key(|item| item.row_idx);
    visible
}

/// Caches shaped single-line bookmark comments keyed by text and foreground color bits.
#[inline]
fn hsla_key(color: Hsla) -> [u32; 4] {
    [color.h.to_bits(), color.s.to_bits(), color.l.to_bits(), color.a.to_bits()]
}

struct ShapedLabelVariant {
    size_key: u32,
    color_key: [u32; 4],
    shaped: ShapedLine,
    width: f32,
}

/// Bounded cache of shaped single-line labels keyed by clean text, font size, and color.
#[derive(Default)]
pub struct CommentShapeCache {
    entries: HashMap<String, Vec<ShapedLabelVariant>>,
}

impl CommentShapeCache {
    const MAX_UNIQUE_TEXTS: usize = 256;

    #[inline]
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    #[inline]
    pub fn get(&self, text: &str, color: Hsla, font_size: Pixels) -> Option<(&ShapedLine, f32)> {
        let size_key = f32::from(font_size).to_bits();
        let key = hsla_key(color);
        let variants = self.entries.get(text)?;
        variants
            .iter()
            .find(|variant| variant.size_key == size_key && variant.color_key == key)
            .map(|variant| (&variant.shaped, variant.width))
    }

    pub fn get_or_shape(&mut self, text: &str, color: Hsla, font: &Font, font_size: Pixels, window: &mut Window) -> (&ShapedLine, f32) {
        let _ = self.ensure_shaped_width(text, color, font, font_size, window);
        self.get(text, color, font_size).expect("shaped entry just inserted")
    }

    pub fn ensure_shaped_width(&mut self, text: &str, color: Hsla, font: &Font, font_size: Pixels, window: &mut Window) -> f32 {
        let size_key = f32::from(font_size).to_bits();
        let key = hsla_key(color);
        if let Some(variants) = self.entries.get(text)
            && let Some(variant) = variants.iter().find(|variant| variant.size_key == size_key && variant.color_key == key)
        {
            return variant.width;
        }

        if self.entries.len() >= Self::MAX_UNIQUE_TEXTS && !self.entries.contains_key(text) {
            self.entries.clear();
        }

        let shared = SharedString::from(text.to_string());
        let run = TextRun {
            len: shared.len(),
            font: font.clone(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = window.text_system().shape_line(shared, font_size, &[run], None);
        let width = f32::from(shaped.width);
        let variant = ShapedLabelVariant {
            size_key,
            color_key: key,
            shaped,
            width,
        };

        if let Some(variants) = self.entries.get_mut(text) {
            variants.push(variant);
        } else {
            self.entries.insert(text.to_string(), vec![variant]);
        }
        width
    }
}

/// Single-entry cache for the active cursor row's formatted address line.
#[derive(Default)]
pub struct CursorAddrShapeCache {
    entry: Option<(usize, [u32; 4], ShapedLine)>,
}

impl CursorAddrShapeCache {
    #[inline]
    pub fn clear(&mut self) {
        self.entry = None;
    }

    pub fn get_or_shape(
        &mut self,
        physical_address: usize,
        color: Hsla,
        font: &Font,
        font_size: Pixels,
        format_addr: impl FnOnce(usize) -> SharedString,
        window: &mut Window,
    ) -> &ShapedLine {
        let key = hsla_key(color);
        let hit = self.entry.as_ref().is_some_and(|(addr, c_key, _)| *addr == physical_address && *c_key == key);
        if !hit {
            let addr_str = format_addr(physical_address);
            let run = TextRun {
                len: addr_str.len(),
                font: font.clone(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window.text_system().shape_line(addr_str, font_size, &[run], None);
            self.entry = Some((physical_address, key, shaped));
        }
        &self.entry.as_ref().expect("cursor address shape populated").2
    }
}

/// Disjoint mutable caches exposed to `paint_hex_row` alongside a cached row reference.
pub struct RowPaintCaches<'a> {
    pub contrast: &'a mut HighlightContrastCache,
    pub comment_shapes: &'a mut CommentShapeCache,
    pub cursor_addr: &'a mut CursorAddrShapeCache,
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
    comment_shapes: CommentShapeCache,
    special_shapes: CommentShapeCache,
    cursor_addr_cache: CursorAddrShapeCache,
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
        if self.font_size != font_size || &self.font_family != font_family {
            self.comment_shapes.clear();
            self.special_shapes.clear();
            self.cursor_addr_cache.clear();
        }
        if self.hex_case != hex_case {
            self.cursor_addr_cache.clear();
        }
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
    pub fn get_or_shape_special(&mut self, text: &str, color: Hsla, font: &Font, font_size: Pixels, window: &mut Window) -> (&ShapedLine, f32) {
        self.special_shapes.get_or_shape(text, color, font, font_size, window)
    }

    #[inline]
    pub fn row_and_caches_mut(&mut self, row_idx: usize) -> Option<(&CachedRow, RowPaintCaches<'_>)> {
        let tick = self.next_tick();
        let caches = RowPaintCaches {
            contrast: &mut self.contrast_cache,
            comment_shapes: &mut self.comment_shapes,
            cursor_addr: &mut self.cursor_addr_cache,
        };
        self.rows.get(&row_idx).map(|entry| {
            entry.last_used.set(tick);
            (&entry.row, caches)
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

/// Merges sorted bookmark highlights and overlay highlights (search/diff) by range.
/// On identical ranges, bookmarks are emitted before overlays so `<=` tie-breaking in
/// `highlight_color_for_range` lets search/diff overlays stay visible on top of bookmarks.
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
    use super::{
        CachedRow, EffectiveHighlightsCache, HighlightContrastCache, HighlightSweep, RowCommentRef, RowRenderCache, clean_comment_first_line,
        collect_visible_comment_refs,
    };
    use crate::core::bookmark::{BookmarkColor, BookmarkItem};
    use crate::core::editor::LineMap;
    use crate::ui::color::BookmarkColorExt;
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
            hex_glyphs: None,
            ascii_cells: Vec::new(),
            ascii_glyphs: None,
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

        // Exact-range overlay highlights stay ordered after equal bookmark items so search highlights win ties.
        let tied_bookmark = BookmarkItem::new(16, 4, BookmarkColor::Blue, "");
        let tied_search_color = hsla(0.5, 0.8, 0.5, 1.0);
        let mut tied_cache = EffectiveHighlightsCache::default();
        tied_cache.ensure_updated(2, std::slice::from_ref(&tied_bookmark), 2, &[(16..20, tied_search_color)]);
        assert_eq!(tied_cache.items, vec![(16..20, tied_bookmark.rgba_color().into()), (16..20, tied_search_color)]);
    }

    #[test]
    fn clean_comment_first_line_trims_and_skips_blank_lines() {
        assert_eq!(clean_comment_first_line(""), None);
        assert_eq!(clean_comment_first_line("   \n\t \r\n "), None);
        assert_eq!(clean_comment_first_line("  hello world  "), Some("hello world"));
        assert_eq!(clean_comment_first_line("\r\n  \n  first line \r\n second line"), Some("first line"));
    }

    #[test]
    fn collect_visible_comment_refs_buckets_and_clamps_to_top_row() {
        // 8 rows * 16 bytes = 128 bytes
        let line_starts = LineMap::Standard {
            total_size: 128,
            bytes_per_row: 16,
        };
        let bookmarks = vec![
            BookmarkItem::new(0, 8, BookmarkColor::Red, "before viewport"),
            BookmarkItem::new(16, 48, BookmarkColor::Blue, "  \n  multi row spans top  \n detail"),
            BookmarkItem::new(48, 8, BookmarkColor::Green, "visible row 3"),
            BookmarkItem::new(64, 16, BookmarkColor::Yellow, "   "),
            BookmarkItem::new(96, 8, BookmarkColor::Cyan, "after viewport"),
        ];

        // Viewport covers rows 2..5 => bytes [32, 80)
        let refs = collect_visible_comment_refs(&bookmarks, &line_starts, 128, 2, 5, 32, 80);
        assert_eq!(
            refs,
            vec![
                RowCommentRef {
                    row_idx: 2,
                    badge_color: BookmarkColor::Blue.to_badge_hsla(),
                    clean_text: "multi row spans top",
                },
                RowCommentRef {
                    row_idx: 3,
                    badge_color: BookmarkColor::Green.to_badge_hsla(),
                    clean_text: "visible row 3",
                },
            ]
        );
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
