use crate::core::editor::Editor;
use crate::core::encoding::Encoding;
use crate::core::search::{PatternByte, SearchMode};
use gpui_kit::Hsla;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HighlightSyncState {
    pub(super) cursor_offset: Option<usize>,
    pub(super) search_generation: usize,
    pub(super) search_current_result: Option<usize>,
    pub(super) search_results_len: usize,
    pub(super) find_generation: usize,
    pub(super) has_search: bool,
    pub(super) scroll_top_row: Option<usize>,
}

impl HighlightSyncState {
    pub(super) fn from_snapshot(ed: &Editor, has_search: bool, scroll_top_row: Option<usize>) -> Self {
        Self {
            cursor_offset: Some(ed.cursor.offset),
            search_generation: ed.search_state.generation,
            search_current_result: ed.search_state.current_result(),
            search_results_len: ed.search_state.results.len(),
            find_generation: ed.find_state.generation,
            has_search,
            scroll_top_row,
        }
    }

    pub(super) fn needs_editor_refresh(self, last: Self) -> bool {
        let search_changed = self.search_generation != last.search_generation
            || self.search_current_result != last.search_current_result
            || self.search_results_len != last.search_results_len;
        let find_changed = self.find_generation != last.find_generation;
        let cursor_changed_with_search = self.has_search && self.cursor_offset != last.cursor_offset;
        let search_visibility_changed = self.has_search != last.has_search;
        cursor_changed_with_search || search_changed || find_changed || search_visibility_changed
    }

    pub(super) fn needs_scroll_refresh(self, last: Self) -> bool {
        self.has_search && self != last
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct CachedSearchPattern {
    query: String,
    mode: SearchMode,
    encoding: Encoding,
    pattern: Option<Vec<PatternByte>>,
}

impl CachedSearchPattern {
    pub(super) fn resolve(&mut self, query: &str, mode: SearchMode, encoding: Encoding) -> Option<&[PatternByte]> {
        let encoding_relevant = mode == SearchMode::Text;
        if self.query != query || self.mode != mode || (encoding_relevant && self.encoding != encoding) {
            self.query.clear();
            self.query.push_str(query);
            self.mode = mode;
            self.encoding = encoding;
            self.pattern = if query.is_empty() {
                None
            } else {
                match mode {
                    SearchMode::Text => crate::core::search::parse_text_pattern(query, encoding),
                    SearchMode::Hex => crate::core::search::parse_hex_pattern(query),
                }
            };
        }
        self.pattern.as_deref().filter(|p| !p.is_empty())
    }
}

/// Returns the subslice of sorted match offsets whose intervals `[m, m + pattern_len)` intersect `viewport`.
#[inline]
pub(super) fn viewport_match_slice(results: &[usize], pattern_len: usize, viewport: Range<usize>) -> &[usize] {
    if results.is_empty() || pattern_len == 0 || viewport.is_empty() {
        return &[];
    }
    let lo = results.partition_point(|&m| m.saturating_add(pattern_len) <= viewport.start);
    let hi = results.partition_point(|&m| m < viewport.end);
    if lo >= hi { &[] } else { &results[lo..hi] }
}

pub(super) fn scan_viewport_matches(data: &[u8], pattern: &[PatternByte], segments: &[Range<usize>], start: usize, end: usize) -> Vec<usize> {
    let pattern_len = pattern.len();
    let scan_start = start.saturating_sub(pattern_len);
    let scan_end = end.saturating_add(pattern_len);
    crate::core::search::find_occurrences_segmented(data, pattern, crate::core::search::SearchLimit::Unlimited, segments, Some(scan_start..scan_end))
}

/// Composes search panel highlights and find widget highlights, giving precedence
/// to find widget highlights where they overlap.
pub(crate) fn compose_search_highlights(
    search_panel_highlights: Vec<(Range<usize>, Hsla)>,
    find_highlights: Vec<(Range<usize>, Hsla)>,
) -> Vec<(Range<usize>, Hsla)> {
    if find_highlights.is_empty() {
        return search_panel_highlights;
    }
    if search_panel_highlights.is_empty() {
        return find_highlights;
    }
    let find_mask: Vec<Range<usize>> = find_highlights.iter().map(|(r, _)| r.clone()).collect();
    let effective_search_highlights = subtract_ranges(search_panel_highlights, &find_mask);
    let mut combined = effective_search_highlights;
    combined.extend(find_highlights);
    combined
}

/// Subtracts a slice of mask ranges from a vector of highlighted ranges, preserving
/// non-overlapping portions with their original colors.
pub(super) fn subtract_ranges(source_ranges: Vec<(Range<usize>, Hsla)>, mask_ranges: &[Range<usize>]) -> Vec<(Range<usize>, Hsla)> {
    if mask_ranges.is_empty() {
        return source_ranges;
    }

    let mut result = Vec::new();

    for (range, color) in source_ranges {
        if range.start >= range.end {
            continue;
        }
        let mut current_pieces = vec![range];

        for mask in mask_ranges {
            if mask.start >= mask.end {
                continue;
            }
            let mut next_pieces = Vec::new();
            for piece in current_pieces {
                if piece.start >= mask.end || piece.end <= mask.start {
                    // No overlap
                    next_pieces.push(piece);
                } else {
                    // Overlap exists
                    if piece.start < mask.start {
                        next_pieces.push(piece.start..mask.start);
                    }
                    if piece.end > mask.end {
                        next_pieces.push(mask.end..piece.end);
                    }
                }
            }
            current_pieces = next_pieces;
            if current_pieces.is_empty() {
                break;
            }
        }

        for piece in current_pieces {
            if piece.start < piece.end {
                result.push((piece, color));
            }
        }
    }

    result
}
