use std::fmt;
use std::ops::Range;

/// The radix mode used for interpreting undecorated numeric offset inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GotoRadix {
    Hex,
    #[default]
    Dec,
}

#[allow(dead_code)]
impl GotoRadix {
    pub fn label(&self) -> &'static str {
        match self {
            GotoRadix::Hex => "Hex",
            GotoRadix::Dec => "Dec",
        }
    }
}

/// The jump origin / mode of the parsed offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GotoOrigin {
    Absolute,
    RelativeForward,
    RelativeBackward,
    FromEnd,
    Percentage,
    Line,
    Range,
}

/// The result of parsing a goto offset expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedGotoOffset {
    /// The target byte offset clamped to the document bounds.
    pub target_offset: usize,
    /// The raw unclamped target offset calculated from the expression.
    pub raw_target: usize,
    /// The origin / interpretation mode used for this offset.
    pub origin: GotoOrigin,
    /// True if the raw target offset exceeded the document size.
    pub is_out_of_bounds: bool,
    /// The half-open selection range `[start, end)` if the expression specified a range.
    pub selection_range: Option<Range<usize>>,
}

#[allow(dead_code)]
impl ParsedGotoOffset {
    pub fn is_range(&self) -> bool {
        self.selection_range.is_some()
    }
}

/// Errors that can occur when parsing a goto offset expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GotoParseError {
    Empty,
    InvalidFormat(String),
}

impl fmt::Display for GotoParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GotoParseError::Empty => write!(f, "Please enter an address"),
            GotoParseError::InvalidFormat(msg) => write!(f, "Invalid address: {}", msg),
        }
    }
}

impl std::error::Error for GotoParseError {}

/// Parses a number string with optional base prefix/suffix or default radix.
fn parse_number_with_base(s: &str, default_radix: GotoRadix) -> Result<usize, GotoParseError> {
    let clean: String = s.chars().filter(|c| *c != '_' && !c.is_whitespace()).collect();
    if clean.is_empty() {
        return Err(GotoParseError::Empty);
    }

    let lower = clean.to_ascii_lowercase();

    // 0x / 0X prefix
    if let Some(rest) = lower.strip_prefix("0x") {
        if rest.is_empty() {
            return Err(GotoParseError::InvalidFormat("Missing digits after 0x".into()));
        }
        return usize::from_str_radix(rest, 16).map_err(|_| GotoParseError::InvalidFormat(format!("'{}' is not a valid hex number", clean)));
    }

    // $ prefix (hex)
    if let Some(rest) = lower.strip_prefix('$') {
        if rest.is_empty() {
            return Err(GotoParseError::InvalidFormat("Missing digits after $".into()));
        }
        return usize::from_str_radix(rest, 16).map_err(|_| GotoParseError::InvalidFormat(format!("'{}' is not a valid hex number", clean)));
    }

    // 0d prefix or # prefix (decimal)
    if let Some(rest) = lower.strip_prefix("0d").or_else(|| lower.strip_prefix('#')) {
        if rest.is_empty() {
            return Err(GotoParseError::InvalidFormat("Missing digits after decimal prefix".into()));
        }
        return rest
            .parse::<usize>()
            .map_err(|_| GotoParseError::InvalidFormat(format!("'{}' is not a valid decimal number", clean)));
    }

    // 0o prefix (octal)
    if let Some(rest) = lower.strip_prefix("0o") {
        if rest.is_empty() {
            return Err(GotoParseError::InvalidFormat("Missing digits after 0o".into()));
        }
        return usize::from_str_radix(rest, 8).map_err(|_| GotoParseError::InvalidFormat(format!("'{}' is not a valid octal number", clean)));
    }

    // 0b prefix (binary)
    if let Some(rest) = lower.strip_prefix("0b") {
        if rest.is_empty() {
            return Err(GotoParseError::InvalidFormat("Missing digits after 0b".into()));
        }
        return usize::from_str_radix(rest, 2).map_err(|_| GotoParseError::InvalidFormat(format!("'{}' is not a valid binary number", clean)));
    }

    // h suffix (hex)
    if let Some(rest) = lower.strip_suffix('h')
        && !rest.is_empty()
        && rest.chars().all(|c| c.is_ascii_hexdigit())
    {
        return usize::from_str_radix(rest, 16).map_err(|_| GotoParseError::InvalidFormat(format!("'{}' is not a valid hex number", clean)));
    }

    // Auto-detect hex if contains a-f
    let contains_hex_letter = clean.chars().any(|c| matches!(c, 'a'..='f' | 'A'..='F'));
    if contains_hex_letter {
        return usize::from_str_radix(&clean, 16).map_err(|_| GotoParseError::InvalidFormat(format!("'{}' is not a valid hex number", clean)));
    }

    // Fallback to default radix
    match default_radix {
        GotoRadix::Hex => usize::from_str_radix(&clean, 16).map_err(|_| GotoParseError::InvalidFormat(format!("'{}' is not a valid hex number", clean))),
        GotoRadix::Dec => clean
            .parse::<usize>()
            .map_err(|_| GotoParseError::InvalidFormat(format!("'{}' is not a valid decimal number", clean))),
    }
}

/// Helper to detect and split range notation if present.
/// Supports `..=`, `...`, `..`, ` - `, and ` to ` (case-insensitive).
fn detect_range_split(input: &str) -> Option<(&str, &str)> {
    if let Some(pos) = input.find("..=") {
        return Some((&input[..pos], &input[pos + 3..]));
    }
    if let Some(pos) = input.find("...") {
        return Some((&input[..pos], &input[pos + 3..]));
    }
    if let Some(pos) = input.find("..") {
        return Some((&input[..pos], &input[pos + 2..]));
    }
    if let Some(pos) = input.find(" - ") {
        return Some((&input[..pos], &input[pos + 3..]));
    }
    let lower = input.to_ascii_lowercase();
    if let Some(pos) = lower.find(" to ") {
        return Some((&input[..pos], &input[pos + 4..]));
    }
    None
}

/// Parses a goto offset expression from user input.
///
/// Supports:
/// - Hexadecimal (`0x1000`, `1A0`, `$100`, `1000h`)
/// - Decimal (`256`, `0d256`, `#256`)
/// - Octal (`0o777`) and Binary (`0b10101010`)
/// - Relative forwards (`+0x100`, `+50`)
/// - Relative backwards (`-0x20`, `-10`)
/// - Relative from end (`end-0x10`, `eof-50`)
/// - Named positions (`begin`, `start`, `first`, `end`, `eof`, `last`)
/// - Percentage (`50%`, `75.5%`, `100%`)
/// - Line / Row syntax (`L10`, `line 10`, `:10`)
/// - Range selection (`0xC6..0x119`, `0xC6..=0x119`, `0x100..0x200`, `..0x50`, `0x100..`, `0xC6 - 0x119`)
///
/// Parses a goto offset expression from user input using an AddressMap to resolve physical memory addresses.
pub fn parse_goto_offset_with_map(
    input: &str,
    current_cursor: usize,
    total_size: usize,
    default_radix: GotoRadix,
    address_map: &crate::core::address_map::AddressMap,
) -> Result<ParsedGotoOffset, GotoParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(GotoParseError::Empty);
    }

    // Range syntax: "0xC6..0x119", "0xC6..=0x119", "0x100..0x200", "0xC6 - 0x119", "0x10..end", "..0x50", "0x100.."
    if let Some((left_str, right_str)) = detect_range_split(trimmed) {
        let left_trimmed = left_str.trim();
        let right_trimmed = right_str.trim();

        if detect_range_split(left_trimmed).is_some() || detect_range_split(right_trimmed).is_some() {
            return Err(GotoParseError::InvalidFormat("Multiple range operators in address".into()));
        }

        let left_parsed = if left_trimmed.is_empty() {
            ParsedGotoOffset {
                target_offset: 0,
                raw_target: 0,
                origin: GotoOrigin::Absolute,
                is_out_of_bounds: false,
                selection_range: None,
            }
        } else {
            parse_goto_offset_with_map(left_trimmed, current_cursor, total_size, default_radix, address_map)?
        };

        // Relative forward length: "+2", "+10", "+0x20" specifies length from start offset
        if let Some(num_str) = right_trimmed.strip_prefix('+') {
            let len = parse_number_with_base(num_str.trim(), default_radix)?;
            let start = left_parsed.target_offset.min(total_size);
            let end = start.saturating_add(len).min(total_size);
            let is_out_of_bounds = left_parsed.is_out_of_bounds || (left_parsed.target_offset.saturating_add(len) > total_size);
            return Ok(ParsedGotoOffset {
                target_offset: start,
                raw_target: left_parsed.raw_target,
                origin: GotoOrigin::Range,
                is_out_of_bounds,
                selection_range: Some(start..end),
            });
        }

        // Relative backward length: "-2", "-10", "-0x20" specifies length backwards from start offset
        if let Some(num_str) = right_trimmed.strip_prefix('-') {
            let len = parse_number_with_base(num_str.trim(), default_radix)?;
            let end = left_parsed.target_offset.min(total_size);
            let start = end.saturating_sub(len);
            let is_out_of_bounds = left_parsed.is_out_of_bounds || (len > left_parsed.target_offset);
            return Ok(ParsedGotoOffset {
                target_offset: start,
                raw_target: left_parsed.raw_target,
                origin: GotoOrigin::Range,
                is_out_of_bounds,
                selection_range: Some(start..end),
            });
        }

        let right_parsed = if right_trimmed.is_empty() {
            let target = total_size.saturating_sub(1);
            ParsedGotoOffset {
                target_offset: target,
                raw_target: target,
                origin: GotoOrigin::FromEnd,
                is_out_of_bounds: false,
                selection_range: None,
            }
        } else {
            parse_goto_offset_with_map(right_trimmed, current_cursor, total_size, default_radix, address_map)?
        };

        let min_offset = left_parsed.target_offset.min(right_parsed.target_offset);
        let max_offset = left_parsed.target_offset.max(right_parsed.target_offset);

        let start = min_offset.min(total_size);
        let end = if total_size == 0 { 0 } else { (max_offset.saturating_add(1)).min(total_size) };
        let range = start..end.max(start);

        let is_out_of_bounds = left_parsed.is_out_of_bounds || right_parsed.is_out_of_bounds;

        return Ok(ParsedGotoOffset {
            target_offset: start,
            raw_target: left_parsed.raw_target.min(right_parsed.raw_target),
            origin: GotoOrigin::Range,
            is_out_of_bounds,
            selection_range: Some(range),
        });
    }

    let lower = trimmed.to_ascii_lowercase();

    // Named positions
    if matches!(lower.as_str(), "begin" | "start" | "first") {
        return Ok(ParsedGotoOffset {
            target_offset: 0,
            raw_target: 0,
            origin: GotoOrigin::Absolute,
            is_out_of_bounds: false,
            selection_range: None,
        });
    }
    if matches!(lower.as_str(), "end" | "eof" | "last") {
        let target = total_size.saturating_sub(1);
        return Ok(ParsedGotoOffset {
            target_offset: target,
            raw_target: target,
            origin: GotoOrigin::FromEnd,
            is_out_of_bounds: false,
            selection_range: None,
        });
    }

    // Percentage: "50%", "75.5%"
    if let Some(pct_str) = trimmed.strip_suffix('%') {
        let pct_clean = pct_str.trim();
        let pct: f64 = pct_clean
            .parse()
            .map_err(|_| GotoParseError::InvalidFormat(format!("'{}' is not a valid percentage", pct_clean)))?;
        if pct < 0.0 {
            return Err(GotoParseError::InvalidFormat("Percentage cannot be negative".into()));
        }
        let raw = ((total_size as f64) * (pct / 100.0)).round() as usize;
        let clamped = if total_size == 0 { 0 } else { raw.min(total_size.saturating_sub(1)) };
        let is_out_of_bounds = raw >= total_size && total_size > 0;
        return Ok(ParsedGotoOffset {
            target_offset: clamped,
            raw_target: raw,
            origin: GotoOrigin::Percentage,
            is_out_of_bounds,
            selection_range: None,
        });
    }

    // Line / Row syntax: "L10", "l 10", ":10", "line 10"
    let line_str = if let Some(rest) = lower.strip_prefix("line") {
        Some(rest.trim())
    } else if let Some(rest) = lower.strip_prefix('l') {
        Some(rest.trim())
    } else {
        lower.strip_prefix(':').map(|rest| rest.trim())
    };

    if let Some(line_num_str) = line_str
        && !line_num_str.is_empty()
    {
        let line_num = parse_number_with_base(line_num_str, GotoRadix::Dec)?;
        // 1-indexed row: L1 = row 0 (offset 0), L2 = row 1 (offset 16)
        let row = line_num.saturating_sub(1);
        let raw = row.saturating_mul(16);
        let clamped = if total_size == 0 { 0 } else { raw.min(total_size.saturating_sub(1)) };
        let is_out_of_bounds = raw >= total_size && total_size > 0;
        return Ok(ParsedGotoOffset {
            target_offset: clamped,
            raw_target: raw,
            origin: GotoOrigin::Line,
            is_out_of_bounds,
            selection_range: None,
        });
    }

    // Relative from end: "end-0x10", "eof-50"
    let end_relative_str = if let Some(rest) = lower.strip_prefix("end-") {
        Some(rest.trim())
    } else {
        lower.strip_prefix("eof-").map(|rest| rest.trim())
    };

    if let Some(num_str) = end_relative_str {
        let val = parse_number_with_base(num_str, default_radix)?;
        let raw = total_size.saturating_sub(val);
        let clamped = if total_size == 0 { 0 } else { raw.min(total_size.saturating_sub(1)) };
        return Ok(ParsedGotoOffset {
            target_offset: clamped,
            raw_target: raw,
            origin: GotoOrigin::FromEnd,
            is_out_of_bounds: false,
            selection_range: None,
        });
    }

    // Relative forward: "+0x100", "+50"
    if let Some(num_str) = trimmed.strip_prefix('+') {
        let val = parse_number_with_base(num_str.trim(), default_radix)?;
        let raw = current_cursor.saturating_add(val);
        let clamped = if total_size == 0 { 0 } else { raw.min(total_size.saturating_sub(1)) };
        let is_out_of_bounds = raw >= total_size && total_size > 0;
        return Ok(ParsedGotoOffset {
            target_offset: clamped,
            raw_target: raw,
            origin: GotoOrigin::RelativeForward,
            is_out_of_bounds,
            selection_range: None,
        });
    }

    // Relative backward: "-0x20", "-10"
    if let Some(num_str) = trimmed.strip_prefix('-') {
        let val = parse_number_with_base(num_str.trim(), default_radix)?;
        let raw = current_cursor.saturating_sub(val);
        let clamped = if total_size == 0 { 0 } else { raw.min(total_size.saturating_sub(1)) };
        return Ok(ParsedGotoOffset {
            target_offset: clamped,
            raw_target: raw,
            origin: GotoOrigin::RelativeBackward,
            is_out_of_bounds: false,
            selection_range: None,
        });
    }

    // Segment:Offset syntax: "0000:0100" (both in Hex)
    if trimmed.contains(':') && !trimmed.starts_with(':') {
        let parts: Vec<&str> = trimmed.split(':').collect();
        if parts.len() == 2 {
            let seg = parse_number_with_base(parts[0].trim(), GotoRadix::Hex)?;
            let off = parse_number_with_base(parts[1].trim(), GotoRadix::Hex)?;
            let raw = seg.saturating_mul(16).saturating_add(off);
            let clamped = if total_size == 0 { 0 } else { raw.min(total_size.saturating_sub(1)) };
            let is_out_of_bounds = raw >= total_size && total_size > 0;
            return Ok(ParsedGotoOffset {
                target_offset: clamped,
                raw_target: raw,
                origin: GotoOrigin::Absolute,
                is_out_of_bounds,
                selection_range: None,
            });
        }
    }

    // Standard absolute address / offset
    let val = parse_number_with_base(trimmed, default_radix)?;

    // If document has custom base address or memory segments, map physical address to offset
    if (address_map.base_address() > 0 || address_map.has_gaps())
        && let Some(mapped_offset) = address_map.address_to_offset(val)
    {
        let clamped = if total_size == 0 {
            0
        } else {
            mapped_offset.min(total_size.saturating_sub(1))
        };
        let is_out_of_bounds = mapped_offset >= total_size && total_size > 0;
        return Ok(ParsedGotoOffset {
            target_offset: clamped,
            raw_target: val,
            origin: GotoOrigin::Absolute,
            is_out_of_bounds,
            selection_range: None,
        });
    }

    let clamped = if total_size == 0 { 0 } else { val.min(total_size.saturating_sub(1)) };
    let is_out_of_bounds = val >= total_size && total_size > 0;
    Ok(ParsedGotoOffset {
        target_offset: clamped,
        raw_target: val,
        origin: GotoOrigin::Absolute,
        is_out_of_bounds,
        selection_range: None,
    })
}

/// Parses a goto offset expression from user input.
#[allow(dead_code)]
pub fn parse_goto_offset(input: &str, current_cursor: usize, total_size: usize, default_radix: GotoRadix) -> Result<ParsedGotoOffset, GotoParseError> {
    parse_goto_offset_with_map(
        input,
        current_cursor,
        total_size,
        default_radix,
        &crate::core::address_map::AddressMap::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_formats() {
        let total = 0x10000;
        let cursor = 0x100;

        // 0x prefix
        let res = parse_goto_offset("0x200", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0x200);
        assert_eq!(res.origin, GotoOrigin::Absolute);
        assert!(!res.is_out_of_bounds);

        // $ prefix
        let res = parse_goto_offset("$300", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0x300);

        // h suffix
        let res = parse_goto_offset("400h", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0x400);

        // Contains hex letter a-f with Dec mode
        let res = parse_goto_offset("1a0", cursor, total, GotoRadix::Dec).unwrap();
        assert_eq!(res.target_offset, 0x1A0);

        // Underscores
        let res = parse_goto_offset("0x10_00", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0x1000);
    }

    #[test]
    fn test_parse_dec_formats() {
        let total = 10000;
        let cursor = 100;

        // Plain decimal with Dec mode
        let res = parse_goto_offset("500", cursor, total, GotoRadix::Dec).unwrap();
        assert_eq!(res.target_offset, 500);

        // 0d prefix with Hex mode
        let res = parse_goto_offset("0d500", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 500);

        // # prefix
        let res = parse_goto_offset("#500", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 500);
    }

    #[test]
    fn test_parse_oct_and_bin_formats() {
        let total = 10000;
        let cursor = 100;

        // 0o prefix (octal)
        let res = parse_goto_offset("0o77", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 63);

        // 0b prefix (binary)
        let res = parse_goto_offset("0b1010", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 10);
    }

    #[test]
    fn test_parse_relative_offsets() {
        let total = 0x1000;
        let cursor = 0x100;

        // Relative forward (+)
        let res = parse_goto_offset("+0x50", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0x150);
        assert_eq!(res.origin, GotoOrigin::RelativeForward);

        // Relative backward (-)
        let res = parse_goto_offset("-0x30", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0xD0);
        assert_eq!(res.origin, GotoOrigin::RelativeBackward);

        // Relative backward underflow saturates to 0
        let res = parse_goto_offset("-0x200", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0);

        // From end
        let res = parse_goto_offset("end-0x10", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0xFF0);
        assert_eq!(res.origin, GotoOrigin::FromEnd);
    }

    #[test]
    fn test_parse_named_positions() {
        let total = 0x1000;
        let cursor = 0x100;

        let res = parse_goto_offset("begin", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0);

        let res = parse_goto_offset("start", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0);

        let res = parse_goto_offset("end", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0xFFF);

        let res = parse_goto_offset("eof", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0xFFF);
    }

    #[test]
    fn test_parse_percentage() {
        let total = 1000;
        let cursor = 0;

        let res = parse_goto_offset("50%", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 500);
        assert_eq!(res.origin, GotoOrigin::Percentage);

        let res = parse_goto_offset("100%", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 999);
    }

    #[test]
    fn test_parse_line() {
        let total = 1000;
        let cursor = 0;

        // Line 1 -> row 0 -> offset 0
        let res = parse_goto_offset("L1", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0);
        assert_eq!(res.origin, GotoOrigin::Line);

        // Line 2 -> row 1 -> offset 16
        let res = parse_goto_offset("L2", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 16);

        // Line 10 -> row 9 -> offset 144
        let res = parse_goto_offset(":10", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 144);
    }

    #[test]
    fn test_parse_segment_offset() {
        let total = 0x10000;
        let cursor = 0;

        // 0010:0020 -> 0x10 * 16 + 0x20 = 0x120
        let res = parse_goto_offset("0010:0020", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0x120);
    }

    #[test]
    fn test_out_of_bounds_clamping() {
        let total = 0x100;
        let cursor = 0;

        let res = parse_goto_offset("0x500", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0xFF);
        assert_eq!(res.raw_target, 0x500);
        assert!(res.is_out_of_bounds);
    }

    #[test]
    fn test_invalid_formats() {
        let total = 1000;
        let cursor = 0;

        assert!(matches!(parse_goto_offset("", cursor, total, GotoRadix::Hex), Err(GotoParseError::Empty)));
        assert!(matches!(parse_goto_offset("   ", cursor, total, GotoRadix::Hex), Err(GotoParseError::Empty)));
        assert!(matches!(
            parse_goto_offset("0xZZZ", cursor, total, GotoRadix::Hex),
            Err(GotoParseError::InvalidFormat(_))
        ));
        assert!(matches!(
            parse_goto_offset("xyz", cursor, total, GotoRadix::Dec),
            Err(GotoParseError::InvalidFormat(_))
        ));
    }

    #[test]
    fn test_parse_range_formats() {
        let total = 0x1000;
        let cursor = 0;

        // Status bar copy format: "0xC6..0x119" (inclusive of 0x119, so buffer range is 0xC6..0x11A)
        let res = parse_goto_offset("0xC6..0x119", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0xC6);
        assert_eq!(res.origin, GotoOrigin::Range);
        assert_eq!(res.selection_range, Some(0xC6..0x11A));
        assert!(!res.is_out_of_bounds);
        assert!(res.is_range());

        // Inclusive range syntax: "0xC6..=0x119"
        let res = parse_goto_offset("0xC6..=0x119", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.selection_range, Some(0xC6..0x11A));

        // Three dots: "0xC6...0x119"
        let res = parse_goto_offset("0xC6...0x119", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.selection_range, Some(0xC6..0x11A));

        // Spaced hyphen: "0xC6 - 0x119"
        let res = parse_goto_offset("0xC6 - 0x119", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.selection_range, Some(0xC6..0x11A));

        // "to" keyword: "0xC6 to 0x119"
        let res = parse_goto_offset("0xC6 to 0x119", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.selection_range, Some(0xC6..0x11A));

        // Whitespace handling
        let res = parse_goto_offset("  0xC6  ..  0x119  ", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.selection_range, Some(0xC6..0x11A));
    }

    #[test]
    fn test_parse_range_relative_lengths() {
        let total = 0x1000;
        let cursor = 0x500; // different from 0x10 to ensure relative length is based on start offset

        // 0x10..+2 selects exactly 2 bytes (0x10..0x12)
        let res = parse_goto_offset("0x10..+2", cursor, total, GotoRadix::Dec).unwrap();
        assert_eq!(res.target_offset, 0x10);
        assert_eq!(res.selection_range, Some(0x10..0x12));
        assert_eq!(res.selection_range.unwrap().len(), 2);

        // 0x10..+10 in default Dec mode selects 10 bytes (0x10..0x1A)
        let res = parse_goto_offset("0x10..+10", cursor, total, GotoRadix::Dec).unwrap();
        assert_eq!(res.target_offset, 0x10);
        assert_eq!(res.selection_range, Some(0x10..0x1A));
        assert_eq!(res.selection_range.unwrap().len(), 10);

        // 0x10..+0x10 explicitly in Hex selects 16 bytes (0x10..0x20)
        let res = parse_goto_offset("0x10..+0x10", cursor, total, GotoRadix::Dec).unwrap();
        assert_eq!(res.target_offset, 0x10);
        assert_eq!(res.selection_range, Some(0x10..0x20));
        assert_eq!(res.selection_range.unwrap().len(), 16);

        // 0x20..-4 selects 4 bytes backwards (0x1C..0x20)
        let res = parse_goto_offset("0x20..-4", cursor, total, GotoRadix::Dec).unwrap();
        assert_eq!(res.target_offset, 0x1C);
        assert_eq!(res.selection_range, Some(0x1C..0x20));
        assert_eq!(res.selection_range.unwrap().len(), 4);
    }

    #[test]
    fn test_parse_range_reversed_and_single_byte() {
        let total = 0x1000;
        let cursor = 0;

        // Reversed range is normalized
        let res = parse_goto_offset("0x119..0xC6", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0xC6);
        assert_eq!(res.selection_range, Some(0xC6..0x11A));

        // Single byte range: 0x5..0x5 selects exactly 1 byte (5..6)
        let res = parse_goto_offset("0x5..0x5", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0x5);
        assert_eq!(res.selection_range, Some(0x5..0x6));
        assert_eq!(res.selection_range.unwrap().len(), 1);
    }

    #[test]
    fn test_parse_range_open_ended_and_named() {
        let total = 0x200;
        let cursor = 0;

        // Open-ended left: "..0x50" -> 0..0x51
        let res = parse_goto_offset("..0x50", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0);
        assert_eq!(res.selection_range, Some(0..0x51));

        // Open-ended right: "0x100.." -> 0x100..0x200
        let res = parse_goto_offset("0x100..", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0x100);
        assert_eq!(res.selection_range, Some(0x100..0x200));

        // Entire document: ".."
        let res = parse_goto_offset("..", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0);
        assert_eq!(res.selection_range, Some(0..0x200));

        // Named positions: "begin..0x50", "0x100..end"
        let res = parse_goto_offset("begin..0x50", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.selection_range, Some(0..0x51));

        let res = parse_goto_offset("0x100..end", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.selection_range, Some(0x100..0x200));
    }

    #[test]
    fn test_parse_range_out_of_bounds() {
        let total = 0x100;
        let cursor = 0;

        let res = parse_goto_offset("0x50..0x200", cursor, total, GotoRadix::Hex).unwrap();
        assert_eq!(res.target_offset, 0x50);
        assert_eq!(res.selection_range, Some(0x50..0x100));
        assert!(res.is_out_of_bounds);
    }

    #[test]
    fn test_parse_range_with_address_map() {
        let map = crate::core::address_map::AddressMap::single_segment(0x8000_0000, 0x1000);
        let res = parse_goto_offset_with_map("0x8000_00C6..0x8000_0119", 0, 0x1000, GotoRadix::Hex, &map).unwrap();
        assert_eq!(res.target_offset, 0xC6);
        assert_eq!(res.selection_range, Some(0xC6..0x11A));
        assert!(!res.is_out_of_bounds);
    }

    #[test]
    fn test_parse_range_invalid() {
        let total = 1000;
        let cursor = 0;

        assert!(matches!(
            parse_goto_offset("0x10..0x20..0x30", cursor, total, GotoRadix::Hex),
            Err(GotoParseError::InvalidFormat(_))
        ));
        assert!(matches!(
            parse_goto_offset("0xZZ..0x100", cursor, total, GotoRadix::Hex),
            Err(GotoParseError::InvalidFormat(_))
        ));
        assert!(matches!(
            parse_goto_offset("0x100..0xZZ", cursor, total, GotoRadix::Hex),
            Err(GotoParseError::InvalidFormat(_))
        ));
    }
}
