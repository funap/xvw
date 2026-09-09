use crate::core::editor::layout_engine::LayoutEngine;
use crate::core::layout::LineMap;
use crate::core::radix::ByteGroupSize;
use crate::core::selection::Selection;
use std::cmp;
use std::collections::BTreeMap;
use std::ops::Range;

/// A snapshot of the transient cursor state surrounding an edit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CursorState {
    pub cursor_offset: usize,
    pub selection: Selection,
}

/// Encapsulates cursor position, selection, and boundary navigation logic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorModel {
    pub offset: usize,
    pub selection: Selection,
    pub group_size: ByteGroupSize,
}

impl Default for CursorModel {
    fn default() -> Self {
        Self {
            offset: 0,
            selection: Selection::collapsed(0),
            group_size: ByteGroupSize::default(),
        }
    }
}

impl CursorModel {
    pub fn new(group_size: ByteGroupSize) -> Self {
        Self {
            offset: 0,
            selection: Selection::collapsed(0),
            group_size,
        }
    }

    /// Returns the selected half-open byte range.
    pub fn selection_range(&self, total_size: usize) -> Option<Range<usize>> {
        let selection = self.selection.clamped(total_size);
        let range = selection.range()?;
        (range.start < total_size).then_some(range.start..range.end.min(total_size))
    }

    /// Returns the current selection clamped to total size.
    pub fn selection(&self, total_size: usize) -> Selection {
        self.selection.clamped(total_size)
    }

    /// Returns whether at least one byte is selected.
    pub fn has_selection(&self, total_size: usize) -> bool {
        self.selection_range(total_size).is_some()
    }

    /// Returns true if the byte at `offset` is inside the current selection.
    pub fn is_selected(&self, offset: usize, total_size: usize) -> bool {
        if let Some(range) = self.selection_range(total_size) {
            range.contains(&offset)
        } else {
            false
        }
    }

    /// Replaces the selection with two half-open buffer boundaries.
    pub fn set_selection(&mut self, anchor: usize, active: usize, total_size: usize) {
        self.selection = Selection::new(anchor, active).clamped(total_size);
    }

    /// Selects `range` and places the overwrite cursor at its first byte.
    pub fn set_selection_range(&mut self, range: Range<usize>, total_size: usize) {
        let start = range.start.min(total_size);
        let end = range.end.min(total_size).max(start);
        self.selection = Selection::new(start, end);
        self.offset = start.min(total_size.saturating_sub(1));
    }

    /// Clears the selected bytes while preserving the current caret position.
    pub fn clear_selection(&mut self, total_size: usize) {
        self.selection = Selection::collapsed(self.offset.min(total_size));
    }

    /// Returns the insertion-boundary offset where a text-style caret should
    /// be painted for the current selection.
    pub fn insert_cursor_offset(&self, total_size: usize) -> usize {
        self.selection_range(total_size)
            .map_or(self.offset.min(total_size), |_| self.selection.active().min(total_size))
    }

    /// Returns the rightmost half-open boundary of the current selection, or the cursor offset.
    pub fn selection_right_boundary(&self, total_size: usize) -> usize {
        let Some(range) = self.selection_range(total_size) else {
            return self.offset.min(total_size);
        };
        range.end.min(total_size)
    }

    /// Returns the display-group aligned range for the current selection or cursor.
    pub fn selected_range_or_cursor(&self, total_size: usize) -> Option<Range<usize>> {
        if total_size == 0 {
            return None;
        }
        let group_bytes = self.group_size.byte_count();
        if let Some(range) = self.selection_range(total_size) {
            let s = ((range.start / group_bytes) * group_bytes).min(total_size);
            let e = range.end.div_ceil(group_bytes).saturating_mul(group_bytes).min(total_size);
            if s < e {
                return Some(s..e);
            }
        }
        let cur = self.offset.min(total_size.saturating_sub(1));
        let group_start = (cur / group_bytes) * group_bytes;
        let group_end = (group_start + group_bytes).min(total_size);
        Some(group_start..group_end)
    }

    /// Returns the exact half-open byte range affected by an edit.
    pub fn edit_range(&self, total_size: usize) -> Option<Range<usize>> {
        if total_size == 0 {
            return None;
        }
        if let Some(range) = self.selection_range(total_size) {
            return Some(range);
        }
        if self.offset >= total_size {
            return None;
        }
        let start = self.offset.min(total_size.saturating_sub(1));
        Some(start..start + 1)
    }

    /// Sets the cursor offset aligned to the current byte group step.
    pub fn set_cursor_offset(&mut self, offset: usize, total_size: usize) {
        let step = self.group_size.byte_count();
        self.offset = if offset >= total_size {
            total_size
        } else {
            let aligned = (offset / step) * step;
            aligned.min(total_size)
        };
        self.clear_selection(total_size);
    }

    /// Sets the exact cursor offset without byte group alignment.
    pub fn set_cursor_offset_exact(&mut self, offset: usize, total_size: usize) {
        self.offset = offset.min(total_size);
        self.clear_selection(total_size);
    }

    /// Captures a snapshot of the current cursor offset and selection.
    pub fn cursor_state(&self) -> CursorState {
        CursorState {
            cursor_offset: self.offset,
            selection: self.selection,
        }
    }

    /// Restores the cursor offset and selection from a snapshot.
    pub fn restore_cursor_state(&mut self, state: CursorState, total_size: usize) {
        self.offset = state.cursor_offset.min(total_size);
        self.selection = state.selection.clamped(total_size);
    }

    /// Updates the byte group size and realigns cursor and selection boundaries.
    pub fn set_group_size(&mut self, group_size: ByteGroupSize, total_size: usize) {
        let has_selection = self.has_selection(total_size);
        self.group_size = group_size;
        let step = group_size.byte_count();
        self.offset = if self.offset >= total_size { total_size } else { (self.offset / step) * step };
        let align_boundary = |offset: usize| if offset >= total_size { total_size } else { (offset / step) * step };
        self.selection = if has_selection {
            Selection::new(align_boundary(self.selection.anchor()), align_boundary(self.selection.active()))
        } else {
            Selection::collapsed(self.offset.min(total_size))
        };
    }

    /// Adjusts cursor offset and selection boundaries after an edit.
    pub fn adjust_after_edit(&mut self, start: usize, old_len: usize, new_len: usize, total_size: usize) {
        let old_end = start.saturating_add(old_len);
        let shift = |offset: usize| {
            if old_len == 0 {
                if offset >= start { offset.saturating_add(new_len) } else { offset }
            } else if offset <= start {
                offset
            } else if offset >= old_end {
                if new_len >= old_len {
                    offset.saturating_add(new_len - old_len)
                } else {
                    offset.saturating_sub(old_len - new_len)
                }
            } else {
                start.saturating_add(new_len)
            }
        };

        self.offset = shift(self.offset).min(total_size);
        self.selection = Selection::new(shift(self.selection.anchor()), shift(self.selection.active())).clamped(total_size);
    }

    /// Calculates the previous group boundary before `cursor`.
    pub fn previous_group_boundary(cursor: usize, total: usize, step: usize) -> usize {
        if cursor >= total && !cursor.is_multiple_of(step) {
            (cursor / step) * step
        } else {
            (cursor / step).saturating_sub(1) * step
        }
    }

    /// Calculates the next group boundary after `offset`.
    pub fn next_group_boundary(offset: usize, total: usize, step: usize) -> usize {
        if offset >= total {
            return total;
        }
        ((offset / step) + 1).saturating_mul(step).min(total)
    }

    pub fn calculate_down_offset(
        &self,
        offset: usize,
        is_insert_mode: bool,
        line_map: &LineMap,
        folded_regions: &BTreeMap<usize, usize>,
        total_size: usize,
    ) -> usize {
        let step = self.group_size.byte_count();
        let last_line_idx = line_map.len().saturating_sub(1);
        let last_line_start = line_map.get(last_line_idx).unwrap_or(0);
        let bytes_per_row = line_map.max_bytes_per_row();
        let is_eof_on_new_line = total_size > last_line_start && (total_size - last_line_start) >= bytes_per_row;
        let current_line_idx = LayoutEngine::find_line_index(offset, line_map);

        if let Some(next_idx) = LayoutEngine::next_data_line(current_line_idx, line_map, total_size, folded_regions) {
            let current_line_start = line_map.get(current_line_idx).expect("valid current line start");
            let offset_in_line = offset - current_line_start;
            let next_line_start = line_map.get(next_idx).expect("valid next line start");
            let next_line_end = if next_idx + 1 < line_map.len() {
                line_map.get(next_idx + 1).expect("valid next line end")
            } else {
                total_size
            };
            let next_line_len = next_line_end - next_line_start;

            if is_insert_mode {
                let col = (cmp::min(offset_in_line, next_line_len) / step) * step;
                (next_line_start + col).min(next_line_end)
            } else if next_line_len > 0 {
                let max_in_line = if next_line_end == total_size && !is_eof_on_new_line {
                    next_line_len
                } else {
                    next_line_len.saturating_sub(1)
                };
                let target_offset = next_line_start + cmp::min(offset_in_line, max_in_line);
                let aligned_offset = (target_offset / step) * step;
                aligned_offset.min(next_line_end)
            } else {
                next_line_start
            }
        } else if (is_eof_on_new_line && offset < total_size) || is_insert_mode {
            total_size
        } else {
            let max_offset = total_size;
            (max_offset / step) * step
        }
    }

    pub fn calculate_up_offset(
        &self,
        offset: usize,
        is_insert_mode: bool,
        line_map: &LineMap,
        folded_regions: &BTreeMap<usize, usize>,
        total_size: usize,
    ) -> usize {
        let step = self.group_size.byte_count();
        let last_line_idx = line_map.len().saturating_sub(1);
        let last_line_start = line_map.get(last_line_idx).unwrap_or(0);
        let bytes_per_row = line_map.max_bytes_per_row();
        let is_eof_on_new_line = total_size > last_line_start && (total_size - last_line_start) >= bytes_per_row;

        if is_eof_on_new_line && offset == total_size {
            let prev_line_start = last_line_start;
            let prev_line_end = total_size;
            let prev_line_len = prev_line_end - prev_line_start;
            let max_in_prev = if is_insert_mode { prev_line_len } else { prev_line_len.saturating_sub(1) };
            let aligned = (prev_line_start / step) * step;
            return aligned.min(prev_line_start + max_in_prev);
        }

        let current_line_idx = LayoutEngine::find_line_index(offset, line_map);

        if let Some(prev_idx) = LayoutEngine::prev_data_line(current_line_idx, line_map, folded_regions) {
            let current_line_start = line_map.get(current_line_idx).expect("valid current line start");
            let offset_in_line = offset - current_line_start;
            let prev_line_start = line_map.get(prev_idx).expect("valid prev line start");
            let prev_line_end = line_map.get(prev_idx + 1).expect("valid prev line end");
            let prev_line_len = prev_line_end - prev_line_start;

            if is_insert_mode {
                let col = (cmp::min(offset_in_line, prev_line_len) / step) * step;
                (prev_line_start + col).min(prev_line_end)
            } else {
                let target_offset = prev_line_start + cmp::min(offset_in_line, prev_line_len.saturating_sub(1));
                let aligned_offset = (target_offset / step) * step;
                aligned_offset.min(prev_line_end.saturating_sub(1))
            }
        } else {
            0
        }
    }

    /// Moves cursor to the left by one group, clearing any selection.
    pub fn move_left(&mut self, total_size: usize) {
        self.clear_selection(total_size);
        let step = self.group_size.byte_count();
        if self.offset > 0 {
            self.offset = Self::previous_group_boundary(self.offset, total_size, step);
        }
    }

    /// Moves cursor to the left in insert mode (collapsing selection to start if present).
    pub fn move_left_for_insert(&mut self, total_size: usize) {
        if let Some(range) = self.selection_range(total_size) {
            self.offset = range.start.min(total_size);
            self.clear_selection(total_size);
            return;
        }

        let step = self.group_size.byte_count();
        if self.offset > 0 {
            self.offset = Self::previous_group_boundary(self.offset, total_size, step);
            self.clear_selection(total_size);
        }
    }

    /// Moves cursor to the right in insert mode (collapsing selection to end if present).
    pub fn move_right_for_insert(&mut self, total_size: usize) {
        if self.has_selection(total_size) {
            self.offset = self.selection_right_boundary(total_size);
            self.clear_selection(total_size);
            return;
        }

        let step = self.group_size.byte_count();
        if self.offset >= total_size {
            return;
        }

        self.offset = Self::next_group_boundary(self.offset, total_size, step);
        self.clear_selection(total_size);
    }

    /// Moves cursor to the right by one group, clearing any selection.
    pub fn move_right(&mut self, total_size: usize) {
        self.clear_selection(total_size);
        let step = self.group_size.byte_count();
        let max_offset = total_size;
        let next = Self::next_group_boundary(self.offset, total_size, step);
        if next <= max_offset {
            self.offset = next;
        }
    }

    /// Moves cursor up by one line, clearing any selection.
    pub fn move_up(&mut self, line_map: &LineMap, folded_regions: &BTreeMap<usize, usize>, total_size: usize) {
        self.clear_selection(total_size);
        self.offset = self.calculate_up_offset(self.offset, false, line_map, folded_regions, total_size);
    }

    /// Moves cursor down by one line, clearing any selection.
    pub fn move_down(&mut self, line_map: &LineMap, folded_regions: &BTreeMap<usize, usize>, total_size: usize) {
        self.clear_selection(total_size);
        self.offset = self.calculate_down_offset(self.offset, false, line_map, folded_regions, total_size);
    }

    /// Moves cursor down in insert mode by one line, clearing any selection.
    pub fn move_down_for_insert(&mut self, line_map: &LineMap, folded_regions: &BTreeMap<usize, usize>, total_size: usize) {
        self.clear_selection(total_size);
        if self.offset >= total_size {
            return;
        }
        self.offset = self.calculate_down_offset(self.offset, true, line_map, folded_regions, total_size);
    }

    pub fn select_left(&mut self, total_size: usize) {
        let step = self.group_size.byte_count();
        if self.offset > 0 {
            let target = (self.offset / step).saturating_sub(1) * step;
            let anchor = if self.has_selection(total_size) {
                self.selection.anchor()
            } else {
                self.offset
            };
            self.offset = target;
            self.selection = Selection::new(anchor, target).clamped(total_size);
        }
    }

    pub fn select_left_for_insert(&mut self, total_size: usize) {
        let step = self.group_size.byte_count();
        let caret = if self.has_selection(total_size) {
            self.selection.active()
        } else {
            self.offset.min(total_size)
        };
        if caret == 0 {
            return;
        }

        let active = Self::previous_group_boundary(caret, total_size, step);
        let anchor = if self.has_selection(total_size) { self.selection.anchor() } else { caret };
        self.selection = Selection::new(anchor, active);
        self.offset = active;
    }

    pub fn select_right(&mut self, total_size: usize) {
        let step = self.group_size.byte_count();
        let next = ((self.offset / step).saturating_add(1)).saturating_mul(step).min(total_size);
        if self.offset < next {
            let anchor = if self.has_selection(total_size) {
                self.selection.anchor()
            } else {
                self.offset
            };
            self.offset = next;
            self.selection = Selection::new(anchor, next);
        }
    }

    pub fn select_right_for_insert(&mut self, total_size: usize) {
        let step = self.group_size.byte_count();
        let caret = if self.has_selection(total_size) {
            self.selection.active()
        } else {
            self.offset.min(total_size)
        };
        if caret >= total_size {
            return;
        }

        let active = Self::next_group_boundary(caret, total_size, step);
        let anchor = if self.has_selection(total_size) { self.selection.anchor() } else { caret };
        self.selection = Selection::new(anchor, active);
        self.offset = active;
    }

    pub fn select_up_for_insert(&mut self, line_map: &LineMap, folded_regions: &BTreeMap<usize, usize>, total_size: usize) {
        let caret = if self.has_selection(total_size) {
            self.selection.active().min(total_size)
        } else {
            self.offset.min(total_size)
        };
        let anchor = if self.has_selection(total_size) { self.selection.anchor() } else { caret };

        let active = self.calculate_up_offset(caret, true, line_map, folded_regions, total_size);
        self.offset = active;
        self.selection = Selection::new(anchor, active);
    }

    pub fn select_up(&mut self, line_map: &LineMap, folded_regions: &BTreeMap<usize, usize>, total_size: usize) {
        let anchor = if self.has_selection(total_size) {
            self.selection.anchor()
        } else {
            self.offset
        };
        self.offset = self.calculate_up_offset(self.offset, false, line_map, folded_regions, total_size);
        self.selection = Selection::new(anchor, self.offset);
    }

    pub fn select_down_for_insert(&mut self, line_map: &LineMap, folded_regions: &BTreeMap<usize, usize>, total_size: usize) {
        let caret = if self.has_selection(total_size) {
            self.selection.active().min(total_size)
        } else {
            self.offset.min(total_size)
        };
        let anchor = if self.has_selection(total_size) { self.selection.anchor() } else { caret };

        let active = self.calculate_down_offset(caret, true, line_map, folded_regions, total_size);
        self.offset = active;
        self.selection = Selection::new(anchor, active);
    }

    pub fn select_down(&mut self, line_map: &LineMap, folded_regions: &BTreeMap<usize, usize>, total_size: usize) {
        let anchor = if self.has_selection(total_size) {
            self.selection.anchor()
        } else {
            self.offset
        };
        self.offset = self.calculate_down_offset(self.offset, false, line_map, folded_regions, total_size);
        self.selection = Selection::new(anchor, self.offset.min(total_size));
    }

    pub fn select_all(&mut self, total_size: usize) {
        self.selection = Selection::new(0, total_size);
        self.offset = total_size;
    }

    pub fn go_to_beginning(&mut self, total_size: usize) {
        self.offset = 0;
        self.clear_selection(total_size);
    }

    pub fn go_to_end(&mut self, total_size: usize) {
        let step = self.group_size.byte_count();
        let max_offset = total_size;
        self.offset = (max_offset / step) * step;
        self.clear_selection(total_size);
    }

    pub fn go_to_offset(&mut self, offset: usize, extend_selection: bool, total_size: usize) {
        let target = offset.min(total_size);
        if extend_selection {
            let anchor = if self.has_selection(total_size) {
                self.selection.anchor()
            } else {
                self.offset
            };
            self.offset = target;
            self.set_selection(anchor, target, total_size);
        } else {
            self.set_cursor_offset_exact(target, total_size);
            self.clear_selection(total_size);
        }
    }

    pub fn page_up(&mut self, visible_rows: usize, line_map: &LineMap, total_size: usize) {
        let step = self.group_size.byte_count();
        let current_line_idx = LayoutEngine::find_line_index(self.offset, line_map);

        let target_line_idx = current_line_idx.saturating_sub(visible_rows);
        let current_line_start = line_map.get(current_line_idx).expect("valid current line start");
        let offset_in_line = self.offset - current_line_start;

        let target_line_start = line_map.get(target_line_idx).expect("valid target line start");
        let target_line_end = if target_line_idx + 1 < line_map.len() {
            line_map.get(target_line_idx + 1).expect("valid target line end")
        } else {
            total_size
        };
        let target_line_len = target_line_end - target_line_start;

        let target_offset = target_line_start + cmp::min(offset_in_line, target_line_len.saturating_sub(1));
        let aligned_offset = (target_offset / step) * step;
        self.offset = aligned_offset.min(target_line_end.saturating_sub(1));
        self.clear_selection(total_size);
    }

    pub fn page_down(&mut self, visible_rows: usize, line_map: &LineMap, total_size: usize) {
        let step = self.group_size.byte_count();
        let current_line_idx = LayoutEngine::find_line_index(self.offset, line_map);

        let target_line_idx = cmp::min(current_line_idx + visible_rows, line_map.len() - 1);
        let current_line_start = line_map.get(current_line_idx).expect("valid current line start");
        let offset_in_line = self.offset - current_line_start;

        let target_line_start = line_map.get(target_line_idx).expect("valid target line start");
        let target_line_end = if target_line_idx + 1 < line_map.len() {
            line_map.get(target_line_idx + 1).expect("valid target line end")
        } else {
            total_size
        };
        let target_line_len = target_line_end - target_line_start;

        if target_line_idx == line_map.len() - 1 && target_line_len == 0 {
            let max_offset = total_size;
            self.offset = (max_offset / step) * step;
        } else {
            let target_offset = target_line_start + cmp::min(offset_in_line, target_line_len.saturating_sub(1));
            let aligned_offset = (target_offset / step) * step;
            self.offset = aligned_offset.min(target_line_end.saturating_sub(1));
        }
        self.clear_selection(total_size);
    }

    pub fn home(&mut self, total_size: usize) {
        self.offset = 0;
        self.clear_selection(total_size);
    }

    pub fn end(&mut self, total_size: usize) {
        let step = self.group_size.byte_count();
        let max_offset = total_size;
        self.offset = (max_offset / step) * step;
        self.clear_selection(total_size);
    }

    pub fn select_page_up(&mut self, visible_rows: usize, line_map: &LineMap, total_size: usize) {
        let step = self.group_size.byte_count();
        let current_line_idx = LayoutEngine::find_line_index(self.offset, line_map);
        let anchor = if self.has_selection(total_size) {
            self.selection.anchor()
        } else {
            self.offset
        };

        let target_line_idx = current_line_idx.saturating_sub(visible_rows);
        let current_line_start = line_map.get(current_line_idx).expect("valid current line start");
        let offset_in_line = self.offset - current_line_start;

        let target_line_start = line_map.get(target_line_idx).expect("valid target line start");
        let target_line_end = if target_line_idx + 1 < line_map.len() {
            line_map.get(target_line_idx + 1).expect("valid target line end")
        } else {
            total_size
        };
        let target_line_len = target_line_end - target_line_start;

        let target_offset = target_line_start + cmp::min(offset_in_line, target_line_len.saturating_sub(1));
        let aligned_offset = (target_offset / step) * step;
        self.offset = aligned_offset.min(target_line_end.saturating_sub(1));
        self.selection = Selection::new(anchor, self.offset);
    }

    pub fn select_page_down(&mut self, visible_rows: usize, line_map: &LineMap, total_size: usize) {
        let step = self.group_size.byte_count();
        let current_line_idx = LayoutEngine::find_line_index(self.offset, line_map);
        let anchor = if self.has_selection(total_size) {
            self.selection.anchor()
        } else {
            self.offset
        };

        let target_line_idx = cmp::min(current_line_idx + visible_rows, line_map.len() - 1);
        let current_line_start = line_map.get(current_line_idx).expect("valid current line start");
        let offset_in_line = self.offset - current_line_start;

        let target_line_start = line_map.get(target_line_idx).expect("valid target line start");
        let target_line_end = if target_line_idx + 1 < line_map.len() {
            line_map.get(target_line_idx + 1).expect("valid target line end")
        } else {
            total_size
        };
        let target_line_len = target_line_end - target_line_start;

        if target_line_idx == line_map.len() - 1 && target_line_len == 0 {
            let max_offset = total_size.saturating_sub(1);
            self.offset = (max_offset / step) * step;
        } else {
            let target_offset = target_line_start + cmp::min(offset_in_line, target_line_len.saturating_sub(1));
            let aligned_offset = (target_offset / step) * step;
            self.offset = aligned_offset.min(target_line_end.saturating_sub(1));
        }
        self.selection = Selection::new(anchor, self.offset.min(total_size));
    }

    pub fn select_home_for_insert(&mut self, total_size: usize) {
        let anchor = if self.has_selection(total_size) {
            self.selection.anchor()
        } else {
            self.offset.min(total_size)
        };
        self.offset = 0;
        self.selection = Selection::new(anchor, 0);
    }

    pub fn select_home(&mut self, total_size: usize) {
        let anchor = if self.has_selection(total_size) {
            self.selection.anchor()
        } else {
            self.offset
        };
        self.offset = 0;
        self.selection = Selection::new(anchor, 0);
    }

    pub fn select_end(&mut self, total_size: usize) {
        let step = self.group_size.byte_count();
        let anchor = if self.has_selection(total_size) {
            self.selection.anchor()
        } else {
            self.offset
        };
        let max_offset = total_size.saturating_sub(1);
        self.offset = (max_offset / step) * step;
        self.selection = Selection::new(anchor, self.offset.saturating_add(1).min(total_size));
    }

    pub fn select_end_for_insert(&mut self, total_size: usize) {
        let anchor = if self.has_selection(total_size) {
            self.selection.anchor()
        } else {
            self.offset.min(total_size)
        };
        self.offset = total_size;
        self.selection = Selection::new(anchor, total_size);
    }

    pub fn start_drag(&mut self, byte_pos: usize, total_size: usize) {
        let step = self.group_size.byte_count();
        let aligned = (byte_pos / step) * step;
        self.offset = aligned.min(total_size);
        self.selection = Selection::collapsed(self.offset);
    }

    pub fn continue_drag(&mut self, anchor_pos: usize, byte_pos: usize, total_size: usize) {
        let step = self.group_size.byte_count();
        let aligned_anchor = (anchor_pos / step) * step;
        let cursor_offset = if byte_pos >= total_size { total_size } else { (byte_pos / step) * step };
        self.offset = cursor_offset;

        let (anchor, active) = if cursor_offset >= aligned_anchor {
            (aligned_anchor.min(total_size), cursor_offset.saturating_add(step).min(total_size))
        } else {
            (aligned_anchor.saturating_add(step).min(total_size), cursor_offset.min(total_size))
        };
        self.selection = Selection::new(anchor, active);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_model_initialization_and_movement() {
        let mut model = CursorModel::default();
        let total = 100;
        assert_eq!(model.offset, 0);
        assert!(!model.has_selection(total));

        model.move_right(total);
        assert_eq!(model.offset, 1);

        model.move_left(total);
        assert_eq!(model.offset, 0);

        // Clamping at boundaries
        model.move_left(total);
        assert_eq!(model.offset, 0);

        model.end(total);
        assert_eq!(model.offset, 100);
        model.move_right(total);
        assert_eq!(model.offset, 100);

        model.home(total);
        assert_eq!(model.offset, 0);
    }

    #[test]
    fn test_cursor_model_group_size_alignment() {
        let mut model = CursorModel::default();
        let total = 64;
        model.set_cursor_offset(7, total);
        assert_eq!(model.offset, 7);

        // Switch to 4-byte groups -> snaps to 4
        model.set_group_size(ByteGroupSize::Four, total);
        assert_eq!(model.offset, 4);

        // Move right -> moves by 4
        model.move_right(total);
        assert_eq!(model.offset, 8);

        // Move left -> moves by 4
        model.move_left(total);
        assert_eq!(model.offset, 4);
    }

    #[test]
    fn test_cursor_model_selection_and_adjust_after_edit() {
        let mut model = CursorModel::default();
        let total = 50;

        model.set_selection(10, 20, total);
        assert!(model.has_selection(total));
        assert_eq!(model.selection_range(total), Some(10..20));

        // Shift selection after insert of 5 bytes at offset 5
        model.adjust_after_edit(5, 0, 5, total + 5);
        assert_eq!(model.selection_range(total + 5), Some(15..25));

        model.clear_selection(total + 5);
        assert!(!model.has_selection(total + 5));
    }
}
