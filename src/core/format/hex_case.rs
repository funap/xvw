use serde::{Deserialize, Serialize};

/// Configures whether hexadecimal digits are formatted in lowercase (`a-f`) or uppercase (`A-F`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HexCase {
    #[default]
    Lower,
    Upper,
}

#[allow(dead_code)]
impl HexCase {
    pub const ALL: [HexCase; 2] = [HexCase::Lower, HexCase::Upper];

    /// Human-readable label for settings UI.
    pub fn label(&self) -> &'static str {
        match self {
            HexCase::Lower => "Lowercase (a-f)",
            HexCase::Upper => "Uppercase (A-F)",
        }
    }

    /// Fast 16-element byte table for hexadecimal digits.
    #[inline]
    pub const fn digits(&self) -> &'static [u8; 16] {
        match self {
            HexCase::Lower => b"0123456789abcdef",
            HexCase::Upper => b"0123456789ABCDEF",
        }
    }

    /// Formats a single byte as a 2-digit hex string without `0x` prefix.
    #[inline]
    pub fn format_byte(&self, b: u8) -> String {
        match self {
            HexCase::Lower => format!("{:02x}", b),
            HexCase::Upper => format!("{:02X}", b),
        }
    }

    /// Formats an unsigned 16-bit integer as a 4-digit hex string without `0x` prefix.
    #[inline]
    pub fn format_u16(&self, v: u16) -> String {
        match self {
            HexCase::Lower => format!("{:04x}", v),
            HexCase::Upper => format!("{:04X}", v),
        }
    }

    /// Formats an unsigned 32-bit integer as an 8-digit hex string without `0x` prefix.
    #[inline]
    pub fn format_u32(&self, v: u32) -> String {
        match self {
            HexCase::Lower => format!("{:08x}", v),
            HexCase::Upper => format!("{:08X}", v),
        }
    }

    /// Formats an unsigned 64-bit integer as a 16-digit hex string without `0x` prefix.
    #[inline]
    pub fn format_u64(&self, v: u64) -> String {
        match self {
            HexCase::Lower => format!("{:016x}", v),
            HexCase::Upper => format!("{:016X}", v),
        }
    }

    /// Formats an offset with the given zero-padding width without `0x` prefix.
    #[inline]
    pub fn format_offset(&self, offset: usize, width: usize) -> String {
        match self {
            HexCase::Lower => format!("{:0width$x}", offset, width = width),
            HexCase::Upper => format!("{:0width$X}", offset, width = width),
        }
    }

    /// Formats a value with a lowercase `0x` prefix and unpadded hex digits.
    #[inline]
    pub fn format_prefixed(&self, value: usize) -> String {
        match self {
            HexCase::Lower => format!("0x{:x}", value),
            HexCase::Upper => format!("0x{:X}", value),
        }
    }

    /// Formats a value with a lowercase `0x` prefix and zero-padded hex digits.
    #[inline]
    pub fn format_prefixed_pad(&self, value: usize, width: usize) -> String {
        match self {
            HexCase::Lower => format!("0x{:0width$x}", value, width = width),
            HexCase::Upper => format!("0x{:0width$X}", value, width = width),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_case_byte_and_number_formatting() {
        assert_eq!(HexCase::Lower.format_byte(0xAB), "ab");
        assert_eq!(HexCase::Upper.format_byte(0xAB), "AB");
        assert_eq!(HexCase::Lower.format_u16(0x1F2E), "1f2e");
        assert_eq!(HexCase::Upper.format_u16(0x1F2E), "1F2E");
        assert_eq!(HexCase::Lower.format_u32(0xDEADBEEF), "deadbeef");
        assert_eq!(HexCase::Upper.format_u32(0xDEADBEEF), "DEADBEEF");
        assert_eq!(HexCase::Lower.format_u64(0xCAFEBABE1234ABCD), "cafebabe1234abcd");
        assert_eq!(HexCase::Upper.format_u64(0xCAFEBABE1234ABCD), "CAFEBABE1234ABCD");
    }

    #[test]
    fn test_hex_case_prefixed_and_padded_offsets() {
        assert_eq!(HexCase::Lower.format_offset(0x1A, 4), "001a");
        assert_eq!(HexCase::Upper.format_offset(0x1A, 4), "001A");
        assert_eq!(HexCase::Lower.format_prefixed(0x1A), "0x1a");
        assert_eq!(HexCase::Upper.format_prefixed(0x1A), "0x1A");
        assert_eq!(HexCase::Lower.format_prefixed_pad(0x1A, 8), "0x0000001a");
        assert_eq!(HexCase::Upper.format_prefixed_pad(0x1A, 8), "0x0000001A");
    }

    #[test]
    fn test_hex_case_digits_table() {
        assert_eq!(HexCase::Lower.digits(), b"0123456789abcdef");
        assert_eq!(HexCase::Upper.digits(), b"0123456789ABCDEF");
    }
}
