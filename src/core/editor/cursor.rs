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
        let shift = |offset: usize| crate::core::document::shift_offset_after_edit(offset, start, old_len, new_len);

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
        let current = self.offset.min(total_size);
        if current > 0 {
            let target = Self::previous_group_boundary(current, total_size, step);
            let anchor = if self.has_selection(total_size) { self.selection.anchor() } else { current };
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
        if line_map.is_empty() {
            return;
        }
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
        if line_map.is_empty() {
            return;
        }
        let step = self.group_size.byte_count();
        let current_line_idx = LayoutEngine::find_line_index(self.offset, line_map);
        let last_line_idx = line_map.len().saturating_sub(1);

        let target_line_idx = cmp::min(current_line_idx.saturating_add(visible_rows), last_line_idx);
        let current_line_start = line_map.get(current_line_idx).expect("valid current line start");
        let offset_in_line = self.offset - current_line_start;

        let target_line_start = line_map.get(target_line_idx).expect("valid target line start");
        let target_line_end = if target_line_idx + 1 < line_map.len() {
            line_map.get(target_line_idx + 1).expect("valid target line end")
        } else {
            total_size
        };
        let target_line_len = target_line_end - target_line_start;

        if target_line_idx == last_line_idx && target_line_len == 0 {
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
        if line_map.is_empty() {
            return;
        }
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
        if line_map.is_empty() {
            return;
        }
        let step = self.group_size.byte_count();
        let current_line_idx = LayoutEngine::find_line_index(self.offset, line_map);
        let anchor = if self.has_selection(total_size) {
            self.selection.anchor()
        } else {
            self.offset
        };

        let last_line_idx = line_map.len().saturating_sub(1);
        let target_line_idx = cmp::min(current_line_idx.saturating_add(visible_rows), last_line_idx);
        let current_line_start = line_map.get(current_line_idx).expect("valid current line start");
        let offset_in_line = self.offset - current_line_start;

        let target_line_start = line_map.get(target_line_idx).expect("valid target line start");
        let target_line_end = if target_line_idx + 1 < line_map.len() {
            line_map.get(target_line_idx + 1).expect("valid target line end")
        } else {
            total_size
        };
        let target_line_len = target_line_end - target_line_start;

        if target_line_idx == last_line_idx && target_line_len == 0 {
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

    /// Returns `(line_start, line_end)` half-open bounds of the visual row containing `offset`.
    pub fn visual_row_bounds(offset: usize, line_map: &LineMap, total_size: usize) -> Option<(usize, usize)> {
        if line_map.is_empty() {
            return None;
        }
        let line_idx = LayoutEngine::find_line_index(offset.min(total_size), line_map);
        let line_start = line_map.get(line_idx)?;
        let line_end = if line_idx + 1 < line_map.len() {
            line_map.get(line_idx + 1).unwrap_or(total_size)
        } else {
            total_size
        };
        Some((line_start.min(total_size), line_end.min(total_size)))
    }

    /// Moves cursor to the first byte of the current visual row.
    pub fn move_to_line_start(&mut self, line_map: &LineMap, total_size: usize) {
        if let Some((line_start, _)) = Self::visual_row_bounds(self.offset, line_map, total_size) {
            self.offset = line_start;
        } else {
            self.offset = 0;
        }
        self.clear_selection(total_size);
    }

    /// Moves cursor to the end of the current visual row.
    /// In Overwrite mode, lands on the last byte (or group-aligned start) of the row.
    /// In Insert mode, lands at the row's exclusive end boundary.
    pub fn move_to_line_end(&mut self, is_insert_mode: bool, line_map: &LineMap, total_size: usize) {
        if let Some((line_start, line_end)) = Self::visual_row_bounds(self.offset, line_map, total_size) {
            if is_insert_mode {
                self.offset = line_end;
            } else if line_end > line_start {
                let step = self.group_size.byte_count();
                let last_byte = line_end.saturating_sub(1);
                let aligned = (last_byte / step) * step;
                self.offset = aligned.max(line_start);
            } else {
                self.offset = line_start;
            }
        } else {
            self.offset = if is_insert_mode { total_size } else { total_size.saturating_sub(1) };
        }
        self.clear_selection(total_size);
    }

    /// Extends selection to the start of the current visual row.
    pub fn select_to_line_start(&mut self, is_insert_mode: bool, line_map: &LineMap, total_size: usize) {
        let active_pos = if self.has_selection(total_size) {
            self.selection.active()
        } else {
            self.offset.min(total_size)
        };
        let lookup = if active_pos > 0 && active_pos == self.selection.active() && self.has_selection(total_size) && active_pos > self.selection.anchor() {
            active_pos.saturating_sub(1)
        } else {
            active_pos
        };
        let line_start = Self::visual_row_bounds(lookup, line_map, total_size).map(|(s, _)| s).unwrap_or(0);
        let anchor = if self.has_selection(total_size) {
            self.selection.anchor()
        } else if is_insert_mode {
            self.offset.min(total_size)
        } else {
            let step = self.group_size.byte_count();
            if self.offset < total_size {
                self.offset.saturating_add(step).min(total_size)
            } else {
                total_size
            }
        };
        self.offset = line_start;
        self.selection = Selection::new(anchor, line_start);
    }

    /// Extends selection to the end of the current visual row.
    pub fn select_to_line_end(&mut self, is_insert_mode: bool, line_map: &LineMap, total_size: usize) {
        let active_pos = if self.has_selection(total_size) {
            self.selection.active()
        } else {
            self.offset.min(total_size)
        };
        let lookup = if active_pos > 0 && active_pos == self.selection.active() && self.has_selection(total_size) && active_pos > self.selection.anchor() {
            active_pos.saturating_sub(1)
        } else {
            active_pos
        };
        let (line_start, line_end) = Self::visual_row_bounds(lookup, line_map, total_size).unwrap_or((0, total_size));
        let anchor = if self.has_selection(total_size) {
            self.selection.anchor()
        } else {
            self.offset.min(total_size)
        };
        self.offset = if is_insert_mode {
            line_end
        } else if line_end > line_start {
            line_end.saturating_sub(1)
        } else {
            line_start
        };
        self.selection = Selection::new(anchor, line_end);
    }

    /// Returns the jump step in bytes for group-jump navigation (`Alt/Ctrl+Left/Right`).
    pub fn group_jump_step(&self) -> usize {
        if self.group_size == ByteGroupSize::One {
            4
        } else {
            self.group_size.byte_count()
        }
    }

    /// Moves cursor backward to the previous group boundary (4-byte word when 1-byte grouping is active).
    pub fn move_group_left(&mut self, total_size: usize) {
        let step = self.group_jump_step();
        self.offset = if self.offset > 0 && !self.offset.is_multiple_of(step) {
            (self.offset / step) * step
        } else {
            Self::previous_group_boundary(self.offset, total_size, step)
        };
        self.clear_selection(total_size);
    }

    /// Moves cursor forward to the next group boundary (4-byte word when 1-byte grouping is active).
    pub fn move_group_right(&mut self, is_insert_mode: bool, total_size: usize) {
        let step = self.group_jump_step();
        let next = Self::next_group_boundary(self.offset, total_size, step);
        self.offset = if is_insert_mode {
            next
        } else if total_size == 0 {
            0
        } else {
            next.min(total_size.saturating_sub(1))
        };
        self.clear_selection(total_size);
    }

    /// Extends selection backward to the previous group boundary.
    pub fn select_group_left(&mut self, is_insert_mode: bool, total_size: usize) {
        let step = self.group_jump_step();
        let (anchor, current_active) = if self.has_selection(total_size) {
            (self.selection.anchor(), self.selection.active())
        } else {
            let initial_anchor = if is_insert_mode {
                self.offset.min(total_size)
            } else {
                self.offset.saturating_add(self.group_size.byte_count()).min(total_size)
            };
            (initial_anchor, self.offset.min(total_size))
        };
        let target = if current_active > 0 && !current_active.is_multiple_of(step) {
            (current_active / step) * step
        } else {
            Self::previous_group_boundary(current_active, total_size, step)
        };
        self.offset = target;
        self.selection = Selection::new(anchor, target);
    }

    /// Extends selection forward to the next group boundary.
    pub fn select_group_right(&mut self, _is_insert_mode: bool, total_size: usize) {
        let step = self.group_jump_step();
        let (anchor, current_active) = if self.has_selection(total_size) {
            (self.selection.anchor(), self.selection.active())
        } else {
            (self.offset.min(total_size), self.offset.min(total_size))
        };
        let target = Self::next_group_boundary(current_active, total_size, step);
        self.offset = target.min(total_size);
        self.selection = Selection::new(anchor, target);
    }
}

/// Returns the half-open byte range for double-clicking a byte in the Hex column:
/// selects the active byte group (or 4-byte aligned dword when 1-byte grouping is active).
pub fn hex_double_click_range(offset: usize, group_size: ByteGroupSize, total_size: usize) -> Option<Range<usize>> {
    if total_size == 0 || offset >= total_size {
        return None;
    }
    let step = if group_size == ByteGroupSize::One { 4 } else { group_size.byte_count() };
    let start = (offset / step) * step;
    let end = (start + step).min(total_size);
    (start < end).then_some(start..end)
}

/// Returns the half-open byte range for double-clicking in the ASCII column:
/// selects a contiguous word (`[A-Za-z0-9_]`), contiguous whitespace, or the clicked character.
pub fn ascii_word_range(data: &[u8], offset: usize) -> Option<Range<usize>> {
    let total = data.len();
    if total == 0 || offset >= total {
        return None;
    }
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let is_space = |b: u8| b == b' ' || b == b'\t';
    let b = data[offset];
    if is_word(b) {
        let mut start = offset;
        while start > 0 && is_word(data[start - 1]) {
            start -= 1;
        }
        let mut end = offset + 1;
        while end < total && is_word(data[end]) {
            end += 1;
        }
        Some(start..end)
    } else if is_space(b) {
        let mut start = offset;
        while start > 0 && is_space(data[start - 1]) {
            start -= 1;
        }
        let mut end = offset + 1;
        while end < total && is_space(data[end]) {
            end += 1;
        }
        Some(start..end)
    } else {
        Some(offset..(offset + 1).min(total))
    }
}

/// Resolves half-open `(anchor, active)` selection endpoints for Shift+click in the ASCII column
/// without the +1 off-by-one error when `char_end` is already an exclusive boundary.
pub fn ascii_shift_click_selection(anchor: usize, char_start: usize, char_end: usize, total_size: usize) -> (usize, usize) {
    let clamped_anchor = anchor.min(total_size);
    let clamped_start = char_start.min(total_size);
    let clamped_end = char_end.min(total_size).max(clamped_start);
    if clamped_start >= clamped_anchor {
        (clamped_anchor, clamped_end)
    } else {
        (clamped_anchor, clamped_start)
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

    #[test]
    fn test_cursor_model_select_left_unaligned_eof_matches_move_left() {
        let total = 10;
        let mut sel_model = CursorModel::new(ByteGroupSize::Four);
        sel_model.set_cursor_offset(total, total);
        sel_model.select_left(total);
        assert_eq!(sel_model.offset, 8);
        assert_eq!(sel_model.selection_range(total), Some(8..10));

        let mut move_model = CursorModel::new(ByteGroupSize::Four);
        move_model.set_cursor_offset(total, total);
        move_model.move_left(total);
        assert_eq!(sel_model.offset, move_model.offset);

        sel_model.select_left(total);
        assert_eq!(sel_model.offset, 4);
        assert_eq!(sel_model.selection_range(total), Some(4..10));
    }

    #[test]
    fn test_cursor_model_page_navigation_overflow_and_empty_line_map() {
        use crate::core::layout::SparseLineMap;
        use std::sync::Arc;

        let line_map = LineMap::Standard {
            total_size: 48,
            bytes_per_row: 16,
        };
        let mut model = CursorModel::default();
        model.page_down(usize::MAX, &line_map, 48);
        assert_eq!(model.offset, 32);

        let mut sel_model = CursorModel::default();
        sel_model.select_page_down(usize::MAX, &line_map, 48);
        assert_eq!(sel_model.offset, 32);
        assert_eq!(sel_model.selection_range(48), Some(0..32));

        let empty_map = LineMap::Sparse(Arc::new(SparseLineMap {
            segments: Vec::new(),
            total_lines: 0,
            total_size: 0,
            max_bytes_per_row: 16,
            bytes_per_row: 16,
        }));
        let mut empty_model = CursorModel::default();
        empty_model.page_up(10, &empty_map, 0);
        empty_model.page_down(10, &empty_map, 0);
        empty_model.select_page_up(10, &empty_map, 0);
        empty_model.select_page_down(10, &empty_map, 0);
        assert_eq!(empty_model.offset, 0);
    }

    #[test]
    fn test_cursor_model_line_home_end_and_select() {
        let line_map = LineMap::Standard {
            total_size: 40,
            bytes_per_row: 16,
        };
        let mut model = CursorModel::default();
        model.set_cursor_offset_exact(21, 40);

        model.move_to_line_start(&line_map, 40);
        assert_eq!(model.offset, 16);

        model.move_to_line_end(false, &line_map, 40);
        assert_eq!(model.offset, 31);

        model.move_to_line_end(true, &line_map, 40);
        assert_eq!(model.offset, 32);

        // Select from middle of row 1 to row start (overwrite mode includes clicked byte)
        model.set_cursor_offset_exact(20, 40);
        model.select_to_line_start(false, &line_map, 40);
        assert_eq!(model.offset, 16);
        assert_eq!(model.selection_range(40), Some(16..21));

        // Select from middle of row 1 to row end
        model.set_cursor_offset_exact(20, 40);
        model.select_to_line_end(false, &line_map, 40);
        assert_eq!(model.offset, 31);
        assert_eq!(model.selection_range(40), Some(20..32));
    }

    #[test]
    fn test_cursor_model_group_jump_and_select() {
        let mut model = CursorModel::default(); // 1-byte grouping -> jumps by 4
        model.set_cursor_offset_exact(2, 32);
        model.move_group_right(false, 32);
        assert_eq!(model.offset, 4);
        model.move_group_right(false, 32);
        assert_eq!(model.offset, 8);
        model.move_group_left(32);
        assert_eq!(model.offset, 4);

        model.set_cursor_offset_exact(6, 32);
        model.move_group_left(32);
        assert_eq!(model.offset, 4);

        // Select group right from 4 -> 4..8, then 4..12
        model.select_group_right(false, 32);
        assert_eq!(model.selection_range(32), Some(4..8));
        model.select_group_right(false, 32);
        assert_eq!(model.selection_range(32), Some(4..12));
    }

    #[test]
    fn test_double_click_and_ascii_shift_click_helpers() {
        assert_eq!(hex_double_click_range(5, ByteGroupSize::One, 16), Some(4..8));
        assert_eq!(hex_double_click_range(5, ByteGroupSize::Two, 16), Some(4..6));
        assert_eq!(hex_double_click_range(14, ByteGroupSize::Four, 15), Some(12..15));

        let data = b"hello_world  123!";
        assert_eq!(ascii_word_range(data, 2), Some(0..11));
        assert_eq!(ascii_word_range(data, 11), Some(11..13));
        assert_eq!(ascii_word_range(data, 16), Some(16..17));

        // Forward ASCII Shift+click from anchor 2 to single-byte char [5..6) => (2, 6), NOT (2, 7)
        assert_eq!(ascii_shift_click_selection(2, 5, 6, 10), (2, 6));
        // Forward ASCII Shift+click to 3-byte UTF-8 char [5..8) => (2, 8)
        assert_eq!(ascii_shift_click_selection(2, 5, 8, 10), (2, 8));
        // Backward ASCII Shift+click from anchor 8 to char [2..3) => (8, 2)
        assert_eq!(ascii_shift_click_selection(8, 2, 3, 10), (8, 2));
    }
}
