use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::radix::ByteOrder;

/// Width (in bytes) for sequential counter generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SequentialWidth {
    #[default]
    U8 = 1,
    U16 = 2,
    U32 = 4,
    U64 = 8,
}

impl SequentialWidth {
    pub const fn byte_size(self) -> usize {
        match self {
            Self::U8 => 1,
            Self::U16 => 2,
            Self::U32 => 4,
            Self::U64 => 8,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::U8 => "1 Byte",
            Self::U16 => "2 Bytes",
            Self::U32 => "4 Bytes",
            Self::U64 => "8 Bytes",
        }
    }
}

/// Random generation mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RandomMode {
    #[default]
    Pseudo,
    Cryptographic,
}

impl RandomMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pseudo => "Pseudo-random",
            Self::Cryptographic => "Cryptographic",
        }
    }
}

/// A pattern specification for filling a byte range.
#[derive(Clone, Debug, PartialEq)]
pub enum FillPattern {
    /// Fills with a repeating single byte.
    SingleByte(u8),
    /// Fills with a repeating byte sequence.
    Pattern(Vec<u8>),
    /// Fills with sequential values of a given width and endianness.
    Sequential {
        width: SequentialWidth,
        start: u64,
        step: u64,
        byte_order: ByteOrder,
    },
    /// Fills with random bytes.
    Random { mode: RandomMode },
}

impl FillPattern {
    /// Generates a newly allocated vector containing the filled bytes of length `len`.
    pub fn generate(&self, len: usize) -> Vec<u8> {
        let mut buf = vec![0u8; len];
        self.fill_slice(&mut buf);
        buf
    }

    /// Fills the given mutable slice according to this pattern.
    pub fn fill_slice(&self, buf: &mut [u8]) {
        match self {
            Self::SingleByte(byte) => {
                buf.fill(*byte);
            }
            Self::Pattern(pattern) => {
                if pattern.is_empty() {
                    buf.fill(0);
                    return;
                }
                for (i, b) in buf.iter_mut().enumerate() {
                    *b = pattern[i % pattern.len()];
                }
            }
            Self::Sequential {
                width,
                start,
                step,
                byte_order,
            } => {
                let unit_bytes = width.byte_size();
                let is_be = byte_order.is_big_endian();
                let mut chunk_idx: u64 = 0;
                let mut offset = 0;

                while offset < buf.len() {
                    let val = start.wrapping_add(chunk_idx.wrapping_mul(*step));
                    let bytes = match width {
                        SequentialWidth::U8 => [val as u8, 0, 0, 0, 0, 0, 0, 0],
                        SequentialWidth::U16 => {
                            let u = val as u16;
                            let b = if is_be { u.to_be_bytes() } else { u.to_le_bytes() };
                            [b[0], b[1], 0, 0, 0, 0, 0, 0]
                        }
                        SequentialWidth::U32 => {
                            let u = val as u32;
                            let b = if is_be { u.to_be_bytes() } else { u.to_le_bytes() };
                            [b[0], b[1], b[2], b[3], 0, 0, 0, 0]
                        }
                        SequentialWidth::U64 => {
                            if is_be {
                                val.to_be_bytes()
                            } else {
                                val.to_le_bytes()
                            }
                        }
                    };

                    let copy_len = unit_bytes.min(buf.len() - offset);
                    buf[offset..offset + copy_len].copy_from_slice(&bytes[..copy_len]);

                    offset += copy_len;
                    chunk_idx = chunk_idx.wrapping_add(1);
                }
            }
            Self::Random { mode } => match mode {
                RandomMode::Pseudo => {
                    fill_pseudo_random(buf);
                }
                RandomMode::Cryptographic => {
                    if getrandom::fill(buf).is_err() {
                        fill_pseudo_random(buf);
                    }
                }
            },
        }
    }
}

/// Simple fast 64-bit XorShift PRNG for pseudo-random fills.
fn fill_pseudo_random(buf: &mut [u8]) {
    let mut state = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0xdeadbeef_cafebabe);

    if state == 0 {
        state = 0x12345678_9abcdef0;
    }

    let (chunks, remainder) = buf.as_chunks_mut::<8>();
    for chunk in chunks {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        chunk.copy_from_slice(&state.to_ne_bytes());
    }

    if !remainder.is_empty() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let bytes = state.to_ne_bytes();
        remainder.copy_from_slice(&bytes[..remainder.len()]);
    }
}

/// Parses a hex pattern string into a byte vector.
///
/// Supports inputs such as:
/// - `"DE AD BE EF"`
/// - `"DEADBEEF"`
/// - `"0xDE, 0xAD, 0xBE, 0xEF"`
/// - `"de:ad:be:ef"`
pub fn parse_pattern_hex(input: &str) -> Result<Vec<u8>, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Hex pattern cannot be empty".to_string());
    }

    let mut chars = trimmed.chars().peekable();
    let mut hex_chars = Vec::new();

    while let Some(c) = chars.next() {
        if c.is_ascii_whitespace() || c == ',' || c == ':' || c == '-' || c == '_' {
            continue;
        }
        if c == '0' && chars.peek().is_some_and(|&next| next == 'x' || next == 'X') {
            chars.next();
            continue;
        }
        if c.is_ascii_hexdigit() {
            hex_chars.push(c);
        } else {
            return Err(format!("Invalid hex character '{c}'"));
        }
    }

    if hex_chars.is_empty() {
        return Err("Hex pattern cannot be empty".to_string());
    }

    if hex_chars.len() % 2 != 0 {
        return Err(format!("Hex pattern has an odd number of digits ({}); must be complete bytes", hex_chars.len()));
    }

    let mut bytes = Vec::with_capacity(hex_chars.len() / 2);
    for chunk in hex_chars.as_chunks::<2>().0 {
        let hi = chunk[0].to_digit(16).unwrap() as u8;
        let lo = chunk[1].to_digit(16).unwrap() as u8;
        bytes.push((hi << 4) | lo);
    }

    Ok(bytes)
}

/// Parses a text pattern string into a UTF-8 byte vector.
///
/// Unescapes common escape sequences like `\0`, `\n`, `\r`, `\t`, `\\`, `\"`, `\'`, and `\xNN`.
pub fn parse_pattern_text(input: &str) -> Result<Vec<u8>, String> {
    if input.is_empty() {
        return Err("Text pattern cannot be empty".to_string());
    }

    let mut result = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('0') => result.push(0),
                Some('n') => result.push(b'\n'),
                Some('r') => result.push(b'\r'),
                Some('t') => result.push(b'\t'),
                Some('\\') => result.push(b'\\'),
                Some('"') => result.push(b'"'),
                Some('\'') => result.push(b'\''),
                Some('x') => {
                    let hi = chars
                        .next()
                        .and_then(|c| c.to_digit(16))
                        .ok_or_else(|| "Invalid \\x escape: expected 2 hex digits".to_string())?;
                    let lo = chars
                        .next()
                        .and_then(|c| c.to_digit(16))
                        .ok_or_else(|| "Invalid \\x escape: expected 2 hex digits".to_string())?;
                    result.push(((hi << 4) | lo) as u8);
                }
                Some(other) => {
                    result.push(b'\\');
                    let mut b = [0u8; 4];
                    result.extend_from_slice(other.encode_utf8(&mut b).as_bytes());
                }
                None => result.push(b'\\'),
            }
        } else {
            let mut b = [0u8; 4];
            result.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
        }
    }

    Ok(result)
}

/// Parses an unsigned 64-bit integer supporting decimal, hex (`0x...`), or binary (`0b...`).
pub fn parse_u64_val(input: &str) -> Result<u64, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Value cannot be empty".to_string());
    }

    let lower = trimmed.to_ascii_lowercase();

    if let Some(hex_str) = lower.strip_prefix("0x") {
        let clean = hex_str.replace('_', "");
        if clean.is_empty() {
            return Err("Invalid hex value".to_string());
        }
        return u64::from_str_radix(&clean, 16).map_err(|_| "Invalid hex number".to_string());
    }

    if let Some(bin_str) = lower.strip_prefix("0b") {
        let clean = bin_str.replace('_', "");
        if clean.is_empty() {
            return Err("Invalid binary value".to_string());
        }
        return u64::from_str_radix(&clean, 2).map_err(|_| "Invalid binary number".to_string());
    }

    let clean = trimmed.replace('_', "");
    clean
        .parse::<u64>()
        .map_err(|_| "Invalid number (decimal, hex 0x..., or bin 0b...)".to_string())
}

/// Parses a step integer which may be positive or negative (e.g. `-1` maps to `u64::MAX`).
pub fn parse_step_val(input: &str) -> Result<u64, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Step cannot be empty".to_string());
    }

    if let Some(stripped) = trimmed.strip_prefix('-') {
        let neg_clean = stripped.trim().replace('_', "");
        let val = if let Some(hex) = neg_clean.to_ascii_lowercase().strip_prefix("0x") {
            i64::from_str_radix(hex, 16).map_err(|_| "Invalid negative hex step".to_string())?
        } else {
            neg_clean.parse::<i64>().map_err(|_| "Invalid negative step".to_string())?
        };
        return Ok((-val) as u64);
    }

    parse_u64_val(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_byte_fill() {
        let pattern = FillPattern::SingleByte(0x90);
        let bytes = pattern.generate(5);
        assert_eq!(bytes, vec![0x90, 0x90, 0x90, 0x90, 0x90]);
    }

    #[test]
    fn test_pattern_fill() {
        let pattern = FillPattern::Pattern(vec![0xDE, 0xAD, 0xBE, 0xEF]);
        let bytes = pattern.generate(10);
        assert_eq!(bytes, vec![0xDE, 0xAD, 0xBE, 0xEF, 0xDE, 0xAD, 0xBE, 0xEF, 0xDE, 0xAD]);
    }

    #[test]
    fn test_sequential_u8() {
        let pattern = FillPattern::Sequential {
            width: SequentialWidth::U8,
            start: 0xFE,
            step: 1,
            byte_order: ByteOrder::LittleEndian,
        };
        let bytes = pattern.generate(5);
        assert_eq!(bytes, vec![0xFE, 0xFF, 0x00, 0x01, 0x02]);
    }

    #[test]
    fn test_sequential_u16_le() {
        let pattern = FillPattern::Sequential {
            width: SequentialWidth::U16,
            start: 0x0100,
            step: 1,
            byte_order: ByteOrder::LittleEndian,
        };
        let bytes = pattern.generate(6);
        assert_eq!(bytes, vec![0x00, 0x01, 0x01, 0x01, 0x02, 0x01]);
    }

    #[test]
    fn test_sequential_u16_be() {
        let pattern = FillPattern::Sequential {
            width: SequentialWidth::U16,
            start: 0x0100,
            step: 1,
            byte_order: ByteOrder::BigEndian,
        };
        let bytes = pattern.generate(6);
        assert_eq!(bytes, vec![0x01, 0x00, 0x01, 0x01, 0x01, 0x02]);
    }

    #[test]
    fn test_sequential_u32_le() {
        let pattern = FillPattern::Sequential {
            width: SequentialWidth::U32,
            start: 0x12345678,
            step: 1,
            byte_order: ByteOrder::LittleEndian,
        };
        let bytes = pattern.generate(8);
        assert_eq!(
            bytes,
            vec![
                0x78, 0x56, 0x34, 0x12, // 0x12345678 LE
                0x79, 0x56, 0x34, 0x12, // 0x12345679 LE
            ]
        );
    }

    #[test]
    fn test_sequential_u32_be() {
        let pattern = FillPattern::Sequential {
            width: SequentialWidth::U32,
            start: 0x12345678,
            step: 1,
            byte_order: ByteOrder::BigEndian,
        };
        let bytes = pattern.generate(8);
        assert_eq!(
            bytes,
            vec![
                0x12, 0x34, 0x56, 0x78, // 0x12345678 BE
                0x12, 0x34, 0x56, 0x79, // 0x12345679 BE
            ]
        );
    }

    #[test]
    fn test_sequential_u64_le() {
        let pattern = FillPattern::Sequential {
            width: SequentialWidth::U64,
            start: 1,
            step: 1,
            byte_order: ByteOrder::LittleEndian,
        };
        let bytes = pattern.generate(10);
        assert_eq!(
            bytes,
            vec![
                0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 1
                0x02, 0x00 // partial 2
            ]
        );
    }

    #[test]
    fn test_sequential_partial_tail() {
        let pattern = FillPattern::Sequential {
            width: SequentialWidth::U16,
            start: 0x0001,
            step: 1,
            byte_order: ByteOrder::LittleEndian,
        };
        let bytes = pattern.generate(3);
        assert_eq!(bytes, vec![0x01, 0x00, 0x02]);
    }

    #[test]
    fn test_random_fill() {
        let pseudo = FillPattern::Random { mode: RandomMode::Pseudo };
        let b1 = pseudo.generate(32);
        assert_eq!(b1.len(), 32);

        let crypto = FillPattern::Random {
            mode: RandomMode::Cryptographic,
        };
        let b2 = crypto.generate(32);
        assert_eq!(b2.len(), 32);
    }

    #[test]
    fn test_parse_pattern_hex() {
        assert_eq!(parse_pattern_hex("DE AD BE EF").unwrap(), vec![0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(parse_pattern_hex("0xDE, 0xAD, 0xBE, 0xEF").unwrap(), vec![0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(parse_pattern_hex("deadbeef").unwrap(), vec![0xDE, 0xAD, 0xBE, 0xEF]);
        assert!(parse_pattern_hex("").is_err());
        assert!(parse_pattern_hex("123").is_err()); // odd
        assert!(parse_pattern_hex("GG").is_err()); // invalid hex
    }

    #[test]
    fn test_parse_pattern_text() {
        assert_eq!(parse_pattern_text("NULL").unwrap(), b"NULL".to_vec());
        assert_eq!(parse_pattern_text("\"NULL\"").unwrap(), b"\"NULL\"".to_vec());
        assert_eq!(parse_pattern_text("\"").unwrap(), vec![b'"']);
        assert_eq!(parse_pattern_text("\"\"").unwrap(), vec![b'"', b'"']);
        assert_eq!(parse_pattern_text("'hello'").unwrap(), b"'hello'".to_vec());
        assert_eq!(parse_pattern_text("hello\\nworld").unwrap(), b"hello\nworld".to_vec());
        assert_eq!(parse_pattern_text("\\x00\\xFF").unwrap(), vec![0x00, 0xFF]);
        assert_eq!(parse_pattern_text("\\\"escaped\\\"").unwrap(), b"\"escaped\"".to_vec());
    }

    #[test]
    fn test_parse_u64_and_step() {
        assert_eq!(parse_u64_val("100").unwrap(), 100);
        assert_eq!(parse_u64_val("0x100").unwrap(), 256);
        assert_eq!(parse_u64_val("0b1010").unwrap(), 10);
        assert_eq!(parse_step_val("-1").unwrap(), (-1i64) as u64);
        assert_eq!(parse_step_val("2").unwrap(), 2);
    }

    #[test]
    fn test_sequential_width_label() {
        assert_eq!(SequentialWidth::U8.label(), "1 Byte");
        assert_eq!(SequentialWidth::U16.label(), "2 Bytes");
        assert_eq!(SequentialWidth::U32.label(), "4 Bytes");
        assert_eq!(SequentialWidth::U64.label(), "8 Bytes");
    }
}
