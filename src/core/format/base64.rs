use crate::core::address_map::AddressMap;
use crate::core::format::raw_binary::export_raw_binary;
use std::fmt;
use std::path::Path;

const BASE64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Error type when parsing a Base64 encoded file or string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Base64ImportError {
    EmptyInput,
    InvalidCharacter { char: char },
    InvalidLength,
    InvalidPadding,
}

impl fmt::Display for Base64ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Base64ImportError::EmptyInput => write!(f, "File is empty"),
            Base64ImportError::InvalidCharacter { char } => write!(f, "Invalid base64 character: '{}'", char),
            Base64ImportError::InvalidLength => write!(f, "Invalid base64 length"),
            Base64ImportError::InvalidPadding => write!(f, "Invalid base64 padding"),
        }
    }
}

impl std::error::Error for Base64ImportError {}

/// Parses Base64 encoded content into raw binary bytes.
///
/// Supports RFC 4648 standard (`+`, `/`) and URL-safe (`-`, `_`) alphabets,
/// handles optional line wrapping and arbitrary ASCII whitespace,
/// and accepts both padded (`=`) and unpadded inputs.
pub fn parse_base64(content: &str) -> Result<Vec<u8>, Base64ImportError> {
    let mut data_values: Vec<u8> = Vec::with_capacity(content.len());
    let mut padding_count = 0usize;
    let mut seen_padding = false;

    for ch in content.chars() {
        if ch.is_ascii_whitespace() {
            continue;
        }

        if ch == '=' {
            seen_padding = true;
            padding_count += 1;
            if padding_count > 2 {
                return Err(Base64ImportError::InvalidPadding);
            }
            continue;
        }

        if seen_padding {
            // Non-padding character encountered after padding
            return Err(Base64ImportError::InvalidPadding);
        }

        let val = match ch {
            'A'..='Z' => (ch as u8) - b'A',
            'a'..='z' => (ch as u8) - b'a' + 26,
            '0'..='9' => (ch as u8) - b'0' + 52,
            '+' | '-' => 62,
            '/' | '_' => 63,
            _ => return Err(Base64ImportError::InvalidCharacter { char: ch }),
        };
        data_values.push(val);
    }

    if data_values.is_empty() && padding_count == 0 {
        return Err(Base64ImportError::EmptyInput);
    }

    let total_symbols = data_values.len() + padding_count;
    if padding_count > 0 && !total_symbols.is_multiple_of(4) {
        return Err(Base64ImportError::InvalidPadding);
    }

    let remainder = data_values.len() % 4;
    if remainder == 1 {
        return Err(Base64ImportError::InvalidLength);
    }

    let full_chunks = data_values.len() / 4;
    let mut output = Vec::with_capacity(full_chunks * 3 + remainder);

    let (chunks, _) = data_values.as_chunks::<4>();
    for chunk in chunks {
        let b0 = (chunk[0] << 2) | (chunk[1] >> 4);
        let b1 = ((chunk[1] & 0x0F) << 4) | (chunk[2] >> 2);
        let b2 = ((chunk[2] & 0x03) << 6) | chunk[3];
        output.push(b0);
        output.push(b1);
        output.push(b2);
    }

    let rem_slice = &data_values[full_chunks * 4..];
    match rem_slice.len() {
        2 => {
            let b0 = (rem_slice[0] << 2) | (rem_slice[1] >> 4);
            output.push(b0);
        }
        3 => {
            let b0 = (rem_slice[0] << 2) | (rem_slice[1] >> 4);
            let b1 = ((rem_slice[1] & 0x0F) << 4) | (rem_slice[2] >> 2);
            output.push(b0);
            output.push(b1);
        }
        _ => {}
    }

    Ok(output)
}

/// Exports raw binary data and its AddressMap to a Base64 encoded string.
///
/// Any memory segment gaps are filled with `0x00`.
/// Output lines are wrapped at 64 characters (RFC 1421 / PEM format),
/// formatted using the line ending specified by `address_map.format_options.line_ending`.
pub fn export_base64(data: &[u8], address_map: &AddressMap) -> String {
    if data.is_empty() {
        return String::new();
    }

    let linear_bytes = export_raw_binary(data, address_map, 0x00);
    if linear_bytes.is_empty() {
        return String::new();
    }

    let line_ending = if address_map.format_options.crlf { "\r\n" } else { "\n" };

    let total_encoded_len = linear_bytes.len().div_ceil(3) * 4;
    let lines_count = total_encoded_len.div_ceil(64);
    let mut out = String::with_capacity(total_encoded_len + lines_count * line_ending.len());

    let mut line_char_count = 0;
    for chunk in linear_bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };

        let c0 = BASE64_TABLE[(b0 >> 2) as usize] as char;
        let c1 = BASE64_TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char;
        let c2 = if chunk.len() > 1 {
            BASE64_TABLE[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char
        } else {
            '='
        };
        let c3 = if chunk.len() > 2 { BASE64_TABLE[(b2 & 0x3F) as usize] as char } else { '=' };

        out.push(c0);
        out.push(c1);
        out.push(c2);
        out.push(c3);
        line_char_count += 4;

        if line_char_count == 64 {
            out.push_str(line_ending);
            line_char_count = 0;
        }
    }

    if line_char_count > 0 {
        out.push_str(line_ending);
    }

    out
}

/// Checks if the file path has a Base64 extension (`.b64` or `.base64`).
#[allow(dead_code)]
pub fn is_base64_extension(path: &Path) -> bool {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    matches!(ext.as_str(), "b64" | "base64")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::address_map::{HexFormatOptions, MemorySegment};

    #[test]
    fn test_parse_base64_rfc_vectors() {
        assert_eq!(parse_base64("").unwrap_err(), Base64ImportError::EmptyInput);
        assert_eq!(parse_base64("   \r\n\t  ").unwrap_err(), Base64ImportError::EmptyInput);
        assert_eq!(parse_base64("Zg==").unwrap(), b"f");
        assert_eq!(parse_base64("Zm8=").unwrap(), b"fo");
        assert_eq!(parse_base64("Zm9v").unwrap(), b"foo");
        assert_eq!(parse_base64("Zm9vYg==").unwrap(), b"foob");
        assert_eq!(parse_base64("Zm9vYmE=").unwrap(), b"fooba");
        assert_eq!(parse_base64("Zm9vYmFy").unwrap(), b"foobar");
    }

    #[test]
    fn test_parse_base64_unpadded() {
        assert_eq!(parse_base64("Zg").unwrap(), b"f");
        assert_eq!(parse_base64("Zm8").unwrap(), b"fo");
        assert_eq!(parse_base64("Zm9vYg").unwrap(), b"foob");
        assert_eq!(parse_base64("Zm9vYmE").unwrap(), b"fooba");
    }

    #[test]
    fn test_parse_base64_url_safe() {
        let sample = [0xFB, 0xFF, 0xFE];
        assert_eq!(parse_base64("+//+").unwrap(), sample);
        assert_eq!(parse_base64("-__-").unwrap(), sample);
    }

    #[test]
    fn test_parse_base64_with_newlines_and_spaces() {
        let wrapped = "Zm9v\n  \t\r\n  YmFy\n";
        assert_eq!(parse_base64(wrapped).unwrap(), b"foobar");
    }

    #[test]
    fn test_parse_base64_errors() {
        assert_eq!(parse_base64("Zm9v!").unwrap_err(), Base64ImportError::InvalidCharacter { char: '!' });
        assert_eq!(parse_base64("Z").unwrap_err(), Base64ImportError::InvalidLength);
        assert_eq!(parse_base64("Zm9vY").unwrap_err(), Base64ImportError::InvalidLength);
        assert_eq!(parse_base64("Zm=v").unwrap_err(), Base64ImportError::InvalidPadding);
        assert_eq!(parse_base64("Z===").unwrap_err(), Base64ImportError::InvalidPadding);
        assert_eq!(parse_base64("Zm8=A").unwrap_err(), Base64ImportError::InvalidPadding);
    }

    #[test]
    fn test_export_base64_empty() {
        let map = AddressMap::default();
        assert_eq!(export_base64(&[], &map), "");
    }

    #[test]
    fn test_export_base64_basic_line_wrapping() {
        let data = vec![0x41; 60];
        let map = AddressMap::default();
        let exported = export_base64(&data, &map);
        let lines: Vec<&str> = exported.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].len(), 64);
        assert_eq!(lines[1].len(), 16);
        assert_eq!(parse_base64(&exported).unwrap(), data);
    }

    #[test]
    fn test_export_base64_crlf_line_ending() {
        let data = vec![0x42; 60];
        let map = AddressMap {
            format_options: HexFormatOptions {
                crlf: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let exported = export_base64(&data, &map);
        assert!(exported.contains("\r\n"));
        assert_eq!(parse_base64(&exported).unwrap(), data);
    }

    #[test]
    fn test_export_base64_with_address_gaps() {
        let data = vec![0x11, 0x22, 0x33, 0x44];
        let map = AddressMap::from_segments(vec![
            MemorySegment {
                buffer_offset: 0,
                address: 2,
                length: 2,
            },
            MemorySegment {
                buffer_offset: 2,
                address: 6,
                length: 2,
            },
        ]);
        let exported = export_base64(&data, &map);
        let imported = parse_base64(&exported).unwrap();
        assert_eq!(imported, vec![0x00, 0x00, 0x11, 0x22, 0x00, 0x00, 0x33, 0x44]);
    }

    #[test]
    fn test_is_base64_extension() {
        assert!(is_base64_extension(Path::new("test.b64")));
        assert!(is_base64_extension(Path::new("test.B64")));
        assert!(is_base64_extension(Path::new("test.base64")));
        assert!(is_base64_extension(Path::new("test.BASE64")));
        assert!(!is_base64_extension(Path::new("test.bin")));
        assert!(!is_base64_extension(Path::new("test.hex")));
    }
}
