use crate::core::bookmark::model::{BookmarkColor, BookmarkItem, BookmarkStore, generate_bookmark_id};
use crate::core::color::RgbaColor;
use std::ops::Range;

impl BookmarkStore {
    pub fn add_custom(&mut self, range: Range<usize>, color: RgbaColor, total_size: usize) {
        if range.is_empty() {
            return;
        }
        let clamped_start = range.start.min(total_size);
        let clamped_end = range.end.min(total_size);
        if clamped_start >= clamped_end {
            return;
        }
        let new_range = clamped_start..clamped_end;
        let hl_color = BookmarkColor::from_rgba(color);

        let mut updated = Vec::with_capacity(self.items.len() + 2);
        for h in self.items.drain(..) {
            let h_range = h.range();
            if h_range.end <= new_range.start || h_range.start >= new_range.end {
                updated.push(h);
            } else {
                if h_range.start < new_range.start {
                    let mut left = h.clone();
                    left.id = generate_bookmark_id();
                    left.size = new_range.start - h_range.start;
                    updated.push(left);
                }
                if h_range.end > new_range.end {
                    let mut right = h.clone();
                    right.id = generate_bookmark_id();
                    right.offset = new_range.end;
                    right.size = h_range.end - new_range.end;
                    updated.push(right);
                }
            }
        }
        updated.push(BookmarkItem::new(new_range.start, new_range.len(), hl_color, ""));
        updated.sort_by_key(|h| (h.offset, h.size));
        self.items = updated;
    }

    pub fn clear_custom(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        let mut updated = Vec::with_capacity(self.items.len() + 2);
        for h in self.items.drain(..) {
            let h_range = h.range();
            if h_range.end <= range.start || h_range.start >= range.end {
                updated.push(h);
            } else {
                if h_range.start < range.start {
                    let mut left = h.clone();
                    left.id = generate_bookmark_id();
                    left.size = range.start - h_range.start;
                    updated.push(left);
                }
                if h_range.end > range.end {
                    let mut right = h.clone();
                    right.id = generate_bookmark_id();
                    right.offset = range.end;
                    right.size = h_range.end - range.end;
                    updated.push(right);
                }
            }
        }
        self.items = updated;
        self.items.sort_by_key(|h| (h.offset, h.size));
    }

    pub fn custom_bookmarks_for_rendering(&self) -> Vec<(Range<usize>, RgbaColor)> {
        self.items.iter().map(|h| (h.range(), h.rgba_color())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_clear_custom() {
        let mut store = BookmarkStore::new();
        let color = RgbaColor::new(255, 0, 0, 255);
        store.add_custom(10..20, color, 100);
        assert_eq!(store.items.len(), 1);
        assert_eq!(store.items[0].offset, 10);
        assert_eq!(store.items[0].size, 10);

        let color2 = RgbaColor::new(0, 255, 0, 255);
        store.add_custom(14..16, color2, 100);
        assert_eq!(store.items.len(), 3);
        assert_eq!(store.items[0].range(), 10..14);
        assert_eq!(store.items[1].range(), 14..16);
        assert_eq!(store.items[2].range(), 16..20);

        store.clear_custom(12..18);
        assert_eq!(store.items.len(), 2);
        assert_eq!(store.items[0].range(), 10..12);
        assert_eq!(store.items[1].range(), 18..20);

        let rendering = store.custom_bookmarks_for_rendering();
        assert_eq!(rendering.len(), 2);
        assert_eq!(rendering[0].0, 10..12);
        assert_eq!(rendering[1].0, 18..20);
    }
}
