use crate::core::encoding::Encoding;

/// Formats a byte size into a friendly human-readable string with exact byte count.
/// E.g.: "1.25 MB (1,310,720 B)", "45.0 KB (46,080 B)", "512 B"
pub fn format_size_friendly(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        let kb = bytes as f64 / 1024.0;
        format!("{:.1} KB ({} B)", kb, format_with_commas(bytes))
    } else if bytes < 1024 * 1024 * 1024 {
        let mb = bytes as f64 / (1024.0 * 1024.0);
        format!("{:.2} MB ({} B)", mb, format_with_commas(bytes))
    } else {
        let gb = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        format!("{:.2} GB ({} B)", gb, format_with_commas(bytes))
    }
}

/// Formats an integer with thousands separator commas.
pub fn format_with_commas(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    let rem = s.len() % 3;
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (i == rem || (i > rem && (i - rem).is_multiple_of(3))) {
            result.push(',');
        }
        result.push(c);
    }
    result
}

/// Formats text representation for bytes based on text encoding.
pub fn format_text_repr(slice: &[u8], encoding: Encoding) -> String {
    match encoding {
        Encoding::Ascii => {
            if slice.len() == 1 {
                let b = slice[0];
                if (0x20..=0x7E).contains(&b) {
                    format!("'{}'", b as char)
                } else if b > 0x7F {
                    "invalid ASCII".to_string()
                } else if b == 0 {
                    "'\\0'".to_string()
                } else if b == b'\n' {
                    "'\\n'".to_string()
                } else if b == b'\r' {
                    "'\\r'".to_string()
                } else if b == b'\t' {
                    "'\\t'".to_string()
                } else {
                    "non-printable ASCII".to_string()
                }
            } else {
                let all_ascii_printable = slice.iter().all(|&b| (0x20..=0x7E).contains(&b));
                if all_ascii_printable {
                    let s: String = slice.iter().map(|&b| b as char).collect();
                    format!("\"{}\"", s)
                } else {
                    let has_non_ascii = slice.iter().any(|&b| b > 0x7F);
                    if has_non_ascii {
                        "invalid ASCII".to_string()
                    } else {
                        "non-printable ASCII".to_string()
                    }
                }
            }
        }
        Encoding::Utf8 => match std::str::from_utf8(slice) {
            Ok(s) => {
                if s.chars().all(|c| !c.is_control() || c == '\n' || c == '\r' || c == '\t') {
                    if slice.len() == 1 { format!("'{}'", s) } else { format!("\"{}\"", s) }
                } else {
                    "non-printable UTF-8".to_string()
                }
            }
            Err(_) => "invalid UTF-8".to_string(),
        },
        Encoding::Utf16Le => {
            if !slice.len().is_multiple_of(2) {
                "invalid UTF-16 LE".to_string()
            } else {
                let u16s: Vec<u16> = slice.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
                match String::from_utf16(&u16s) {
                    Ok(s) => {
                        if s.chars().all(|c| !c.is_control() || c == '\n' || c == '\r' || c == '\t') {
                            format!("\"{}\"", s)
                        } else {
                            "non-printable UTF-16 LE".to_string()
                        }
                    }
                    Err(_) => "invalid UTF-16 LE".to_string(),
                }
            }
        }
        Encoding::Utf16Be => {
            if !slice.len().is_multiple_of(2) {
                "invalid UTF-16 BE".to_string()
            } else {
                let u16s: Vec<u16> = slice.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect();
                match String::from_utf16(&u16s) {
                    Ok(s) => {
                        if s.chars().all(|c| !c.is_control() || c == '\n' || c == '\r' || c == '\t') {
                            format!("\"{}\"", s)
                        } else {
                            "non-printable UTF-16 BE".to_string()
                        }
                    }
                    Err(_) => "invalid UTF-16 BE".to_string(),
                }
            }
        }
        _ => {
            let mut s = String::new();
            let mut offset = 0;
            let mut valid = true;
            while offset < slice.len() {
                if let Some((c, len)) = encoding.decode_char_at(slice, offset) {
                    s.push(c);
                    offset += len;
                } else {
                    valid = false;
                    break;
                }
            }
            if valid && !s.is_empty() {
                if s.chars().all(|c| !c.is_control() || c == '\n' || c == '\r' || c == '\t') {
                    if s.chars().count() == 1 { format!("'{}'", s) } else { format!("\"{}\"", s) }
                } else {
                    format!("non-printable {}", encoding.label())
                }
            } else {
                format!("invalid {}", encoding.label())
            }
        }
    }
}

/// Formats integer value as binary with 8-bit grouping separated by space.
pub fn format_binary_repr(val: u64, byte_len: usize) -> String {
    let mut parts = Vec::with_capacity(byte_len);
    for i in (0..byte_len).rev() {
        let b = ((val >> (i * 8)) & 0xFF) as u8;
        parts.push(format!("{:08b}", b));
    }
    format!("0b{}", parts.join(" "))
}

/// Decodes up to 8 bytes into a uint value and formatted hex string according to endianness.
pub fn decode_uint_value(slice: &[u8], is_big_endian: bool) -> (u64, String) {
    let len = slice.len();
    match len {
        1 => {
            let val = slice[0] as u64;
            let hex_str = format!("0x{:02X}", val);
            (val, hex_str)
        }
        2 => {
            let val = if is_big_endian {
                u16::from_be_bytes([slice[0], slice[1]]) as u64
            } else {
                u16::from_le_bytes([slice[0], slice[1]]) as u64
            };
            let hex_str = format!("0x{:04X}", val);
            (val, hex_str)
        }
        3 => {
            let val = if is_big_endian {
                ((slice[0] as u64) << 16) | ((slice[1] as u64) << 8) | (slice[2] as u64)
            } else {
                (slice[0] as u64) | ((slice[1] as u64) << 8) | ((slice[2] as u64) << 16)
            };
            let hex_str = format!("0x{:06X}", val);
            (val, hex_str)
        }
        4 => {
            let val = if is_big_endian {
                u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]) as u64
            } else {
                u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]) as u64
            };
            let hex_str = format!("0x{:08X}", val);
            (val, hex_str)
        }
        5..=7 => {
            let mut buf = [0u8; 8];
            let val = if is_big_endian {
                buf[8 - len..].copy_from_slice(slice);
                u64::from_be_bytes(buf)
            } else {
                buf[..len].copy_from_slice(slice);
                u64::from_le_bytes(buf)
            };
            let hex_str = format!("0x{:0width$X}", val, width = len * 2);
            (val, hex_str)
        }
        8 => {
            let val = if is_big_endian {
                u64::from_be_bytes([slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7]])
            } else {
                u64::from_le_bytes([slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7]])
            };
            let hex_str = format!("0x{:016X}", val);
            (val, hex_str)
        }
        _ => (0, "0x0".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_binary_repr_spacing() {
        assert_eq!(format_binary_repr(0x4D, 1), "0b01001101");
        assert_eq!(format_binary_repr(0x1234, 2), "0b00010010 00110100");
        assert_eq!(format_binary_repr(0x12345678, 4), "0b00010010 00110100 01010110 01111000");
    }

    #[test]
    fn test_decode_uint_value_endianness() {
        let bytes = [0x12, 0x34];
        let (le_val, le_hex) = decode_uint_value(&bytes, false);
        assert_eq!(le_val, 0x3412);
        assert_eq!(le_hex, "0x3412");

        let (be_val, be_hex) = decode_uint_value(&bytes, true);
        assert_eq!(be_val, 0x1234);
        assert_eq!(be_hex, "0x1234");

        let bytes4 = [0x01, 0x02, 0x03, 0x04];
        let (le_val4, le_hex4) = decode_uint_value(&bytes4, false);
        assert_eq!(le_val4, 0x04030201);
        assert_eq!(le_hex4, "0x04030201");

        let (be_val4, be_hex4) = decode_uint_value(&bytes4, true);
        assert_eq!(be_val4, 0x01020304);
        assert_eq!(be_hex4, "0x01020304");
    }

    #[test]
    fn test_format_text_repr_encodings() {
        // ASCII
        assert_eq!(format_text_repr(b"A", Encoding::Ascii), "'A'");
        assert_eq!(format_text_repr(b"Test", Encoding::Ascii), "\"Test\"");
        assert_eq!(format_text_repr(&[0xFF], Encoding::Ascii), "invalid ASCII");

        // UTF-8
        assert_eq!(format_text_repr("あ".as_bytes(), Encoding::Utf8), "\"あ\"");
        assert_eq!(format_text_repr(&[0xFF, 0xFE], Encoding::Utf8), "invalid UTF-8");

        // UTF-16
        let utf16_le = [0x41, 0x00]; // 'A' in UTF-16 LE
        assert_eq!(format_text_repr(&utf16_le, Encoding::Utf16Le), "\"A\"");
        assert_eq!(format_text_repr(&[0x41], Encoding::Utf16Le), "invalid UTF-16 LE");
    }

    #[test]
    fn test_format_size_friendly() {
        assert_eq!(format_size_friendly(512), "512 B");
        assert_eq!(format_size_friendly(1024), "1.0 KB (1,024 B)");
        assert_eq!(format_size_friendly(46080), "45.0 KB (46,080 B)");
        assert_eq!(format_size_friendly(1310720), "1.25 MB (1,310,720 B)");
    }

    #[test]
    fn test_format_with_commas() {
        assert_eq!(format_with_commas(0), "0");
        assert_eq!(format_with_commas(999), "999");
        assert_eq!(format_with_commas(1000), "1,000");
        assert_eq!(format_with_commas(1234567), "1,234,567");
    }
}
