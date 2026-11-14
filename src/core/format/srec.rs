use super::{HexFormat, HexImportError, HexImportResult, assemble_chunks_into_result, parse_hex_byte};
use crate::core::address_map::{AddressMap, HexFormatOptions, MemorySegment};
use std::collections::BTreeMap;
use std::path::Path;

/// Parse a Motorola S-Record file from string content.
pub fn parse_motorola_srec(content: &str) -> Result<HexImportResult, HexImportError> {
    let mut raw_chunks: Vec<(usize, Vec<u8>)> = Vec::new();
    let mut entry_point = None;
    let mut header = None;
    let mut max_addr_width = 2; // 2 = S19, 3 = S28, 4 = S37
    let mut chunk_length_counts: BTreeMap<usize, usize> = BTreeMap::new();
    let mut has_count_record = false;
    let crlf = content.contains("\r\n");

    let mut line_count = 0;
    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        line_count += 1;

        if !trimmed.starts_with('S') && !trimmed.starts_with('s') {
            return Err(HexImportError::InvalidRecordStart {
                line: line_idx + 1,
                char: trimmed.chars().next().unwrap_or(' '),
            });
        }

        if !trimmed.is_ascii() {
            return Err(HexImportError::InvalidHexDigits {
                line: line_idx + 1,
                content: trimmed.to_string(),
            });
        }

        if trimmed.len() < 4 {
            return Err(HexImportError::LineTooShort { line: line_idx + 1 });
        }

        let type_char = trimmed.chars().nth(1).unwrap();
        let count_str = &trimmed[2..4];
        let byte_count = parse_hex_byte(count_str, line_idx)? as usize;

        // Total expected characters after 'S<type>': byte_count * 2
        let data_str = &trimmed[4..];
        if data_str.len() < byte_count * 2 {
            return Err(HexImportError::LineTooShort { line: line_idx + 1 });
        }

        // Checksum verification: sum of byte_count + all following bytes + checksum == 0xFF
        let mut sum: u32 = byte_count as u32;
        let mut record_bytes = Vec::with_capacity(byte_count);
        for chunk in data_str.as_bytes()[..byte_count * 2].as_chunks::<2>().0 {
            let hex_slice = std::str::from_utf8(chunk).map_err(|_| HexImportError::InvalidHexDigits {
                line: line_idx + 1,
                content: String::from_utf8_lossy(chunk).to_string(),
            })?;
            let b = parse_hex_byte(hex_slice, line_idx)?;
            sum += b as u32;
            record_bytes.push(b);
        }

        let expected_checksum = !((sum - record_bytes.last().copied().unwrap_or(0) as u32) as u8);
        let actual_checksum = record_bytes.pop().unwrap_or(0);
        if expected_checksum != actual_checksum {
            return Err(HexImportError::ChecksumMismatch {
                line: line_idx + 1,
                expected: expected_checksum,
                actual: actual_checksum,
            });
        }

        match type_char {
            '0' => {
                // S0: Header
                if record_bytes.len() >= 2 {
                    // Skip 2-byte address 0x0000
                    let header_data = &record_bytes[2..];
                    if let Ok(text) = std::str::from_utf8(header_data) {
                        header = Some(text.trim_matches('\0').to_string());
                    }
                }
            }
            '1' => {
                // S1: 16-bit address (2 bytes) + data
                if record_bytes.len() < 2 {
                    return Err(HexImportError::LineTooShort { line: line_idx + 1 });
                }
                max_addr_width = max_addr_width.max(2);
                let addr = ((record_bytes[0] as usize) << 8) | (record_bytes[1] as usize);
                let data = record_bytes[2..].to_vec();
                if !data.is_empty() {
                    *chunk_length_counts.entry(data.len()).or_insert(0) += 1;
                    raw_chunks.push((addr, data));
                }
            }
            '2' => {
                // S2: 24-bit address (3 bytes) + data
                if record_bytes.len() < 3 {
                    return Err(HexImportError::LineTooShort { line: line_idx + 1 });
                }
                max_addr_width = max_addr_width.max(3);
                let addr = ((record_bytes[0] as usize) << 16) | ((record_bytes[1] as usize) << 8) | (record_bytes[2] as usize);
                let data = record_bytes[3..].to_vec();
                if !data.is_empty() {
                    *chunk_length_counts.entry(data.len()).or_insert(0) += 1;
                    raw_chunks.push((addr, data));
                }
            }
            '3' => {
                // S3: 32-bit address (4 bytes) + data
                if record_bytes.len() < 4 {
                    return Err(HexImportError::LineTooShort { line: line_idx + 1 });
                }
                max_addr_width = max_addr_width.max(4);
                let addr =
                    ((record_bytes[0] as usize) << 24) | ((record_bytes[1] as usize) << 16) | ((record_bytes[2] as usize) << 8) | (record_bytes[3] as usize);
                let data = record_bytes[4..].to_vec();
                if !data.is_empty() {
                    *chunk_length_counts.entry(data.len()).or_insert(0) += 1;
                    raw_chunks.push((addr, data));
                }
            }
            '5' | '6' => {
                // S5 / S6: Record count
                has_count_record = true;
            }
            '7' => {
                // S7: 32-bit termination / entry address
                if record_bytes.len() >= 4 {
                    let addr = ((record_bytes[0] as usize) << 24)
                        | ((record_bytes[1] as usize) << 16)
                        | ((record_bytes[2] as usize) << 8)
                        | (record_bytes[3] as usize);
                    entry_point = Some(addr);
                }
            }
            '8' => {
                // S8: 24-bit termination / entry address
                if record_bytes.len() >= 3 {
                    let addr = ((record_bytes[0] as usize) << 16) | ((record_bytes[1] as usize) << 8) | (record_bytes[2] as usize);
                    entry_point = Some(addr);
                }
            }
            '9' => {
                // S9: 16-bit termination / entry address
                if record_bytes.len() >= 2 {
                    let addr = ((record_bytes[0] as usize) << 8) | (record_bytes[1] as usize);
                    entry_point = Some(addr);
                }
            }
            _ => {
                return Err(HexImportError::UnsupportedRecordType {
                    line: line_idx + 1,
                    record_type: format!("S{}", type_char),
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

    let format = match max_addr_width {
        4 => HexFormat::MotorolaS37,
        3 => HexFormat::MotorolaS28,
        _ => HexFormat::MotorolaS19,
    };

    let record_data_length = chunk_length_counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(len, _)| len)
        .unwrap_or(20);

    let format_options = HexFormatOptions {
        record_data_length,
        address_width: max_addr_width,
        header: header.clone(),
        entry_point,
        has_count_record,
        crlf,
    };

    assemble_chunks_into_result(raw_chunks, format, entry_point, header, format_options)
}

/// Exports binary data and its AddressMap to Motorola S-Record (`.mot` / `.srec`) string format.
pub fn export_motorola_srec(data: &[u8], address_map: &AddressMap) -> String {
    let mut out = String::new();
    let line_ending = if address_map.format_options.crlf { "\r\n" } else { "\n" };

    // S0 Header record: only if header was present in source or specified.
    // byte_count = 2 address bytes + data bytes + 1 checksum byte <= 255 -> max 252 data bytes.
    if let Some(hdr) = &address_map.format_options.header {
        const MAX_S0_HEADER_BYTES: usize = (u8::MAX as usize) - 3;
        let hdr_bytes = if hdr.len() > MAX_S0_HEADER_BYTES {
            let mut end = MAX_S0_HEADER_BYTES;
            while end > 0 && !hdr.is_char_boundary(end) {
                end -= 1;
            }
            &hdr.as_bytes()[..end]
        } else {
            hdr.as_bytes()
        };
        let byte_count = u8::try_from(2 + hdr_bytes.len() + 1).expect("S0 byte count fits in u8");
        let mut sum: u32 = byte_count as u32;
        out.push_str(&format!("S0{:02X}0000", byte_count));
        for &b in hdr_bytes {
            sum += b as u32;
            out.push_str(&format!("{:02X}", b));
        }
        let checksum = !(sum as u8);
        out.push_str(&format!("{:02X}{}", checksum, line_ending));
    }

    let segments = if address_map.segments.is_empty() {
        vec![MemorySegment {
            buffer_offset: 0,
            address: 0,
            length: data.len(),
        }]
    } else {
        address_map.segments.clone()
    };

    let max_address = segments
        .iter()
        .filter_map(|s| {
            let seg_start = s.buffer_offset.min(data.len());
            let seg_end = s.buffer_offset.saturating_add(s.length).min(data.len());
            let effective_len = seg_end.saturating_sub(seg_start);
            (effective_len > 0).then(|| s.address.saturating_add(effective_len - 1))
        })
        .max()
        .unwrap_or(0);

    let address_bytes = if address_map.format_options.address_width >= 2 {
        address_map.format_options.address_width.min(4)
    } else if max_address <= 0xFFFF {
        2
    } else if max_address <= 0xFF_FFFF {
        3
    } else {
        4
    };

    let max_address_for_width = match address_bytes {
        2 => 0xFFFFusize,
        3 => 0xFF_FFFFusize,
        _ => u32::MAX as usize,
    };

    let max_data_per_record = (u8::MAX as usize) - address_bytes - 1;
    let requested_chunk_size = if address_map.format_options.record_data_length > 0 {
        address_map.format_options.record_data_length
    } else {
        20 // standard 0x14 for Motorola S-Record
    };
    let chunk_size = requested_chunk_size.min(max_data_per_record);

    let mut record_count: usize = 0;

    for seg in &segments {
        let seg_start = seg.buffer_offset.min(data.len());
        let seg_end = seg.buffer_offset.saturating_add(seg.length).min(data.len());
        if seg_start >= seg_end {
            continue;
        }

        let seg_data = &data[seg_start..seg_end];
        let mut cur_addr = seg.address;
        let mut pos = 0;

        while pos < seg_data.len() {
            if cur_addr > max_address_for_width {
                break;
            }
            let max_len_in_addr_space = (max_address_for_width - cur_addr).min(chunk_size - 1) + 1;
            let record_len = (seg_data.len() - pos).min(chunk_size).min(max_len_in_addr_space);
            let chunk = &seg_data[pos..pos + record_len];

            record_count = record_count.saturating_add(1);
            let byte_count = u8::try_from(address_bytes + chunk.len() + 1).expect("SREC record byte count fits in u8");
            let mut sum: u32 = byte_count as u32;

            out.push('S');
            match address_bytes {
                2 => out.push('1'),
                3 => out.push('2'),
                _ => out.push('3'),
            }

            out.push_str(&format!("{:02X}", byte_count));

            match address_bytes {
                2 => {
                    let a1 = ((cur_addr >> 8) & 0xFF) as u8;
                    let a0 = (cur_addr & 0xFF) as u8;
                    sum += a1 as u32 + a0 as u32;
                    out.push_str(&format!("{:02X}{:02X}", a1, a0));
                }
                3 => {
                    let a2 = ((cur_addr >> 16) & 0xFF) as u8;
                    let a1 = ((cur_addr >> 8) & 0xFF) as u8;
                    let a0 = (cur_addr & 0xFF) as u8;
                    sum += a2 as u32 + a1 as u32 + a0 as u32;
                    out.push_str(&format!("{:02X}{:02X}{:02X}", a2, a1, a0));
                }
                _ => {
                    let a3 = ((cur_addr >> 24) & 0xFF) as u8;
                    let a2 = ((cur_addr >> 16) & 0xFF) as u8;
                    let a1 = ((cur_addr >> 8) & 0xFF) as u8;
                    let a0 = (cur_addr & 0xFF) as u8;
                    sum += a3 as u32 + a2 as u32 + a1 as u32 + a0 as u32;
                    out.push_str(&format!("{:02X}{:02X}{:02X}{:02X}", a3, a2, a1, a0));
                }
            }

            for &b in chunk {
                sum += b as u32;
                out.push_str(&format!("{:02X}", b));
            }

            let checksum = !(sum as u8);
            out.push_str(&format!("{:02X}{}", checksum, line_ending));

            pos += record_len;
            let Some(next_addr) = cur_addr.checked_add(chunk.len()) else {
                break;
            };
            cur_addr = next_addr;
        }
    }

    if address_map.format_options.has_count_record {
        if record_count <= 0xFFFF {
            let count_u16 = record_count as u16;
            let c1 = ((count_u16 >> 8) & 0xFF) as u8;
            let c0 = (count_u16 & 0xFF) as u8;
            let sum: u32 = 3 + c1 as u32 + c0 as u32;
            let checksum = !(sum as u8);
            out.push_str(&format!("S503{:04X}{:02X}{}", count_u16, checksum, line_ending));
        } else if record_count <= 0xFF_FFFF {
            let count_u32 = record_count as u32;
            let c2 = ((count_u32 >> 16) & 0xFF) as u8;
            let c1 = ((count_u32 >> 8) & 0xFF) as u8;
            let c0 = (count_u32 & 0xFF) as u8;
            let sum: u32 = 4 + c2 as u32 + c1 as u32 + c0 as u32;
            let checksum = !(sum as u8);
            out.push_str(&format!("S604{:06X}{:02X}{}", count_u32, checksum, line_ending));
        }
    }

    let start_addr = address_map
        .format_options
        .entry_point
        .unwrap_or_else(|| segments.first().map(|s| s.address).unwrap_or(0));

    match address_bytes {
        2 => {
            let addr = (start_addr & 0xFFFF) as u16;
            let a1 = ((addr >> 8) & 0xFF) as u8;
            let a0 = (addr & 0xFF) as u8;
            let sum: u32 = 3 + a1 as u32 + a0 as u32;
            let checksum = !(sum as u8);
            out.push_str(&format!("S903{:04X}{:02X}{}", addr, checksum, line_ending));
        }
        3 => {
            let addr = (start_addr & 0xFF_FFFF) as u32;
            let a2 = ((addr >> 16) & 0xFF) as u8;
            let a1 = ((addr >> 8) & 0xFF) as u8;
            let a0 = (addr & 0xFF) as u8;
            let sum: u32 = 4 + a2 as u32 + a1 as u32 + a0 as u32;
            let checksum = !(sum as u8);
            out.push_str(&format!("S804{:06X}{:02X}{}", addr, checksum, line_ending));
        }
        _ => {
            let addr = (start_addr & 0xFFFF_FFFF) as u32;
            let a3 = ((addr >> 24) & 0xFF) as u8;
            let a2 = ((addr >> 16) & 0xFF) as u8;
            let a1 = ((addr >> 8) & 0xFF) as u8;
            let a0 = (addr & 0xFF) as u8;
            let sum: u32 = 5 + a3 as u32 + a2 as u32 + a1 as u32 + a0 as u32;
            let checksum = !(sum as u8);
            out.push_str(&format!("S705{:08X}{:02X}{}", addr, checksum, line_ending));
        }
    }

    out
}

/// Checks if the file path has a Motorola S-Record extension.
#[allow(dead_code)]
pub fn is_mot_extension(path: &Path) -> bool {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    matches!(ext.as_str(), "mot" | "srec" | "s19" | "s28" | "s37")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_motorola_srec_s19() {
        let srec = "S0030000FC\nS107100001020304DE\nS9030000FC\n";
        let res = parse_motorola_srec(srec).expect("valid srec");
        assert_eq!(res.format, HexFormat::MotorolaS19);
        assert_eq!(res.data, vec![0x01, 0x02, 0x03, 0x04]);
        assert_eq!(res.address_map.base_address(), 0x1000);
        assert_eq!(res.address_map.offset_to_address(0), 0x1000);
        assert_eq!(res.address_map.offset_to_address(3), 0x1003);
    }

    #[test]
    fn test_parse_motorola_srec_with_gaps() {
        let srec = "S0030000FC\nS3090001000001020304EB\nS3090002000005060708DA\nS70500000000FA\n";
        let res = parse_motorola_srec(srec).expect("valid srec with gaps");
        assert_eq!(res.format, HexFormat::MotorolaS37);
        assert_eq!(res.data, vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        assert_eq!(res.address_map.segments.len(), 2);
        assert_eq!(res.address_map.segments[0].address, 0x0001_0000);
        assert_eq!(res.address_map.segments[0].length, 4);
        assert_eq!(res.address_map.segments[1].address, 0x0002_0000);
        assert_eq!(res.address_map.segments[1].length, 4);
        assert!(res.address_map.has_gaps());

        assert_eq!(res.address_map.offset_to_address(0), 0x0001_0000);
        assert_eq!(res.address_map.offset_to_address(3), 0x0001_0003);
        assert_eq!(res.address_map.offset_to_address(4), 0x0002_0000);
        assert_eq!(res.address_map.offset_to_address(7), 0x0002_0003);

        assert_eq!(res.address_map.address_to_offset(0x0001_0002), Some(2));
        assert_eq!(res.address_map.address_to_offset(0x0002_0001), Some(5));
        assert_eq!(res.address_map.segment_ranges(), vec![0..4, 4..8]);
        assert_eq!(res.address_map.segment_at_offset(2).map(|s| s.address), Some(0x0001_0000));
        assert_eq!(res.address_map.segment_at_offset(5).map(|s| s.address), Some(0x0002_0000));
        assert_eq!(res.address_map.segment_at_offset(8), None);
    }

    #[test]
    fn test_export_and_import_motorola_srec_roundtrip() {
        let original_data = vec![0x11, 0x22, 0x33, 0x44, 0xAA, 0xBB, 0xCC, 0xDD];
        let map = AddressMap::from_segments(vec![
            MemorySegment {
                buffer_offset: 0,
                address: 0x0004_0000,
                length: 4,
            },
            MemorySegment {
                buffer_offset: 4,
                address: 0x0008_0000,
                length: 4,
            },
        ]);

        let exported = export_motorola_srec(&original_data, &map);
        let imported = parse_motorola_srec(&exported).expect("re-parse exported srec");

        assert_eq!(imported.data, original_data);
        assert_eq!(imported.address_map.segments.len(), 2);
        assert_eq!(imported.address_map.segments[0].address, 0x0004_0000);
        assert_eq!(imported.address_map.segments[1].address, 0x0008_0000);
    }

    #[test]
    fn test_checksum_failure() {
        let invalid_srec = "S10710000102030400\n";
        let err = parse_motorola_srec(invalid_srec).unwrap_err();
        match err {
            HexImportError::ChecksumMismatch { line, expected, actual } => {
                assert_eq!(line, 1);
                assert_eq!(actual, 0x00);
                assert_eq!(expected, 0xDE);
            }
            _ => panic!("expected checksum mismatch"),
        }
    }

    #[test]
    fn test_reexport_preserves_exact_mot_structure() {
        let srec = "S31900FD00000064213403200A000000000100050A0005DC05DC31\nS31900FD001405DC05050164000A000000003D4CCCCD3D4CCCCD37\nS70501000400F5\n";
        let res = parse_motorola_srec(srec).expect("valid srec");
        assert_eq!(res.address_map.format_options.record_data_length, 20);
        assert_eq!(res.address_map.format_options.entry_point, Some(0x0100_0400));
        assert_eq!(res.address_map.format_options.header, None);
        assert!(!res.address_map.format_options.crlf);

        let reexported = export_motorola_srec(&res.data, &res.address_map);
        assert_eq!(reexported, srec);
    }

    #[test]
    fn test_parse_motorola_srec_non_ascii_does_not_panic() {
        assert_eq!(
            parse_motorola_srec("Sあ\n").unwrap_err(),
            HexImportError::InvalidHexDigits {
                line: 1,
                content: "Sあ".to_string(),
            }
        );
        assert_eq!(
            parse_motorola_srec("S1あ\n").unwrap_err(),
            HexImportError::InvalidHexDigits {
                line: 1,
                content: "S1あ".to_string(),
            }
        );
        assert_eq!(
            parse_motorola_srec("S\u{00e9}030000FC\n").unwrap_err(),
            HexImportError::InvalidHexDigits {
                line: 1,
                content: "S\u{00e9}030000FC".to_string(),
            }
        );
        assert_eq!(
            parse_motorola_srec("S103ああ\n").unwrap_err(),
            HexImportError::InvalidHexDigits {
                line: 1,
                content: "S103ああ".to_string(),
            }
        );
        assert_eq!(
            parse_motorola_srec("あいう\n").unwrap_err(),
            HexImportError::InvalidRecordStart { line: 1, char: 'あ' }
        );
    }

    #[test]
    fn test_export_motorola_srec_clamps_s0_header_and_data_byte_count() {
        let data: Vec<u8> = (0..300usize).map(|i| (i & 0xFF) as u8).collect();
        let mut map = AddressMap::single_segment(0x0001_0000, data.len());
        map.format_options.header = Some("あ".repeat(100)); // 300 UTF-8 bytes (> 252 byte cap)
        map.format_options.record_data_length = 300; // exceeds max_data_per_record (<= 251 for S2)

        let exported = export_motorola_srec(&data, &map);
        for line in exported.lines() {
            assert!(line.starts_with('S'));
            let byte_count = u8::from_str_radix(&line[2..4], 16).unwrap() as usize;
            assert_eq!(line.len(), 4 + byte_count * 2, "unexpected record length in line {line}");
        }

        let reimported = parse_motorola_srec(&exported).expect("clamped S0/data SREC must parse cleanly");
        assert_eq!(reimported.data, data);
        assert_eq!(reimported.address_map.base_address(), 0x0001_0000);
        assert_eq!(reimported.header, Some("あ".repeat(84))); // 84 * 3 = 252 bytes on UTF-8 char boundary
    }

    #[test]
    fn test_export_motorola_srec_emits_s6_when_record_count_exceeds_u16() {
        let data = vec![0xAA; 65_537];
        let mut map = AddressMap::single_segment(0, data.len());
        map.format_options.record_data_length = 1;
        map.format_options.has_count_record = true;

        let exported = export_motorola_srec(&data, &map);
        let mut lines = exported.lines().rev();
        let term = lines.next().expect("termination record");
        let count_rec = lines.next().expect("S6 count record");
        assert_eq!(term, "S804000000FB");
        assert_eq!(count_rec, "S604010001F9");

        let reimported = parse_motorola_srec(&exported).expect("S6 SREC must parse cleanly");
        assert_eq!(reimported.data.len(), 65_537);
        assert!(reimported.address_map.format_options.has_count_record);
    }

    #[test]
    fn test_export_motorola_srec_masks_start_addr_and_handles_max_segment_length() {
        let data = vec![0x12, 0x34, 0x56, 0x78];
        let mut map = AddressMap::default(); // segments[0].length == usize::MAX
        map.format_options.address_width = 4;
        if let Ok(large_ep) = usize::try_from(0x1_0000_0042u64) {
            map.format_options.entry_point = Some(large_ep);
        }

        let exported = export_motorola_srec(&data, &map);
        if usize::BITS > 32 {
            assert!(
                exported.lines().any(|l| l == "S70500000042B8"),
                "expected valid 8-digit S7 record, got:\n{exported}"
            );
        }

        let reimported = parse_motorola_srec(&exported).expect("valid SREC output");
        assert_eq!(reimported.data, data);
        if usize::BITS > 32 {
            assert_eq!(reimported.entry_point, Some(0x0000_0042));
        }
    }
}
