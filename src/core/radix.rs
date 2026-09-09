use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DisplayRadix {
    #[default]
    Hexadecimal,
    Decimal,
    Octal,
    Binary,
}

#[allow(dead_code)]
impl DisplayRadix {
    pub const ALL: [DisplayRadix; 4] = [DisplayRadix::Hexadecimal, DisplayRadix::Decimal, DisplayRadix::Octal, DisplayRadix::Binary];

    pub fn label(&self) -> &'static str {
        match self {
            DisplayRadix::Hexadecimal => "Hexadecimal",
            DisplayRadix::Decimal => "Decimal",
            DisplayRadix::Octal => "Octal",
            DisplayRadix::Binary => "Binary",
        }
    }

    pub fn short_label(&self) -> &'static str {
        match self {
            DisplayRadix::Hexadecimal => "HEX",
            DisplayRadix::Decimal => "DEC",
            DisplayRadix::Octal => "OCT",
            DisplayRadix::Binary => "BIN",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ByteGroupSize {
    #[default]
    One = 1,
    Two = 2,
    Four = 4,
    Eight = 8,
}

#[allow(dead_code)]
impl ByteGroupSize {
    pub const ALL: [ByteGroupSize; 4] = [ByteGroupSize::One, ByteGroupSize::Two, ByteGroupSize::Four, ByteGroupSize::Eight];

    pub fn byte_count(&self) -> usize {
        match self {
            ByteGroupSize::One => 1,
            ByteGroupSize::Two => 2,
            ByteGroupSize::Four => 4,
            ByteGroupSize::Eight => 8,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ByteGroupSize::One => "1 Byte (8-bit)",
            ByteGroupSize::Two => "2 Bytes (16-bit)",
            ByteGroupSize::Four => "4 Bytes (32-bit)",
            ByteGroupSize::Eight => "8 Bytes (64-bit)",
        }
    }

    pub fn short_label(&self) -> &'static str {
        match self {
            ByteGroupSize::One => "1B",
            ByteGroupSize::Two => "2B",
            ByteGroupSize::Four => "4B",
            ByteGroupSize::Eight => "8B",
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ByteOrder {
    #[default]
    LittleEndian,
    BigEndian,
}

#[allow(dead_code)]
impl ByteOrder {
    pub fn is_big_endian(&self) -> bool {
        matches!(self, ByteOrder::BigEndian)
    }

    pub fn label(&self) -> &'static str {
        match self {
            ByteOrder::LittleEndian => "Little Endian",
            ByteOrder::BigEndian => "Big Endian",
        }
    }

    pub fn short_label(&self) -> &'static str {
        match self {
            ByteOrder::LittleEndian => "LE",
            ByteOrder::BigEndian => "BE",
        }
    }
}

/// Returns the fixed character width for a given radix and group size combination.
pub fn digit_count(radix: DisplayRadix, group_size: ByteGroupSize) -> usize {
    match (group_size, radix) {
        (ByteGroupSize::One, DisplayRadix::Hexadecimal) => 2,
        (ByteGroupSize::One, DisplayRadix::Decimal) => 3,
        (ByteGroupSize::One, DisplayRadix::Octal) => 3,
        (ByteGroupSize::One, DisplayRadix::Binary) => 8,

        (ByteGroupSize::Two, DisplayRadix::Hexadecimal) => 4,
        (ByteGroupSize::Two, DisplayRadix::Decimal) => 5,
        (ByteGroupSize::Two, DisplayRadix::Octal) => 6,
        (ByteGroupSize::Two, DisplayRadix::Binary) => 16,

        (ByteGroupSize::Four, DisplayRadix::Hexadecimal) => 8,
        (ByteGroupSize::Four, DisplayRadix::Decimal) => 10,
        (ByteGroupSize::Four, DisplayRadix::Octal) => 11,
        (ByteGroupSize::Four, DisplayRadix::Binary) => 32,

        (ByteGroupSize::Eight, DisplayRadix::Hexadecimal) => 16,
        (ByteGroupSize::Eight, DisplayRadix::Decimal) => 20,
        (ByteGroupSize::Eight, DisplayRadix::Octal) => 22,
        (ByteGroupSize::Eight, DisplayRadix::Binary) => 64,
    }
}

/// Formats a group slice of bytes based on the selected Radix, Group Size, Endianness, and starting slot within the group.
///
/// If `start_slot == 0 && bytes.len() == group_size.byte_count()`, the integer value is formatted with exact padding (space padding for decimal, zero padding for hex/octal/binary).
/// If `bytes.len() < group_size.byte_count()` or `start_slot > 0` (e.g. midway line break), each byte slot `k` (0..group_size)
/// is formatted at its exact positional offset, with missing slots padded with '.' so byte position can be identified.
pub fn format_group(bytes: &[u8], start_slot: usize, radix: DisplayRadix, group_size: ByteGroupSize, is_big_endian: bool) -> String {
    let expected = group_size.byte_count();
    let total_digits = digit_count(radix, group_size);

    if bytes.is_empty() {
        return ".".repeat(total_digits);
    }

    if radix == DisplayRadix::Decimal {
        let copy_len = bytes.len().min(expected.saturating_sub(start_slot));
        return match group_size {
            ByteGroupSize::One => format!("{:3}", bytes[0]),
            ByteGroupSize::Two => {
                let mut arr = [0u8; 2];
                arr[start_slot..start_slot + copy_len].copy_from_slice(&bytes[..copy_len]);
                let val = if is_big_endian { u16::from_be_bytes(arr) } else { u16::from_le_bytes(arr) };
                format!("{:5}", val)
            }
            ByteGroupSize::Four => {
                let mut arr = [0u8; 4];
                arr[start_slot..start_slot + copy_len].copy_from_slice(&bytes[..copy_len]);
                let val = if is_big_endian { u32::from_be_bytes(arr) } else { u32::from_le_bytes(arr) };
                format!("{:10}", val)
            }
            ByteGroupSize::Eight => {
                let mut arr = [0u8; 8];
                arr[start_slot..start_slot + copy_len].copy_from_slice(&bytes[..copy_len]);
                let val = if is_big_endian { u64::from_be_bytes(arr) } else { u64::from_le_bytes(arr) };
                format!("{:20}", val)
            }
        };
    }

    if radix == DisplayRadix::Octal {
        let copy_len = bytes.len().min(expected.saturating_sub(start_slot));
        return match group_size {
            ByteGroupSize::One => format!("{:03o}", bytes[0]),
            ByteGroupSize::Two => {
                let mut arr = [0u8; 2];
                arr[start_slot..start_slot + copy_len].copy_from_slice(&bytes[..copy_len]);
                let val = if is_big_endian { u16::from_be_bytes(arr) } else { u16::from_le_bytes(arr) };
                format!("{:06o}", val)
            }
            ByteGroupSize::Four => {
                let mut arr = [0u8; 4];
                arr[start_slot..start_slot + copy_len].copy_from_slice(&bytes[..copy_len]);
                let val = if is_big_endian { u32::from_be_bytes(arr) } else { u32::from_le_bytes(arr) };
                format!("{:011o}", val)
            }
            ByteGroupSize::Eight => {
                let mut arr = [0u8; 8];
                arr[start_slot..start_slot + copy_len].copy_from_slice(&bytes[..copy_len]);
                let val = if is_big_endian { u64::from_be_bytes(arr) } else { u64::from_le_bytes(arr) };
                format!("{:022o}", val)
            }
        };
    }

    if start_slot == 0 && bytes.len() >= expected {
        match (group_size, radix) {
            (ByteGroupSize::One, DisplayRadix::Hexadecimal) => format!("{:02x}", bytes[0]),
            (ByteGroupSize::One, DisplayRadix::Binary) => format!("{:08b}", bytes[0]),

            (ByteGroupSize::Two, DisplayRadix::Hexadecimal) => {
                let arr: [u8; 2] = [bytes[0], bytes[1]];
                let val = if is_big_endian { u16::from_be_bytes(arr) } else { u16::from_le_bytes(arr) };
                format!("{:04x}", val)
            }
            (ByteGroupSize::Two, DisplayRadix::Binary) => {
                let arr: [u8; 2] = [bytes[0], bytes[1]];
                let val = if is_big_endian { u16::from_be_bytes(arr) } else { u16::from_le_bytes(arr) };
                format!("{:016b}", val)
            }

            (ByteGroupSize::Four, DisplayRadix::Hexadecimal) => {
                let arr: [u8; 4] = [bytes[0], bytes[1], bytes[2], bytes[3]];
                let val = if is_big_endian { u32::from_be_bytes(arr) } else { u32::from_le_bytes(arr) };
                format!("{:08x}", val)
            }
            (ByteGroupSize::Four, DisplayRadix::Binary) => {
                let arr: [u8; 4] = [bytes[0], bytes[1], bytes[2], bytes[3]];
                let val = if is_big_endian { u32::from_be_bytes(arr) } else { u32::from_le_bytes(arr) };
                format!("{:032b}", val)
            }

            (ByteGroupSize::Eight, DisplayRadix::Hexadecimal) => {
                let arr: [u8; 8] = [bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]];
                let val = if is_big_endian { u64::from_be_bytes(arr) } else { u64::from_le_bytes(arr) };
                format!("{:016x}", val)
            }
            (ByteGroupSize::Eight, DisplayRadix::Binary) => {
                let arr: [u8; 8] = [bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]];
                let val = if is_big_endian { u64::from_be_bytes(arr) } else { u64::from_le_bytes(arr) };
                format!("{:064b}", val)
            }
            _ => unreachable!(),
        }
    } else {
        // Partial group for Hexadecimal and Binary: format each slot k in 0..expected
        let mut formatted = String::with_capacity(total_digits);
        for k in 0..expected {
            if k >= start_slot && (k - start_slot) < bytes.len() {
                let b = bytes[k - start_slot];
                match radix {
                    DisplayRadix::Hexadecimal => formatted.push_str(&format!("{:02x}", b)),
                    DisplayRadix::Binary => formatted.push_str(&format!("{:08b}", b)),
                    _ => unreachable!(),
                }
            } else {
                let slot_dots = match radix {
                    DisplayRadix::Hexadecimal => 2,
                    DisplayRadix::Binary => 8,
                    _ => unreachable!(),
                };
                formatted.push_str(&".".repeat(slot_dots));
            }
        }
        if formatted.len() < total_digits {
            formatted.push_str(&".".repeat(total_digits - formatted.len()));
        } else if formatted.len() > total_digits {
            formatted.truncate(total_digits);
        }
        formatted
    }
}

/// Checks if all bytes in the slice are zero.
pub fn is_group_zero(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(|&b| b == 0)
}

/// Returns the next byte offset in visual reading order (left to right) on screen.
pub fn next_visual_byte(pos: usize, total_len: usize, group_size: ByteGroupSize, is_big_endian: bool) -> usize {
    if total_len == 0 || pos >= total_len {
        return total_len;
    }
    let g = group_size.byte_count();
    if is_big_endian || g <= 1 {
        return (pos + 1).min(total_len);
    }

    let group_start = (pos / g) * g;
    let slot = pos.saturating_sub(group_start);
    // In Little Endian, visual slots within a group run from (group_len - 1) down to 0.
    if slot > 0 {
        pos - 1
    } else {
        let next_group_start = group_start + g;
        if next_group_start >= total_len {
            total_len
        } else {
            let next_group_len = (total_len - next_group_start).min(g);
            next_group_start + next_group_len - 1
        }
    }
}

/// Returns the previous byte offset in visual reading order (right to left) on screen.
pub fn prev_visual_byte(pos: usize, total_len: usize, group_size: ByteGroupSize, is_big_endian: bool) -> usize {
    if total_len == 0 {
        return 0;
    }
    if pos > total_len {
        return total_len.saturating_sub(1);
    }
    let g = group_size.byte_count();
    if pos == total_len {
        if is_big_endian || g <= 1 {
            return total_len.saturating_sub(1);
        }
        let last_group_start = ((total_len.saturating_sub(1)) / g) * g;
        return last_group_start;
    }

    if is_big_endian || g <= 1 {
        return pos.saturating_sub(1);
    }

    let group_start = (pos / g) * g;
    let slot = pos.saturating_sub(group_start);
    let current_group_len = (total_len - group_start).min(g);

    if slot + 1 < current_group_len {
        pos + 1
    } else if group_start == 0 {
        pos
    } else {
        group_start.saturating_sub(g)
    }
}

/// Formats a contiguous slice of bytes using the specified display radix, group size, and endianness.
pub fn format_display_content(bytes: &[u8], start_offset: usize, radix: DisplayRadix, group_size: ByteGroupSize, is_big_endian: bool) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let group_bytes = group_size.byte_count();
    let mut result = String::with_capacity(bytes.len() * 3);
    let mut chunk_idx = 0;

    while chunk_idx < bytes.len() {
        let item_start_offset = start_offset + chunk_idx;
        let start_slot = item_start_offset % group_bytes;
        let item_slice_len = (bytes.len() - chunk_idx).min(group_bytes - start_slot);

        if !result.is_empty() {
            result.push(' ');
        }
        let group_str = format_group(&bytes[chunk_idx..chunk_idx + item_slice_len], start_slot, radix, group_size, is_big_endian);
        if radix == DisplayRadix::Decimal {
            result.push_str(group_str.trim());
        } else {
            result.push_str(&group_str);
        }
        chunk_idx += item_slice_len;
    }

    result
}

/// Formats a range of bytes across lines in a document/editor, preserving line breaks.
pub fn format_display_content_with_lines(
    buffer_data: &[u8],
    range: std::ops::Range<usize>,
    line_starts: &crate::core::editor::LineMap,
    radix: DisplayRadix,
    group_size: ByteGroupSize,
    is_big_endian: bool,
) -> String {
    if range.is_empty() || range.start >= buffer_data.len() {
        return String::new();
    }
    let clamped_end = range.end.min(buffer_data.len());
    let mut lines = Vec::new();
    let start_line_idx = crate::core::editor::Editor::find_line_index(range.start, line_starts);

    for line_idx in start_line_idx..line_starts.len() {
        let line_start = match line_starts.get(line_idx) {
            Some(o) => o,
            None => break,
        };
        if line_start >= clamped_end {
            break;
        }
        let line_end = if line_idx + 1 < line_starts.len() {
            line_starts.get(line_idx + 1).unwrap_or(buffer_data.len())
        } else {
            buffer_data.len()
        };
        let seg_start = range.start.max(line_start);
        let seg_end = clamped_end.min(line_end);
        if seg_start >= seg_end {
            continue;
        }

        let slice = &buffer_data[seg_start..seg_end];
        let line_text = format_display_content(slice, seg_start, radix, group_size, is_big_endian);
        if !line_text.is_empty() {
            lines.push(line_text);
        }
    }

    if lines.is_empty() {
        let slice = &buffer_data[range.start..clamped_end];
        format_display_content(slice, range.start, radix, group_size, is_big_endian)
    } else {
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_digit_count() {
        assert_eq!(digit_count(DisplayRadix::Hexadecimal, ByteGroupSize::One), 2);
        assert_eq!(digit_count(DisplayRadix::Decimal, ByteGroupSize::One), 3);
        assert_eq!(digit_count(DisplayRadix::Octal, ByteGroupSize::One), 3);
        assert_eq!(digit_count(DisplayRadix::Binary, ByteGroupSize::One), 8);

        assert_eq!(digit_count(DisplayRadix::Hexadecimal, ByteGroupSize::Two), 4);
        assert_eq!(digit_count(DisplayRadix::Decimal, ByteGroupSize::Two), 5);
        assert_eq!(digit_count(DisplayRadix::Octal, ByteGroupSize::Two), 6);
        assert_eq!(digit_count(DisplayRadix::Binary, ByteGroupSize::Two), 16);

        assert_eq!(digit_count(DisplayRadix::Hexadecimal, ByteGroupSize::Four), 8);
        assert_eq!(digit_count(DisplayRadix::Decimal, ByteGroupSize::Four), 10);
        assert_eq!(digit_count(DisplayRadix::Octal, ByteGroupSize::Four), 11);
        assert_eq!(digit_count(DisplayRadix::Binary, ByteGroupSize::Four), 32);

        assert_eq!(digit_count(DisplayRadix::Hexadecimal, ByteGroupSize::Eight), 16);
        assert_eq!(digit_count(DisplayRadix::Decimal, ByteGroupSize::Eight), 20);
        assert_eq!(digit_count(DisplayRadix::Octal, ByteGroupSize::Eight), 22);
        assert_eq!(digit_count(DisplayRadix::Binary, ByteGroupSize::Eight), 64);
    }

    #[test]
    fn test_format_group_one_byte() {
        let b = &[0x2a]; // 42 decimal, 052 octal, 00101010 binary
        assert_eq!(format_group(b, 0, DisplayRadix::Hexadecimal, ByteGroupSize::One, false), "2a");
        assert_eq!(format_group(b, 0, DisplayRadix::Decimal, ByteGroupSize::One, false), " 42");
        assert_eq!(format_group(b, 0, DisplayRadix::Octal, ByteGroupSize::One, false), "052");
        assert_eq!(format_group(b, 0, DisplayRadix::Binary, ByteGroupSize::One, false), "00101010");
    }

    #[test]
    fn test_format_group_two_bytes() {
        // [0x12, 0x34]
        // LE: 0x3412 = 13330 = 032022 octal
        // BE: 0x1234 = 4660 = 011064 octal
        let bytes = &[0x12, 0x34];
        assert_eq!(format_group(bytes, 0, DisplayRadix::Hexadecimal, ByteGroupSize::Two, false), "3412");
        assert_eq!(format_group(bytes, 0, DisplayRadix::Hexadecimal, ByteGroupSize::Two, true), "1234");

        assert_eq!(format_group(bytes, 0, DisplayRadix::Decimal, ByteGroupSize::Two, false), "13330");
        assert_eq!(format_group(bytes, 0, DisplayRadix::Decimal, ByteGroupSize::Two, true), " 4660");

        assert_eq!(format_group(bytes, 0, DisplayRadix::Octal, ByteGroupSize::Two, false), "032022");
        assert_eq!(format_group(bytes, 0, DisplayRadix::Octal, ByteGroupSize::Two, true), "011064");

        assert_eq!(format_group(bytes, 0, DisplayRadix::Binary, ByteGroupSize::Two, false), "0011010000010010");
        assert_eq!(format_group(bytes, 0, DisplayRadix::Binary, ByteGroupSize::Two, true), "0001001000110100");
    }

    #[test]
    fn test_format_group_four_bytes() {
        let bytes = &[0x01, 0x02, 0x03, 0x04];
        // LE: 0x04030201 = 67305985
        // BE: 0x01020304 = 16909060
        assert_eq!(format_group(bytes, 0, DisplayRadix::Hexadecimal, ByteGroupSize::Four, false), "04030201");
        assert_eq!(format_group(bytes, 0, DisplayRadix::Hexadecimal, ByteGroupSize::Four, true), "01020304");

        assert_eq!(format_group(bytes, 0, DisplayRadix::Decimal, ByteGroupSize::Four, false), "  67305985");
        assert_eq!(format_group(bytes, 0, DisplayRadix::Decimal, ByteGroupSize::Four, true), "  16909060");
    }

    #[test]
    fn test_format_group_partial_positions_four_bytes() {
        // Line 1: offset 0, byte [00] -> slot 0 -> "00......"
        let b0 = &[0x00];
        assert_eq!(format_group(b0, 0, DisplayRadix::Hexadecimal, ByteGroupSize::Four, false), "00......");

        // Line 2: offset 1, byte [01] -> slot 1 -> "..01...."
        let b1 = &[0x01];
        assert_eq!(format_group(b1, 1, DisplayRadix::Hexadecimal, ByteGroupSize::Four, false), "..01....");

        // Line 3: offset 2, bytes [03, 04] -> slot 2..4 -> "....0304"
        let b23 = &[0x03, 0x04];
        assert_eq!(format_group(b23, 2, DisplayRadix::Hexadecimal, ByteGroupSize::Four, false), "....0304");

        // Single byte at slot 3 -> "......05"
        let b3 = &[0x05];
        assert_eq!(format_group(b3, 3, DisplayRadix::Hexadecimal, ByteGroupSize::Four, false), "......05");
    }

    #[test]
    fn test_format_group_partial_positions_two_bytes() {
        // Slot 0 in 2B -> "12.."
        let b = &[0x12];
        assert_eq!(format_group(b, 0, DisplayRadix::Hexadecimal, ByteGroupSize::Two, false), "12..");

        // Slot 1 in 2B -> "..12"
        assert_eq!(format_group(b, 1, DisplayRadix::Hexadecimal, ByteGroupSize::Two, false), "..12");
    }

    #[test]
    fn test_format_group_eight_bytes() {
        let bytes = &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        // LE: 0x0807060504030201 = 578437695752307201
        assert_eq!(
            format_group(bytes, 0, DisplayRadix::Decimal, ByteGroupSize::Eight, false),
            "  578437695752307201"
        );
    }

    #[test]
    fn test_format_group_partial_decimal() {
        // 3 bytes in 8-byte group: 0x0000000000563412 = 5649426
        let b = &[0x12, 0x34, 0x56];
        assert_eq!(format_group(b, 0, DisplayRadix::Decimal, ByteGroupSize::Eight, false), "             5649426");
        assert_eq!(format_group(b, 0, DisplayRadix::Decimal, ByteGroupSize::Eight, false).len(), 20);

        // 1 byte in 8-byte group: 99
        let b1 = &[99];
        assert_eq!(format_group(b1, 0, DisplayRadix::Decimal, ByteGroupSize::Eight, false), "                  99");
        assert_eq!(format_group(b1, 0, DisplayRadix::Decimal, ByteGroupSize::Eight, false).len(), 20);

        // 2 bytes in 4-byte group: 0x00003412 = 13330
        let b2 = &[0x12, 0x34];
        assert_eq!(format_group(b2, 0, DisplayRadix::Decimal, ByteGroupSize::Four, false), "     13330");
        assert_eq!(format_group(b2, 0, DisplayRadix::Decimal, ByteGroupSize::Four, false).len(), 10);

        // 1 byte in 2-byte group: 99
        assert_eq!(format_group(b1, 0, DisplayRadix::Decimal, ByteGroupSize::Two, false), "   99");
        assert_eq!(format_group(b1, 0, DisplayRadix::Decimal, ByteGroupSize::Two, false).len(), 5);
    }

    #[test]
    fn test_is_group_zero() {
        assert!(is_group_zero(&[0, 0, 0, 0]));
        assert!(!is_group_zero(&[0, 1, 0, 0]));
        assert!(!is_group_zero(&[]));
    }

    #[test]
    fn test_format_display_content() {
        let bytes = [0x12, 0x34, 0x56, 0x78];

        // 1-byte Hex
        assert_eq!(
            format_display_content(&bytes, 0, DisplayRadix::Hexadecimal, ByteGroupSize::One, false),
            "12 34 56 78"
        );

        // 2-byte Hex LE
        assert_eq!(
            format_display_content(&bytes, 0, DisplayRadix::Hexadecimal, ByteGroupSize::Two, false),
            "3412 7856"
        );

        // 2-byte Hex BE
        assert_eq!(
            format_display_content(&bytes, 0, DisplayRadix::Hexadecimal, ByteGroupSize::Two, true),
            "1234 5678"
        );

        // 4-byte Hex LE
        assert_eq!(
            format_display_content(&bytes, 0, DisplayRadix::Hexadecimal, ByteGroupSize::Four, false),
            "78563412"
        );

        // 4-byte Hex BE
        assert_eq!(
            format_display_content(&bytes, 0, DisplayRadix::Hexadecimal, ByteGroupSize::Four, true),
            "12345678"
        );

        // 1-byte Decimal
        assert_eq!(
            format_display_content(&bytes, 0, DisplayRadix::Decimal, ByteGroupSize::One, false),
            "18 52 86 120"
        );

        // 2-byte Decimal LE
        assert_eq!(
            format_display_content(&bytes, 0, DisplayRadix::Decimal, ByteGroupSize::Two, false),
            "13330 30806"
        );

        // 2-byte Decimal BE
        assert_eq!(format_display_content(&bytes, 0, DisplayRadix::Decimal, ByteGroupSize::Two, true), "4660 22136");
    }

    #[test]
    fn test_visual_byte_navigation_one_byte() {
        let total = 4;
        assert_eq!(next_visual_byte(0, total, ByteGroupSize::One, false), 1);
        assert_eq!(next_visual_byte(1, total, ByteGroupSize::One, false), 2);
        assert_eq!(next_visual_byte(2, total, ByteGroupSize::One, false), 3);
        assert_eq!(next_visual_byte(3, total, ByteGroupSize::One, false), 4);
        assert_eq!(next_visual_byte(4, total, ByteGroupSize::One, false), 4);

        assert_eq!(prev_visual_byte(4, total, ByteGroupSize::One, false), 3);
        assert_eq!(prev_visual_byte(3, total, ByteGroupSize::One, false), 2);
        assert_eq!(prev_visual_byte(2, total, ByteGroupSize::One, false), 1);
        assert_eq!(prev_visual_byte(1, total, ByteGroupSize::One, false), 0);
        assert_eq!(prev_visual_byte(0, total, ByteGroupSize::One, false), 0);
    }

    #[test]
    fn test_visual_byte_navigation_two_bytes_le() {
        let total = 4;
        assert_eq!(next_visual_byte(1, total, ByteGroupSize::Two, false), 0);
        assert_eq!(next_visual_byte(0, total, ByteGroupSize::Two, false), 3);
        assert_eq!(next_visual_byte(3, total, ByteGroupSize::Two, false), 2);
        assert_eq!(next_visual_byte(2, total, ByteGroupSize::Two, false), 4);
        assert_eq!(next_visual_byte(4, total, ByteGroupSize::Two, false), 4);

        assert_eq!(prev_visual_byte(4, total, ByteGroupSize::Two, false), 2);
        assert_eq!(prev_visual_byte(2, total, ByteGroupSize::Two, false), 3);
        assert_eq!(prev_visual_byte(3, total, ByteGroupSize::Two, false), 0);
        assert_eq!(prev_visual_byte(0, total, ByteGroupSize::Two, false), 1);
        assert_eq!(prev_visual_byte(1, total, ByteGroupSize::Two, false), 1);
    }

    #[test]
    fn test_visual_byte_navigation_two_bytes_le_odd_length() {
        let total = 3;
        assert_eq!(next_visual_byte(1, total, ByteGroupSize::Two, false), 0);
        assert_eq!(next_visual_byte(0, total, ByteGroupSize::Two, false), 2);
        assert_eq!(next_visual_byte(2, total, ByteGroupSize::Two, false), 3);
        assert_eq!(next_visual_byte(3, total, ByteGroupSize::Two, false), 3);

        assert_eq!(prev_visual_byte(3, total, ByteGroupSize::Two, false), 2);
        assert_eq!(prev_visual_byte(2, total, ByteGroupSize::Two, false), 0);
        assert_eq!(prev_visual_byte(0, total, ByteGroupSize::Two, false), 1);
        assert_eq!(prev_visual_byte(1, total, ByteGroupSize::Two, false), 1);
    }

    #[test]
    fn test_visual_byte_navigation_two_bytes_be() {
        let total = 4;
        assert_eq!(next_visual_byte(0, total, ByteGroupSize::Two, true), 1);
        assert_eq!(next_visual_byte(1, total, ByteGroupSize::Two, true), 2);
        assert_eq!(next_visual_byte(2, total, ByteGroupSize::Two, true), 3);
        assert_eq!(next_visual_byte(3, total, ByteGroupSize::Two, true), 4);
        assert_eq!(next_visual_byte(4, total, ByteGroupSize::Two, true), 4);

        assert_eq!(prev_visual_byte(4, total, ByteGroupSize::Two, true), 3);
        assert_eq!(prev_visual_byte(3, total, ByteGroupSize::Two, true), 2);
        assert_eq!(prev_visual_byte(2, total, ByteGroupSize::Two, true), 1);
        assert_eq!(prev_visual_byte(1, total, ByteGroupSize::Two, true), 0);
        assert_eq!(prev_visual_byte(0, total, ByteGroupSize::Two, true), 0);
    }

    #[test]
    fn test_visual_byte_navigation_four_bytes_le() {
        let total = 8;
        assert_eq!(next_visual_byte(3, total, ByteGroupSize::Four, false), 2);
        assert_eq!(next_visual_byte(2, total, ByteGroupSize::Four, false), 1);
        assert_eq!(next_visual_byte(1, total, ByteGroupSize::Four, false), 0);
        assert_eq!(next_visual_byte(0, total, ByteGroupSize::Four, false), 7);
        assert_eq!(next_visual_byte(7, total, ByteGroupSize::Four, false), 6);
        assert_eq!(next_visual_byte(6, total, ByteGroupSize::Four, false), 5);
        assert_eq!(next_visual_byte(5, total, ByteGroupSize::Four, false), 4);
        assert_eq!(next_visual_byte(4, total, ByteGroupSize::Four, false), 8);
        assert_eq!(next_visual_byte(8, total, ByteGroupSize::Four, false), 8);

        assert_eq!(prev_visual_byte(8, total, ByteGroupSize::Four, false), 4);
        assert_eq!(prev_visual_byte(4, total, ByteGroupSize::Four, false), 5);
        assert_eq!(prev_visual_byte(5, total, ByteGroupSize::Four, false), 6);
        assert_eq!(prev_visual_byte(6, total, ByteGroupSize::Four, false), 7);
        assert_eq!(prev_visual_byte(7, total, ByteGroupSize::Four, false), 0);
        assert_eq!(prev_visual_byte(0, total, ByteGroupSize::Four, false), 1);
        assert_eq!(prev_visual_byte(1, total, ByteGroupSize::Four, false), 2);
        assert_eq!(prev_visual_byte(2, total, ByteGroupSize::Four, false), 3);
        assert_eq!(prev_visual_byte(3, total, ByteGroupSize::Four, false), 3);
    }
}
