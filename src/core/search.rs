#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SearchMode {
    #[default]
    Hex,
    Text,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum SearchLimit {
    /// Limit to a maximum number of results
    Count(usize),
    /// Limit to results within N bytes from the first match
    Range(usize),
    /// No limit
    Unlimited,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct SearchOptions {
    pub mode: SearchMode,
    pub encoding: crate::core::encoding::Encoding,
    pub limit: SearchLimit,
    pub range: Option<std::ops::Range<usize>>,
}

#[allow(dead_code)]
impl SearchOptions {
    pub fn new(mode: SearchMode) -> Self {
        Self {
            mode,
            encoding: crate::core::encoding::Encoding::default(),
            limit: SearchLimit::Unlimited,
            range: None,
        }
    }

    pub fn with_encoding(mut self, encoding: crate::core::encoding::Encoding) -> Self {
        self.encoding = encoding;
        self
    }

    pub fn with_count_limit(mode: SearchMode, max_results: usize) -> Self {
        Self {
            mode,
            encoding: crate::core::encoding::Encoding::default(),
            limit: SearchLimit::Count(max_results),
            range: None,
        }
    }

    pub fn with_range_limit(mode: SearchMode, range_bytes: usize) -> Self {
        Self {
            mode,
            encoding: crate::core::encoding::Encoding::default(),
            limit: SearchLimit::Range(range_bytes),
            range: None,
        }
    }

    pub fn with_range(mode: SearchMode, range: std::ops::Range<usize>) -> Self {
        Self {
            mode,
            encoding: crate::core::encoding::Encoding::default(),
            limit: SearchLimit::Unlimited,
            range: Some(range),
        }
    }
}

/// A pattern matcher byte, specifying value and mask.
/// E.g., for exact matching `value = B, mask = 0xFF`.
/// For wildcard `value = 0, mask = 0`.
/// For half-byte wildcard `value = 0x40, mask = 0xF0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PatternByte {
    pub value: u8,
    pub mask: u8,
}

impl PatternByte {
    /// Create a PatternByte requiring an exact match of the byte value.
    pub fn new_exact(value: u8) -> Self {
        Self { value, mask: 0xFF }
    }

    /// Create a PatternByte that matches any byte value.
    pub fn new_wildcard() -> Self {
        Self { value: 0, mask: 0 }
    }

    /// Check if this PatternByte matches the given byte under its mask.
    #[inline]
    pub fn matches(&self, byte: u8) -> bool {
        (byte & self.mask) == self.value
    }
}

/// Parses a text search query into a sequence of `PatternByte` matchers
/// using the specified `Encoding`.
/// Returns None if the query is empty or cannot be encoded in `encoding`.
pub fn parse_text_pattern(query: &str, encoding: crate::core::encoding::Encoding) -> Option<Vec<PatternByte>> {
    if query.is_empty() {
        return None;
    }
    let bytes = encoding.encode_str(query)?;
    if bytes.is_empty() {
        return None;
    }
    Some(bytes.into_iter().map(PatternByte::new_exact).collect())
}

/// Parses a hex search pattern containing hex digits, wildcards (`?` or `*`),
/// and optional spaces/separators into a sequence of `PatternByte` matchers.
/// Returns None if the pattern contains invalid characters.
pub fn parse_hex_pattern(query: &str) -> Option<Vec<PatternByte>> {
    let mut pattern = Vec::new();

    for token in query.split_whitespace() {
        let bytes = token.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let b1 = bytes[i];
            if i + 1 < bytes.len() {
                let b2 = bytes[i + 1];
                let is_b1_wild = b1 == b'?' || b1 == b'*';
                let is_b2_wild = b2 == b'?' || b2 == b'*';
                let is_b1_hex = b1.is_ascii_hexdigit();
                let is_b2_hex = b2.is_ascii_hexdigit();

                if is_b1_wild && is_b2_wild {
                    pattern.push(PatternByte::new_wildcard());
                } else if is_b1_hex && is_b2_hex {
                    let val_high = (b1 as char).to_digit(16)? as u8;
                    let val_low = (b2 as char).to_digit(16)? as u8;
                    pattern.push(PatternByte::new_exact((val_high << 4) | val_low));
                } else if is_b1_hex && is_b2_wild {
                    let val_high = (b1 as char).to_digit(16)? as u8;
                    pattern.push(PatternByte {
                        value: val_high << 4,
                        mask: 0xF0,
                    });
                } else if is_b1_wild && is_b2_hex {
                    let val_low = (b2 as char).to_digit(16)? as u8;
                    pattern.push(PatternByte { value: val_low, mask: 0x0F });
                } else {
                    return None;
                }
                i += 2;
            } else {
                if b1 == b'?' || b1 == b'*' {
                    pattern.push(PatternByte::new_wildcard());
                } else if b1.is_ascii_hexdigit() {
                    let val = (b1 as char).to_digit(16)? as u8;
                    pattern.push(PatternByte::new_exact(val));
                } else {
                    return None;
                }
                i += 1;
            }
        }
    }

    if pattern.is_empty() { None } else { Some(pattern) }
}

/// A stateless function to find occurrences of a pattern in a byte slice.
pub fn find_occurrences(data: &[u8], pattern: &[PatternByte], limit: SearchLimit, range: Option<std::ops::Range<usize>>) -> Vec<usize> {
    if pattern.is_empty() || pattern.len() > data.len() {
        return Vec::new();
    }

    let mut results = Vec::new();
    let pattern_len = pattern.len();
    let data_len = data.len();

    // Determine search range
    let (start, end) = if let Some(r) = range {
        (r.start.min(data_len), r.end.min(data_len))
    } else {
        (0, data_len)
    };

    if start >= end || end < pattern_len {
        return Vec::new();
    }

    use bstr::ByteSlice as _;

    let search_end = end - pattern_len;
    let mut first_match: Option<usize> = None;
    let mut i = start.min(search_end + 1);

    while i <= search_end {
        if pattern[0].mask == 0xFF {
            let target = pattern[0].value;
            match data[i..=search_end].find_byte(target) {
                Some(pos) => {
                    i += pos;
                }
                None => break,
            }
        } else if !pattern[0].matches(data[i]) {
            i += 1;
            continue;
        }

        let mut matched = true;
        for j in 1..pattern_len {
            if !pattern[j].matches(data[i + j]) {
                matched = false;
                break;
            }
        }

        if matched {
            // Track first match for range-based limiting
            if first_match.is_none() {
                first_match = Some(i);
            }

            // Check limit
            match limit {
                SearchLimit::Count(max) => {
                    results.push(i);
                    if results.len() >= max {
                        break;
                    }
                }
                SearchLimit::Range(range_bytes) => {
                    if let Some(first) = first_match
                        && i >= first + range_bytes
                    {
                        break;
                    }
                    results.push(i);
                }
                SearchLimit::Unlimited => {
                    results.push(i);
                }
            }
        }

        i += 1;
    }

    results
}

/// Finds occurrences of a pattern across multiple memory segments.
///
/// If `segments` is empty, searches the entire buffer (optionally restricted by `range_filter`).
/// When `segments` are provided, searches each segment independently, ensuring that matches
/// never straddle across segment boundaries or unmapped address gaps.
pub fn find_occurrences_segmented(
    data: &[u8],
    pattern: &[PatternByte],
    limit: SearchLimit,
    segments: &[std::ops::Range<usize>],
    range_filter: Option<std::ops::Range<usize>>,
) -> Vec<usize> {
    if segments.is_empty() {
        return find_occurrences(data, pattern, limit, range_filter);
    }

    if pattern.is_empty() || data.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();
    let mut first_match = None;

    for seg in segments {
        let seg_start = seg.start.min(data.len());
        let seg_end = seg.end.min(data.len());
        if seg_start >= seg_end || pattern.len() > seg_end - seg_start {
            continue;
        }

        let effective_range = match &range_filter {
            Some(filter) => {
                let start = seg_start.max(filter.start.min(data.len()));
                let end = seg_end.min(filter.end.min(data.len()));
                if start >= end || pattern.len() > end - start {
                    continue;
                }
                start..end
            }
            None => seg_start..seg_end,
        };

        // Determine remaining limit
        let seg_limit = match limit {
            SearchLimit::Count(max) => {
                let rem = max.saturating_sub(results.len());
                if rem == 0 {
                    break;
                }
                SearchLimit::Count(rem)
            }
            SearchLimit::Range(range_bytes) => {
                if let Some(first) = first_match
                    && effective_range.start >= first + range_bytes
                {
                    break;
                }
                SearchLimit::Range(range_bytes)
            }
            SearchLimit::Unlimited => SearchLimit::Unlimited,
        };

        let seg_matches = find_occurrences(data, pattern, seg_limit, Some(effective_range));
        if !seg_matches.is_empty() && first_match.is_none() {
            first_match = seg_matches.first().copied();
        }
        results.extend(seg_matches);

        match limit {
            SearchLimit::Count(max) if results.len() >= max => break,
            SearchLimit::Range(range_bytes) => {
                if let Some(first) = first_match
                    && let Some(&last) = results.last()
                    && last >= first + range_bytes
                {
                    break;
                }
            }
            _ => {}
        }
    }

    results
}

/// Helper to find a single occurrence scanning forward in range `start..end`.
fn find_single_forward(data: &[u8], pattern: &[PatternByte], start: usize, end: usize) -> Option<usize> {
    if pattern.is_empty() || pattern.len() > data.len() {
        return None;
    }
    let data_len = data.len();
    let start = start.min(data_len);
    let end = end.min(data_len);
    let pattern_len = pattern.len();

    if start >= end || end < pattern_len {
        return None;
    }

    use bstr::ByteSlice as _;
    let search_end = end - pattern_len;
    let mut i = start.min(search_end + 1);

    while i <= search_end {
        if pattern[0].mask == 0xFF {
            let target = pattern[0].value;
            match data[i..=search_end].find_byte(target) {
                Some(pos) => {
                    i += pos;
                }
                None => break,
            }
        } else if !pattern[0].matches(data[i]) {
            i += 1;
            continue;
        }

        let mut matched = true;
        for j in 1..pattern_len {
            if !pattern[j].matches(data[i + j]) {
                matched = false;
                break;
            }
        }

        if matched {
            return Some(i);
        }

        i += 1;
    }

    None
}

/// Helper to find a single occurrence scanning backward in range `start..end`.
fn find_single_backward(data: &[u8], pattern: &[PatternByte], start: usize, end: usize) -> Option<usize> {
    if pattern.is_empty() || pattern.len() > data.len() {
        return None;
    }
    let data_len = data.len();
    let start = start.min(data_len);
    let end = end.min(data_len);
    let pattern_len = pattern.len();

    if start >= end || end < pattern_len {
        return None;
    }

    use bstr::ByteSlice as _;
    let search_end = end - pattern_len;
    if start > search_end {
        return None;
    }

    let mut i = search_end;
    loop {
        if pattern[0].mask == 0xFF {
            let target = pattern[0].value;
            match data[start..=i].rfind_byte(target) {
                Some(pos) => {
                    i = start + pos;
                }
                None => break,
            }
        } else if !pattern[0].matches(data[i]) {
            if i <= start {
                break;
            }
            i -= 1;
            continue;
        }

        let mut matched = true;
        for j in 1..pattern_len {
            if !pattern[j].matches(data[i + j]) {
                matched = false;
                break;
            }
        }

        if matched {
            return Some(i);
        }

        if i <= start {
            break;
        }
        i -= 1;
    }

    None
}

/// Finds the next occurrence of `pattern` starting at `from_offset`, wrapping around to 0 if needed.
pub fn find_next_occurrence(data: &[u8], pattern: &[PatternByte], from_offset: usize) -> Option<usize> {
    if data.is_empty() || pattern.is_empty() || pattern.len() > data.len() {
        return None;
    }

    // 1. Search forward from `from_offset` to end of data
    if let Some(pos) = find_single_forward(data, pattern, from_offset, data.len()) {
        return Some(pos);
    }

    // 2. Wrap-around: search from 0 to `from_offset`
    if from_offset > 0 {
        let wrap_end = (from_offset + pattern.len().saturating_sub(1)).min(data.len());
        return find_single_forward(data, pattern, 0, wrap_end);
    }

    None
}

/// Finds the previous occurrence of `pattern` strictly starting before `before_offset`, wrapping around to the end of data if needed.
pub fn find_prev_occurrence(data: &[u8], pattern: &[PatternByte], before_offset: usize) -> Option<usize> {
    if data.is_empty() || pattern.is_empty() || pattern.len() > data.len() {
        return None;
    }

    // 1. Search backward strictly before `before_offset` if `before_offset > 0`
    if before_offset > 0 {
        let end = (before_offset + pattern.len().saturating_sub(1)).min(data.len());
        if let Some(pos) = find_single_backward(data, pattern, 0, end) {
            return Some(pos);
        }
    }

    // 2. Wrap-around: search backward from end of data
    find_single_backward(data, pattern, 0, data.len())
}

/// Finds all occurrences of `pattern` within the specified `range` of `data`.
#[allow(dead_code)]
pub fn find_occurrences_in_range(data: &[u8], pattern: &[PatternByte], range: std::ops::Range<usize>) -> Vec<usize> {
    find_occurrences(data, pattern, SearchLimit::Unlimited, Some(range))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_exact_pattern(bytes: &[u8]) -> Vec<PatternByte> {
        bytes.iter().map(|&b| PatternByte::new_exact(b)).collect()
    }

    #[test]
    fn test_find_occurrences_text() {
        let data = b"Hello World Hello";
        let pattern = to_exact_pattern(b"Hello");
        let results = find_occurrences(data, &pattern, SearchLimit::Unlimited, None);
        assert_eq!(results, vec![0, 12]);
    }

    #[test]
    fn test_find_occurrences_limit_count() {
        let data = b"AA AA AA AA";
        let pattern = to_exact_pattern(b"AA");
        let results = find_occurrences(data, &pattern, SearchLimit::Count(2), None);
        assert_eq!(results, vec![0, 3]);
    }

    #[test]
    fn test_find_occurrences_limit_range() {
        let data = b"AA..AA....AA";
        let pattern = to_exact_pattern(b"AA");
        let results = find_occurrences(data, &pattern, SearchLimit::Range(5), None);
        assert_eq!(results, vec![0, 4]);
    }

    #[test]
    fn test_find_occurrences_range_restriction() {
        let data = b"0123456789";
        let pattern = to_exact_pattern(b"34");

        let results = find_occurrences(data, &pattern, SearchLimit::Unlimited, Some(2..6));
        assert_eq!(results, vec![3]);

        let results = find_occurrences(data, &pattern, SearchLimit::Unlimited, Some(5..8));
        assert!(results.is_empty());

        let results = find_occurrences(data, &pattern, SearchLimit::Unlimited, Some(0..3));
        assert!(results.is_empty());
    }

    #[test]
    fn test_empty_pattern_or_data() {
        assert!(find_occurrences(b"", &[], SearchLimit::Unlimited, None).is_empty());
        assert!(find_occurrences(b"data", &[], SearchLimit::Unlimited, None).is_empty());
        assert_eq!(find_next_occurrence(b"", &[], 0), None);
        assert_eq!(find_prev_occurrence(b"", &[], 0), None);
    }

    #[test]
    fn test_find_next_and_prev_occurrence() {
        let data = b"abc 123 abc 456 abc";
        let pattern = to_exact_pattern(b"abc");

        // Forward from 0 -> 0
        assert_eq!(find_next_occurrence(data, &pattern, 0), Some(0));
        // Forward from 1 -> 8
        assert_eq!(find_next_occurrence(data, &pattern, 1), Some(8));
        // Forward from 9 -> 16
        assert_eq!(find_next_occurrence(data, &pattern, 9), Some(16));
        // Forward from 17 -> wraps to 0
        assert_eq!(find_next_occurrence(data, &pattern, 17), Some(0));

        // Backward from 16 (matches at 0, 8, 16) -> strictly before 16 is 8
        assert_eq!(find_prev_occurrence(data, &pattern, 16), Some(8));
        // Backward before 8 is 0
        assert_eq!(find_prev_occurrence(data, &pattern, 8), Some(0));
        // Backward before 0 wraps to 16
        assert_eq!(find_prev_occurrence(data, &pattern, 0), Some(16));
    }

    #[test]
    fn test_find_occurrences_in_range() {
        let data = b"XX--XX--XX--XX";
        let pattern = to_exact_pattern(b"XX");

        let matches = find_occurrences_in_range(data, &pattern, 4..11);
        assert_eq!(matches, vec![4, 8]);
    }

    #[test]
    fn test_parse_hex_pattern() {
        let parsed = parse_hex_pattern("48 89 ?? 24 ?8").unwrap();
        assert_eq!(parsed.len(), 5);
        assert_eq!(parsed[0], PatternByte::new_exact(0x48));
        assert_eq!(parsed[1], PatternByte::new_exact(0x89));
        assert_eq!(parsed[2], PatternByte::new_wildcard());
        assert_eq!(parsed[3], PatternByte::new_exact(0x24));
        assert_eq!(parsed[4], PatternByte { value: 0x08, mask: 0x0F });

        // Contiguous
        let parsed2 = parse_hex_pattern("4889??24?8").unwrap();
        assert_eq!(parsed2, parsed);

        // Asterisk wildcard
        let parsed3 = parse_hex_pattern("48 89 ** 24 *8").unwrap();
        assert_eq!(parsed3, parsed);

        // Half-byte high-nibble wildcard
        let parsed4 = parse_hex_pattern("4?").unwrap();
        assert_eq!(parsed4.len(), 1);
        assert_eq!(parsed4[0], PatternByte { value: 0x40, mask: 0xF0 });

        // Single digit padding
        let parsed5 = parse_hex_pattern("A").unwrap();
        assert_eq!(parsed5.len(), 1);
        assert_eq!(parsed5[0], PatternByte::new_exact(0x0A));
    }

    #[test]
    fn test_find_occurrences_wildcard() {
        let data = &[0x48, 0x89, 0x54, 0x24, 0x08, 0x48, 0x89, 0x4c, 0x24, 0x18];
        let pattern = parse_hex_pattern("48 89 ?? 24 ?8").unwrap();
        let results = find_occurrences(data, &pattern, SearchLimit::Unlimited, None);
        assert_eq!(results, vec![0, 5]);

        let pattern_half = parse_hex_pattern("24 ?8").unwrap();
        let results_half = find_occurrences(data, &pattern_half, SearchLimit::Unlimited, None);
        assert_eq!(results_half, vec![3, 8]);
    }

    #[test]
    fn test_parse_text_pattern() {
        use crate::core::encoding::Encoding;

        // UTF-8
        let pat_utf8 = parse_text_pattern("ABC", Encoding::Utf8).unwrap();
        assert_eq!(
            pat_utf8,
            vec![PatternByte::new_exact(b'A'), PatternByte::new_exact(b'B'), PatternByte::new_exact(b'C')]
        );

        // Shift-JIS
        let pat_sjis = parse_text_pattern("こん", Encoding::ShiftJis).unwrap();
        assert_eq!(
            pat_sjis,
            vec![
                PatternByte::new_exact(0x82),
                PatternByte::new_exact(0xB1),
                PatternByte::new_exact(0x82),
                PatternByte::new_exact(0xF1),
            ]
        );

        // UTF-16 LE
        let pat_utf16le = parse_text_pattern("AB", Encoding::Utf16Le).unwrap();
        assert_eq!(
            pat_utf16le,
            vec![
                PatternByte::new_exact(0x41),
                PatternByte::new_exact(0x00),
                PatternByte::new_exact(0x42),
                PatternByte::new_exact(0x00),
            ]
        );

        // Incompatible character returns None
        assert!(parse_text_pattern("こんにちは", Encoding::Ascii).is_none());
        assert!(parse_text_pattern("", Encoding::Utf8).is_none());
    }

    #[test]
    fn test_find_occurrences_encoded_text() {
        use crate::core::encoding::Encoding;

        // Search Shift-JIS in buffer
        let data = [
            0x00, 0x01, 0x82, 0xB1, 0x82, 0xF1, 0x82, 0xC9, 0x82, 0xBF, 0x82, 0xCD, // "こんにちは" in SJIS at offset 2
            0x00, 0x82, 0xB1, 0x82, 0xF1, // "こん" in SJIS at offset 13
        ];
        let pattern = parse_text_pattern("こん", Encoding::ShiftJis).unwrap();
        let results = find_occurrences(&data, &pattern, SearchLimit::Unlimited, None);
        assert_eq!(results, vec![2, 13]);
    }

    #[test]
    fn test_find_occurrences_segmented_prevents_gap_crossing() {
        use crate::core::encoding::Encoding;

        // Data representing two separate memory segments packed contiguously
        let data = b"Hello 0Hello 1";
        let segments = vec![0..7, 7..14];

        // Pattern "0Hello" straddles the boundary (last byte of seg 0, first 5 bytes of seg 1)
        let straddle_pattern = parse_text_pattern("0Hello", Encoding::Ascii).unwrap();

        // Without segment awareness, it matches at offset 6 across the gap
        let unsegmented = find_occurrences(data, &straddle_pattern, SearchLimit::Unlimited, None);
        assert_eq!(unsegmented, vec![6]);

        // With segment awareness, matches across boundaries are forbidden
        let segmented = find_occurrences_segmented(data, &straddle_pattern, SearchLimit::Unlimited, &segments, None);
        assert!(segmented.is_empty());

        // Normal pattern "Hello" matches inside each segment independently
        let hello_pattern = parse_text_pattern("Hello", Encoding::Ascii).unwrap();
        let segmented_hello = find_occurrences_segmented(data, &hello_pattern, SearchLimit::Unlimited, &segments, None);
        assert_eq!(segmented_hello, vec![0, 7]);

        // Count limit respects max across segments
        let limited = find_occurrences_segmented(data, &hello_pattern, SearchLimit::Count(1), &segments, None);
        assert_eq!(limited, vec![0]);

        // Range filter restriction (e.g. viewport)
        let filtered = find_occurrences_segmented(data, &hello_pattern, SearchLimit::Unlimited, &segments, Some(5..14));
        assert_eq!(filtered, vec![7]);
    }
}
