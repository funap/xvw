use super::model::{CalculationRange, format_byte_size, resolve_target_range, split_hex_chunks, step_algorithm_selection, truncate_menu_preview};
use crate::core::checksum::ChecksumAlgorithm;

#[test]
fn test_format_byte_size() {
    assert_eq!(format_byte_size(0), "0 B");
    assert_eq!(format_byte_size(512), "512 B");
    assert_eq!(format_byte_size(1024), "1.0 KB");
    assert_eq!(format_byte_size(1536), "1.5 KB");
    assert_eq!(format_byte_size(1024 * 1024), "1.00 MB");
    assert_eq!(format_byte_size(5 * 1024 * 1024), "5.00 MB");
    assert_eq!(format_byte_size(1024 * 1024 * 1024), "1.00 GB");
}

#[test]
fn test_resolve_target_range_normalizes_empty_selection() {
    assert_eq!(resolve_target_range(CalculationRange::Selection, None, 100), 0..0);
    assert_eq!(resolve_target_range(CalculationRange::Selection, Some(42..42), 100), 0..0);
    assert_eq!(resolve_target_range(CalculationRange::Selection, Some(10..20), 100), 10..20);
    assert_eq!(resolve_target_range(CalculationRange::Selection, Some(90..150), 100), 90..100);
    assert_eq!(resolve_target_range(CalculationRange::EntireFile, None, 100), 0..100);
    assert_eq!(resolve_target_range(CalculationRange::EntireFile, Some(10..20), 0), 0..0);
}

#[test]
fn test_split_hex_chunks() {
    let sha256_hex = "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592";
    let chunks = split_hex_chunks(sha256_hex, 16);
    assert_eq!(chunks, vec!["d7a8fbb307d78094", "69ca9abcb0082e4f", "8d5651e46d3cdb76", "2d02d0bf37c9e592"]);
}

#[test]
fn test_truncate_menu_preview() {
    assert_eq!(truncate_menu_preview("0x12345678", 20), "0x12345678");
    let sha = "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592";
    let truncated = truncate_menu_preview(sha, 20);
    assert_eq!(truncated, "d7a8fbb3...f37c9e592");
    assert!(truncated.len() <= 20);
}

#[test]
fn test_step_algorithm_selection_stops_at_boundaries() {
    let algos = ChecksumAlgorithm::ALL;
    let first = algos[0];
    let second = algos[1];
    let last = algos[algos.len() - 1];
    let prev_last = algos[algos.len() - 2];

    assert_eq!(step_algorithm_selection(None, false), Some(first));
    assert_eq!(step_algorithm_selection(None, true), Some(first));
    assert_eq!(step_algorithm_selection(Some(first), false), Some(first));
    assert_eq!(step_algorithm_selection(Some(first), true), Some(second));
    assert_eq!(step_algorithm_selection(Some(second), false), Some(first));
    assert_eq!(step_algorithm_selection(Some(last), true), Some(last));
    assert_eq!(step_algorithm_selection(Some(last), false), Some(prev_last));
}
