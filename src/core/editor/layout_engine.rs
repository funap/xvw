use crate::core::document::Document;
use crate::core::layout::{BYTES_PER_ROW, LayoutSegment, LineMap, SegmentKind, SparseLineMap};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

/// Caches and computes layout mapping from buffer offsets to visual lines.
#[derive(Debug)]
pub struct LayoutEngine {
    cached_line_map: RefCell<Option<LineMap>>,
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
            cached_layout_version: Cell::new(initial_layout_version),
            bytes_per_row: Cell::new(BYTES_PER_ROW),
        }
    }

    /// Creates a new `LayoutEngine` with an initial layout version and custom bytes per row.
    pub fn with_bytes_per_row(initial_layout_version: usize, bytes_per_row: usize) -> Self {
        Self {
            cached_line_map: RefCell::new(None),
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
        if self.cached_layout_version.get() != current_layout_version {
            self.cached_line_map.replace(None);
            self.cached_layout_version.set(current_layout_version);
        }

        if let Some(cached) = self.cached_line_map.borrow().as_ref() {
            return cached.clone();
        }

        let bytes_per_row = self.bytes_per_row.get();
        let meta = &doc.metadata;

        // The parser prepares the default expanded structure layout before it
        // publishes the 100% result. Reuse it directly on the UI thread; the
        // dynamic builder below remains the compatibility path for custom
        // joins/breaks and collapsed structures.
        if show_inline_structure_view
            && !is_parsing_structure
            && collapsed_struct_ids.is_empty()
            && meta.custom_layout.is_empty()
            && meta.bookmarks.hidden_colors.is_empty()
            && meta.bookmarks.hidden_ids.is_empty()
            && !doc.address_map.has_gaps()
            && let Some(parse_res) = &meta.parse_result
            && let Some(line_map) = &parse_res.structure_line_map
            && line_map.bytes_per_row() == bytes_per_row
        {
            let map = (**line_map).clone();
            *self.cached_line_map.borrow_mut() = Some(map.clone());
            return map;
        }

        let total_size = doc.buffer.len();
        let map = if !Self::has_custom_layout(doc, show_inline_structure_view, is_parsing_structure) {
            LineMap::Standard { total_size, bytes_per_row }
        } else {
            let folded_regions_guard = doc.computed_folded_regions();
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

                let mut layout_events: Vec<usize> = Vec::new();
                layout_events.extend(custom_breaks.iter().copied());
                layout_events.extend(custom_joins.iter().copied());
                layout_events.extend(segment_breaks.iter().copied());
                layout_events.extend(empty_line_counts.keys().copied());
                for (&s, &e) in folded_regions_guard.iter() {
                    layout_events.push(s);
                    layout_events.push(e);
                }
                if show_inline_structure_view
                    && !is_parsing_structure
                    && let Some(parse_res) = &meta.parse_result
                {
                    parse_res.collect_field_breaks(&mut layout_events, collapsed_struct_ids);
                    parse_res.collect_structure_header_lines(&mut empty_line_counts, collapsed_struct_ids);
                }
                layout_events.extend(empty_line_counts.keys().copied());
                layout_events.sort_unstable();
                layout_events.dedup();

                let mut break_events: Vec<usize> = Vec::new();
                break_events.extend(custom_breaks.iter().copied());
                break_events.extend(segment_breaks.iter().copied());
                break_events.extend(empty_line_counts.keys().copied());
                for (&s, &e) in folded_regions_guard.iter() {
                    break_events.push(s);
                    break_events.push(e);
                }
                if show_inline_structure_view
                    && !is_parsing_structure
                    && let Some(parse_res) = &meta.parse_result
                {
                    parse_res.collect_field_breaks(&mut break_events, collapsed_struct_ids);
                }
                break_events.sort_unstable();
                break_events.dedup();

                let mut event_idx = 0;
                let mut break_idx = 0;

                while current < total_size {
                    // Check if current is a fold start
                    if let Some(&fold_end) = folded_regions_guard.get(&current) {
                        segments.push(LayoutSegment {
                            start_offset: current,
                            start_line: current_line,
                            byte_len: fold_end - current,
                            line_count: 1,
                            kind: SegmentKind::Custom {
                                starts: Arc::new(vec![current]),
                            },
                        });
                        current = fold_end;
                        current_line += 1;
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
                        if let Some(&fold_end) = folded_regions_guard.get(&current) {
                            if !starts.is_empty() {
                                break;
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
                        if let Some(&count) = empty_line_counts.get(&current) {
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
                    let byte_len = current - start_offset;

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
                            if folded_regions_guard.contains_key(&line_st) {
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
}
