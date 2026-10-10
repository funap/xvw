use std::ops::Range;

/// Represents a navigation target parsed from an offset hyperlink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OffsetLinkTarget {
    /// Jump to a single offset (cursor position).
    Offset(usize),
    /// Jump to and select a contiguous byte range `[start..end)`.
    Range(Range<usize>),
}

#[allow(dead_code)]
impl OffsetLinkTarget {
    /// Returns the beginning offset of this target.
    pub fn start_offset(&self) -> usize {
        match self {
            OffsetLinkTarget::Offset(off) => *off,
            OffsetLinkTarget::Range(range) => range.start,
        }
    }

    /// Returns the range if this target specifies a range.
    pub fn as_range(&self) -> Option<Range<usize>> {
        match self {
            OffsetLinkTarget::Offset(_) => None,
            OffsetLinkTarget::Range(range) => Some(range.clone()),
        }
    }
}

/// Helper to parse an integer (decimal or hex with 0x prefix).
fn parse_number(s: &str) -> Option<usize> {
    let clean = s.trim().replace('_', "");
    if clean.is_empty() {
        return None;
    }
    let lower = clean.to_ascii_lowercase();
    if let Some(hex_str) = lower.strip_prefix("0x").or_else(|| lower.strip_prefix('$')) {
        usize::from_str_radix(hex_str, 16).ok()
    } else if let Some(hex_str) = lower.strip_suffix('h') {
        usize::from_str_radix(hex_str, 16).ok()
    } else if clean.chars().any(|c| matches!(c, 'a'..='f' | 'A'..='F')) {
        usize::from_str_radix(&clean, 16).ok()
    } else {
        clean.parse::<usize>().ok()
    }
}

/// Helper to parse a range string like `0x1000..0x1040`, `0x1000..+64`, `0x1000-0x1040`.
fn parse_range_spec(s: &str) -> Option<OffsetLinkTarget> {
    // Check for `..` or `...` or `..=`
    if let Some(pos) = s.find("..") {
        let start_str = &s[..pos];
        let end_part = s[pos + 2..].trim_start_matches(['.', '=']);
        let start = parse_number(start_str)?;
        if let Some(len_str) = end_part.strip_prefix('+') {
            let len = parse_number(len_str)?;
            return Some(OffsetLinkTarget::Range(start..start.saturating_add(len)));
        }
        let end = parse_number(end_part)?;
        let min_offset = start.min(end);
        let max_offset = start.max(end);
        return Some(OffsetLinkTarget::Range(min_offset..max_offset.saturating_add(1)));
    }

    // Check for `-` (with boundary checks)
    if let Some(pos) = s.find('-') {
        let start_str = &s[..pos];
        let end_str = &s[pos + 1..];
        let start = parse_number(start_str)?;
        let end = parse_number(end_str)?;
        let min_offset = start.min(end);
        let max_offset = start.max(end);
        return Some(OffsetLinkTarget::Range(min_offset..max_offset.saturating_add(1)));
    }

    parse_number(s).map(OffsetLinkTarget::Offset)
}

/// Parses a potential offset or shorthand expression into an `OffsetLinkTarget`.
///
/// Supported formats:
/// - Shorthands: `#0x1040`, `@0x1040`
/// - Direct hex: `0x1040`, `0x1000..0x1040`, `0x1000-0x1040`, `0x1000..+64`
/// - Optional `offset:` URI scheme
pub fn parse_offset_link(url: &str) -> Option<OffsetLinkTarget> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }

    let clean = trimmed.strip_prefix("offset:").unwrap_or(trimmed).trim();
    if clean.is_empty() {
        return None;
    }

    // Shorthand with leading `#` or `@`
    if let Some(rest) = clean.strip_prefix('#').or_else(|| clean.strip_prefix('@')) {
        return parse_range_spec(rest);
    }

    // Direct hex pattern: e.g. "0x1000" or "0x1000..0x1040"
    if clean.starts_with("0x") || clean.starts_with("0X") {
        return parse_range_spec(clean);
    }

    None
}

fn is_token_delimiter(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '[' | ']' | '(' | ')' | '{' | '}' | '<' | '>' | ',' | ';')
}

/// Finds an offset-like token at `offset` in `text`, returning its byte range and parsed target.
pub fn find_offset_token_at(text: &str, offset: usize) -> Option<(std::ops::Range<usize>, OffsetLinkTarget)> {
    if text.is_empty() || offset > text.len() {
        return None;
    }

    let safe_offset = text.floor_char_boundary(offset.min(text.len()));

    let line_start = text[..safe_offset].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
    let line_end = text[safe_offset..].find('\n').map(|idx| safe_offset + idx).unwrap_or(text.len());
    let line = &text[line_start..line_end];
    let rel_offset = safe_offset - line_start;

    // Scan backwards from rel_offset for delimiter
    let mut start = rel_offset;
    for (i, c) in line[..rel_offset].char_indices().rev() {
        if is_token_delimiter(c) {
            start = i + c.len_utf8();
            break;
        }
        start = i;
    }

    // Scan forwards from rel_offset for delimiter
    let mut end = rel_offset;
    for (i, c) in line[rel_offset..].char_indices() {
        if is_token_delimiter(c) {
            end = rel_offset + i;
            break;
        }
        end = rel_offset + i + c.len_utf8();
    }

    if start >= end {
        return None;
    }

    let mut raw_token = &line[start..end];

    // Trim trailing punctuation if not part of range syntax (e.g. "0x1000." -> "0x1000", but keep "0x1000..0x2000")
    if raw_token.ends_with('.') && !raw_token.ends_with("..") {
        let trimmed_len = raw_token.trim_end_matches('.').len();
        end = start + trimmed_len;
        raw_token = &line[start..end];
    }

    let trimmed_len = raw_token.trim_end_matches([':', ',']).len();
    end = start + trimmed_len;
    raw_token = &line[start..end];

    let target = parse_offset_link(raw_token)?;
    let abs_range = (line_start + start)..(line_start + end);

    if abs_range.contains(&safe_offset) || (safe_offset == abs_range.end && safe_offset > 0) {
        Some((abs_range, target))
    } else {
        None
    }
}

/// Determines hexadecimal padding width based on file size or offset.
fn hex_padding(total_size: usize, offset: usize) -> usize {
    let max_val = total_size.max(offset);
    if max_val > 0xFFFF_FFFF {
        16
    } else if max_val > 0xFFFF {
        8
    } else {
        4
    }
}

/// Formats an offset or range as plain text (e.g. `0x00001040` or `0x00001000..0x0000103F`).
pub fn format_offset_plain(offset: usize, length: Option<usize>, total_size: usize) -> String {
    let pad = hex_padding(total_size, offset);
    match length {
        Some(len) if len > 1 => {
            let end_inclusive = offset + len - 1;
            format!("0x{:0width$X}..0x{:0width$X}", offset, end_inclusive, width = pad)
        }
        _ => {
            format!("0x{:0width$X}", offset, width = pad)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_offset_plain() {
        assert_eq!(format_offset_plain(0x10, None, 0x100), "0x0010");
        assert_eq!(format_offset_plain(0x10, Some(16), 0x100), "0x0010..0x001F");
    }

    #[test]
    fn test_parse_offset_link_shorthands() {
        assert_eq!(parse_offset_link("#0x1040"), Some(OffsetLinkTarget::Offset(0x1040)));
        assert_eq!(parse_offset_link("@0x1040"), Some(OffsetLinkTarget::Offset(0x1040)));
        assert_eq!(parse_offset_link("0x1040"), Some(OffsetLinkTarget::Offset(0x1040)));
        // Inclusive range: 0x1000..0x1040 includes byte 0x1040, so half-open buffer range is 0x1000..0x1041
        assert_eq!(parse_offset_link("0x1000..0x1040"), Some(OffsetLinkTarget::Range(0x1000..0x1041)));
        assert_eq!(parse_offset_link("0x1000..=0x1040"), Some(OffsetLinkTarget::Range(0x1000..0x1041)));
        assert_eq!(parse_offset_link("0x1000...0x1040"), Some(OffsetLinkTarget::Range(0x1000..0x1041)));
        assert_eq!(parse_offset_link("0x1000-0x1040"), Some(OffsetLinkTarget::Range(0x1000..0x1041)));
        assert_eq!(parse_offset_link("offset:0x1000..0x1040"), Some(OffsetLinkTarget::Range(0x1000..0x1041)));
        // format_offset_plain round-trip: 16 bytes starting at 0x10 formatted as 0x0010..0x001F
        assert_eq!(parse_offset_link("0x0010..0x001F"), Some(OffsetLinkTarget::Range(0x10..0x20)));
        // Relative length: +32 bytes from 0x1000
        assert_eq!(parse_offset_link("@0x1000..+32"), Some(OffsetLinkTarget::Range(0x1000..0x1020)));
        // Single byte range
        assert_eq!(parse_offset_link("0x1000..0x1000"), Some(OffsetLinkTarget::Range(0x1000..0x1001)));
    }

    #[test]
    fn test_parse_offset_link_invalid() {
        assert_eq!(parse_offset_link(""), None);
        assert_eq!(parse_offset_link("https://google.com"), None);
        assert_eq!(parse_offset_link("xvw://goto/0x1040"), None);
        assert_eq!(parse_offset_link("not-an-address"), None);
    }

    #[test]
    fn test_find_offset_token_at() {
        let text = "Found header at 0x1000 and range 0x2000..0x2040.\nCheck @0x300 here.";

        // 0x1000 is at index 16..22
        let at_16 = find_offset_token_at(text, 16).expect("should find 0x1000 at start");
        assert_eq!(at_16.0, 16..22);
        assert_eq!(at_16.1, OffsetLinkTarget::Offset(0x1000));

        let at_19 = find_offset_token_at(text, 19).expect("should find 0x1000 in middle");
        assert_eq!(at_19.0, 16..22);
        assert_eq!(at_19.1, OffsetLinkTarget::Offset(0x1000));

        // 0x2000..0x2040 is at index 33..47 (inclusive of 0x2040, so buffer range is 0x2000..0x2041)
        let at_35 = find_offset_token_at(text, 35).expect("should find range");
        assert_eq!(at_35.0, 33..47);
        assert_eq!(at_35.1, OffsetLinkTarget::Range(0x2000..0x2041));

        // @0x300 is at index 55..61
        let at_56 = find_offset_token_at(text, 56).expect("should find @0x300");
        assert_eq!(at_56.0, 55..61);
        assert_eq!(at_56.1, OffsetLinkTarget::Offset(0x300));

        // Regular word should return None
        assert_eq!(find_offset_token_at(text, 0), None);
        assert_eq!(find_offset_token_at(text, 7), None);
    }
}
