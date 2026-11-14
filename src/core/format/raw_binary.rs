use crate::core::address_map::AddressMap;

/// Reconstructs a full linear binary image from the given buffer and its AddressMap,
/// placing each segment at its physical memory address and filling unmapped regions and gaps with `fill_byte` (default 0x00).
pub fn export_raw_binary(data: &[u8], address_map: &AddressMap, fill_byte: u8) -> Vec<u8> {
    if address_map.segments.is_empty()
        || matches!(
            address_map.segments.as_slice(),
            [seg] if seg.address == 0 && seg.buffer_offset == 0 && (seg.length == usize::MAX || seg.length == data.len())
        )
    {
        return data.to_vec();
    }

    let max_address = address_map
        .segments
        .iter()
        .map(|s| {
            let mapped_len = if s.length == usize::MAX {
                data.len().saturating_sub(s.buffer_offset)
            } else {
                s.length
            };
            s.address.saturating_add(mapped_len)
        })
        .max()
        .unwrap_or(data.len());

    let mut out = vec![fill_byte; max_address];

    for seg in &address_map.segments {
        let seg_start = seg.buffer_offset.min(data.len());
        let seg_end = seg.buffer_offset.saturating_add(seg.length).min(data.len());
        if seg_start >= seg_end {
            continue;
        }

        let seg_data = &data[seg_start..seg_end];
        let target_start = seg.address.min(out.len());
        let target_end = seg.address.saturating_add(seg_data.len()).min(out.len());
        let copy_len = target_end.saturating_sub(target_start);
        if copy_len > 0 {
            out[target_start..target_start + copy_len].copy_from_slice(&seg_data[..copy_len]);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::address_map::MemorySegment;

    #[test]
    fn test_export_raw_binary_with_offset_and_gaps() {
        let data = vec![0xAA, 0xBB, 0xCC, 0xDD];
        let map = AddressMap::from_segments(vec![
            MemorySegment {
                buffer_offset: 0,
                address: 4,
                length: 2,
            },
            MemorySegment {
                buffer_offset: 2,
                address: 8,
                length: 2,
            },
        ]);

        let raw = export_raw_binary(&data, &map, 0x00);
        assert_eq!(raw.len(), 10);
        assert_eq!(raw, vec![0, 0, 0, 0, 0xAA, 0xBB, 0, 0, 0xCC, 0xDD]);
    }

    #[test]
    fn test_export_raw_binary_single_zero_based_segment_respects_offset_and_length() {
        let data = vec![0x10, 0x20, 0x30, 0x40];

        // Default map (usize::MAX sentinel) and explicitly empty segment list return full buffer directly.
        assert_eq!(export_raw_binary(&data, &AddressMap::default(), 0xFF), data);
        let literally_empty = AddressMap {
            segments: Vec::new(),
            format_options: Default::default(),
        };
        assert_eq!(export_raw_binary(&data, &literally_empty, 0xFF), data);

        // Unbounded sentinel segment starting at non-zero buffer_offset/address still stays bounded by `data.len()`.
        let unbounded_shifted = AddressMap::from_segments(vec![MemorySegment {
            buffer_offset: 1,
            address: 2,
            length: usize::MAX,
        }]);
        assert_eq!(export_raw_binary(&data, &unbounded_shifted, 0xCC), vec![0xCC, 0xCC, 0x20, 0x30, 0x40]);

        // Single zero-based segment starting past buffer_offset 0 must slice rather than return full data.
        let offset_slice = AddressMap::from_segments(vec![MemorySegment {
            buffer_offset: 1,
            address: 0,
            length: 2,
        }]);
        assert_eq!(export_raw_binary(&data, &offset_slice, 0xFF), vec![0x20, 0x30]);

        // Single zero-based segment shorter than buffer must truncate to segment length.
        let truncated = AddressMap::from_segments(vec![MemorySegment {
            buffer_offset: 0,
            address: 0,
            length: 2,
        }]);
        assert_eq!(export_raw_binary(&data, &truncated, 0xFF), vec![0x10, 0x20]);

        // Zero-length segment yields an empty image.
        let empty_seg = AddressMap::from_segments(vec![MemorySegment {
            buffer_offset: 0,
            address: 0,
            length: 0,
        }]);
        assert!(export_raw_binary(&data, &empty_seg, 0xFF).is_empty());

        // Single segment extending beyond buffer pads the tail with fill_byte.
        let padded = AddressMap::from_segments(vec![MemorySegment {
            buffer_offset: 0,
            address: 0,
            length: 6,
        }]);
        assert_eq!(export_raw_binary(&data, &padded, 0xEE), vec![0x10, 0x20, 0x30, 0x40, 0xEE, 0xEE]);
    }

    #[test]
    fn test_export_raw_binary_high_base_address_and_saturating_segment_length() {
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let base_addr = 0x0010_0000;
        let high_base_map = AddressMap::from_segments(vec![MemorySegment {
            buffer_offset: 1,
            address: base_addr,
            length: 3,
        }]);
        let raw = export_raw_binary(&data, &high_base_map, 0xAA);
        assert_eq!(raw.len(), base_addr + 3);
        assert!(raw[..base_addr].iter().all(|&b| b == 0xAA));
        assert_eq!(&raw[base_addr..], &[0xAD, 0xBE, 0xEF]);

        // Unchecked `buffer_offset + length` would overflow `usize` here.
        let overflow_offset_map = AddressMap::from_segments(vec![MemorySegment {
            buffer_offset: usize::MAX - 2,
            address: 0,
            length: 4,
        }]);
        assert_eq!(export_raw_binary(&data, &overflow_offset_map, 0x55), vec![0x55; 4]);

        // Segment at address 0 whose buffer length exceeds data pads missing bytes with fill_byte.
        let truncated_buf_map = AddressMap::from_segments(vec![MemorySegment {
            buffer_offset: 2,
            address: 0,
            length: 4,
        }]);
        assert_eq!(export_raw_binary(&data, &truncated_buf_map, 0x55), vec![0xBE, 0xEF, 0x55, 0x55]);
    }
}
