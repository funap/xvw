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
    pub const ALL: [ByteOrder; 2] = [ByteOrder::LittleEndian, ByteOrder::BigEndian];

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

/// Byte coloring mode and preset selection for rendering values in the hex view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ByteColoring {
    DimZero,
    DimPadding,
    Category,
    Custom,
    #[default]
    #[serde(other, alias = "none", alias = "plain")]
    Uniform,
}

#[allow(dead_code)]
impl ByteColoring {
    /// All selectable preset modes (excluding `Custom`).
    pub const PRESETS: [ByteColoring; 4] = [ByteColoring::Uniform, ByteColoring::DimZero, ByteColoring::DimPadding, ByteColoring::Category];

    /// All modes including `Custom`.
    pub const ALL: [ByteColoring; 5] = [
        ByteColoring::Uniform,
        ByteColoring::DimZero,
        ByteColoring::DimPadding,
        ByteColoring::Category,
        ByteColoring::Custom,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ByteColoring::Uniform => "None",
            ByteColoring::DimZero => "Dim Zero",
            ByteColoring::DimPadding => "Dim Padding",
            ByteColoring::Category => "Category",
            ByteColoring::Custom => "Custom",
        }
    }

    /// Returns the explicit category color mapping for this preset, or `None` if `Custom`.
    pub const fn colors(self) -> Option<ByteCategoryColors> {
        match self {
            ByteColoring::DimZero => Some(ByteCategoryColors {
                null: CategoryColor::Muted,
                ascii_printable: CategoryColor::Default,
                ascii_whitespace: CategoryColor::Default,
                ascii_control: CategoryColor::Default,
                non_ascii: CategoryColor::Default,
                max: CategoryColor::Default,
            }),
            ByteColoring::DimPadding => Some(ByteCategoryColors {
                null: CategoryColor::Muted,
                ascii_printable: CategoryColor::Default,
                ascii_whitespace: CategoryColor::Default,
                ascii_control: CategoryColor::Default,
                non_ascii: CategoryColor::Default,
                max: CategoryColor::Muted,
            }),
            ByteColoring::Uniform => Some(ByteCategoryColors {
                null: CategoryColor::Default,
                ascii_printable: CategoryColor::Default,
                ascii_whitespace: CategoryColor::Default,
                ascii_control: CategoryColor::Default,
                non_ascii: CategoryColor::Default,
                max: CategoryColor::Default,
            }),
            ByteColoring::Category => Some(ByteCategoryColors {
                null: CategoryColor::Muted,
                ascii_printable: CategoryColor::Cyan,
                ascii_whitespace: CategoryColor::Green,
                ascii_control: CategoryColor::Magenta,
                non_ascii: CategoryColor::Yellow,
                max: CategoryColor::Red,
            }),
            ByteColoring::Custom => None,
        }
    }

    /// Determines the matching `ByteColoring` preset for the given category colors,
    /// or `ByteColoring::Custom` if it does not match any preset.
    pub fn from_colors(colors: ByteCategoryColors) -> Self {
        Self::PRESETS
            .into_iter()
            .find(|preset| preset.colors() == Some(colors))
            .unwrap_or(ByteColoring::Custom)
    }
}

impl gpui_kit::Global for ByteColoring {}

/// Semantic byte classification for category-based coloring.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ByteCategory {
    /// Zero byte (0x00)
    Null,
    /// ASCII control characters (0x01..=0x1F, 0x7F)
    AsciiControl,
    /// ASCII whitespace character (space: 0x20)
    AsciiWhitespace,
    /// Printable ASCII characters (0x21..=0x7E)
    AsciiPrintable,
    /// Non-ASCII bytes (0x80..=0xFE)
    NonAscii,
    /// 0xFF (often unprogrammed flash / padding / broadcast)
    Max,
}

impl ByteCategory {
    pub const ALL: &'static [ByteCategory] = &[
        ByteCategory::Null,
        ByteCategory::AsciiControl,
        ByteCategory::AsciiWhitespace,
        ByteCategory::AsciiPrintable,
        ByteCategory::NonAscii,
        ByteCategory::Max,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ByteCategory::Null => "Null (0x00)",
            ByteCategory::AsciiControl => "ASCII Control (0x01..0x1F, 0x7F)",
            ByteCategory::AsciiWhitespace => "ASCII Whitespace (0x20)",
            ByteCategory::AsciiPrintable => "ASCII Printable (0x21..0x7E)",
            ByteCategory::NonAscii => "Non-ASCII (0x80..0xFE)",
            ByteCategory::Max => "0xFF",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            ByteCategory::Null => "null",
            ByteCategory::AsciiControl => "ascii-control",
            ByteCategory::AsciiWhitespace => "ascii-whitespace",
            ByteCategory::AsciiPrintable => "ascii-printable",
            ByteCategory::NonAscii => "non-ascii",
            ByteCategory::Max => "0xff",
        }
    }
}

/// Explicit color choices available for an individual byte category from the active theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CategoryColor {
    Muted,
    Red,
    Green,
    Blue,
    Yellow,
    Cyan,
    Magenta,
    #[default]
    #[serde(other)]
    Default,
}

impl CategoryColor {
    pub const ALL: &'static [CategoryColor] = &[
        CategoryColor::Default,
        CategoryColor::Muted,
        CategoryColor::Red,
        CategoryColor::Green,
        CategoryColor::Blue,
        CategoryColor::Yellow,
        CategoryColor::Cyan,
        CategoryColor::Magenta,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CategoryColor::Default => "Default",
            CategoryColor::Muted => "Gray",
            CategoryColor::Red => "Red",
            CategoryColor::Green => "Green",
            CategoryColor::Blue => "Blue",
            CategoryColor::Yellow => "Yellow",
            CategoryColor::Cyan => "Cyan",
            CategoryColor::Magenta => "Magenta",
        }
    }
}

/// Configuration for individual byte category colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct ByteCategoryColors {
    pub null: CategoryColor,
    pub ascii_printable: CategoryColor,
    pub ascii_whitespace: CategoryColor,
    pub ascii_control: CategoryColor,
    pub non_ascii: CategoryColor,
    pub max: CategoryColor,
}

impl Default for ByteCategoryColors {
    fn default() -> Self {
        ByteColoring::Uniform.colors().expect("Uniform preset has explicit colors")
    }
}

impl gpui_kit::Global for ByteCategoryColors {}

impl ByteCategoryColors {
    /// Resets all category colors to the default (`ByteColoring::Uniform`) mapping.
    pub fn reset_all(&mut self) {
        *self = Self::default();
    }

    /// Returns the color configured for the given byte category, or `CategoryColor::Default` for mixed groups (`None`).
    #[inline]
    pub fn color_for(&self, category: Option<ByteCategory>) -> CategoryColor {
        match category {
            Some(ByteCategory::Null) => self.null,
            Some(ByteCategory::AsciiPrintable) => self.ascii_printable,
            Some(ByteCategory::AsciiWhitespace) => self.ascii_whitespace,
            Some(ByteCategory::AsciiControl) => self.ascii_control,
            Some(ByteCategory::NonAscii) => self.non_ascii,
            Some(ByteCategory::Max) => self.max,
            None => CategoryColor::Default,
        }
    }

    /// Sets the color for the given byte category.
    pub fn set(&mut self, category: ByteCategory, color: CategoryColor) {
        match category {
            ByteCategory::Null => self.null = color,
            ByteCategory::AsciiControl => self.ascii_control = color,
            ByteCategory::AsciiWhitespace => self.ascii_whitespace = color,
            ByteCategory::AsciiPrintable => self.ascii_printable = color,
            ByteCategory::NonAscii => self.non_ascii = color,
            ByteCategory::Max => self.max = color,
        }
    }
}

/// Classifies a single byte into a `ByteCategory`.
pub fn byte_category(b: u8) -> ByteCategory {
    match b {
        0x00 => ByteCategory::Null,
        0x01..=0x1F | 0x7F => ByteCategory::AsciiControl,
        0x20 => ByteCategory::AsciiWhitespace,
        0x21..=0x7E => ByteCategory::AsciiPrintable,
        0x80..=0xFE => ByteCategory::NonAscii,
        0xFF => ByteCategory::Max,
    }
}

/// Classifies a slice of bytes into a `ByteCategory` if all bytes share the same category.
pub fn group_category(bytes: &[u8]) -> Option<ByteCategory> {
    if bytes.is_empty() {
        return None;
    }
    let first = byte_category(bytes[0]);
    if bytes.iter().all(|&b| byte_category(b) == first) {
        Some(first)
    } else {
        None
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
    fn test_byte_coloring_labels_and_serde() {
        for coloring in ByteColoring::ALL {
            assert!(!coloring.label().is_empty());
        }

        let serialized = serde_json::to_string(&ByteColoring::DimZero).expect("serialize");
        assert_eq!(serialized, "\"dim_zero\"");
        let deserialized: ByteColoring = serde_json::from_str("\"dim_zero\"").expect("deserialize");
        assert_eq!(deserialized, ByteColoring::DimZero);

        let deserialized_uniform: ByteColoring = serde_json::from_str("\"uniform\"").expect("deserialize");
        assert_eq!(deserialized_uniform, ByteColoring::Uniform);

        let deserialized_padding: ByteColoring = serde_json::from_str("\"dim_padding\"").expect("deserialize");
        assert_eq!(deserialized_padding, ByteColoring::DimPadding);

        let deserialized_category: ByteColoring = serde_json::from_str("\"category\"").expect("deserialize");
        assert_eq!(deserialized_category, ByteColoring::Category);

        let deserialized_custom: ByteColoring = serde_json::from_str("\"custom\"").expect("deserialize");
        assert_eq!(deserialized_custom, ByteColoring::Custom);

        assert_eq!(ByteColoring::default(), ByteColoring::Uniform);
    }

    #[test]
    fn test_byte_coloring_presets_and_from_colors() {
        for preset in ByteColoring::PRESETS {
            let colors = preset.colors().expect("preset must have colors");
            assert_eq!(ByteColoring::from_colors(colors), preset);
        }
        assert_eq!(ByteColoring::Custom.colors(), None);

        let mut custom = ByteCategoryColors::default();
        custom.set(ByteCategory::Null, CategoryColor::Red);
        assert_eq!(ByteColoring::from_colors(custom), ByteColoring::Custom);
    }

    #[test]
    fn test_byte_category() {
        assert_eq!(byte_category(0x00), ByteCategory::Null);
        assert_eq!(byte_category(0x01), ByteCategory::AsciiControl);
        assert_eq!(byte_category(b'\t'), ByteCategory::AsciiControl);
        assert_eq!(byte_category(b'\n'), ByteCategory::AsciiControl);
        assert_eq!(byte_category(0x0B), ByteCategory::AsciiControl);
        assert_eq!(byte_category(0x0C), ByteCategory::AsciiControl);
        assert_eq!(byte_category(b'\r'), ByteCategory::AsciiControl);
        assert_eq!(byte_category(0x7F), ByteCategory::AsciiControl);
        assert_eq!(byte_category(b' '), ByteCategory::AsciiWhitespace);
        assert_eq!(byte_category(b'A'), ByteCategory::AsciiPrintable);
        assert_eq!(byte_category(b'~'), ByteCategory::AsciiPrintable);
        assert_eq!(byte_category(0x80), ByteCategory::NonAscii);
        assert_eq!(byte_category(0xFE), ByteCategory::NonAscii);
        assert_eq!(byte_category(0xFF), ByteCategory::Max);
    }

    #[test]
    fn test_group_category() {
        assert_eq!(group_category(&[]), None);
        assert_eq!(group_category(&[0x00, 0x00]), Some(ByteCategory::Null));
        assert_eq!(group_category(&[0x01, b'\t']), Some(ByteCategory::AsciiControl));
        assert_eq!(group_category(b"  "), Some(ByteCategory::AsciiWhitespace));
        assert_eq!(group_category(b"AB"), Some(ByteCategory::AsciiPrintable));
        assert_eq!(group_category(&[0x80, 0x81]), Some(ByteCategory::NonAscii));
        assert_eq!(group_category(&[0xFF, 0xFF]), Some(ByteCategory::Max));
        assert_eq!(group_category(b" \t"), None);
        assert_eq!(group_category(&[0x00, 0xFF]), None);
        assert_eq!(group_category(&[b'A', 0x00]), None);
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

    #[test]
    fn test_category_color_labels_and_serde() {
        for color in CategoryColor::ALL {
            assert!(!color.label().is_empty());
            let serialized = serde_json::to_string(color).expect("serialize");
            let deserialized: CategoryColor = serde_json::from_str(&serialized).expect("deserialize");
            assert_eq!(*color, deserialized);
        }
    }

    #[test]
    fn test_byte_category_colors_color_for_and_set() {
        let mut colors = ByteCategoryColors::default();
        assert_eq!(ByteColoring::from_colors(colors), ByteColoring::Uniform);
        assert_eq!(colors.color_for(Some(ByteCategory::Null)), CategoryColor::Default);
        assert_eq!(colors.color_for(None), CategoryColor::Default);
        assert_eq!(colors.color_for(Some(ByteCategory::AsciiPrintable)), CategoryColor::Default);

        colors.set(ByteCategory::Null, CategoryColor::Red);
        assert_eq!(colors.color_for(Some(ByteCategory::Null)), CategoryColor::Red);
        assert_eq!(ByteColoring::from_colors(colors), ByteColoring::Custom);

        colors.set(ByteCategory::AsciiPrintable, CategoryColor::Cyan);
        assert_eq!(colors.color_for(Some(ByteCategory::AsciiPrintable)), CategoryColor::Cyan);

        colors.reset_all();
        assert_eq!(ByteColoring::from_colors(colors), ByteColoring::Uniform);
        assert_eq!(colors.color_for(Some(ByteCategory::Null)), CategoryColor::Default);
        assert_eq!(colors.color_for(None), CategoryColor::Default);
    }

    #[test]
    fn test_byte_coloring_fallback_on_unknown_variant() {
        let de: ByteColoring = serde_json::from_str("\"unknown_preset\"").expect("deserialize unknown");
        assert_eq!(de, ByteColoring::Uniform);

        let de_legacy: ByteColoring = serde_json::from_str("\"ascii_printable\"").expect("deserialize legacy");
        assert_eq!(de_legacy, ByteColoring::Uniform);
    }

    #[test]
    fn test_category_color_fallback_on_unknown_variant() {
        let de: CategoryColor = serde_json::from_str("\"unknown_color\"").expect("deserialize unknown");
        assert_eq!(de, CategoryColor::Default);
    }
}
