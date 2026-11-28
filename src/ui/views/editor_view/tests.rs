#![allow(clippy::single_range_in_vec_init)]

use super::highlights::{
    CachedSearchPattern, HighlightSyncState, compose_search_highlights, scan_viewport_ranges, subtract_ranges, viewport_match_slice, viewport_range_slice,
};
use super::search_bar::{SearchMatchStatus, format_match_status_label, format_selection_for_find};
use crate::core::encoding::Encoding;
use crate::core::format::HexCase;
use crate::core::search::{SearchMode, TextSearchFlags};
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

    let ranges = [0..3, 8..13, 15..18, 28..34];
    assert_eq!(viewport_range_slice(&ranges, 10..20), &[8..13, 15..18]);
    assert_eq!(viewport_range_slice(&ranges, 20..28), &[] as &[std::ops::Range<usize>]);

    let mut cache = CachedSearchPattern::default();
    let default_flags = TextSearchFlags::default();
    assert_eq!(
        cache
            .resolve_compiled("41 42", SearchMode::Hex, Encoding::Ascii, default_flags)
            .and_then(|c| c.fixed_len()),
        Some(2)
    );
    // Hex mode reuses cached pattern even if display encoding changes.
    assert_eq!(
        cache
            .resolve_compiled("41 42", SearchMode::Hex, Encoding::Utf8, default_flags)
            .and_then(|c| c.fixed_len()),
        Some(2)
    );
    // Text mode re-parses when encoding, flags, or query changes.
    assert_eq!(
        cache
            .resolve_compiled("AB", SearchMode::Text, Encoding::Ascii, default_flags)
            .and_then(|c| c.fixed_len()),
        Some(2)
    );
    assert_eq!(
        cache
            .resolve_compiled("AB", SearchMode::Text, Encoding::Utf16Le, default_flags)
            .and_then(|c| c.fixed_len()),
        Some(4)
    );

    // Regex viewport scan finds variable-length matches intersecting the viewport.
    let regex_flags = TextSearchFlags {
        case_insensitive: true,
        regex: true,
    };
    let compiled = cache
        .resolve_compiled("ab+", SearchMode::Text, Encoding::Ascii, regex_flags)
        .expect("valid regex");
    let data = b"xxABBBxxabxx";
    let scanned = scan_viewport_ranges(data, compiled, &[], 0, data.len());
    assert_eq!(scanned, vec![2..6, 8..10]);
}

#[test]
fn test_format_selection_for_find_and_match_status_label() {
    assert_eq!(
        format_selection_for_find(b"Hi", SearchMode::Hex, Encoding::Ascii, HexCase::Upper),
        Some("48 69".to_string())
    );
    assert_eq!(
        format_selection_for_find(b"Hi", SearchMode::Hex, Encoding::Ascii, HexCase::Lower),
        Some("48 69".to_string())
    );
    assert_eq!(
        format_selection_for_find(&[0xDE, 0xAD], SearchMode::Hex, Encoding::Ascii, HexCase::Lower),
        Some("de ad".to_string())
    );
    assert_eq!(
        format_selection_for_find(b"Hello", SearchMode::Text, Encoding::Utf8, HexCase::Upper),
        Some("Hello".to_string())
    );
    // Non-printable binary selection does not corrupt text search bar
    assert_eq!(format_selection_for_find(b"Hi\x00!", SearchMode::Text, Encoding::Ascii, HexCase::Upper), None);
    // Empty or oversized (>64 bytes) selections are ignored
    assert_eq!(format_selection_for_find(&[], SearchMode::Hex, Encoding::Ascii, HexCase::Upper), None);
    let oversized = vec![0x41u8; 65];
    assert_eq!(format_selection_for_find(&oversized, SearchMode::Hex, Encoding::Ascii, HexCase::Upper), None);

    assert_eq!(format_match_status_label(&SearchMatchStatus::None), None);
    assert_eq!(format_match_status_label(&SearchMatchStatus::Scanning), Some("...".to_string()));
    assert_eq!(
        format_match_status_label(&SearchMatchStatus::Count {
            current: None,
            total: 0,
            truncated: false,
        }),
        Some("No results".to_string())
    );
    assert_eq!(
        format_match_status_label(&SearchMatchStatus::Count {
            current: Some(2),
            total: 10,
            truncated: false,
        }),
        Some("3/10".to_string())
    );
    assert_eq!(
        format_match_status_label(&SearchMatchStatus::Count {
            current: Some(0),
            total: 10_000,
            truncated: true,
        }),
        Some("1/10000+".to_string())
    );
    assert_eq!(format_match_status_label(&SearchMatchStatus::Error("bad".into())), None);

    let ranges = [2..5, 10..14, 20..21];
    assert_eq!(super::find_index_containing_offset(&ranges, 0), None);
    assert_eq!(super::find_index_containing_offset(&ranges, 2), Some(0));
    assert_eq!(super::find_index_containing_offset(&ranges, 4), Some(0));
    assert_eq!(super::find_index_containing_offset(&ranges, 5), None);
    assert_eq!(super::find_index_containing_offset(&ranges, 12), Some(1));
    assert_eq!(super::find_index_containing_offset(&ranges, 20), Some(2));
    assert_eq!(super::find_index_containing_offset(&ranges, 21), None);
}
