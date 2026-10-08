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
    // Check for `..` or `...`
    if let Some(pos) = s.find("..") {
        let start_str = &s[..pos];
        let end_part = s[pos + 2..].trim_start_matches('.');
        let start = parse_number(start_str)?;
        if let Some(len_str) = end_part.strip_prefix('+') {
            let len = parse_number(len_str)?;
            return Some(OffsetLinkTarget::Range(start..start + len));
        }
        let end = parse_number(end_part)?;
        if end >= start {
            return Some(OffsetLinkTarget::Range(start..end));
        }
        return Some(OffsetLinkTarget::Range(end..start));
    }

    // Check for `-` (with boundary checks)
    if let Some(pos) = s.find('-') {
        let start_str = &s[..pos];
        let end_str = &s[pos + 1..];
        let start = parse_number(start_str)?;
        let end = parse_number(end_str)?;
        if end >= start {
            return Some(OffsetLinkTarget::Range(start..end));
        }
        return Some(OffsetLinkTarget::Range(end..start));
    }

    parse_number(s).map(OffsetLinkTarget::Offset)
}

/// Parses a potential offset link URL or shorthand expression into an `OffsetLinkTarget`.
///
/// Supported formats:
/// - `xvw://goto/0x1040`
/// - `xvw://goto/0x1040?len=64` or `?len=0x40`
/// - `xvw://goto/0x1000?end=0x1040`
/// - `xvw://goto/0x1000..0x1040`
/// - `xvw://offset/0x1040`
/// - `xvw://range/0x1000..0x1040`
/// - Shorthands: `#0x1040`, `@0x1040`, `0x1040`, `0x1000..0x1040`
pub fn parse_offset_link(url: &str) -> Option<OffsetLinkTarget> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Standard xvw URI scheme: `xvw://...`
    if let Some(rest) = trimmed.strip_prefix("xvw://") {
        let (path_part, query_part) = match rest.find('?') {
            Some(idx) => (&rest[..idx], Some(&rest[idx + 1..])),
            None => (rest, None),
        };

        let target_str = path_part
            .strip_prefix("goto/")
            .or_else(|| path_part.strip_prefix("offset/"))
            .or_else(|| path_part.strip_prefix("range/"))
            .unwrap_or(path_part)
            .trim_matches('/');

        // Check query parameters: `len=...` or `end=...`
        if let Some(query) = query_part {
            let start = parse_number(target_str)?;
            for param in query.split('&') {
                if let Some(len_str) = param.strip_prefix("len=")
                    && let Some(len) = parse_number(len_str)
                {
                    return Some(OffsetLinkTarget::Range(start..start + len));
                } else if let Some(end_str) = param.strip_prefix("end=")
                    && let Some(end) = parse_number(end_str)
                {
                    let r = if end >= start { start..end } else { end..start };
                    return Some(OffsetLinkTarget::Range(r));
                }
            }
        }

        return parse_range_spec(target_str);
    }

    // Shorthand with leading `#` or `@`
    if let Some(rest) = trimmed.strip_prefix('#').or_else(|| trimmed.strip_prefix('@')) {
        return parse_range_spec(rest);
    }

    // Direct hex pattern: e.g. "0x1000" or "0x1000..0x1040"
    if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        return parse_range_spec(trimmed);
    }

    None
}

/// Computes the URL for an offset or range link.
pub fn format_offset_url(offset: usize, length: Option<usize>) -> String {
    match length {
        Some(len) if len > 0 => format!("xvw://goto/0x{:X}?len={}", offset, len),
        _ => format!("xvw://goto/0x{:X}", offset),
    }
}

/// Formats a byte size into a concise human-readable string (e.g. `64 B`, `1.2 KiB`).
fn format_size_human(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        let kib = bytes as f64 / 1024.0;
        format!("{kib:.1} KiB")
    } else {
        let mib = bytes as f64 / (1024.0 * 1024.0);
        format!("{mib:.1} MiB")
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

/// Formats a full Markdown link string for an offset or range.
///
/// Examples:
/// - Single offset: `[0x00001040](xvw://goto/0x1040)`
/// - Range: `[0x00001000..0x0000103F (64 B)](xvw://goto/0x1000?len=64)`
pub fn format_offset_markdown(offset: usize, length: Option<usize>, total_size: usize) -> String {
    let pad = hex_padding(total_size, offset);
    let url = format_offset_url(offset, length);

    match length {
        Some(len) if len > 1 => {
            let end_inclusive = offset + len - 1;
            let size_str = format_size_human(len);
            let label = format!("0x{:0width$X}..0x{:0width$X} ({})", offset, end_inclusive, size_str, width = pad);
            format!("[{label}]({url})")
        }
        _ => {
            let label = format!("0x{:0width$X}", offset, width = pad);
            format!("[{label}]({url})")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_offset_link_goto() {
        assert_eq!(parse_offset_link("xvw://goto/0x1040"), Some(OffsetLinkTarget::Offset(0x1040)));
        assert_eq!(parse_offset_link("xvw://goto/4160"), Some(OffsetLinkTarget::Offset(4160)));
        assert_eq!(parse_offset_link("xvw://goto/0x1000?len=64"), Some(OffsetLinkTarget::Range(0x1000..0x1040)));
        assert_eq!(parse_offset_link("xvw://goto/0x1000?len=0x40"), Some(OffsetLinkTarget::Range(0x1000..0x1040)));
        assert_eq!(parse_offset_link("xvw://goto/0x1000?end=0x1080"), Some(OffsetLinkTarget::Range(0x1000..0x1080)));
        assert_eq!(parse_offset_link("xvw://goto/0x1000..0x1080"), Some(OffsetLinkTarget::Range(0x1000..0x1080)));
    }

    #[test]
    fn test_parse_offset_link_other_schemes() {
        assert_eq!(parse_offset_link("xvw://offset/0x200"), Some(OffsetLinkTarget::Offset(0x200)));
        assert_eq!(parse_offset_link("xvw://range/0x100..0x200"), Some(OffsetLinkTarget::Range(0x100..0x200)));
    }

    #[test]
    fn test_parse_offset_link_shorthands() {
        assert_eq!(parse_offset_link("#0x1040"), Some(OffsetLinkTarget::Offset(0x1040)));
        assert_eq!(parse_offset_link("@0x1040"), Some(OffsetLinkTarget::Offset(0x1040)));
        assert_eq!(parse_offset_link("0x1040"), Some(OffsetLinkTarget::Offset(0x1040)));
        assert_eq!(parse_offset_link("0x1000..0x1040"), Some(OffsetLinkTarget::Range(0x1000..0x1040)));
        assert_eq!(parse_offset_link("@0x1000..+32"), Some(OffsetLinkTarget::Range(0x1000..0x1020)));
    }

    #[test]
    fn test_parse_offset_link_invalid() {
        assert_eq!(parse_offset_link(""), None);
        assert_eq!(parse_offset_link("https://google.com"), None);
        assert_eq!(parse_offset_link("not-an-address"), None);
    }

    #[test]
    fn test_format_offset_url() {
        assert_eq!(format_offset_url(0x1040, None), "xvw://goto/0x1040");
        assert_eq!(format_offset_url(0x1000, Some(64)), "xvw://goto/0x1000?len=64");
    }

    #[test]
    fn test_format_offset_markdown() {
        let md_single = format_offset_markdown(0x1040, None, 0x10000);
        assert_eq!(md_single, "[0x00001040](xvw://goto/0x1040)");

        let md_range = format_offset_markdown(0x1000, Some(64), 0x10000);
        assert_eq!(md_range, "[0x00001000..0x0000103F (64 B)](xvw://goto/0x1000?len=64)");
    }
}
