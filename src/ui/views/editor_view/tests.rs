#![allow(clippy::single_range_in_vec_init)]

use super::highlights::{CachedSearchPattern, HighlightSyncState, compose_search_highlights, subtract_ranges, viewport_match_slice};
use crate::core::encoding::Encoding;
use crate::core::search::SearchMode;
use gpui_kit::Hsla;

#[test]
fn test_subtract_ranges_no_overlap() {
    let color = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.5,
        a: 1.0,
    };
    let source = vec![(0..10, color), (20..30, color)];
    let result = subtract_ranges(source.clone(), &[12..18]);
    assert_eq!(result, source);
}

#[test]
fn test_subtract_ranges_exact_match() {
    let color = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.5,
        a: 1.0,
    };
    let source = vec![(10..20, color)];
    let result = subtract_ranges(source, &[10..20]);
    assert!(result.is_empty());
}

#[test]
fn test_subtract_ranges_interior_split() {
    let color = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.5,
        a: 1.0,
    };
    let source = vec![(10..20, color)];
    let result = subtract_ranges(source, &[12..15]);
    assert_eq!(result, vec![(10..12, color), (15..20, color)]);
}

#[test]
fn test_subtract_ranges_overlap_left_and_right() {
    let color = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.5,
        a: 1.0,
    };
    let source = vec![(10..20, color), (30..40, color)];
    let result = subtract_ranges(source, &[5..15, 35..45]);
    assert_eq!(result, vec![(15..20, color), (30..35, color)]);
}

#[test]
fn test_subtract_ranges_multiple_masks() {
    let color = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.5,
        a: 1.0,
    };
    let source = vec![(0..50, color)];
    let result = subtract_ranges(source, &[10..20, 30..40]);
    assert_eq!(result, vec![(0..10, color), (20..30, color), (40..50, color)]);
}

#[test]
fn test_subtract_ranges_overlapping_masks() {
    let color = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.5,
        a: 1.0,
    };
    let source = vec![(0..50, color)];
    let result = subtract_ranges(source, &[10..25, 20..35]);
    assert_eq!(result, vec![(0..10, color), (35..50, color)]);
}

#[test]
fn test_subtract_ranges_encompassing_mask() {
    let color = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.5,
        a: 1.0,
    };
    let source = vec![(10..20, color)];
    let result = subtract_ranges(source, &[5..25]);
    assert!(result.is_empty());
}

#[test]
fn test_compose_search_highlights_precedence() {
    let search_color = Hsla {
        h: 0.1,
        s: 0.5,
        l: 0.5,
        a: 0.5,
    };
    let find_color = Hsla {
        h: 0.2,
        s: 0.8,
        l: 0.5,
        a: 1.0,
    };
    let search = vec![(10..30, search_color)];
    let find = vec![(15..25, find_color)];
    let composed = compose_search_highlights(search, find.clone());
    assert_eq!(composed, vec![(10..15, search_color), (25..30, search_color), (15..25, find_color)]);
}

#[test]
fn test_highlight_sync_state_skips_redundant_scroll_after_editor_refresh() {
    let last = HighlightSyncState {
        cursor_offset: Some(0x2000),
        search_generation: 5,
        search_current_result: Some(0x2000),
        search_results_len: 12,
        find_generation: 1,
        has_search: true,
        scroll_top_row: Some(128),
    };

    // Scrolled event arriving for the same scroll row and editor snapshot should not rescan.
    assert!(!last.needs_scroll_refresh(last));
    assert!(!last.needs_editor_refresh(last));

    // Scrolling to a new row with active search triggers a refresh.
    let mut moved_scroll = last;
    moved_scroll.scroll_top_row = Some(140);
    assert!(moved_scroll.needs_scroll_refresh(last));

    // Moving cursor without active search only tracks cursor unless search changed.
    let mut no_search = last;
    no_search.has_search = false;
    let mut cursor_moved = no_search;
    cursor_moved.cursor_offset = Some(0x2010);
    assert!(!cursor_moved.needs_editor_refresh(no_search));
    assert!(!cursor_moved.needs_scroll_refresh(no_search));
}

#[test]
fn test_viewport_match_slice_and_cached_search_pattern() {
    let results = [0, 8, 12, 16, 28, 64];
    assert_eq!(viewport_match_slice(&results, 4, 10..20), &[8, 12, 16]);
    assert_eq!(viewport_match_slice(&results, 4, 12..16), &[12]);
    assert_eq!(viewport_match_slice(&results, 4, 20..28), &[] as &[usize]);
    assert_eq!(viewport_match_slice(&results, 4, 16..16), &[] as &[usize]);

    let mut cache = CachedSearchPattern::default();
    let hex_a = cache.resolve("41 42", SearchMode::Hex, Encoding::Ascii).unwrap().to_vec();
    assert_eq!(hex_a.len(), 2);
    // Hex mode reuses cached pattern even if display encoding changes.
    assert_eq!(cache.resolve("41 42", SearchMode::Hex, Encoding::Utf8).map(|p| p.len()), Some(2));
    // Text mode re-parses when encoding or query changes.
    let text_ascii = cache.resolve("AB", SearchMode::Text, Encoding::Ascii).unwrap().to_vec();
    assert_eq!(text_ascii.len(), 2);
    let text_utf16 = cache.resolve("AB", SearchMode::Text, Encoding::Utf16Le).unwrap().to_vec();
    assert_eq!(text_utf16.len(), 4);
}
