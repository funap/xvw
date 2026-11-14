use crate::core::bookmark::model::{BookmarkColor, BookmarkStore};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

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
        let found = folded_regions.range(..=offset).next_back().filter(|&(_, &end)| offset < end);
        if let Some((&start, &end)) = found {
            let mut colors_to_decompose = HashSet::new();
            let mut ids_to_unhide = Vec::new();
            let mut covered_bookmarks = Vec::new();

            for it in &self.items {
                if it.size == 0 {
                    continue;
                }
                let item_end = it.offset.saturating_add(it.size);
                if it.offset < end && item_end > start {
                    colors_to_decompose.insert(it.color);
                    ids_to_unhide.push(it.id.clone());
                    covered_bookmarks.push((it.offset.max(start), item_end.min(end)));
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

            if self.hide_unbookmarked {
                covered_bookmarks.sort_unstable_by_key(|&(s, e)| (s, e));
                let mut cursor = start;
                for (bm_s, bm_e) in covered_bookmarks {
                    if bm_s > cursor && self.insert_unfolded_unbookmarked_range(cursor, bm_s) {
                        changed = true;
                    }
                    cursor = cursor.max(bm_e);
                    if cursor >= end {
                        break;
                    }
                }
                if cursor < end && self.insert_unfolded_unbookmarked_range(cursor, end) {
                    changed = true;
                }
            }

            if changed {
                self.bump_version();
            }
            changed
        } else {
            false
        }
    }

    pub fn computed_folded_regions(&self, total_size: usize) -> Arc<BTreeMap<usize, usize>> {
        if let Ok(guard) = self.cached_folded_regions.lock()
            && let Some((cached_size, ref cached_arc)) = *guard
            && cached_size == total_size
        {
            return Arc::clone(cached_arc);
        }

        let folds = Arc::new(self.calculate_folded_regions(total_size));
        if let Ok(mut guard) = self.cached_folded_regions.lock() {
            *guard = Some((total_size, Arc::clone(&folds)));
        }
        folds
    }

    pub fn calculate_folded_regions(&self, total_size: usize) -> BTreeMap<usize, usize> {
        let is_hide_unbookmarked = self.hide_unbookmarked;
        let hidden_colors = &self.hidden_colors;
        let hidden_ids = &self.hidden_ids;

        if total_size == 0 || (!is_hide_unbookmarked && hidden_colors.is_empty() && hidden_ids.is_empty()) {
            return BTreeMap::new();
        }

        let mut bookmarked_ranges = Vec::new();
        let mut hidden_ranges = Vec::new();

        for item in &self.items {
            if item.size > 0 {
                let start = item.offset.min(total_size);
                let end = item.offset.saturating_add(item.size).min(total_size);
                if start < end {
                    if is_hide_unbookmarked {
                        bookmarked_ranges.push((start, end));
                    }
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

        // 2. If hide_unbookmarked is enabled, unbookmarked gaps (minus individually unfolded gaps) become folds
        if is_hide_unbookmarked {
            if bookmarked_ranges.is_empty() {
                insert_unbookmarked_gap_excluding_unfolded(0, total_size, &self.unfolded_unbookmarked_ranges, &mut folds);
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
                        insert_unbookmarked_gap_excluding_unfolded(cursor, bm_s, &self.unfolded_unbookmarked_ranges, &mut folds);
                    }
                    cursor = bm_e;
                }
                if cursor < total_size {
                    insert_unbookmarked_gap_excluding_unfolded(cursor, total_size, &self.unfolded_unbookmarked_ranges, &mut folds);
                }
            }
        }

        folds
    }

    pub fn fold_bookmark_summary_at(&self, offset: usize, total_size: usize) -> Option<FoldedBookmarkSummary> {
        if !self.hide_unbookmarked && self.hidden_colors.is_empty() && self.hidden_ids.is_empty() {
            return None;
        }
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

fn insert_unbookmarked_gap_excluding_unfolded(gap_start: usize, gap_end: usize, unfolded_ranges: &[(usize, usize)], folds: &mut BTreeMap<usize, usize>) {
    let mut cursor = gap_start;
    for &(u_start, u_end) in unfolded_ranges {
        if u_end <= cursor {
            continue;
        }
        if u_start >= gap_end {
            break;
        }
        if u_start > cursor {
            folds.insert(cursor, u_start);
        }
        cursor = u_end.min(gap_end);
        if cursor >= gap_end {
            break;
        }
    }
    if cursor < gap_end {
        folds.insert(cursor, gap_end);
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

    #[test]
    fn test_unfold_at_single_unbookmarked_gap_preserves_hide_unbookmarked_and_other_gaps() {
        let mut store = BookmarkStore::new();
        store.add(BookmarkItem::new(10, 10, BookmarkColor::Green, "First"), 100);
        store.add(BookmarkItem::new(60, 10, BookmarkColor::Blue, "Second"), 100);
        store.set_hide_unbookmarked(true);

        let folds = store.computed_folded_regions(100);
        assert_eq!(*folds, [(0, 10), (20, 60), (70, 100)].into_iter().collect());

        // Unfolding the middle gap [20..60) must leave hide_unbookmarked enabled and keep [0..10) & [70..100) folded.
        assert!(store.unfold_at(35, &folds));
        assert!(store.is_hide_unbookmarked());
        assert_eq!(*store.computed_folded_regions(100), [(0, 10), (70, 100)].into_iter().collect());

        // Unfolding the same already-unfolded gap again is a no-op.
        assert!(!store.unfold_at(35, &folds));

        // Toggling hide_unbookmarked off and back on resets per-gap unfolds.
        store.toggle_hide_unbookmarked();
        assert!(!store.is_hide_unbookmarked());
        assert!(store.computed_folded_regions(100).is_empty());

        store.toggle_hide_unbookmarked();
        assert!(store.is_hide_unbookmarked());
        assert_eq!(*store.computed_folded_regions(100), [(0, 10), (20, 60), (70, 100)].into_iter().collect());
    }

    #[test]
    fn test_unfold_at_mixed_fold_and_non_bookmark_fold() {
        let mut store = BookmarkStore::new();
        // When hide_unbookmarked is false, unfolding a range that has no hidden bookmarks (e.g. collapsed struct)
        // must not mutate BookmarkStore or record unbookmarked ranges.
        let external_fold: BTreeMap<usize, usize> = [(16, 32)].into_iter().collect();
        assert!(!store.unfold_at(20, &external_fold));
        assert!(store.unfolded_unbookmarked_ranges.is_empty());

        // With hide_unbookmarked enabled and a merged range [20..60) spanning a hidden bookmark [30..40)
        // plus unbookmarked margins [20..30) and [40..60), unfolding [20..60) unhides the bookmark
        // and reveals only the [20..30) and [40..60) unbookmarked sub-ranges while keeping [0..20) and [60..100) folded.
        let hidden = store.add(BookmarkItem::new(30, 10, BookmarkColor::Red, "Hidden"), 100);
        store.hide_color(BookmarkColor::Red);
        store.set_hide_unbookmarked(true);

        let merged_with_struct: BTreeMap<usize, usize> = [(0, 20), (20, 60), (60, 100)].into_iter().collect();
        assert!(store.unfold_at(25, &merged_with_struct));
        assert!(store.is_hide_unbookmarked());
        assert!(!store.is_id_hidden(&hidden));
        assert_eq!(store.unfolded_unbookmarked_ranges, vec![(20, 30), (40, 60)]);
        assert_eq!(*store.computed_folded_regions(100), [(0, 20), (60, 100)].into_iter().collect());
    }
}
