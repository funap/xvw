use super::{HexFormat, HexImportError, HexImportResult, assemble_chunks_into_result, parse_hex_byte};
use crate::core::address_map::{AddressMap, HexFormatOptions, MemorySegment};
use std::collections::BTreeMap;
use std::path::Path;

/// Parse an Intel HEX file from string content.
pub fn parse_intel_hex(content: &str) -> Result<HexImportResult, HexImportError> {
    let mut raw_chunks: Vec<(usize, Vec<u8>)> = Vec::new();
    let mut base_addr: usize = 0;
    let mut entry_point = None;
    let mut chunk_length_counts: BTreeMap<usize, usize> = BTreeMap::new();
    let crlf = content.contains("\r\n");

    let mut line_count = 0;
    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        line_count += 1;

        if !trimmed.starts_with(':') {
            return Err(HexImportError::InvalidRecordStart {
                line: line_idx + 1,
                char: trimmed.chars().next().unwrap_or(' '),
            });
        }

        if trimmed.len() < 11 {
            return Err(HexImportError::LineTooShort { line: line_idx + 1 });
        }

        let byte_count = parse_hex_byte(&trimmed[1..3], line_idx)? as usize;
        let addr_hi = parse_hex_byte(&trimmed[3..5], line_idx)?;
        let addr_lo = parse_hex_byte(&trimmed[5..7], line_idx)?;
        let record_type = parse_hex_byte(&trimmed[7..9], line_idx)?;
        let record_addr = ((addr_hi as usize) << 8) | (addr_lo as usize);

        let data_and_checksum_len = (byte_count + 1) * 2;
        if trimmed.len() < 9 + data_and_checksum_len {
            return Err(HexImportError::LineTooShort { line: line_idx + 1 });
        }

        // Checksum verification: sum of all bytes in line modulo 256 == 0
        let mut sum: u32 = (byte_count as u32) + (addr_hi as u32) + (addr_lo as u32) + (record_type as u32);
        let mut record_bytes = Vec::with_capacity(byte_count);

        let data_str = &trimmed[9..9 + byte_count * 2];
        for chunk in data_str.as_bytes().as_chunks::<2>().0 {
            let hex_slice = std::str::from_utf8(chunk).map_err(|_| HexImportError::InvalidHexDigits {
                line: line_idx + 1,
                content: String::from_utf8_lossy(chunk).to_string(),
            })?;
            let b = parse_hex_byte(hex_slice, line_idx)?;
            sum += b as u32;
            record_bytes.push(b);
        }

        let checksum_str = &trimmed[9 + byte_count * 2..9 + (byte_count + 1) * 2];
        let actual_checksum = parse_hex_byte(checksum_str, line_idx)?;
        let expected_checksum = ((!sum + 1) & 0xFF) as u8;

        if actual_checksum != expected_checksum {
            return Err(HexImportError::ChecksumMismatch {
                line: line_idx + 1,
                expected: expected_checksum,
                actual: actual_checksum,
            });
        }

        match record_type {
            0x00 => {
                // Data record
                let full_addr = base_addr.saturating_add(record_addr);
                if !record_bytes.is_empty() {
                    *chunk_length_counts.entry(record_bytes.len()).or_insert(0) += 1;
                    raw_chunks.push((full_addr, record_bytes));
                }
            }
            0x01 => {
                // End of File
                break;
            }
            0x02 => {
                // Extended Segment Address: segment << 4
                if record_bytes.len() >= 2 {
                    let seg = ((record_bytes[0] as usize) << 8) | (record_bytes[1] as usize);
                    base_addr = seg << 4;
                }
            }
            0x03 => {
                // Start Segment Address (CS:IP)
                if record_bytes.len() >= 4 {
                    let cs = ((record_bytes[0] as usize) << 8) | (record_bytes[1] as usize);
                    let ip = ((record_bytes[2] as usize) << 8) | (record_bytes[3] as usize);
                    entry_point = Some((cs << 4) + ip);
                }
            }
            0x04 => {
                // Extended Linear Address: upper 16 bits
                if record_bytes.len() >= 2 {
                    let upper = ((record_bytes[0] as usize) << 8) | (record_bytes[1] as usize);
                    base_addr = upper << 16;
                }
            }
            0x05 => {
                // Start Linear Address (EIP)
                if record_bytes.len() >= 4 {
                    let eip = ((record_bytes[0] as usize) << 24)
                        | ((record_bytes[1] as usize) << 16)
                        | ((record_bytes[2] as usize) << 8)
                        | (record_bytes[3] as usize);
                    entry_point = Some(eip);
                }
            }
            _ => {
                return Err(HexImportError::UnsupportedRecordType {
                    line: line_idx + 1,
                    record_type: format!("{:02X}", record_type),
                });
            }
        }
    }

    if line_count == 0 {
        return Err(HexImportError::EmptyInput);
    }
    if raw_chunks.is_empty() {
        return Err(HexImportError::NoDataRecords);
    }

    let record_data_length = chunk_length_counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(len, _)| len)
        .unwrap_or(16);

    let format_options = HexFormatOptions {
        record_data_length,
        address_width: 0,
        header: None,
        entry_point,
        has_count_record: false,
        crlf,
    };

    assemble_chunks_into_result(raw_chunks, HexFormat::IntelHex, entry_point, None, format_options)
}

/// Exports binary data and its AddressMap to Intel HEX string format.
pub fn export_intel_hex(data: &[u8], address_map: &AddressMap) -> String {
    let mut out = String::new();
    let line_ending = if address_map.format_options.crlf { "\r\n" } else { "\n" };

    let segments = if address_map.segments.is_empty() {
        vec![MemorySegment {
            buffer_offset: 0,
            address: 0,
            length: data.len(),
        }]
    } else {
        address_map.segments.clone()
    };

    let mut current_upper_16: Option<u16> = None;
    let chunk_size = if address_map.format_options.record_data_length > 0 {
        address_map.format_options.record_data_length
    } else {
        16 // standard 0x10 for Intel HEX
    };

    for seg in &segments {
        let seg_start = seg.buffer_offset.min(data.len());
        let seg_end = (seg.buffer_offset + seg.length).min(data.len());
        if seg_start >= seg_end {
            continue;
        }

        let seg_data = &data[seg_start..seg_end];
        let mut cur_addr = seg.address;

        for chunk in seg_data.chunks(chunk_size) {
            let upper_16 = ((cur_addr >> 16) & 0xFFFF) as u16;
            let lower_16 = (cur_addr & 0xFFFF) as u16;

            if current_upper_16 != Some(upper_16) {
                current_upper_16 = Some(upper_16);
                let u1 = ((upper_16 >> 8) & 0xFF) as u8;
                let u0 = (upper_16 & 0xFF) as u8;
                let sum: u32 = 2 + 4 + u1 as u32 + u0 as u32;
                let checksum = (!((sum & 0xFF) as u8)).wrapping_add(1);
                out.push_str(&format!(":02000004{:04X}{:02X}{}", upper_16, checksum, line_ending));
            }

            let byte_count = chunk.len() as u8;
            let a1 = ((lower_16 >> 8) & 0xFF) as u8;
            let a0 = (lower_16 & 0xFF) as u8;
            let mut sum: u32 = byte_count as u32 + a1 as u32 + a0 as u32;

            out.push_str(&format!(":{:02X}{:04X}00", byte_count, lower_16));
            for &b in chunk {
                sum += b as u32;
                out.push_str(&format!("{:02X}", b));
            }
            let checksum = (!((sum & 0xFF) as u8)).wrapping_add(1);
            out.push_str(&format!("{:02X}{}", checksum, line_ending));

            cur_addr += chunk.len();
        }
    }

    if let Some(entry) = address_map.format_options.entry_point {
        let e3 = ((entry >> 24) & 0xFF) as u8;
        let e2 = ((entry >> 16) & 0xFF) as u8;
        let e1 = ((entry >> 8) & 0xFF) as u8;
        let e0 = (entry & 0xFF) as u8;
        let sum: u32 = 4 + 5 + e3 as u32 + e2 as u32 + e1 as u32 + e0 as u32;
        let checksum = (!((sum & 0xFF) as u8)).wrapping_add(1);
        out.push_str(&format!(":04000005{:08X}{:02X}{}", entry, checksum, line_ending));
    }

    out.push_str(&format!(":00000001FF{}", line_ending));
    out
}

/// Checks if the file path has an Intel HEX extension.
#[allow(dead_code)]
pub fn is_hex_extension(path: &Path) -> bool {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    matches!(ext.as_str(), "hex" | "ihex" | "ihx")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_intel_hex_basic() {
        let ihex = ":0400000001020304F2\n:00000001FF\n";
        let res = parse_intel_hex(ihex).expect("valid intel hex");
        assert_eq!(res.format, HexFormat::IntelHex);
        assert_eq!(res.data, vec![0x01, 0x02, 0x03, 0x04]);
        assert_eq!(res.address_map.base_address(), 0x0000);
        assert_eq!(res.address_map.offset_to_address(0), 0x0000);
        assert_eq!(res.address_map.offset_to_address(3), 0x0003);
    }

    #[test]
    fn test_parse_intel_hex_extended_linear_with_gaps() {
        let ihex = ":020000040800F2\n:04000000DEADBEEFC4\n:020000042000DA\n:02001000CAFE26\n:00000001FF\n";
        let res = parse_intel_hex(ihex).expect("valid intel hex with extended address");
        assert_eq!(res.format, HexFormat::IntelHex);
        assert_eq!(res.data, vec![0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE]);
        assert_eq!(res.address_map.segments.len(), 2);
        assert_eq!(res.address_map.segments[0].address, 0x0800_0000);
        assert_eq!(res.address_map.segments[0].length, 4);
        assert_eq!(res.address_map.segments[1].address, 0x2000_0010);
        assert_eq!(res.address_map.segments[1].length, 2);
        assert!(res.address_map.has_gaps());
        assert_eq!(res.address_map.offset_to_address(4), 0x2000_0010);
        assert_eq!(res.address_map.offset_to_address(5), 0x2000_0011);
    }

    #[test]
    fn test_export_and_import_intel_hex_roundtrip() {
        let original_data = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02];
        let map = AddressMap::from_segments(vec![
            MemorySegment {
                buffer_offset: 0,
                address: 0x0800_0000,
                length: 4,
            },
            MemorySegment {
                buffer_offset: 4,
                address: 0x0800_0010,
                length: 2,
            },
        ]);

        let exported = export_intel_hex(&original_data, &map);
        let imported = parse_intel_hex(&exported).expect("re-parse exported intel hex");

        assert_eq!(imported.data, original_data);
        assert_eq!(imported.address_map.segments.len(), 2);
        assert_eq!(imported.address_map.segments[0].address, 0x0800_0000);
        assert_eq!(imported.address_map.segments[1].address, 0x0800_0010);
    }
}
