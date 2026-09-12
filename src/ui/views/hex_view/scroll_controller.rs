use super::types::{HexViewLayout, HorizontalScrollTarget, ROW_HEIGHT, ScrollAxisLock, ScrollColumn};
use gpui_kit::{ScrollHandle, point, px};
use std::time::Instant;

/// Manages vertical and horizontal scroll state, axis locking, and scrollbar drag tracking.
pub struct ScrollController {
    pub scroll_offset: usize,
    pub accum_scroll_y: f32,
    pub outer_scroll_x: f32,
    pub outer_scroll_handle: ScrollHandle,
    pub is_dragging_scrollbar: bool,
    pub scrollbar_hovered: bool,
    pub scrollbar_drag_start_y: f32,
    pub scrollbar_drag_start_row: usize,
    pub hex_scroll_x: f32,
    pub ascii_scroll_x: f32,
    pub desc_scroll_x: f32,
    pub comment_scroll_x: f32,
    pub scroll_lock_axis: Option<ScrollAxisLock>,
    pub last_scroll_time: Option<Instant>,
    pub scroll_lock_top_row: usize,
    pub top_overscroll_rows: usize,
    pub max_top_overscroll_rows: usize,
}

impl Default for ScrollController {
    fn default() -> Self {
        Self {
            scroll_offset: 0,
            accum_scroll_y: 0.0,
            outer_scroll_x: 0.0,
            outer_scroll_handle: ScrollHandle::new(),
            is_dragging_scrollbar: false,
            scrollbar_hovered: false,
            scrollbar_drag_start_y: 0.0,
            scrollbar_drag_start_row: 0,
            hex_scroll_x: 0.0,
            ascii_scroll_x: 0.0,
            desc_scroll_x: 0.0,
            comment_scroll_x: 0.0,
            scroll_lock_axis: None,
            last_scroll_time: None,
            scroll_lock_top_row: 0,
            top_overscroll_rows: 0,
            max_top_overscroll_rows: 0,
        }
    }
}

impl ScrollController {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn horizontal_offset(&self, target: HorizontalScrollTarget) -> f32 {
        match target {
            HorizontalScrollTarget::View => self.outer_scroll_x,
            HorizontalScrollTarget::Column(ScrollColumn::Hex) => self.hex_scroll_x,
            HorizontalScrollTarget::Column(ScrollColumn::Ascii) => self.ascii_scroll_x,
            HorizontalScrollTarget::Column(ScrollColumn::Description) => self.desc_scroll_x,
            HorizontalScrollTarget::Column(ScrollColumn::Comment) => self.comment_scroll_x,
        }
    }

    /// Sets the horizontal scroll offset for the target column or whole view.
    /// Returns `(changed, new_offset)`.
    pub fn set_horizontal_offset(&mut self, target: HorizontalScrollTarget, offset: f32, layout: HexViewLayout) -> (bool, f32) {
        let max_offset = layout.max_offset(target);
        let new_offset = offset.clamp(0.0, max_offset);
        let current_offset = self.horizontal_offset(target);
        if (new_offset - current_offset).abs() <= 0.01 {
            return (false, new_offset);
        }

        match target {
            HorizontalScrollTarget::View => {
                self.outer_scroll_x = new_offset;
                self.outer_scroll_handle.set_offset(point(-px(new_offset), px(0.0)));
            }
            HorizontalScrollTarget::Column(ScrollColumn::Hex) => self.hex_scroll_x = new_offset,
            HorizontalScrollTarget::Column(ScrollColumn::Ascii) => self.ascii_scroll_x = new_offset,
            HorizontalScrollTarget::Column(ScrollColumn::Description) => self.desc_scroll_x = new_offset,
            HorizontalScrollTarget::Column(ScrollColumn::Comment) => self.comment_scroll_x = new_offset,
        }

        (true, new_offset)
    }

    pub fn clamp_scroll_offsets(&mut self, max_hex: f32, max_desc: f32, max_comment: f32, layout: HexViewLayout) {
        self.hex_scroll_x = self.hex_scroll_x.clamp(0.0, max_hex);
        self.desc_scroll_x = self.desc_scroll_x.clamp(0.0, max_desc);
        self.comment_scroll_x = self.comment_scroll_x.clamp(0.0, max_comment);
        self.ascii_scroll_x = self
            .ascii_scroll_x
            .clamp(0.0, layout.max_offset(HorizontalScrollTarget::Column(ScrollColumn::Ascii)));
        self.outer_scroll_x = self.outer_scroll_x.clamp(0.0, layout.outer_max);
        self.outer_scroll_handle.set_offset(point(-px(self.outer_scroll_x), px(0.0)));
    }

    pub fn update_scrollbar_drag(&mut self, current_y: f32, total_rows: usize, list_h: f32) -> Option<usize> {
        if !self.is_dragging_scrollbar {
            return None;
        }

        let delta_y = current_y - self.scrollbar_drag_start_y;
        let visible_rows = (list_h / ROW_HEIGHT).floor() as usize;
        let max_top_row = total_rows.saturating_sub(visible_rows.max(1));
        let ratio = (visible_rows as f64 / total_rows as f64).clamp(0.0, 1.0);
        let thumb_h = (list_h as f64 * ratio).clamp(24.0, list_h as f64) as f32;
        let max_thumb_top = (list_h - thumb_h).max(0.0);

        if max_thumb_top > 0.0 && max_top_row > 0 {
            let delta_ratio = delta_y as f64 / max_thumb_top as f64;
            let delta_rows = delta_ratio * max_top_row as f64;
            let new_row = ((self.scrollbar_drag_start_row as f64 + delta_rows).round() as isize).clamp(0, max_top_row as isize) as usize;
            Some(new_row)
        } else {
            None
        }
    }

    pub fn set_max_top_overscroll_rows(&mut self, max: usize) -> bool {
        if self.max_top_overscroll_rows != max {
            self.max_top_overscroll_rows = max;
            if self.top_overscroll_rows > max {
                self.top_overscroll_rows = max;
                return true;
            }
        }
        false
    }

    pub fn scroll_to_row(&mut self, target_row: usize, total_rows: usize, visible_rows: usize) -> bool {
        let max_top_row = total_rows.saturating_sub(visible_rows.max(1));
        let target = target_row.clamp(0, max_top_row);
        let mut changed = false;
        if self.scroll_offset != target {
            self.scroll_offset = target;
            self.accum_scroll_y = 0.0;
            changed = true;
        }
        if self.scroll_offset > 0 && self.top_overscroll_rows > 0 {
            self.top_overscroll_rows = 0;
            changed = true;
        }
        changed
    }

    pub fn handle_wheel_vertical(&mut self, delta_y: f32, max_top_row: usize) -> Option<usize> {
        self.accum_scroll_y += delta_y;
        let rows_to_scroll = -(self.accum_scroll_y / ROW_HEIGHT) as isize;
        if rows_to_scroll != 0 {
            self.accum_scroll_y += (rows_to_scroll as f32) * ROW_HEIGHT;
            let mut changed = false;

            if rows_to_scroll < 0 {
                // Scrolling UP (content moves DOWN, revealing top rows)
                let rows_up = (-rows_to_scroll) as usize;
                if self.scroll_offset > 0 {
                    let new_offset = self.scroll_offset.saturating_sub(rows_up);
                    let rem = rows_up - (self.scroll_offset - new_offset);
                    if new_offset != self.scroll_offset {
                        self.scroll_offset = new_offset;
                        changed = true;
                    }
                    if self.scroll_offset == 0 && rem > 0 {
                        let new_overscroll = (self.top_overscroll_rows + rem).min(self.max_top_overscroll_rows);
                        if new_overscroll != self.top_overscroll_rows {
                            self.top_overscroll_rows = new_overscroll;
                            changed = true;
                        }
                    }
                } else {
                    let new_overscroll = (self.top_overscroll_rows + rows_up).min(self.max_top_overscroll_rows);
                    if new_overscroll != self.top_overscroll_rows {
                        self.top_overscroll_rows = new_overscroll;
                        changed = true;
                    }
                }
            } else {
                // Scrolling DOWN (content moves UP)
                let rows_down = rows_to_scroll as usize;
                if self.top_overscroll_rows > 0 {
                    let new_overscroll = self.top_overscroll_rows.saturating_sub(rows_down);
                    let rem = rows_down - (self.top_overscroll_rows - new_overscroll);
                    if new_overscroll != self.top_overscroll_rows {
                        self.top_overscroll_rows = new_overscroll;
                        changed = true;
                    }
                    if rem > 0 {
                        let new_offset = (self.scroll_offset + rem).min(max_top_row);
                        if new_offset != self.scroll_offset {
                            self.scroll_offset = new_offset;
                            changed = true;
                        }
                    }
                } else {
                    let new_offset = (self.scroll_offset + rows_down).min(max_top_row);
                    if new_offset != self.scroll_offset {
                        self.scroll_offset = new_offset;
                        changed = true;
                    }
                }
            }

            if changed {
                return Some(self.scroll_offset);
            }
        }
        None
    }

    pub fn reveal_cursor(&mut self, cursor_left: f32, cursor_right: f32, hex_col_width: f32, max_hex_scroll: f32, layout: &HexViewLayout) {
        if cursor_left < self.hex_scroll_x {
            self.hex_scroll_x = cursor_left.clamp(0.0, max_hex_scroll);
        } else if cursor_right > self.hex_scroll_x + hex_col_width {
            self.hex_scroll_x = (cursor_right - hex_col_width).clamp(0.0, max_hex_scroll);
        }

        let visual_left = layout.hex.start + cursor_left - self.hex_scroll_x;
        let visual_right = layout.hex.start + cursor_right - self.hex_scroll_x;
        if visual_left - self.outer_scroll_x < layout.fixed_width {
            self.outer_scroll_x = (visual_left - layout.fixed_width).max(0.0);
        } else if visual_right - self.outer_scroll_x > layout.fixed_width + layout.viewport_width {
            self.outer_scroll_x = (visual_right - layout.fixed_width - layout.viewport_width).max(0.0);
        }
        self.outer_scroll_x = self.outer_scroll_x.clamp(0.0, layout.outer_max);
        self.outer_scroll_handle.set_offset(point(-px(self.outer_scroll_x), px(0.0)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_top_overscroll_wheel_behavior() {
        let mut controller = ScrollController::new();
        let max_top_row = 100;

        // 1. Without overlay, overscroll is disabled
        controller.handle_wheel_vertical(ROW_HEIGHT * 2.0, max_top_row);
        assert_eq!(controller.scroll_offset, 0);
        assert_eq!(controller.top_overscroll_rows, 0);

        // 2. Enable overlay overscroll (up to 3 rows)
        controller.set_max_top_overscroll_rows(3);
        assert_eq!(controller.max_top_overscroll_rows, 3);

        // Scroll up by 1 row
        controller.handle_wheel_vertical(ROW_HEIGHT, max_top_row);
        assert_eq!(controller.scroll_offset, 0);
        assert_eq!(controller.top_overscroll_rows, 1);

        // Scroll up by 3 more rows (should clamp to max 3)
        controller.handle_wheel_vertical(ROW_HEIGHT * 3.0, max_top_row);
        assert_eq!(controller.scroll_offset, 0);
        assert_eq!(controller.top_overscroll_rows, 3);

        // Scroll down by 1 row
        controller.handle_wheel_vertical(-ROW_HEIGHT, max_top_row);
        assert_eq!(controller.scroll_offset, 0);
        assert_eq!(controller.top_overscroll_rows, 2);

        // Scroll down by 3 rows (2 consumes overscroll, 1 increases scroll_offset)
        controller.handle_wheel_vertical(-ROW_HEIGHT * 3.0, max_top_row);
        assert_eq!(controller.top_overscroll_rows, 0);
        assert_eq!(controller.scroll_offset, 1);

        // Scroll up by 2 rows: 1 decreases scroll_offset to 0, 1 enters overscroll to 1
        controller.handle_wheel_vertical(ROW_HEIGHT * 2.0, max_top_row);
        assert_eq!(controller.scroll_offset, 0);
        assert_eq!(controller.top_overscroll_rows, 1);

        // 3. Disable overlay resets overscroll
        let changed = controller.set_max_top_overscroll_rows(0);
        assert!(changed);
        assert_eq!(controller.top_overscroll_rows, 0);
    }

    #[test]
    fn test_scroll_to_row_clears_overscroll() {
        let mut controller = ScrollController::new();
        controller.set_max_top_overscroll_rows(3);
        controller.top_overscroll_rows = 3;

        controller.scroll_to_row(10, 100, 20);
        assert_eq!(controller.scroll_offset, 10);
        assert_eq!(controller.top_overscroll_rows, 0);
    }
}
