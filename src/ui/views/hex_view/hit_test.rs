use super::HexView;
use super::layout::*;
use super::types::*;
use crate::app_state::InsertModeState;
use crate::core::radix::{DisplayRadix, HexCase};
use gpui_kit::{App, Pixels, Point, Window, px};

impl HexView {
    pub(super) fn edit_target_from_point(&self, point: Point<Pixels>, window: &Window, cx: &App) -> Option<EditTarget> {
        let insert_mode = InsertModeState::is_enabled(cx);
        let offset = self.offset_from_point(point, window, cx)?;
        let list_bounds = self.list_bounds.get()?;
        let editor = self.editor.read(cx);
        let doc = editor.document.read().ok()?;
        let line_starts = editor.line_starts();
        let rel_y = f32::from(point.y - list_bounds.top()).max(0.0);
        let top_overscroll_px = self.scroll.top_overscroll_rows as f32 * ROW_HEIGHT;
        if rel_y < top_overscroll_px {
            return None;
        }
        let row_offset_in_view = ((rel_y - top_overscroll_px) / ROW_HEIGHT).floor() as usize;
        let row_idx = self.scroll.scroll_offset + row_offset_in_view;
        let total_size = doc.buffer.len();
        let bytes_per_row = line_starts.max_bytes_per_row();
        let last_line_idx = line_starts.len().saturating_sub(1);
        let last_line_start = line_starts.get(last_line_idx).unwrap_or(0);
        let is_eof_on_new_line = total_size > last_line_start && (total_size - last_line_start) >= bytes_per_row;
        let is_eof_row = row_idx == line_starts.len() && is_eof_on_new_line;
        if row_idx > line_starts.len() || (row_idx == line_starts.len() && !is_eof_row) {
            return None;
        }
        let line_offset = if is_eof_row { total_size } else { line_starts.get(row_idx)? };
        let next_offset = if is_eof_row {
            total_size
        } else {
            line_starts.get(row_idx + 1).unwrap_or(doc.buffer.len())
        };
        let chunk_len = next_offset.saturating_sub(line_offset);
        if !is_eof_row && editor.is_folded(line_offset) {
            return None;
        }
        if chunk_len == 0 {
            let parse_result = editor.parse_result();
            let is_struct_mode = editor.structure.show_inline_structure_view && parse_result.is_some();
            let base_x = f32::from(list_bounds.left()) + 8.0;
            let layout = self.current_layout(cx);
            let relative_x = f32::from(point.x) - base_x;
            if relative_x >= layout.fixed_width {
                let world_x = relative_x + self.scroll.outer_scroll_x;
                if !is_struct_mode
                    && let Some(ascii_column) = layout.ascii
                    && world_x >= ascii_column.start
                    && world_x <= ascii_column.end()
                {
                    return Some(EditTarget::Ascii { offset: line_offset });
                }
                if world_x >= layout.hex.start && world_x <= layout.hex.end() {
                    return Some(EditTarget::Hex {
                        offset: line_offset,
                        nibble: 0,
                    });
                }
            }
            return None;
        }

        let parse_result = editor.parse_result();
        let is_struct_mode = editor.structure.show_inline_structure_view && parse_result.is_some();
        let base_x = f32::from(list_bounds.left()) + 8.0;
        let layout = self.current_layout(cx);
        let relative_x = f32::from(point.x) - base_x;
        if relative_x < layout.fixed_width {
            return None;
        }
        let world_x = relative_x + self.scroll.outer_scroll_x;

        if !is_struct_mode
            && let Some(ascii_column) = layout.ascii
            && world_x >= ascii_column.start
            && world_x < ascii_column.end()
        {
            let raw_idx = ascii_byte_index_from_world_x(world_x, ascii_column, self.scroll.ascii_scroll_x);
            if raw_idx < chunk_len {
                let abs_offset = line_offset + raw_idx;
                let char_range = self.encoding.char_range_at(doc.buffer.data(), abs_offset);
                return Some(EditTarget::Ascii { offset: char_range.start });
            }
            let is_last_row = row_idx + 1 == line_starts.len();
            if is_last_row && raw_idx >= chunk_len {
                return Some(EditTarget::Ascii {
                    offset: line_offset + chunk_len,
                });
            }
            if !insert_mode && chunk_len > 0 && raw_idx >= chunk_len {
                let abs_offset = line_offset + chunk_len - 1;
                let char_range = self.encoding.char_range_at(doc.buffer.data(), abs_offset);
                return Some(EditTarget::Ascii { offset: char_range.start });
            }
            return None;
        }

        if world_x < layout.hex.start || world_x > layout.hex.end() {
            return None;
        }

        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
        let source = build_hex_text_source(
            doc.buffer.get_range(line_offset, chunk_len),
            line_offset,
            self.radix,
            self.group_size,
            self.is_big_endian,
            hex_case,
        );
        let cell_width = px(self.hex_cell_width.max(1.0));
        let origin_x = px(base_x + layout.hex.start - self.scroll.outer_scroll_x - self.scroll.hex_scroll_x);
        let mut selected_group = None;
        for (index, group) in source.groups.iter().enumerate() {
            let (group_start, group_end) = hex_group_x(*group, origin_x, cell_width);
            let next_start = source
                .groups
                .get(index + 1)
                .map(|next| origin_x + hex_grid_x(next.text_start, cell_width))
                .unwrap_or(group_end);
            if point.x >= group_start && point.x < next_start {
                selected_group = Some(*group);
                break;
            }
        }
        if selected_group.is_none()
            && let Some(last_group) = source.groups.last().copied()
        {
            let is_last_row = row_idx + 1 == line_starts.len();
            let (_, last_group_end) = hex_group_x(last_group, origin_x, cell_width);
            if is_last_row && point.x >= last_group_end && world_x <= layout.hex.end() {
                return Some(EditTarget::Hex {
                    offset: line_offset + chunk_len,
                    nibble: 0,
                });
            }
        }
        let group = selected_group.or_else(|| {
            source
                .groups
                .iter()
                .find(|group| group.chunk_start <= offset.saturating_sub(line_offset) && offset.saturating_sub(line_offset) < group.chunk_end)
                .copied()
        })?;

        let (group_start, _) = hex_group_x(group, origin_x, cell_width);
        let text_index = ((f32::from(point.x - group_start) / f32::from(cell_width)).floor() as usize)
            .min(group.text_end.saturating_sub(group.text_start).saturating_sub(1));
        let display_byte_index = if self.radix == DisplayRadix::Hexadecimal { text_index / 2 } else { 0 };
        let group_len = group.chunk_end.saturating_sub(group.chunk_start);
        let local_byte_index = if self.radix == DisplayRadix::Hexadecimal && !self.is_big_endian && group_len > 1 {
            group_len.saturating_sub(display_byte_index + 1)
        } else {
            display_byte_index.min(group_len.saturating_sub(1))
        };
        let target_offset = line_offset + group.chunk_start + local_byte_index.min(group_len.saturating_sub(1));
        let nibble = if self.radix == DisplayRadix::Hexadecimal { (text_index % 2) as u8 } else { 0 };
        Some(EditTarget::Hex { offset: target_offset, nibble })
    }

    pub(super) fn offset_from_point(&self, point: Point<Pixels>, _window: &Window, cx: &App) -> Option<usize> {
        let root_bounds = self.bounds.get()?;
        let header_h = if self.show_header { HEADER_HEIGHT } else { 0.0 };

        if point.x < root_bounds.left() || point.y < root_bounds.top() + px(header_h) {
            return None;
        }

        let list_bounds = self.list_bounds.get()?;

        let editor = self.editor.read(cx);
        let doc = editor.document.read().ok()?;
        let buffer_len = doc.buffer.len();
        if buffer_len == 0 {
            return Some(0);
        }
        let line_starts = editor.line_starts();
        if line_starts.is_empty() {
            return Some(0);
        }

        let rel_y = f32::from(point.y - list_bounds.top()).max(0.0);
        let top_overscroll_px = self.scroll.top_overscroll_rows as f32 * ROW_HEIGHT;
        if rel_y < top_overscroll_px {
            return None;
        }
        let row_offset_in_view = ((rel_y - top_overscroll_px) / ROW_HEIGHT).floor() as usize;
        let row_idx = self.scroll.scroll_offset + row_offset_in_view;
        let total_size = buffer_len;
        let bytes_per_row = line_starts.max_bytes_per_row();
        let last_line_idx = line_starts.len().saturating_sub(1);
        let last_line_start = line_starts.get(last_line_idx).unwrap_or(0);
        let is_eof_on_new_line = total_size > last_line_start && (total_size - last_line_start) >= bytes_per_row;
        let is_eof_row = row_idx == line_starts.len() && is_eof_on_new_line;
        if row_idx > line_starts.len() || (row_idx == line_starts.len() && !is_eof_row) {
            return None;
        }
        let line_offset = if is_eof_row { total_size } else { line_starts.get(row_idx)? };
        if !is_eof_row && editor.is_folded(line_offset) {
            return Some(line_offset);
        }

        let next_offset = if is_eof_row {
            total_size
        } else if row_idx + 1 < line_starts.len() {
            line_starts.get(row_idx + 1).unwrap_or(buffer_len)
        } else {
            buffer_len
        };
        let chunk_len = next_offset.saturating_sub(line_offset);
        if chunk_len == 0 {
            return Some(line_offset);
        }

        let parse_result = editor.parse_result();
        let is_struct_mode = editor.structure.show_inline_structure_view && parse_result.is_some();
        let base_x = f32::from(list_bounds.left()) + 8.0;
        let layout = self.current_layout(cx);
        let relative_x = f32::from(point.x) - base_x;
        if relative_x < layout.fixed_width {
            return Some(line_offset);
        }
        let world_x = relative_x + self.scroll.outer_scroll_x;

        if is_struct_mode && layout.description.map(|column| world_x >= column.start).unwrap_or(false) {
            if let Some(parse_res) = parse_result {
                let first_offset = if parse_res.is_live() {
                    parse_res.find_live_leaf_fields_starting_at(line_offset, chunk_len).first().map(|f| f.offset)
                } else {
                    parse_res.find_leaf_fields_starting_at(line_offset, chunk_len).first().map(|f| f.offset)
                };
                if let Some(offset) = first_offset {
                    return Some(offset);
                }
            }
            return Some(line_offset);
        }

        let byte_offset_in_row = if !is_struct_mode
            && let Some(ascii_column) = layout.ascii
            && world_x >= ascii_column.start
            && world_x < ascii_column.end()
        {
            let raw_idx = ascii_byte_index_from_world_x(world_x, ascii_column, self.scroll.ascii_scroll_x);
            let is_last_row = row_idx + 1 == line_starts.len();
            if is_last_row && raw_idx >= chunk_len {
                return Some(buffer_len);
            }
            let raw_idx = raw_idx.min(chunk_len.saturating_sub(1));
            let abs_offset = line_offset + raw_idx;
            let char_range = self.encoding.char_range_at(doc.buffer.data(), abs_offset);
            char_range.start.saturating_sub(line_offset)
        } else {
            let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
            let source = build_hex_text_source(
                doc.buffer.get_range(line_offset, chunk_len),
                line_offset,
                self.radix,
                self.group_size,
                self.is_big_endian,
                hex_case,
            );
            let cell_width = px(self.hex_cell_width.max(1.0));
            let origin_x = px(base_x + layout.hex.start - self.scroll.outer_scroll_x - self.scroll.hex_scroll_x);

            let mut target_group_idx = 0;
            for (idx, group) in source.groups.iter().enumerate() {
                let (group_start, group_end) = hex_group_x(*group, origin_x, cell_width);
                let next_start = source
                    .groups
                    .get(idx + 1)
                    .map(|next| origin_x + hex_grid_x(next.text_start, cell_width))
                    .unwrap_or(group_end);
                if point.x < next_start || idx + 1 == source.groups.len() {
                    target_group_idx = idx;
                    break;
                }
                if point.x >= group_start {
                    target_group_idx = idx;
                }
            }

            let is_last_row = row_idx + 1 == line_starts.len();
            if is_last_row && let Some(last_group) = source.groups.last().copied() {
                let (_, last_group_end) = hex_group_x(last_group, origin_x, cell_width);
                if point.x >= last_group_end && world_x <= layout.hex.end() {
                    return Some(buffer_len);
                }
            }

            let group = source.groups[target_group_idx];
            group.chunk_start
        };

        let byte_idx = byte_offset_in_row.min(chunk_len.saturating_sub(1));
        Some(line_offset + byte_idx)
    }
}
