use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::sync::Arc;

pub const DEFAULT_BYTES_PER_ROW: usize = 16;
pub const MIN_BYTES_PER_ROW: usize = 1;
pub const MAX_BYTES_PER_ROW: usize = 64;
pub const BYTES_PER_ROW: usize = DEFAULT_BYTES_PER_ROW;

/// Application-wide setting for the number of bytes displayed per row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BytesPerRow(pub usize);

impl Default for BytesPerRow {
    fn default() -> Self {
        Self(DEFAULT_BYTES_PER_ROW)
    }
}

impl std::ops::Deref for BytesPerRow {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<usize> for BytesPerRow {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl gpui::Global for BytesPerRow {}

#[derive(Clone, Debug)]
pub enum LineMap {
    Standard { total_size: usize, bytes_per_row: usize },
    Sparse(Arc<SparseLineMap>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SparseLineMap {
    pub segments: Vec<LayoutSegment>,
    pub total_lines: usize,
    pub total_size: usize,
    pub max_bytes_per_row: usize,
    pub bytes_per_row: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutSegment {
    pub start_offset: usize,
    pub start_line: usize,
    pub byte_len: usize,
    pub line_count: usize,
    pub kind: SegmentKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SegmentKind {
    Standard,
    Custom { starts: Arc<Vec<usize>> },
}

impl PartialEq for LineMap {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                LineMap::Standard {
                    total_size: s1,
                    bytes_per_row: b1,
                },
                LineMap::Standard {
                    total_size: s2,
                    bytes_per_row: b2,
                },
            ) => s1 == s2 && b1 == b2,
            (LineMap::Sparse(sm1), LineMap::Sparse(sm2)) => sm1 == sm2,
            _ => {
                if self.len() != other.len() {
                    return false;
                }
                for i in 0..self.len() {
                    if self.get(i) != other.get(i) {
                        return false;
                    }
                }
                true
            }
        }
    }
}

impl Eq for LineMap {}

impl PartialEq<Vec<usize>> for LineMap {
    fn eq(&self, other: &Vec<usize>) -> bool {
        if self.len() != other.len() {
            return false;
        }
        for (i, &item) in other.iter().enumerate().take(self.len()) {
            if self.get(i) != Some(item) {
                return false;
            }
        }
        true
    }
}

impl PartialEq<LineMap> for Vec<usize> {
    fn eq(&self, other: &LineMap) -> bool {
        other.eq(self)
    }
}

impl SparseLineMap {
    pub fn len(&self) -> usize {
        self.total_lines
    }

    pub fn is_empty(&self) -> bool {
        self.total_lines == 0
    }

    pub fn get(&self, index: usize) -> Option<usize> {
        if index >= self.total_lines {
            return None;
        }
        let seg_idx = match self.segments.binary_search_by(|seg| {
            if index < seg.start_line {
                std::cmp::Ordering::Greater
            } else if index >= seg.start_line + seg.line_count {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        }) {
            Ok(idx) => idx,
            Err(_) => return None,
        };
        let seg = &self.segments[seg_idx];
        match &seg.kind {
            SegmentKind::Standard => {
                let rel_line = index - seg.start_line;
                Some(seg.start_offset + rel_line * self.bytes_per_row)
            }
            SegmentKind::Custom { starts } => {
                let rel_line = index - seg.start_line;
                starts.get(rel_line).copied()
            }
        }
    }

    pub fn binary_search(&self, offset: &usize) -> Result<usize, usize> {
        if self.total_size == 0 {
            return if *offset == 0 { Ok(0) } else { Err(1) };
        }
        if *offset >= self.total_size {
            return Err(self.total_lines);
        }
        let seg_idx = match self.segments.binary_search_by(|seg| {
            if *offset < seg.start_offset {
                std::cmp::Ordering::Greater
            } else if *offset >= seg.start_offset + seg.byte_len {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        }) {
            Ok(idx) => idx,
            Err(_) => return Err(self.total_lines),
        };
        let seg = &self.segments[seg_idx];
        match &seg.kind {
            SegmentKind::Standard => {
                let rel_offset = offset - seg.start_offset;
                if rel_offset.is_multiple_of(self.bytes_per_row) {
                    Ok(seg.start_line + rel_offset / self.bytes_per_row)
                } else {
                    Err(seg.start_line + rel_offset / self.bytes_per_row + 1)
                }
            }
            SegmentKind::Custom { starts } => match starts.binary_search(offset) {
                Ok(idx) => Ok(seg.start_line + idx),
                Err(idx) => Err(seg.start_line + idx),
            },
        }
    }
}

impl LineMap {
    pub fn len(&self) -> usize {
        match self {
            LineMap::Standard { total_size, bytes_per_row } => {
                if *total_size == 0 {
                    1
                } else {
                    (*total_size).div_ceil(*bytes_per_row)
                }
            }
            LineMap::Sparse(sparse) => sparse.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, index: usize) -> Option<usize> {
        match self {
            LineMap::Standard { bytes_per_row, .. } => {
                let len = self.len();
                if index < len { Some(index * *bytes_per_row) } else { None }
            }
            LineMap::Sparse(sparse) => sparse.get(index),
        }
    }

    pub fn binary_search(&self, offset: &usize) -> Result<usize, usize> {
        match self {
            LineMap::Standard { total_size, bytes_per_row } => {
                if *total_size == 0 {
                    return if *offset == 0 { Ok(0) } else { Err(1) };
                }
                let row = *offset / *bytes_per_row;
                let len = self.len();
                if row < len {
                    if (*offset).is_multiple_of(*bytes_per_row) { Ok(row) } else { Err(row + 1) }
                } else {
                    Err(len)
                }
            }
            LineMap::Sparse(sparse) => sparse.binary_search(offset),
        }
    }

    pub fn max_bytes_per_row(&self) -> usize {
        match self {
            LineMap::Standard { bytes_per_row, .. } => *bytes_per_row,
            LineMap::Sparse(sparse) => sparse.max_bytes_per_row,
        }
    }

    pub fn bytes_per_row(&self) -> usize {
        match self {
            LineMap::Standard { bytes_per_row, .. } => *bytes_per_row,
            LineMap::Sparse(sparse) => sparse.bytes_per_row,
        }
    }
}

/// Builds a line map when the layout and break events are already sorted and
/// deduplicated. The default expanded structure layout uses the same boundary
/// list for both event streams, so this avoids a second allocation and sort.
pub fn build_line_map_from_sorted_events(total_size: usize, events: &[usize], custom_joins: &BTreeSet<usize>, empty_lines: &BTreeMap<usize, usize>) -> LineMap {
    build_line_map_from_sorted_events_with_bytes_per_row(total_size, events, custom_joins, empty_lines, BYTES_PER_ROW)
}

pub fn build_line_map_from_sorted_events_with_bytes_per_row(
    total_size: usize,
    events: &[usize],
    custom_joins: &BTreeSet<usize>,
    empty_lines: &BTreeMap<usize, usize>,
    bytes_per_row: usize,
) -> LineMap {
    let bytes_per_row = bytes_per_row.max(1);
    if events.is_empty() && custom_joins.is_empty() && empty_lines.is_empty() {
        return LineMap::Standard { total_size, bytes_per_row };
    }

    build_line_map_from_sorted_event_lists(total_size, events, events, custom_joins, empty_lines, bytes_per_row)
}

fn build_line_map_from_sorted_event_lists(
    total_size: usize,
    layout_events: &[usize],
    break_events: &[usize],
    custom_joins: &BTreeSet<usize>,
    empty_lines: &BTreeMap<usize, usize>,
    bytes_per_row: usize,
) -> LineMap {
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
        let mut event_idx = 0;
        let mut break_idx = 0;

        while current < total_size {
            // Find next event > current.
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
                    // We can fit one or more standard lines of bytes_per_row.
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
                    // No more events, and we have at least one full standard line remaining.
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
            // Generate a custom segment using localized layout logic.
            let mut starts = Vec::new();
            let start_offset = current;
            let start_line = current_line;

            while current < total_size {
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

                // Process empty lines at current.
                if let Some(&count) = empty_lines.get(&current) {
                    for _ in 0..count {
                        starts.push(current);
                    }
                }

                starts.push(current);

                // Find the next event break after current (includes structure
                // field breaks and custom breaks) in O(1) amortized time.
                while break_idx < break_events.len() && break_events[break_idx] <= current {
                    break_idx += 1;
                }
                let next_event_break = break_events.get(break_idx).copied();

                // Advance in bytes_per_row increments, skipping joined boundaries.
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

    // Compute max_bytes_per_row and total_lines once from the compact segments.
    let mut max_bytes_per_row = bytes_per_row;
    let mut total_lines = 0;
    for i in 0..segments.len() {
        let segment = &segments[i];
        total_lines += segment.line_count;
        match &segment.kind {
            SegmentKind::Standard => {
                if i + 1 == segments.len() {
                    let last_line_start = segment.start_offset + (segment.line_count - 1) * bytes_per_row;
                    let last_line_len = total_size - last_line_start;
                    max_bytes_per_row = max_bytes_per_row.max(last_line_len);
                }
            }
            SegmentKind::Custom { starts } => {
                let next_start_offset = if i + 1 < segments.len() { segments[i + 1].start_offset } else { total_size };
                for j in 0..segment.line_count {
                    let end = if j + 1 < segment.line_count { starts[j + 1] } else { next_start_offset };
                    max_bytes_per_row = max_bytes_per_row.max(end.saturating_sub(starts[j]));
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
}

/// Rules for custom breaks, joins, and empty lines that alter the standard row layout.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CustomLayoutRules {
    pub breaks: BTreeSet<usize>,
    pub joins: BTreeSet<usize>,
    pub empty_lines: BTreeMap<usize, usize>,
}

impl CustomLayoutRules {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.breaks.is_empty() && self.joins.is_empty() && self.empty_lines.is_empty()
    }

    pub fn add_break(&mut self, offset: usize, line_starts: &LineMap, total_size: usize) {
        if offset >= total_size {
            return;
        }
        let current_line_idx = match line_starts.binary_search(&offset) {
            Ok(mut idx) => {
                while idx + 1 < line_starts.len() && line_starts.get(idx + 1) == Some(offset) {
                    idx += 1;
                }
                idx
            }
            Err(idx) => idx.saturating_sub(1),
        };
        let line_start = line_starts.get(current_line_idx).unwrap_or(0);
        let line_end = if current_line_idx + 1 < line_starts.len() {
            line_starts.get(current_line_idx + 1).unwrap_or(total_size)
        } else {
            total_size
        };
        let line_length = line_end.saturating_sub(line_start);

        if offset < line_end {
            let joins_to_remove: Vec<usize> = self.joins.range((offset + 1)..line_end).copied().collect();
            for j in joins_to_remove {
                self.joins.remove(&j);
            }
        }

        self.breaks.insert(offset);
        self.joins.remove(&offset);

        let bytes_per_row = line_starts.bytes_per_row();
        if line_length > bytes_per_row && offset != line_start {
            let mut step = offset + bytes_per_row;
            while step < line_end {
                self.joins.insert(step);
                step += bytes_per_row;
            }
            if line_end < total_size && !(line_end - offset).is_multiple_of(bytes_per_row) && !self.breaks.contains(&line_end) {
                self.breaks.insert(line_end);
            }
        }
    }

    pub fn remove_break(&mut self, offset: usize) -> bool {
        self.breaks.remove(&offset)
    }

    pub fn toggle_break(&mut self, offset: usize, line_starts: &LineMap, total_size: usize) {
        if self.has_break(offset) {
            self.remove_break(offset);
        } else {
            self.add_break(offset, line_starts, total_size);
        }
    }

    pub fn has_break(&self, offset: usize) -> bool {
        self.breaks.contains(&offset)
    }

    pub fn breaks_count(&self) -> usize {
        self.breaks.len()
    }

    pub fn breaks_snapshot(&self) -> BTreeSet<usize> {
        self.breaks.clone()
    }

    pub fn has_join(&self, offset: usize) -> bool {
        self.joins.contains(&offset)
    }

    pub fn empty_lines_at(&self, offset: usize) -> usize {
        self.empty_lines.get(&offset).copied().unwrap_or(0)
    }

    pub fn add_empty_line(&mut self, offset: usize, total_size: usize) {
        if offset <= total_size {
            *self.empty_lines.entry(offset).or_insert(0) += 1;
        }
    }

    pub fn remove_empty_line(&mut self, offset: usize) -> bool {
        if let Some(count) = self.empty_lines.get_mut(&offset) {
            if *count > 1 {
                *count -= 1;
            } else {
                self.empty_lines.remove(&offset);
            }
            true
        } else {
            false
        }
    }

    pub fn join_line(&mut self, line_starts: &LineMap, cursor_offset: usize) {
        let current_line_idx = match line_starts.binary_search(&cursor_offset) {
            Ok(mut idx) => {
                while idx + 1 < line_starts.len() && line_starts.get(idx + 1) == Some(cursor_offset) {
                    idx += 1;
                }
                idx
            }
            Err(idx) => idx.saturating_sub(1),
        };

        if current_line_idx + 1 >= line_starts.len() {
            return;
        }

        let next_line_start = line_starts.get(current_line_idx + 1).expect("valid next line start");
        if self.breaks.contains(&next_line_start) {
            self.breaks.remove(&next_line_start);
        } else if next_line_start != line_starts.get(current_line_idx).unwrap_or(0) {
            self.joins.insert(next_line_start);
        }
    }

    pub fn join_range(&mut self, range: Range<usize>, line_starts: &LineMap, total_size: usize) {
        let s = range.start.min(total_size);
        let e = range.end.min(total_size);

        if s >= e {
            return;
        }

        let current_line_idx = match line_starts.binary_search(&s) {
            Ok(mut idx) => {
                while idx + 1 < line_starts.len() && line_starts.get(idx + 1) == Some(s) {
                    idx += 1;
                }
                idx
            }
            Err(idx) => idx.saturating_sub(1),
        };
        let line_start_of_s = line_starts.get(current_line_idx).unwrap_or(0);

        // 1. s が行の先頭でなければ、s に break を追加して s から始まるようにする
        if s > 0 && s != line_start_of_s {
            self.breaks.insert(s);
        }
        self.joins.remove(&s);

        // 2. e がファイル末尾でなく、e で改行する必要がある場合は e に break を追加する
        if e < total_size {
            self.breaks.insert(e);
        }
        self.joins.remove(&e);

        // 3. (s..e) 内の breaks, joins, empty_lines をすべて削除する
        let breaks_to_remove: Vec<usize> = self.breaks.range((s + 1)..e).copied().collect();
        for b in breaks_to_remove {
            self.breaks.remove(&b);
        }
        let joins_to_remove: Vec<usize> = self.joins.range((s + 1)..e).copied().collect();
        for j in joins_to_remove {
            self.joins.remove(&j);
        }
        let empty_lines_to_remove: Vec<usize> = self.empty_lines.range((s + 1)..e).map(|(&k, _)| k).collect();
        for el in empty_lines_to_remove {
            self.empty_lines.remove(&el);
        }

        // 4. s から bytes_per_row ずつ進むステップを joins に追加し、1行に結合する
        let bytes_per_row = line_starts.bytes_per_row();
        let mut step = s + bytes_per_row;
        while step < e {
            self.joins.insert(step);
            step += bytes_per_row;
        }
    }

    pub fn clear_breaks(&mut self) {
        self.breaks.clear();
    }

    pub fn clear_all(&mut self) {
        self.breaks.clear();
        self.joins.clear();
        self.empty_lines.clear();
    }

    pub fn custom_layout_count(&self, folded_count: usize) -> usize {
        self.breaks.len() + self.joins.len() + self.empty_lines.values().sum::<usize>() + folded_count
    }

    pub fn adjust_after_edit(&mut self, _start: usize, _old_len: usize, _new_len: usize, shift: impl Fn(usize) -> usize) {
        let shifted_breaks = self.breaks.iter().copied().map(&shift).collect::<BTreeSet<_>>();
        self.breaks = shifted_breaks;

        let shifted_joins = self.joins.iter().copied().map(&shift).collect::<BTreeSet<_>>();
        self.joins = shifted_joins;

        let shifted_lines = self.empty_lines.iter().map(|(&offset, &count)| (shift(offset), count)).collect();
        self.empty_lines = shifted_lines;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_linemap() {
        let map = LineMap::Standard {
            total_size: 32,
            bytes_per_row: 16,
        };
        assert_eq!(map.len(), 2);
        assert!(!map.is_empty());
        assert_eq!(map.get(0), Some(0));
        assert_eq!(map.get(1), Some(16));
        assert_eq!(map.get(2), None);
        assert_eq!(map.binary_search(&0), Ok(0));
        assert_eq!(map.binary_search(&16), Ok(1));
        assert_eq!(map.binary_search(&8), Err(1));
        assert_eq!(map.max_bytes_per_row(), 16);
        assert_eq!(map.bytes_per_row(), 16);
    }

    #[test]
    fn test_standard_linemap_custom_bytes_per_row() {
        let map = LineMap::Standard {
            total_size: 32,
            bytes_per_row: 8,
        };
        assert_eq!(map.len(), 4);
        assert_eq!(map.get(0), Some(0));
        assert_eq!(map.get(1), Some(8));
        assert_eq!(map.get(2), Some(16));
        assert_eq!(map.get(3), Some(24));
        assert_eq!(map.get(4), None);
        assert_eq!(map.binary_search(&0), Ok(0));
        assert_eq!(map.binary_search(&8), Ok(1));
        assert_eq!(map.binary_search(&12), Err(2));
        assert_eq!(map.max_bytes_per_row(), 8);
        assert_eq!(map.bytes_per_row(), 8);
    }

    #[test]
    fn test_standard_linemap_empty() {
        let map = LineMap::Standard {
            total_size: 0,
            bytes_per_row: 16,
        };
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(0), Some(0));
        assert_eq!(map.binary_search(&0), Ok(0));
        assert_eq!(map.binary_search(&10), Err(1));
    }

    #[test]
    fn test_sparse_linemap() {
        let seg = LayoutSegment {
            start_offset: 0,
            start_line: 0,
            byte_len: 10,
            line_count: 1,
            kind: SegmentKind::Standard,
        };
        let sparse = SparseLineMap {
            segments: vec![seg],
            total_lines: 1,
            total_size: 10,
            max_bytes_per_row: 10,
            bytes_per_row: 16,
        };
        let map = LineMap::Sparse(Arc::new(sparse));
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(0), Some(0));
        assert_eq!(map.binary_search(&0), Ok(0));
        assert_eq!(map.max_bytes_per_row(), 10);
        assert_eq!(map.bytes_per_row(), 16);
    }
}
