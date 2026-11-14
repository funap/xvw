use crate::core::document::Document;
use crate::core::layout::{BYTES_PER_ROW, LayoutSegment, LineMap, SegmentKind, SparseLineMap};
use crate::core::structure::ParseResult;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
struct LayoutCacheKey {
    layout_version: usize,
    bookmarks_version: u64,
    bytes_per_row: usize,
    total_size: usize,
    show_inline_structure_view: bool,
    is_parsing_structure: bool,
    parse_result_identity: Option<(usize, usize, usize)>,
    collapsed_struct_ids: HashSet<String>,
}

impl LayoutCacheKey {
    fn parse_identity(parse_result: &Option<Arc<ParseResult>>) -> Option<(usize, usize, usize)> {
        parse_result
            .as_ref()
            .map(|res| (Arc::as_ptr(res) as usize, res.total_parsed_bytes, res.fields.len()))
    }

    fn matches(
        &self,
        doc: &Document,
        bytes_per_row: usize,
        show_inline_structure_view: bool,
        is_parsing_structure: bool,
        collapsed_struct_ids: &HashSet<String>,
    ) -> bool {
        self.layout_version == doc.layout_version()
            && self.bookmarks_version == doc.metadata.bookmarks.version()
            && self.bytes_per_row == bytes_per_row
            && self.total_size == doc.buffer.len()
            && self.show_inline_structure_view == show_inline_structure_view
            && self.is_parsing_structure == is_parsing_structure
            && self.parse_result_identity == Self::parse_identity(&doc.metadata.parse_result)
            && &self.collapsed_struct_ids == collapsed_struct_ids
    }
}

/// Caches and computes layout mapping from buffer offsets to visual lines.
#[derive(Debug)]
pub struct LayoutEngine {
    cached_line_map: RefCell<Option<LineMap>>,
    cached_key: RefCell<Option<LayoutCacheKey>>,
    cached_layout_version: Cell<usize>,
    bytes_per_row: Cell<usize>,
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new(0)
    }
}

impl LayoutEngine {
    /// Creates a new `LayoutEngine` with an initial layout version.
    pub fn new(initial_layout_version: usize) -> Self {
        Self {
            cached_line_map: RefCell::new(None),
            cached_key: RefCell::new(None),
            cached_layout_version: Cell::new(initial_layout_version),
            bytes_per_row: Cell::new(BYTES_PER_ROW),
        }
    }

    /// Creates a new `LayoutEngine` with an initial layout version and custom bytes per row.
    pub fn with_bytes_per_row(initial_layout_version: usize, bytes_per_row: usize) -> Self {
        Self {
            cached_line_map: RefCell::new(None),
            cached_key: RefCell::new(None),
            cached_layout_version: Cell::new(initial_layout_version),
            bytes_per_row: Cell::new(bytes_per_row.max(1)),
        }
    }

    /// Returns the number of bytes displayed per standard row.
    pub fn bytes_per_row(&self) -> usize {
        self.bytes_per_row.get()
    }

    /// Sets the number of bytes displayed per row and invalidates the cached layout if changed.
    pub fn set_bytes_per_row(&self, bytes_per_row: usize) {
        let bytes_per_row = bytes_per_row.max(1);
        if self.bytes_per_row.get() != bytes_per_row {
            self.bytes_per_row.set(bytes_per_row);
            self.invalidate();
        }
    }

    /// Invalidates the cached line map.
    pub fn invalidate(&self) {
        self.cached_line_map.replace(None);
        self.cached_key.replace(None);
    }

    /// Returns the active layout version cached by this engine.
    pub fn cached_layout_version(&self) -> usize {
        self.cached_layout_version.get()
    }

    /// Finds the line index containing `offset`, returning the data line if duplicate empty lines exist.
    pub fn find_line_index(offset: usize, line_starts: &LineMap) -> usize {
        match line_starts.binary_search(&offset) {
            Ok(mut idx) => {
                while idx + 1 < line_starts.len() && line_starts.get(idx + 1) == Some(offset) {
                    idx += 1;
                }
                idx
            }
            Err(idx) => idx.saturating_sub(1),
        }
    }

    /// Binary search lookup for `offset` within a sorted slice of line start offsets.
    pub fn find_line_index_in_slice(offset: usize, line_starts: &[usize]) -> usize {
        match line_starts.binary_search(&offset) {
            Ok(mut idx) => {
                while idx + 1 < line_starts.len() && line_starts[idx + 1] == offset {
                    idx += 1;
                }
                idx
            }
            Err(idx) => idx.saturating_sub(1),
        }
    }

    /// 上方向の次のデータ行（空行・折りたたみ行をスキップ）のインデックスを返す。
    pub fn prev_data_line(idx: usize, line_starts: &LineMap, folded_regions: &BTreeMap<usize, usize>) -> Option<usize> {
        let mut i = idx.checked_sub(1)?;
        if line_starts.is_empty() {
            return None;
        }
        loop {
            let line_start = line_starts.get(i)?;
            let line_end = if i + 1 < line_starts.len() {
                line_starts.get(i + 1)?
            } else {
                return if folded_regions.contains_key(&line_start) { None } else { Some(i) };
            };
            if line_end > line_start && !folded_regions.contains_key(&line_start) {
                return Some(i);
            }
            if i == 0 {
                return None;
            }
            i -= 1;
        }
    }

    /// 下方向の次のデータ行（空行・折りたたみ行をスキップ）のインデックスを返す。
    pub fn next_data_line(idx: usize, line_starts: &LineMap, total_size: usize, folded_regions: &BTreeMap<usize, usize>) -> Option<usize> {
        let mut i = idx + 1;
        while i < line_starts.len() {
            let line_start = line_starts.get(i)?;
            let line_end = if i + 1 < line_starts.len() { line_starts.get(i + 1)? } else { total_size };
            if line_end > line_start && !folded_regions.contains_key(&line_start) {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    /// Returns true if the document layout deviates from standard fixed-width 16-byte rows.
    pub fn has_custom_layout(doc: &Document, show_inline_structure_view: bool, is_parsing_structure: bool) -> bool {
        let meta = &doc.metadata;
        !meta.custom_layout.is_empty()
            || !meta.bookmarks.hidden_colors.is_empty()
            || !meta.bookmarks.hidden_ids.is_empty()
            || meta.bookmarks.hide_unbookmarked
            || (show_inline_structure_view && !is_parsing_structure && meta.parse_result.is_some())
            || doc.address_map.has_gaps()
    }

    /// Computes or retrieves the cached line mapping for `doc`.
    pub fn line_starts(&self, doc: &Document, show_inline_structure_view: bool, is_parsing_structure: bool, collapsed_struct_ids: &HashSet<String>) -> LineMap {
        let current_layout_version = doc.layout_version();
        let bytes_per_row = self.bytes_per_row.get();
        let total_size = doc.buffer.len();

        let cache_hit = self
            .cached_key
            .borrow()
            .as_ref()
            .is_some_and(|key| key.matches(doc, bytes_per_row, show_inline_structure_view, is_parsing_structure, collapsed_struct_ids));

        if cache_hit && let Some(cached) = self.cached_line_map.borrow().as_ref() {
            return cached.clone();
        }

        self.cached_line_map.replace(None);
        self.cached_key.replace(None);
        self.cached_layout_version.set(current_layout_version);

        let meta = &doc.metadata;

        // The parser prepares the default expanded structure layout before it
        // publishes the 100% result. Reuse it directly on the UI thread; the
        // dynamic builder below remains the compatibility path for custom
        // joins/breaks and collapsed structures.
        let map = if show_inline_structure_view
            && !is_parsing_structure
            && collapsed_struct_ids.is_empty()
            && meta.custom_layout.is_empty()
            && meta.bookmarks.hidden_colors.is_empty()
            && meta.bookmarks.hidden_ids.is_empty()
            && !meta.bookmarks.hide_unbookmarked
            && !doc.address_map.has_gaps()
            && let Some(parse_res) = &meta.parse_result
            && let Some(line_map) = &parse_res.structure_line_map
            && line_map.bytes_per_row() == bytes_per_row
            && line_map.total_size() == total_size
        {
            (**line_map).clone()
        } else if !Self::has_custom_layout(doc, show_inline_structure_view, is_parsing_structure) {
            LineMap::Standard { total_size, bytes_per_row }
        } else {
            let mut folded_regions = (*doc.computed_folded_regions()).clone();
            if show_inline_structure_view
                && !is_parsing_structure
                && let Some(parse_res) = &meta.parse_result
            {
                parse_res.collect_collapsed_ranges(collapsed_struct_ids, &mut folded_regions);
            }
            let mut segments = Vec::new();

            if total_size == 0 {
                segments.push(LayoutSegment {
                    start_offset: 0,
                    start_line: 0,
                    byte_len: 0,
                    line_count: 1,
                    kind: SegmentKind::Custom { starts: Arc::new(vec![0]) },
                });
            } else {
                let mut current = 0;
                let mut current_line = 0;

                let custom_breaks = &meta.custom_layout.breaks;
                let custom_joins = &meta.custom_layout.joins;
                let mut empty_line_counts = meta.custom_layout.empty_lines.clone();

                let mut segment_breaks = std::collections::BTreeSet::new();
                doc.address_map.collect_segment_breaks(&mut segment_breaks);
                doc.address_map.collect_gap_lines(&mut empty_line_counts);

                let mut field_breaks: Vec<usize> = Vec::new();
                if show_inline_structure_view
                    && !is_parsing_structure
                    && let Some(parse_res) = &meta.parse_result
                {
                    parse_res.collect_field_breaks(&mut field_breaks, collapsed_struct_ids);
                    parse_res.collect_structure_header_lines(&mut empty_line_counts, collapsed_struct_ids);
                }

                let mut break_events: Vec<usize> =
                    Vec::with_capacity(custom_breaks.len() + segment_breaks.len() + empty_line_counts.len() + folded_regions.len() * 2 + field_breaks.len());
                break_events.extend(custom_breaks.iter().copied());
                break_events.extend(segment_breaks.iter().copied());
                break_events.extend(empty_line_counts.keys().copied());
                for (&s, &e) in folded_regions.iter() {
                    let s = s.min(total_size);
                    let e = e.min(total_size);
                    if s < e {
                        break_events.push(s);
                        break_events.push(e);
                    }
                }
                break_events.extend_from_slice(&field_breaks);
                break_events.sort_unstable();
                break_events.dedup();

                let mut layout_events: Vec<usize> = Vec::with_capacity(break_events.len() + custom_joins.len());
                layout_events.extend_from_slice(&break_events);
                layout_events.extend(custom_joins.iter().copied());
                layout_events.sort_unstable();
                layout_events.dedup();

                let mut event_idx = 0;
                let mut break_idx = 0;

                while current < total_size {
                    // Check if current is a fold start
                    if let Some(&raw_fold_end) = folded_regions.get(&current) {
                        let fold_end = raw_fold_end.min(total_size);
                        if fold_end <= current {
                            current += 1;
                            continue;
                        }
                        let empty_count = empty_line_counts.remove(&current).unwrap_or(0);
                        let mut starts = Vec::with_capacity(empty_count + 1);
                        for _ in 0..empty_count {
                            starts.push(current);
                        }
                        starts.push(current);
                        let line_count = starts.len();
                        segments.push(LayoutSegment {
                            start_offset: current,
                            start_line: current_line,
                            byte_len: fold_end - current,
                            line_count,
                            kind: SegmentKind::Custom { starts: Arc::new(starts) },
                        });
                        current = fold_end;
                        current_line += line_count;
                        continue;
                    }

                    // Find next event > current
                    while event_idx < layout_events.len() && layout_events[event_idx] <= current {
                        event_idx += 1;
                    }
                    let next_event = if event_idx < layout_events.len() {
                        Some(layout_events[event_idx])
                    } else {
                        None
                    };

                    match next_event {
                        Some(ev) if ev - current > bytes_per_row => {
                            // We can fit one or more standard lines of bytes_per_row
                            let n = (ev - current - 1) / bytes_per_row;
                            if n > 0 {
                                let len_bytes = n * bytes_per_row;
                                segments.push(LayoutSegment {
                                    start_offset: current,
                                    start_line: current_line,
                                    byte_len: len_bytes,
                                    line_count: n,
                                    kind: SegmentKind::Standard,
                                });
                                current += len_bytes;
                                current_line += n;
                                continue;
                            }
                        }
                        None if total_size - current >= bytes_per_row => {
                            // No more events, and we have at least one full standard line remaining
                            let remaining_bytes = total_size - current;
                            let n = remaining_bytes / bytes_per_row;
                            let len_bytes = n * bytes_per_row;
                            segments.push(LayoutSegment {
                                start_offset: current,
                                start_line: current_line,
                                byte_len: len_bytes,
                                line_count: n,
                                kind: SegmentKind::Standard,
                            });
                            current += len_bytes;
                            current_line += n;
                            continue;
                        }
                        _ => {}
                    }

                    // Otherwise, we are too close to an event or at the end of the file.
                    // We must generate a Custom segment using localized layout logic.
                    let mut starts = Vec::new();
                    let start_offset = current;
                    let start_line = current_line;

                    while current < total_size {
                        // If current is a fold start, finish this segment if not empty, or handle fold
                        if let Some(&raw_fold_end) = folded_regions.get(&current) {
                            let fold_end = raw_fold_end.min(total_size);
                            if !starts.is_empty() {
                                break;
                            }
                            if fold_end <= current {
                                current += 1;
                                break;
                            }
                            if let Some(count) = empty_line_counts.remove(&current) {
                                for _ in 0..count {
                                    starts.push(current);
                                }
                            }
                            starts.push(current);
                            current = fold_end;
                            break;
                        }

                        // Check if we can transition back to Standard mode.
                        if !starts.is_empty() {
                            while event_idx < layout_events.len() && layout_events[event_idx] < current {
                                event_idx += 1;
                            }
                            let next_ev = if event_idx < layout_events.len() {
                                Some(layout_events[event_idx])
                            } else {
                                None
                            };

                            let can_transition = match next_ev {
                                Some(ev) => ev - current > bytes_per_row,
                                None => total_size - current >= bytes_per_row,
                            };

                            if can_transition {
                                break;
                            }
                        }

                        // Process empty lines at current
                        if let Some(count) = empty_line_counts.remove(&current) {
                            for _ in 0..count {
                                starts.push(current);
                            }
                        }

                        starts.push(current);

                        // Find next event break after current in O(1) amortized
                        while break_idx < break_events.len() && break_events[break_idx] <= current {
                            break_idx += 1;
                        }
                        let next_event_break = break_events.get(break_idx).copied();

                        // Advance in bytes_per_row increments, skipping joined boundaries
                        let mut next_pos = current + bytes_per_row;
                        while custom_joins.contains(&next_pos) && next_pos < total_size {
                            next_pos += bytes_per_row;
                        }

                        match next_event_break {
                            Some(break_pos) if break_pos < next_pos && break_pos > current => {
                                current = break_pos;
                            }
                            _ => {
                                current = next_pos;
                            }
                        }
                    }

                    let line_count = starts.len();
                    let byte_len = current.saturating_sub(start_offset);

                    segments.push(LayoutSegment {
                        start_offset,
                        start_line,
                        byte_len,
                        line_count,
                        kind: SegmentKind::Custom { starts: Arc::new(starts) },
                    });
                    current_line += line_count;
                }
            }

            // Quick final pass to compute max_bytes_per_row and total_lines
            let mut max_bytes_per_row = bytes_per_row;
            let mut total_lines = 0;
            for i in 0..segments.len() {
                let seg = &segments[i];
                total_lines += seg.line_count;
                match &seg.kind {
                    SegmentKind::Standard => {
                        if i + 1 == segments.len() {
                            let last_line_start = seg.start_offset + (seg.line_count - 1) * bytes_per_row;
                            let last_line_len = total_size - last_line_start;
                            max_bytes_per_row = max_bytes_per_row.max(last_line_len);
                        }
                    }
                    SegmentKind::Custom { starts } => {
                        let next_start_offset = if i + 1 < segments.len() { segments[i + 1].start_offset } else { total_size };
                        for j in 0..seg.line_count {
                            let line_st = starts[j];
                            if folded_regions.contains_key(&line_st) {
                                continue;
                            }
                            let end = if j + 1 < seg.line_count { starts[j + 1] } else { next_start_offset };
                            max_bytes_per_row = max_bytes_per_row.max(end.saturating_sub(line_st));
                        }
                    }
                }
            }

            LineMap::Sparse(Arc::new(SparseLineMap {
                segments,
                total_lines,
                total_size,
                max_bytes_per_row,
                bytes_per_row,
            }))
        };

        *self.cached_key.borrow_mut() = Some(LayoutCacheKey {
            layout_version: current_layout_version,
            bookmarks_version: doc.metadata.bookmarks.version(),
            bytes_per_row,
            total_size,
            show_inline_structure_view,
            is_parsing_structure,
            parse_result_identity: LayoutCacheKey::parse_identity(&doc.metadata.parse_result),
            collapsed_struct_ids: collapsed_struct_ids.clone(),
        });
        *self.cached_line_map.borrow_mut() = Some(map.clone());
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::buffer::Buffer;
    use std::path::PathBuf;

    #[test]
    fn test_layout_engine_standard_and_invalidation() {
        let engine = LayoutEngine::new(0);
        let doc = Document::new(PathBuf::from("test.bin"), Buffer::new(vec![0; 48]));
        let collapsed = HashSet::new();

        let map = engine.line_starts(&doc, false, false, &collapsed);
        assert_eq!(map.len(), 3); // 48 / 16 = 3 lines
        assert_eq!(map.get(0), Some(0));
        assert_eq!(map.get(1), Some(16));
        assert_eq!(map.get(2), Some(32));

        // Binary search find_line_index
        assert_eq!(LayoutEngine::find_line_index(0, &map), 0);
        assert_eq!(LayoutEngine::find_line_index(15, &map), 0);
        assert_eq!(LayoutEngine::find_line_index(16, &map), 1);
        assert_eq!(LayoutEngine::find_line_index(47, &map), 2);

        // Invalidate cached map
        engine.invalidate();
        assert!(engine.cached_line_map.borrow().is_none());
    }

    #[test]
    fn test_layout_engine_custom_bytes_per_row() {
        let engine = LayoutEngine::with_bytes_per_row(0, 8);
        assert_eq!(engine.bytes_per_row(), 8);
        let doc = Document::new(PathBuf::from("test.bin"), Buffer::new(vec![0; 48]));
        let collapsed = HashSet::new();

        let map = engine.line_starts(&doc, false, false, &collapsed);
        assert_eq!(map.len(), 6); // 48 / 8 = 6 lines
        assert_eq!(map.get(0), Some(0));
        assert_eq!(map.get(1), Some(8));
        assert_eq!(map.get(2), Some(16));
        assert_eq!(map.bytes_per_row(), 8);

        // Update bytes_per_row to 24
        engine.set_bytes_per_row(24);
        assert_eq!(engine.bytes_per_row(), 24);
        let map2 = engine.line_starts(&doc, false, false, &collapsed);
        assert_eq!(map2.len(), 2); // 48 / 24 = 2 lines
        assert_eq!(map2.get(0), Some(0));
        assert_eq!(map2.get(1), Some(24));
        assert_eq!(map2.bytes_per_row(), 24);
    }

    #[test]
    fn test_layout_engine_fast_path_respects_hide_unbookmarked() {
        use crate::core::bookmark::{BookmarkColor, BookmarkItem};
        use crate::core::color::RgbaColor;
        use crate::core::structure::{FieldValue, ParsedField};

        let mut doc = Document::new(PathBuf::from("test.bin"), Buffer::new(vec![0; 64]));
        let field = ParsedField {
            id: "magic".into(),
            field_type: "u4be".into(),
            offset: 0,
            size: 4,
            value: FieldValue::U32(0),
            color: RgbaColor::default(),
            description: None,
            children: Vec::new(),
            enum_label: None,
            is_instance: false,
        };
        let parse_res = ParseResult::new("test".into(), vec![field], 4, Vec::new()).with_structure_line_map(64);
        doc.metadata.parse_result = Some(Arc::new(parse_res));

        doc.metadata.bookmarks.add(BookmarkItem::new(16, 16, BookmarkColor::Red, "Marked"), 64);
        doc.metadata.bookmarks.set_hide_unbookmarked(true);

        let engine = LayoutEngine::new(doc.layout_version());
        let map = engine.line_starts(&doc, true, false, &HashSet::new());

        // Unbookmarked gaps [0..16) and [32..64) must remain folded even when structure_line_map exists.
        assert_eq!(map, vec![0, 16, 32]);
    }

    #[test]
    fn test_layout_engine_cache_tracks_structure_and_bookmark_inputs() {
        use crate::core::bookmark::{BookmarkColor, BookmarkItem};
        use crate::core::color::RgbaColor;
        use crate::core::structure::{FieldValue, ParsedField};

        let mut doc = Document::new(PathBuf::from("test.bin"), Buffer::new(vec![0; 48]));
        let header = ParsedField {
            id: "header".into(),
            field_type: "header_t".into(),
            offset: 0,
            size: 16,
            value: FieldValue::Struct,
            color: RgbaColor::default(),
            description: None,
            children: vec![
                ParsedField {
                    id: "magic".into(),
                    field_type: "u8".into(),
                    offset: 0,
                    size: 8,
                    value: FieldValue::U64(1),
                    color: RgbaColor::default(),
                    description: None,
                    children: Vec::new(),
                    enum_label: None,
                    is_instance: false,
                },
                ParsedField {
                    id: "flags".into(),
                    field_type: "u8".into(),
                    offset: 8,
                    size: 8,
                    value: FieldValue::U64(2),
                    color: RgbaColor::default(),
                    description: None,
                    children: Vec::new(),
                    enum_label: None,
                    is_instance: false,
                },
            ],
            enum_label: None,
            is_instance: false,
        };
        let parse_res = ParseResult::new("test".into(), vec![header], 16, Vec::new()).with_structure_line_map(48);
        doc.metadata.parse_result = Some(Arc::new(parse_res));

        let engine = LayoutEngine::new(doc.layout_version());
        let mut collapsed = HashSet::new();

        // Standard layout when inline structure view is off
        assert_eq!(engine.line_starts(&doc, false, false, &collapsed), vec![0, 16, 32]);

        // Enabling inline structure view must invalidate cache automatically
        assert_eq!(engine.line_starts(&doc, true, false, &collapsed), vec![0, 0, 8, 16, 32]);

        // Switching into active parse state must fall back to standard layout automatically
        assert_eq!(engine.line_starts(&doc, true, true, &collapsed), vec![0, 16, 32]);

        // Collapsing a structure ID must invalidate cache without manual engine.invalidate()
        collapsed.insert("0:0:header".to_string());
        assert_eq!(engine.line_starts(&doc, true, false, &collapsed), vec![0, 16, 32]);

        // Bookmark state changes track bookmarks.version() even if layout_version wasn't bumped directly
        doc.metadata.bookmarks.add(BookmarkItem::new(32, 16, BookmarkColor::Blue, "Tail"), 48);
        doc.metadata.bookmarks.hide_color(BookmarkColor::Blue);
        assert_eq!(engine.line_starts(&doc, false, false, &HashSet::new()), vec![0, 16, 32]);
        assert!(engine.line_starts(&doc, false, false, &HashSet::new()).len() == 3);

        doc.metadata.bookmarks.set_hide_unbookmarked(true);
        assert_eq!(engine.line_starts(&doc, false, false, &HashSet::new()), vec![0, 32]);
    }
}
