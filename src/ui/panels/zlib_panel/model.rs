use crate::core::address_map::AddressMap;
use crate::core::dealloc::discard_in_background;
use crate::core::format::HexCase;
use crate::core::zlib::ZlibMatch;
use gpui_kit::component::table::ColumnSort;
use gpui_kit::component::theme::Theme;
use gpui_kit::{EntityId, Hsla, SharedString};
use std::fmt::Write as _;
use std::sync::Arc;

/// Maximum number of per-editor scan states retained in [`ZlibStateCache`].
pub(super) const MAX_CACHED_EDITORS: usize = 16;

/// Document content fingerprint used to detect edits that invalidate cached zlib results.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScanSignature {
    pub content_state: usize,
    pub byte_len: usize,
}

/// Unified table row view-model for a single validated zlib stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ZlibResultRow {
    pub offset: usize,
    pub compressed_len: usize,
    pub decompressed_len: usize,
    pub compressed_label: SharedString,
    pub decompressed_label: SharedString,
    pub preview: SharedString,
    pub preview_char_count: usize,
}

/// Converts pure domain [`ZlibMatch`] items into display-ready [`ZlibResultRow`] entries.
pub(super) fn build_zlib_rows(matches: &[ZlibMatch]) -> Arc<[ZlibResultRow]> {
    matches
        .iter()
        .map(|item| {
            let preview_string = item.format_preview();
            let preview_char_count = preview_string.chars().count();
            ZlibResultRow {
                offset: item.offset,
                compressed_len: item.compressed_len,
                decompressed_len: item.decompressed_len,
                compressed_label: SharedString::from(format!("{} B", item.compressed_len)),
                decompressed_label: SharedString::from(format!("{} B", item.decompressed_len)),
                preview: SharedString::from(preview_string),
                preview_char_count,
            }
        })
        .collect::<Vec<_>>()
        .into()
}

/// Snapshot of scanned zlib results cached while another editor tab is active.
pub(super) struct CachedZlibState {
    pub results_id: u64,
    pub rows: Arc<[ZlibResultRow]>,
    pub is_truncated: bool,
    pub selected_index: Option<usize>,
    pub scan_signature: Option<ScanSignature>,
}

/// Bounded LRU cache of scanned editor states that offloads heavy evicted buffers to background threads.
pub(super) struct ZlibStateCache {
    max_entries: usize,
    entries: Vec<(EntityId, CachedZlibState)>,
}

impl ZlibStateCache {
    pub(super) fn new(max_entries: usize) -> Self {
        Self {
            max_entries,
            entries: Vec::new(),
        }
    }

    pub(super) fn insert(&mut self, editor_id: EntityId, state: CachedZlibState) {
        if self.max_entries == 0 {
            discard_in_background(state.rows);
            return;
        }
        if let Some(pos) = self.entries.iter().position(|(id, _)| *id == editor_id) {
            let (_, old) = self.entries.remove(pos);
            discard_in_background(old.rows);
        }
        self.entries.push((editor_id, state));
        while self.entries.len() > self.max_entries {
            let (_, evicted) = self.entries.remove(0);
            discard_in_background(evicted.rows);
        }
    }

    pub(super) fn take(&mut self, editor_id: EntityId) -> Option<CachedZlibState> {
        let pos = self.entries.iter().position(|(id, _)| *id == editor_id)?;
        Some(self.entries.remove(pos).1)
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Lazy per-row cache of formatted hexadecimal address labels.
pub(super) struct AddressLabelCache {
    results_id: u64,
    editor_id: Option<EntityId>,
    hex_case: HexCase,
    address_map: AddressMap,
    labels: Vec<Option<SharedString>>,
}

impl AddressLabelCache {
    pub(super) fn new(results_id: u64, editor_id: Option<EntityId>, hex_case: HexCase, address_map: AddressMap, len: usize) -> Self {
        Self {
            results_id,
            editor_id,
            hex_case,
            address_map,
            labels: vec![None; len],
        }
    }

    pub(super) fn matches(&self, results_id: u64, editor_id: Option<EntityId>, hex_case: HexCase, len: usize) -> bool {
        self.results_id == results_id && self.editor_id == editor_id && self.hex_case == hex_case && self.labels.len() == len
    }

    pub(super) fn get_or_format(&mut self, index: usize, offset: usize) -> SharedString {
        if let Some(Some(cached)) = self.labels.get(index) {
            return cached.clone();
        }
        let formatted: SharedString = self.hex_case.format_prefixed_pad(self.address_map.offset_to_address(offset), 8).into();
        if let Some(slot) = self.labels.get_mut(index) {
            *slot = Some(formatted.clone());
        }
        formatted
    }
}

#[inline]
fn contains_ignore_case(haystack: &str, lowercase_needle: &str) -> bool {
    if lowercase_needle.is_empty() {
        return true;
    }
    if haystack.is_ascii() && lowercase_needle.is_ascii() {
        let h = haystack.as_bytes();
        let n = lowercase_needle.as_bytes();
        return h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n));
    }
    haystack.to_lowercase().contains(lowercase_needle)
}

/// Returns `true` if `lowercase_filter` consists solely of characters that can appear in `0x<hex>`.
#[inline]
pub(super) fn can_match_hex_address(lowercase_filter: &str) -> bool {
    !lowercase_filter.is_empty() && lowercase_filter.bytes().all(|b| b.is_ascii_hexdigit() || b == b'x')
}

/// Filters `rows` by case-insensitive substring match on preview or formatted physical address.
///
/// Reuses a single stack/scratch buffer for address matching (zero per-row heap allocations)
/// and skips address mapping altogether when `lowercase_filter` cannot match a hex address.
pub(super) fn filter_zlib_indices(rows: &[ZlibResultRow], address_map: Option<&AddressMap>, lowercase_filter: &str) -> Vec<usize> {
    if lowercase_filter.is_empty() {
        return (0..rows.len()).collect();
    }

    let check_address = can_match_hex_address(lowercase_filter);
    let mut addr_buf = if check_address { String::with_capacity(24) } else { String::new() };
    let mut matched = Vec::new();

    for (index, item) in rows.iter().enumerate() {
        if contains_ignore_case(item.preview.as_ref(), lowercase_filter) {
            matched.push(index);
            continue;
        }
        if check_address {
            let address = address_map.map_or(item.offset, |map| map.offset_to_address(item.offset));
            addr_buf.clear();
            let _ = write!(&mut addr_buf, "0x{address:08x}");
            if addr_buf.contains(lowercase_filter) {
                matched.push(index);
                continue;
            }
            addr_buf.clear();
            let _ = write!(&mut addr_buf, "0x{address:x}");
            if addr_buf.contains(lowercase_filter) {
                matched.push(index);
            }
        }
    }

    matched
}

/// Sorts row indices in place according to the active column sort order.
pub(super) fn sort_zlib_indices(rows: &[ZlibResultRow], indices: &mut [usize], sort_column: Option<(usize, ColumnSort)>) {
    let Some((col_ix, sort)) = sort_column else {
        indices.sort_unstable();
        return;
    };
    if sort == ColumnSort::Default {
        indices.sort_unstable();
        return;
    }

    indices.sort_by(|left, right| {
        let left_row = &rows[*left];
        let right_row = &rows[*right];
        let ordering = match col_ix {
            0 => left_row.offset.cmp(&right_row.offset),
            1 => left_row.compressed_len.cmp(&right_row.compressed_len),
            2 => left_row.decompressed_len.cmp(&right_row.decompressed_len),
            3 => left_row.preview.cmp(&right_row.preview),
            _ => std::cmp::Ordering::Equal,
        };
        let ordering = if sort == ColumnSort::Descending { ordering.reverse() } else { ordering };
        ordering.then_with(|| left.cmp(right))
    });
}

/// Foreground text colors used inside [`ZlibTableDelegate`](super::ZlibTableDelegate) cells.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ZlibCellColors {
    pub address_color: Hsla,
    pub size_color: Hsla,
    pub preview_color: Hsla,
}

pub(super) fn zlib_cell_colors(theme: &Theme) -> ZlibCellColors {
    ZlibCellColors {
        address_color: theme.foreground,
        size_color: theme.muted_foreground,
        preview_color: theme.foreground,
    }
}
