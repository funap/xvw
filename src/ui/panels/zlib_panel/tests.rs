use super::ZlibPanelEvent;
use super::model::{
    AddressLabelCache, CachedZlibState, ScanSignature, ZlibResultRow, ZlibStateCache, build_zlib_rows, is_at_vertical_boundary, sort_zlib_indices,
    zlib_cell_colors,
};
use crate::core::address_map::{AddressMap, MemorySegment};
use crate::core::format::HexCase;
use crate::core::zlib::ZlibMatch;
use gpui_kit::component::table::ColumnSort;
use gpui_kit::component::theme::Theme;
use gpui_kit::{EntityId, Hsla, KeyBindingContextPredicate, KeyContext, Rgba};
use std::sync::Arc;

fn make_row(offset: usize, compressed_len: usize, decompressed_len: usize, preview: &str) -> ZlibResultRow {
    ZlibResultRow {
        offset,
        compressed_len,
        decompressed_len,
        compressed_label: format!("{compressed_len} B").into(),
        decompressed_label: format!("{decompressed_len} B").into(),
        preview: preview.to_string().into(),
        preview_char_count: preview.chars().count(),
    }
}

#[test]
fn test_zlib_cell_colors_match_theme_foregrounds() {
    let theme = Theme::default();
    let colors = zlib_cell_colors(&theme);
    assert_eq!(colors.address_color, theme.foreground);
    assert_eq!(colors.size_color, theme.muted_foreground);
    assert_eq!(colors.preview_color, theme.foreground);
}

#[test]
fn test_zlib_address_contrast_across_all_embedded_themes() {
    use crate::assets::Assets;
    use crate::theme::{load_themes_from_assets, theme_names};

    let registry = load_themes_from_assets(&Assets);
    let names = theme_names(registry.themes());
    assert!(!names.is_empty());

    fn channel_to_linear(c: f32) -> f32 {
        if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    }

    fn rel_luminance(c: Hsla) -> f32 {
        let rgba = Rgba::from(c);
        0.2126 * channel_to_linear(rgba.r) + 0.7152 * channel_to_linear(rgba.g) + 0.0722 * channel_to_linear(rgba.b)
    }

    fn contrast_ratio(c1: Hsla, c2: Hsla) -> f32 {
        let l1 = rel_luminance(c1);
        let l2 = rel_luminance(c2);
        let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
        (lighter + 0.05) / (darker + 0.05)
    }

    for name in names {
        let config = registry.themes().get(&name).unwrap();
        let mut theme = Theme::default();
        theme.apply_config(config);

        let colors = zlib_cell_colors(&theme);
        let ratio = contrast_ratio(colors.address_color, theme.sidebar);
        assert!(
            ratio >= 4.5,
            "Theme '{}' failed WCAG AA contrast for address: ratio was {:.2}:1 (expected >= 4.5:1)",
            name,
            ratio
        );
    }
}

#[test]
fn test_sort_zlib_indices_all_columns() {
    let rows = vec![make_row(0x30, 30, 100, "bbb"), make_row(0x10, 80, 50, "ccc"), make_row(0x20, 50, 200, "aaa")];

    let mut indices = vec![0, 1, 2];
    sort_zlib_indices(&rows, &mut indices, Some((0, ColumnSort::Ascending)));
    assert_eq!(indices, vec![1, 2, 0]);

    sort_zlib_indices(&rows, &mut indices, Some((1, ColumnSort::Descending)));
    assert_eq!(indices, vec![1, 2, 0]);

    sort_zlib_indices(&rows, &mut indices, Some((2, ColumnSort::Descending)));
    assert_eq!(indices, vec![2, 0, 1]);

    sort_zlib_indices(&rows, &mut indices, Some((3, ColumnSort::Ascending)));
    assert_eq!(indices, vec![2, 0, 1]);

    sort_zlib_indices(&rows, &mut indices, Some((3, ColumnSort::Default)));
    assert_eq!(indices, vec![0, 1, 2]);
}

#[test]
fn test_is_at_vertical_boundary_stops_at_ends_without_wrapping() {
    // Empty table always blocks vertical navigation
    assert!(is_at_vertical_boundary(false, None, 0));
    assert!(is_at_vertical_boundary(true, None, 0));

    // Single row blocks both Up and Down when selected
    assert!(is_at_vertical_boundary(false, Some(0), 1));
    assert!(is_at_vertical_boundary(true, Some(0), 1));

    // Multi-row table: stops at top on Up and bottom on Down, allows interior navigation
    assert!(is_at_vertical_boundary(false, Some(0), 5));
    assert!(!is_at_vertical_boundary(true, Some(0), 5));
    assert!(!is_at_vertical_boundary(false, Some(2), 5));
    assert!(!is_at_vertical_boundary(true, Some(2), 5));
    assert!(!is_at_vertical_boundary(false, Some(4), 5));
    assert!(is_at_vertical_boundary(true, Some(4), 5));

    // No selection yet allows table to select the initial row
    assert!(!is_at_vertical_boundary(false, None, 5));
    assert!(!is_at_vertical_boundary(true, None, 5));
}

#[test]
fn test_build_zlib_rows_and_address_cache() {
    let matches = [ZlibMatch::from_sample(0x1A, 42, 128, [0x78, 0x9C], b"hello zlib")];
    let rows = build_zlib_rows(&matches);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].offset, 0x1A);
    assert_eq!(rows[0].compressed_label.as_ref(), "42 B");
    assert_eq!(rows[0].decompressed_label.as_ref(), "128 B");
    assert_eq!(rows[0].preview.as_ref(), "hello zlib…");
    assert_eq!(rows[0].preview_char_count, 11);

    let map = AddressMap::from_segments(vec![MemorySegment {
        buffer_offset: 0,
        address: 0x0800_0000,
        length: 64,
    }]);
    let mut cache = AddressLabelCache::new(1, None, HexCase::Upper, map, 1);
    assert_eq!(cache.get_or_format(0, 0x1A).as_ref(), "0x0800001A");
    assert!(cache.matches(1, None, HexCase::Upper, 1));
    assert!(!cache.matches(1, None, HexCase::Lower, 1));
}

#[test]
fn test_state_cache_evicts_least_recently_used_entries() {
    let id1 = EntityId::from(1u64);
    let id2 = EntityId::from(2u64);
    let id3 = EntityId::from(3u64);

    let make_state = |results_id: u64| CachedZlibState {
        results_id,
        rows: Arc::default(),
        is_truncated: false,
        selected_index: None,
        scan_signature: Some(ScanSignature {
            content_state: results_id as usize,
            byte_len: 16,
        }),
    };

    let mut cache = ZlibStateCache::new(2);
    cache.insert(id1, make_state(1));
    cache.insert(id2, make_state(2));
    assert_eq!(cache.len(), 2);

    // Inserting a third entry evicts oldest (id1)
    cache.insert(id3, make_state(3));
    assert_eq!(cache.len(), 2);
    assert!(cache.take(id1).is_none());
    assert_eq!(cache.take(id2).unwrap().results_id, 2);
    assert_eq!(cache.take(id3).unwrap().results_id, 3);
    assert_eq!(cache.len(), 0);
}

#[test]
fn test_zlib_panel_key_context_predicate() {
    let event = ZlibPanelEvent::FocusEditor;
    assert!(matches!(event, ZlibPanelEvent::FocusEditor));

    let parent_ctx = KeyContext::try_from("ZlibPanel").unwrap();
    let child_ctx = KeyContext::try_from("DataTable").unwrap();
    let contexts = vec![parent_ctx, child_ctx];

    let pred_desc = KeyBindingContextPredicate::parse("ZlibPanel > DataTable").unwrap();
    assert!(pred_desc.eval(&contexts));
    assert_eq!(pred_desc.depth_of(&contexts), Some(2));
}
