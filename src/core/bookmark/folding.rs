use crate::core::bookmark::model::{BookmarkColor, BookmarkStore};
use std::collections::{BTreeMap, HashSet};

/// Summary details for a folded region created by hidden bookmarks or gaps.
#[derive(Debug, Clone, PartialEq)]
pub struct FoldedBookmarkSummary {
    pub start_offset: usize,
    pub end_offset: usize,
    pub size: usize,
    pub color: BookmarkColor,
    pub comment: String,
    pub bookmark_ids: Vec<String>,
    pub is_unbookmarked: bool,
}

impl BookmarkStore {
    pub fn unfold_at(&mut self, offset: usize, folded_regions: &BTreeMap<usize, usize>) -> bool {
        let found = folded_regions.iter().find(|&(&start, &end)| offset >= start && offset < end);
        if let Some((&start, &end)) = found {
            let mut colors_to_decompose = HashSet::new();
            let mut ids_to_unhide = Vec::new();

            for it in &self.items {
                if it.offset < end && it.offset.saturating_add(it.size) > start {
                    colors_to_decompose.insert(it.color);
                    ids_to_unhide.push(it.id.clone());
                }
            }

            let mut changed = false;
            for &color in &colors_to_decompose {
                if self.hidden_colors.contains(&color) {
                    self.hidden_colors.remove(&color);
                    for other_bm in &self.items {
                        if other_bm.color == color {
                            let other_start = other_bm.offset;
                            let other_end = other_bm.offset.saturating_add(other_bm.size);
                            if !(other_start < end && other_end > start) {
                                self.hidden_ids.insert(other_bm.id.clone());
                            }
                        }
                    }
                    changed = true;
                }
            }

            for id in ids_to_unhide {
                if self.hidden_ids.remove(&id) {
                    changed = true;
                }
            }

            if self.hide_unbookmarked && colors_to_decompose.is_empty() {
                self.hide_unbookmarked = false;
                changed = true;
            }

            changed
        } else {
            false
        }
    }

    pub fn computed_folded_regions(&self, total_size: usize) -> BTreeMap<usize, usize> {
        if total_size == 0 {
            return BTreeMap::new();
        }

        let is_hide_unbookmarked = self.hide_unbookmarked;
        let hidden_colors = &self.hidden_colors;
        let hidden_ids = &self.hidden_ids;

        let mut bookmarked_ranges = Vec::new();
        let mut hidden_ranges = Vec::new();

        for item in &self.items {
            if item.size > 0 {
                let start = item.offset.min(total_size);
                let end = item.offset.saturating_add(item.size).min(total_size);
                if start < end {
                    bookmarked_ranges.push((start, end));
                    if hidden_colors.contains(&item.color) || hidden_ids.contains(&item.id) {
                        hidden_ranges.push((start, end));
                    }
                }
            }
        }

        let mut folds = BTreeMap::new();

        // 1. Hidden bookmark ranges become folds
        if !hidden_ranges.is_empty() {
            hidden_ranges.sort_unstable_by_key(|&(s, e)| (s, e));
            let mut cur_start = hidden_ranges[0].0;
            let mut cur_end = hidden_ranges[0].1;
            for &(s, e) in &hidden_ranges[1..] {
                if s < cur_end {
                    cur_end = cur_end.max(e);
                } else {
                    folds.insert(cur_start, cur_end);
                    cur_start = s;
                    cur_end = e;
                }
            }
            folds.insert(cur_start, cur_end);
        }

        // 2. If hide_unbookmarked is enabled, unbookmarked gaps also become folds
        if is_hide_unbookmarked {
            if bookmarked_ranges.is_empty() {
                folds.insert(0, total_size);
            } else {
                bookmarked_ranges.sort_unstable_by_key(|&(s, e)| (s, e));
                let mut merged_bm = Vec::new();
                let mut cur_start = bookmarked_ranges[0].0;
                let mut cur_end = bookmarked_ranges[0].1;
                for &(s, e) in &bookmarked_ranges[1..] {
                    if s <= cur_end {
                        cur_end = cur_end.max(e);
                    } else {
                        merged_bm.push((cur_start, cur_end));
                        cur_start = s;
                        cur_end = e;
                    }
                }
                merged_bm.push((cur_start, cur_end));

                let mut cursor = 0;
                for (bm_s, bm_e) in merged_bm {
                    if bm_s > cursor {
                        folds.insert(cursor, bm_s);
                    }
                    cursor = bm_e;
                }
                if cursor < total_size {
                    folds.insert(cursor, total_size);
                }
            }
        }

        folds
    }

    pub fn fold_bookmark_summary_at(&self, offset: usize, total_size: usize) -> Option<FoldedBookmarkSummary> {
        let folded = self.computed_folded_regions(total_size);
        let fold_end = folded.get(&offset).copied()?;

        let mut matched_items = Vec::new();
        for item in &self.items {
            if (self.hidden_colors.contains(&item.color) || self.hidden_ids.contains(&item.id))
                && item.offset < fold_end
                && item.offset.saturating_add(item.size) > offset
            {
                matched_items.push(item);
            }
        }

        let is_unbookmarked = matched_items.is_empty();
        let primary = matched_items.first().copied();
        let color = primary.map(|it| it.color).unwrap_or_default();
        let comment = primary
            .map(|it| it.comment.clone())
            .unwrap_or_else(|| if is_unbookmarked { "Unbookmarked".to_string() } else { String::new() });
        let bookmark_ids = matched_items.iter().map(|it| it.id.clone()).collect();

        Some(FoldedBookmarkSummary {
            start_offset: offset,
            end_offset: fold_end,
            size: fold_end.saturating_sub(offset),
            color,
            comment,
            bookmark_ids,
            is_unbookmarked,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::bookmark::model::BookmarkItem;

    #[test]
    fn test_computed_folded_regions_and_unfold() {
        let mut store = BookmarkStore::new();
        let item1 = BookmarkItem::new(10, 20, BookmarkColor::Red, "Red");
        let item2 = BookmarkItem::new(50, 10, BookmarkColor::Blue, "Blue");
        store.items.push(item1);
        store.items.push(item2);

        // Hide Red bookmarks
        store.hidden_colors.insert(BookmarkColor::Red);
        let folds = store.computed_folded_regions(100);
        assert_eq!(folds.len(), 1);
        assert_eq!(folds.get(&10), Some(&30));

        let summary = store.fold_bookmark_summary_at(10, 100).unwrap();
        assert_eq!(summary.start_offset, 10);
        assert_eq!(summary.end_offset, 30);
        assert_eq!(summary.color, BookmarkColor::Red);
        assert!(!summary.is_unbookmarked);

        // Unfold at offset 15
        assert!(store.unfold_at(15, &folds));
        assert!(store.computed_folded_regions(100).is_empty());
    }
}
