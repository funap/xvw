use crate::core::checksum::{ChecksumAlgorithm, ChecksumResults};
use crate::core::editor::Editor;
use gpui_kit::EntityId;
use std::ops::Range;

/// Maximum byte length automatically computed without manual confirmation.
pub const AUTO_CALC_MAX_BYTES: usize = 1_000_000;

/// Maximum byte length computed synchronously on the foreground thread to avoid async 1-frame flicker.
pub const SYNC_CALC_MAX_BYTES: usize = 32 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CalculationRange {
    Selection,
    EntireFile,
}

/// Identifies the exact byte slice and document state for a checksum calculation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CalculationSignature {
    pub editor_id: EntityId,
    pub content_state: usize,
    pub total_len: usize,
    pub range_mode: CalculationRange,
    pub byte_range: Range<usize>,
}

pub(super) struct CachedChecksumState {
    pub(super) calculation_range: CalculationRange,
    pub(super) auto_calculate: bool,
    pub(super) results: Option<ChecksumResults>,
    pub(super) computed_signature: Option<CalculationSignature>,
    pub(super) last_observed_signature: Option<CalculationSignature>,
    pub(super) selected_algorithm: Option<ChecksumAlgorithm>,
}

/// Events emitted by the checksum panel for workspace navigation and focus coordination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChecksumPanelEvent {
    NavigateTo { offset: usize, len: usize },
    FocusEditor,
}

pub(crate) fn format_byte_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Resolves the effective byte range to hash for the given range mode, selection, and total file length.
/// Empty ranges are normalized to `0..0` so cursor movements without a selection do not change the range identity.
pub(crate) fn resolve_target_range(range_mode: CalculationRange, selected_range: Option<Range<usize>>, total_len: usize) -> Range<usize> {
    match range_mode {
        CalculationRange::Selection => {
            let range = selected_range.unwrap_or(0..0);
            let start = range.start.min(total_len);
            let end = range.end.min(total_len).max(start);
            if start == end { 0..0 } else { start..end }
        }
        CalculationRange::EntireFile => 0..total_len,
    }
}

/// Splits a hex string into fixed-size chunks for responsive multi-line wrapping without ellipsis truncation.
pub(crate) fn split_hex_chunks(hex: &str, chunk_size: usize) -> Vec<&str> {
    if hex.is_empty() || chunk_size == 0 {
        return Vec::new();
    }
    let mut chunks = Vec::with_capacity(hex.len().div_ceil(chunk_size));
    let mut start = 0;
    while start < hex.len() {
        let end = (start + chunk_size).min(hex.len());
        chunks.push(&hex[start..end]);
        start = end;
    }
    chunks
}

/// Truncates a long checksum value for compact context menu labels while preserving start and end digits.
pub(crate) fn truncate_menu_preview(value: &str, max_len: usize) -> String {
    if value.len() <= max_len || max_len < 8 {
        return value.to_string();
    }
    let head = (max_len - 3) / 2;
    let tail = max_len - 3 - head;
    format!("{}...{}", &value[..head], &value[value.len() - tail..])
}

pub(super) fn selected_range_for_checksum(editor: &Editor) -> Option<Range<usize>> {
    if editor.has_selection() { editor.selected_range_or_cursor() } else { None }
}

pub(super) fn step_algorithm_selection(current: Option<ChecksumAlgorithm>, moving_down: bool) -> Option<ChecksumAlgorithm> {
    let algos = ChecksumAlgorithm::ALL;
    let last_idx = algos.len().checked_sub(1)?;
    let next_idx = match current.and_then(|a| algos.iter().position(|&x| x == a)) {
        Some(idx) if moving_down => (idx + 1).min(last_idx),
        Some(idx) => idx.saturating_sub(1),
        None => 0,
    };
    algos.get(next_idx).copied()
}
