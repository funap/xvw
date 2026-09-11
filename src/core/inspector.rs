//! Pure binary data inspection and type conversion logic.
//!
//! Provides conversion between raw byte sequences and primitive data types
//! (Hex, signed/unsigned integers, floating-point numbers) with endianness support
//! and strict validation.

use serde::{Deserialize, Serialize};

/// Supported field types in the data inspector.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum InspectorField {
    Hex8,
    Hex16,
    Hex32,
    Hex64,
    Int8,
    UInt8,
    Int16,
    UInt16,
    Int32,
    UInt32,
    Int64,
    UInt64,
    Float32,
    Float64,
}

impl InspectorField {
    /// All inspector fields in display order.
    #[allow(dead_code)]
    pub const ALL: &'static [InspectorField] = &[
        Self::Hex8,
        Self::Hex16,
        Self::Hex32,
        Self::Hex64,
        Self::Int8,
        Self::UInt8,
        Self::Int16,
        Self::UInt16,
        Self::Int32,
        Self::UInt32,
        Self::Int64,
        Self::UInt64,
        Self::Float32,
        Self::Float64,
    ];

    /// Returns the required byte length for this field type.
    #[inline]
    pub fn byte_len(&self) -> usize {
        match self {
            Self::Hex8 | Self::Int8 | Self::UInt8 => 1,
            Self::Hex16 | Self::Int16 | Self::UInt16 => 2,
            Self::Hex32 | Self::Int32 | Self::UInt32 | Self::Float32 => 4,
            Self::Hex64 | Self::Int64 | Self::UInt64 | Self::Float64 => 8,
        }
    }

    /// Returns the human-readable label for this field.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Hex8 => "Hex (1 byte)",
            Self::Hex16 => "Hex (2 bytes)",
            Self::Hex32 => "Hex (4 bytes)",
            Self::Hex64 => "Hex (8 bytes)",
            Self::Int8 => "Int8",
            Self::UInt8 => "UInt8",
            Self::Int16 => "Int16",
            Self::UInt16 => "UInt16",
            Self::Int32 => "Int32",
            Self::UInt32 => "UInt32",
            Self::Int64 => "Int64",
            Self::UInt64 => "UInt64",
            Self::Float32 => "Float32",
            Self::Float64 => "Float64",
        }
    }

    /// Formats raw bytes as an input value string according to this field type and endianness.
    pub fn current_input_value(&self, bytes: &[u8], is_big_endian: bool) -> String {
        if bytes.len() < self.byte_len() {
            return String::new();
        }
        match self {
            Self::Hex8 => format!("0x{:02X}", bytes[0]),
            Self::Hex16 => {
                let arr: [u8; 2] = bytes[0..2].try_into().expect("2-byte slice");
                let val = if is_big_endian { u16::from_be_bytes(arr) } else { u16::from_le_bytes(arr) };
                format!("0x{:04X}", val)
            }
            Self::Hex32 => {
                let arr: [u8; 4] = bytes[0..4].try_into().expect("4-byte slice");
                let val = if is_big_endian { u32::from_be_bytes(arr) } else { u32::from_le_bytes(arr) };
                format!("0x{:08X}", val)
            }
            Self::Hex64 => {
                let arr: [u8; 8] = bytes[0..8].try_into().expect("8-byte slice");
                let val = if is_big_endian { u64::from_be_bytes(arr) } else { u64::from_le_bytes(arr) };
                format!("0x{:016X}", val)
            }
            Self::Int8 => format!("{}", bytes[0] as i8),
            Self::UInt8 => format!("{}", bytes[0]),
            Self::Int16 => {
                let arr: [u8; 2] = bytes[0..2].try_into().expect("2-byte slice");
                let val = if is_big_endian { i16::from_be_bytes(arr) } else { i16::from_le_bytes(arr) };
                format!("{}", val)
            }
            Self::UInt16 => {
                let arr: [u8; 2] = bytes[0..2].try_into().expect("2-byte slice");
                let val = if is_big_endian { u16::from_be_bytes(arr) } else { u16::from_le_bytes(arr) };
                format!("{}", val)
            }
            Self::Int32 => {
                let arr: [u8; 4] = bytes[0..4].try_into().expect("4-byte slice");
                let val = if is_big_endian { i32::from_be_bytes(arr) } else { i32::from_le_bytes(arr) };
                format!("{}", val)
            }
            Self::UInt32 => {
                let arr: [u8; 4] = bytes[0..4].try_into().expect("4-byte slice");
                let val = if is_big_endian { u32::from_be_bytes(arr) } else { u32::from_le_bytes(arr) };
                format!("{}", val)
            }
            Self::Int64 => {
                let arr: [u8; 8] = bytes[0..8].try_into().expect("8-byte slice");
                let val = if is_big_endian { i64::from_be_bytes(arr) } else { i64::from_le_bytes(arr) };
                format!("{}", val)
            }
            Self::UInt64 => {
                let arr: [u8; 8] = bytes[0..8].try_into().expect("8-byte slice");
                let val = if is_big_endian { u64::from_be_bytes(arr) } else { u64::from_le_bytes(arr) };
                format!("{}", val)
            }
            Self::Float32 => {
                let arr: [u8; 4] = bytes[0..4].try_into().expect("4-byte slice");
                let val = if is_big_endian { f32::from_be_bytes(arr) } else { f32::from_le_bytes(arr) };
                format!("{}", val)
            }
            Self::Float64 => {
                let arr: [u8; 8] = bytes[0..8].try_into().expect("8-byte slice");
                let val = if is_big_endian { f64::from_be_bytes(arr) } else { f64::from_le_bytes(arr) };
                format!("{}", val)
            }
        }
    }

    /// Parses user-entered text and serializes it into byte representation with given endianness.
    pub fn parse_and_serialize(&self, text: &str, is_big_endian: bool) -> Result<Vec<u8>, String> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err("Value cannot be empty".to_string());
        }

        match self {
            Self::Hex8 => {
                let hex_str = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")).unwrap_or(trimmed);
                let val = u8::from_str_radix(hex_str, 16).map_err(|_| "Invalid Hex (1 byte) value (expected 0x00..0xFF)".to_string())?;
                Ok(vec![val])
            }
            Self::Hex16 => {
                let hex_str = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")).unwrap_or(trimmed);
                let val = u16::from_str_radix(hex_str, 16).map_err(|_| "Invalid Hex (2 bytes) value (expected 0x0000..0xFFFF)".to_string())?;
                Ok(if is_big_endian {
                    val.to_be_bytes().to_vec()
                } else {
                    val.to_le_bytes().to_vec()
                })
            }
            Self::Hex32 => {
                let hex_str = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")).unwrap_or(trimmed);
                let val = u32::from_str_radix(hex_str, 16).map_err(|_| "Invalid Hex (4 bytes) value (expected 0x00000000..0xFFFFFFFF)".to_string())?;
                Ok(if is_big_endian {
                    val.to_be_bytes().to_vec()
                } else {
                    val.to_le_bytes().to_vec()
                })
            }
            Self::Hex64 => {
                let hex_str = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")).unwrap_or(trimmed);
                let val = u64::from_str_radix(hex_str, 16).map_err(|_| "Invalid Hex (8 bytes) value".to_string())?;
                Ok(if is_big_endian {
                    val.to_be_bytes().to_vec()
                } else {
                    val.to_le_bytes().to_vec()
                })
            }
            Self::Int8 => {
                let val = if let Some(hex_str) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
                    let u = u8::from_str_radix(hex_str, 16).map_err(|_| "Hex out of range for 1 byte (0x00..0xFF)".to_string())?;
                    u as i8
                } else {
                    trimmed.parse::<i8>().map_err(|_| "Value out of range for Int8 (-128..127)".to_string())?
                };
                Ok(vec![val as u8])
            }
            Self::UInt8 => {
                let val = if let Some(hex_str) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
                    u8::from_str_radix(hex_str, 16).map_err(|_| "Hex out of range for UInt8 (0x00..0xFF)".to_string())?
                } else {
                    trimmed.parse::<u8>().map_err(|_| "Value out of range for UInt8 (0..255)".to_string())?
                };
                Ok(vec![val])
            }
            Self::Int16 => {
                let val = if let Some(hex_str) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
                    let u = u16::from_str_radix(hex_str, 16).map_err(|_| "Hex out of range for 2 bytes (0x0000..0xFFFF)".to_string())?;
                    u as i16
                } else {
                    trimmed.parse::<i16>().map_err(|_| "Value out of range for Int16 (-32768..32767)".to_string())?
                };
                Ok(if is_big_endian {
                    val.to_be_bytes().to_vec()
                } else {
                    val.to_le_bytes().to_vec()
                })
            }
            Self::UInt16 => {
                let val = if let Some(hex_str) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
                    u16::from_str_radix(hex_str, 16).map_err(|_| "Hex out of range for UInt16 (0x0000..0xFFFF)".to_string())?
                } else {
                    trimmed.parse::<u16>().map_err(|_| "Value out of range for UInt16 (0..65535)".to_string())?
                };
                Ok(if is_big_endian {
                    val.to_be_bytes().to_vec()
                } else {
                    val.to_le_bytes().to_vec()
                })
            }
            Self::Int32 => {
                let val = if let Some(hex_str) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
                    let u = u32::from_str_radix(hex_str, 16).map_err(|_| "Hex out of range for 4 bytes".to_string())?;
                    u as i32
                } else {
                    trimmed
                        .parse::<i32>()
                        .map_err(|_| "Value out of range for Int32 (-2147483648..2147483647)".to_string())?
                };
                Ok(if is_big_endian {
                    val.to_be_bytes().to_vec()
                } else {
                    val.to_le_bytes().to_vec()
                })
            }
            Self::UInt32 => {
                let val = if let Some(hex_str) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
                    u32::from_str_radix(hex_str, 16).map_err(|_| "Hex out of range for UInt32".to_string())?
                } else {
                    trimmed
                        .parse::<u32>()
                        .map_err(|_| "Value out of range for UInt32 (0..4294967295)".to_string())?
                };
                Ok(if is_big_endian {
                    val.to_be_bytes().to_vec()
                } else {
                    val.to_le_bytes().to_vec()
                })
            }
            Self::Int64 => {
                let val = if let Some(hex_str) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
                    let u = u64::from_str_radix(hex_str, 16).map_err(|_| "Hex out of range for 8 bytes".to_string())?;
                    u as i64
                } else {
                    trimmed.parse::<i64>().map_err(|_| "Value out of range for Int64".to_string())?
                };
                Ok(if is_big_endian {
                    val.to_be_bytes().to_vec()
                } else {
                    val.to_le_bytes().to_vec()
                })
            }
            Self::UInt64 => {
                let val = if let Some(hex_str) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
                    u64::from_str_radix(hex_str, 16).map_err(|_| "Hex out of range for UInt64".to_string())?
                } else {
                    trimmed.parse::<u64>().map_err(|_| "Value out of range for UInt64".to_string())?
                };
                Ok(if is_big_endian {
                    val.to_be_bytes().to_vec()
                } else {
                    val.to_le_bytes().to_vec()
                })
            }
            Self::Float32 => {
                let val = trimmed.parse::<f32>().map_err(|_| "Invalid Float32 value".to_string())?;
                Ok(if is_big_endian {
                    val.to_be_bytes().to_vec()
                } else {
                    val.to_le_bytes().to_vec()
                })
            }
            Self::Float64 => {
                let val = trimmed.parse::<f64>().map_err(|_| "Invalid Float64 value".to_string())?;
                Ok(if is_big_endian {
                    val.to_be_bytes().to_vec()
                } else {
                    val.to_le_bytes().to_vec()
                })
            }
        }
    }
}

/// Formats up to 8 bytes into hex strings for 8-bit, 16-bit, 32-bit, and 64-bit representations.
pub fn format_hex_values(bytes: &[u8], is_big_endian: bool) -> (String, String, String, String) {
    let mut hex8 = "--".to_string();
    let mut hex16 = "--".to_string();
    let mut hex32 = "--".to_string();
    let mut hex64 = "--".to_string();

    if !bytes.is_empty() {
        hex8 = format!("0x{:02X}", bytes[0]);
    }

    if bytes.len() >= 2 {
        let arr: [u8; 2] = bytes[0..2].try_into().expect("2-byte slice");
        let val = if is_big_endian { u16::from_be_bytes(arr) } else { u16::from_le_bytes(arr) };
        hex16 = format!("0x{:04X}", val);
    }

    if bytes.len() >= 4 {
        let arr: [u8; 4] = bytes[0..4].try_into().expect("4-byte slice");
        let val = if is_big_endian { u32::from_be_bytes(arr) } else { u32::from_le_bytes(arr) };
        hex32 = format!("0x{:08X}", val);
    }

    if bytes.len() >= 8 {
        let arr: [u8; 8] = bytes[0..8].try_into().expect("8-byte slice");
        let val = if is_big_endian { u64::from_be_bytes(arr) } else { u64::from_le_bytes(arr) };
        hex64 = format!("0x{:016X}", val);
    }

    (hex8, hex16, hex32, hex64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inspector_field_lengths() {
        assert_eq!(InspectorField::Hex8.byte_len(), 1);
        assert_eq!(InspectorField::Hex16.byte_len(), 2);
        assert_eq!(InspectorField::Hex32.byte_len(), 4);
        assert_eq!(InspectorField::Hex64.byte_len(), 8);

        assert_eq!(InspectorField::Int8.byte_len(), 1);
        assert_eq!(InspectorField::UInt8.byte_len(), 1);
        assert_eq!(InspectorField::Int16.byte_len(), 2);
        assert_eq!(InspectorField::UInt16.byte_len(), 2);
        assert_eq!(InspectorField::Int32.byte_len(), 4);
        assert_eq!(InspectorField::UInt32.byte_len(), 4);
        assert_eq!(InspectorField::Int64.byte_len(), 8);
        assert_eq!(InspectorField::UInt64.byte_len(), 8);

        assert_eq!(InspectorField::Float32.byte_len(), 4);
        assert_eq!(InspectorField::Float64.byte_len(), 8);
    }

    #[test]
    fn test_inspector_int8_parsing_and_overflow() {
        let field = InspectorField::Int8;
        // Valid decimal
        assert_eq!(field.parse_and_serialize("0", false).unwrap(), vec![0]);
        assert_eq!(field.parse_and_serialize("127", false).unwrap(), vec![127]);
        assert_eq!(field.parse_and_serialize("-128", false).unwrap(), vec![0x80]);
        assert_eq!(field.parse_and_serialize("-1", false).unwrap(), vec![0xFF]);

        // Overflow checks - must reject and never write into adjacent bytes!
        assert!(field.parse_and_serialize("128", false).is_err());
        assert!(field.parse_and_serialize("255", false).is_err());
        assert!(field.parse_and_serialize("-129", false).is_err());

        // Hex formatting
        assert_eq!(field.parse_and_serialize("0x7F", false).unwrap(), vec![0x7F]);
        assert_eq!(field.parse_and_serialize("0x80", false).unwrap(), vec![0x80]);
        assert_eq!(field.parse_and_serialize("0xFF", false).unwrap(), vec![0xFF]);
        assert!(field.parse_and_serialize("0x100", false).is_err());
    }

    #[test]
    fn test_inspector_uint8_parsing_and_overflow() {
        let field = InspectorField::UInt8;
        assert_eq!(field.parse_and_serialize("0", false).unwrap(), vec![0]);
        assert_eq!(field.parse_and_serialize("255", false).unwrap(), vec![255]);
        assert!(field.parse_and_serialize("256", false).is_err());
        assert!(field.parse_and_serialize("-1", false).is_err());
        assert_eq!(field.parse_and_serialize("0xFF", false).unwrap(), vec![0xFF]);
        assert!(field.parse_and_serialize("0x100", false).is_err());
    }

    #[test]
    fn test_inspector_uint16_parsing_and_endianness() {
        let field = InspectorField::UInt16;
        // 0x1234 = 4660
        let le = field.parse_and_serialize("4660", false).unwrap();
        assert_eq!(le, vec![0x34, 0x12]);

        let be = field.parse_and_serialize("4660", true).unwrap();
        assert_eq!(be, vec![0x12, 0x34]);

        // Hex input
        let hex_le = field.parse_and_serialize("0x1234", false).unwrap();
        assert_eq!(hex_le, vec![0x34, 0x12]);

        // Overflow
        assert!(field.parse_and_serialize("65536", false).is_err());
        assert!(field.parse_and_serialize("-1", false).is_err());
        assert!(field.parse_and_serialize("0x10000", false).is_err());
    }

    #[test]
    fn test_inspector_int16_parsing_and_endianness() {
        let field = InspectorField::Int16;
        let le = field.parse_and_serialize("-32768", false).unwrap();
        assert_eq!(le, vec![0x00, 0x80]);

        let be = field.parse_and_serialize("-32768", true).unwrap();
        assert_eq!(be, vec![0x80, 0x00]);

        assert!(field.parse_and_serialize("32768", false).is_err());
        assert!(field.parse_and_serialize("-32769", false).is_err());
    }

    #[test]
    fn test_inspector_int32_and_uint32() {
        let u32_field = InspectorField::UInt32;
        let val = u32_field.parse_and_serialize("4294967295", false).unwrap();
        assert_eq!(val, vec![0xFF, 0xFF, 0xFF, 0xFF]);
        assert!(u32_field.parse_and_serialize("4294967296", false).is_err());

        let i32_field = InspectorField::Int32;
        let val_neg = i32_field.parse_and_serialize("-1", false).unwrap();
        assert_eq!(val_neg, vec![0xFF, 0xFF, 0xFF, 0xFF]);
        assert!(i32_field.parse_and_serialize("2147483648", false).is_err());
    }

    #[test]
    fn test_inspector_float_parsing() {
        let f32_field = InspectorField::Float32;
        let le = f32_field.parse_and_serialize("1.0", false).unwrap();
        assert_eq!(le, 1.0f32.to_le_bytes().to_vec());

        let be = f32_field.parse_and_serialize("1.0", true).unwrap();
        assert_eq!(be, 1.0f32.to_be_bytes().to_vec());

        assert!(f32_field.parse_and_serialize("not_a_number", false).is_err());
    }

    #[test]
    fn test_inspector_hex_fields() {
        let hex8 = InspectorField::Hex8;
        assert_eq!(hex8.parse_and_serialize("AB", false).unwrap(), vec![0xAB]);
        assert_eq!(hex8.parse_and_serialize("0xAB", false).unwrap(), vec![0xAB]);
        assert!(hex8.parse_and_serialize("100", false).is_err());

        let hex16 = InspectorField::Hex16;
        assert_eq!(hex16.parse_and_serialize("ABCD", false).unwrap(), vec![0xCD, 0xAB]);
        assert_eq!(hex16.parse_and_serialize("0xABCD", true).unwrap(), vec![0xAB, 0xCD]);
        assert!(hex16.parse_and_serialize("10000", false).is_err());
    }

    #[test]
    fn test_inspector_current_input_value() {
        let bytes = [0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(InspectorField::Hex8.current_input_value(&bytes, false), "0x34");
        assert_eq!(InspectorField::UInt8.current_input_value(&bytes, false), "52");
        assert_eq!(InspectorField::Int8.current_input_value(&bytes, false), "52");

        // UInt16 Little Endian: 0x1234 = 4660
        assert_eq!(InspectorField::UInt16.current_input_value(&bytes, false), "4660");
        assert_eq!(InspectorField::Hex16.current_input_value(&bytes, false), "0x1234");

        // UInt16 Big Endian: 0x3412 = 13330
        assert_eq!(InspectorField::UInt16.current_input_value(&bytes, true), "13330");
        assert_eq!(InspectorField::Hex16.current_input_value(&bytes, true), "0x3412");
    }

    #[test]
    fn test_empty_string_error() {
        assert!(InspectorField::Hex8.parse_and_serialize("", false).is_err());
        assert!(InspectorField::Hex8.parse_and_serialize("   ", false).is_err());
    }
}
