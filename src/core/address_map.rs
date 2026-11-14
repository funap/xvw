use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Strongly typed offset within a contiguous linear buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct BufferOffset(pub usize);

impl BufferOffset {
    #[inline]
    pub const fn new(offset: usize) -> Self {
        Self(offset)
    }

    #[inline]
    pub const fn as_usize(self) -> usize {
        self.0
    }
}

impl From<usize> for BufferOffset {
    #[inline]
    fn from(offset: usize) -> Self {
        Self(offset)
    }
}

impl From<BufferOffset> for usize {
    #[inline]
    fn from(offset: BufferOffset) -> Self {
        offset.0
    }
}

/// Strongly typed physical/memory address mapped from an imported structure/object file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PhysicalAddress(pub usize);

impl PhysicalAddress {
    #[inline]
    pub const fn new(addr: usize) -> Self {
        Self(addr)
    }

    #[inline]
    pub const fn as_usize(self) -> usize {
        self.0
    }
}

impl From<usize> for PhysicalAddress {
    #[inline]
    fn from(addr: usize) -> Self {
        Self(addr)
    }
}

impl From<PhysicalAddress> for usize {
    #[inline]
    fn from(addr: PhysicalAddress) -> Self {
        addr.0
    }
}

/// Represents a contiguous range of memory from an imported object file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemorySegment {
    /// Starting offset within the linear buffer
    pub buffer_offset: usize,
    /// Physical memory address in the original source file
    pub address: usize,
    /// Byte length of this segment
    pub length: usize,
}

impl MemorySegment {
    #[inline]
    pub fn end_address(&self) -> usize {
        self.address.saturating_add(self.length)
    }

    #[inline]
    pub fn end_buffer_offset(&self) -> usize {
        self.buffer_offset.saturating_add(self.length)
    }

    #[inline]
    #[allow(dead_code)]
    pub fn contains_buffer_offset(&self, offset: usize) -> bool {
        offset >= self.buffer_offset && offset < self.end_buffer_offset()
    }

    #[inline]
    pub fn contains_address(&self, addr: usize) -> bool {
        addr >= self.address && addr < self.end_address()
    }
}

/// Formatting options and metadata for exporting Motorola S-Record and Intel HEX files.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HexFormatOptions {
    /// Number of data bytes per record line (0 = format default: 20 / 0x14 for Motorola S-Record, 16 / 0x10 for Intel HEX)
    pub record_data_length: usize,
    /// Explicit address width in bytes (2 = S1, 3 = S2, 4 = S3; 0 = auto-detect)
    pub address_width: usize,
    /// Header string for S0 record (None = no S0 record)
    pub header: Option<String>,
    /// Optional execution entry point address for termination record
    pub entry_point: Option<usize>,
    /// Whether an S5/S6 record count was present
    pub has_count_record: bool,
    /// Preferred line ending (CRLF or LF)
    pub crlf: bool,
}

/// Maps buffer offsets to physical memory addresses across one or more segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressMap {
    pub segments: Vec<MemorySegment>,
    pub format_options: HexFormatOptions,
}

impl Default for AddressMap {
    fn default() -> Self {
        Self {
            segments: vec![MemorySegment {
                buffer_offset: 0,
                address: 0,
                length: usize::MAX,
            }],
            format_options: HexFormatOptions::default(),
        }
    }
}

impl AddressMap {
    /// Creates a single contiguous address map with a custom base address.
    #[allow(dead_code)]
    pub fn single_segment(base_address: usize, total_size: usize) -> Self {
        Self {
            segments: vec![MemorySegment {
                buffer_offset: 0,
                address: base_address,
                length: total_size,
            }],
            format_options: HexFormatOptions::default(),
        }
    }

    /// Creates an AddressMap from a list of memory segments.
    #[allow(dead_code)]
    pub fn from_segments(segments: Vec<MemorySegment>) -> Self {
        Self::from_segments_with_options(segments, HexFormatOptions::default())
    }

    /// Creates an AddressMap from memory segments and format options.
    pub fn from_segments_with_options(segments: Vec<MemorySegment>, format_options: HexFormatOptions) -> Self {
        if segments.is_empty() {
            Self {
                format_options,
                ..Self::default()
            }
        } else {
            Self { segments, format_options }
        }
    }

    /// Returns the lowest base address of the first segment.
    pub fn base_address(&self) -> usize {
        self.segments.first().map(|s| s.address).unwrap_or(0)
    }

    /// Returns true if there are multiple segments with address gaps between them.
    pub fn has_gaps(&self) -> bool {
        if self.segments.len() <= 1 {
            return false;
        }
        for window in self.segments.windows(2) {
            if window[0].end_address() < window[1].address {
                return true;
            }
        }
        false
    }

    /// Finds the memory segment containing the given linear buffer offset.
    pub fn segment_at_offset(&self, offset: usize) -> Option<&MemorySegment> {
        self.segments
            .binary_search_by(|seg| {
                if offset < seg.buffer_offset {
                    std::cmp::Ordering::Greater
                } else if offset >= seg.end_buffer_offset() {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .ok()
            .map(|idx| &self.segments[idx])
    }

    /// Converts a linear buffer offset to its physical memory address.
    pub fn offset_to_address(&self, offset: usize) -> usize {
        if self.segments.is_empty() {
            return offset;
        }

        // Binary search for the segment containing `offset`
        match self.segments.binary_search_by(|seg| {
            if offset < seg.buffer_offset {
                std::cmp::Ordering::Greater
            } else if offset >= seg.end_buffer_offset() {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        }) {
            Ok(idx) => {
                let seg = &self.segments[idx];
                seg.address.saturating_add(offset - seg.buffer_offset)
            }
            Err(idx) => {
                if idx == 0 {
                    self.segments[0].address
                } else if idx >= self.segments.len() {
                    let last = &self.segments[self.segments.len() - 1];
                    last.address.saturating_add(offset.saturating_sub(last.buffer_offset))
                } else {
                    let seg = &self.segments[idx];
                    seg.address
                }
            }
        }
    }

    /// Converts a physical memory address to a linear buffer offset.
    pub fn address_to_offset(&self, address: usize) -> Option<usize> {
        if self.segments.is_empty() {
            return Some(address);
        }

        for seg in &self.segments {
            if seg.contains_address(address) {
                return seg.buffer_offset.checked_add(address - seg.address);
            }
        }

        // If not directly inside a segment, find the closest segment containing or after this address
        for seg in &self.segments {
            if address < seg.address {
                return Some(seg.buffer_offset);
            }
        }

        self.segments.last().map(|s| s.end_buffer_offset().saturating_sub(1))
    }

    /// Checks if a gap immediately precedes the given buffer offset, and if so returns `Some((gap_start_addr, gap_end_addr))`.
    pub fn gap_before_offset(&self, offset: usize) -> Option<(usize, usize)> {
        if offset == 0 || self.segments.len() <= 1 {
            return None;
        }
        for window in self.segments.windows(2) {
            if window[1].buffer_offset == offset && window[0].end_address() < window[1].address {
                return Some((window[0].end_address(), window[1].address));
            }
        }
        None
    }

    /// Collects layout break events corresponding to segment boundaries.
    pub fn collect_segment_breaks(&self, breaks: &mut BTreeSet<usize>) {
        for seg in &self.segments {
            if seg.buffer_offset > 0 {
                breaks.insert(seg.buffer_offset);
            }
        }
    }

    /// Collects empty line counts for address gaps between segments so layout reserves a row for the gap bar.
    pub fn collect_gap_lines(&self, empty_lines: &mut BTreeMap<usize, usize>) {
        for window in self.segments.windows(2) {
            if window[0].end_address() < window[1].address && window[1].buffer_offset > 0 {
                *empty_lines.entry(window[1].buffer_offset).or_default() += 1;
            }
        }
    }

    /// Returns the buffer offset ranges for all memory segments.
    pub fn segment_ranges(&self) -> Vec<std::ops::Range<usize>> {
        self.segments.iter().map(|seg| seg.buffer_offset..seg.end_buffer_offset()).collect()
    }

    /// Resolves a target address or offset to a linear buffer offset for editor navigation/jumping.
    ///
    /// If the map has a custom base address or address gaps, this maps the physical address to
    /// the corresponding linear buffer offset. Otherwise, it treats the value directly as a buffer offset.
    pub fn resolve_jump_offset(&self, target: usize) -> usize {
        if self.base_address() > 0 || self.has_gaps() {
            self.address_to_offset(target).unwrap_or(target)
        } else {
            target
        }
    }

    /// Resolves an address range or offset range to a linear buffer range `[start..end)`.
    ///
    /// If the map has a custom base address or address gaps, `range.start` and the inclusive end
    /// `range.end - 1` are translated from physical addresses to buffer offsets.
    pub fn resolve_jump_range(&self, range: std::ops::Range<usize>, total_size: usize) -> std::ops::Range<usize> {
        if self.base_address() > 0 || self.has_gaps() {
            let start_offset = self.address_to_offset(range.start).unwrap_or(range.start);
            let end_offset = if range.end > range.start {
                let last_addr = range.end.saturating_sub(1);
                self.address_to_offset(last_addr).unwrap_or(last_addr).saturating_add(1)
            } else {
                start_offset
            };
            let min_off = start_offset.min(end_offset).min(total_size);
            let max_off = start_offset.max(end_offset).min(total_size);
            min_off..max_off
        } else {
            let start = range.start.min(total_size);
            let end = range.end.min(total_size).max(start);
            start..end
        }
    }

    /// Returns the recommended default file extension ("mot", "hex", or "bin") based on the current path and address map structure.
    pub fn default_extension(&self, current_path: &Path) -> &'static str {
        let ext = current_path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
        let is_mot = matches!(ext.as_str(), "mot" | "srec" | "s19" | "s28" | "s37");
        let is_hex = matches!(ext.as_str(), "hex" | "ihex" | "ihx");
        let is_b64 = matches!(ext.as_str(), "b64" | "base64");

        if is_mot {
            "mot"
        } else if is_hex {
            "hex"
        } else if is_b64 {
            "b64"
        } else if self.has_gaps()
            || self.format_options.address_width > 0
            || self.format_options.header.is_some()
            || self.format_options.has_count_record
            || self.format_options.record_data_length == 20
        {
            "mot"
        } else if self.format_options.record_data_length > 0 && self.base_address() > 0 {
            "hex"
        } else {
            "bin"
        }
    }

    /// Adjusts segment buffer offsets, lengths, and addresses after an edit at `[start..start + old_len)` replaced with `new_len` bytes.
    pub fn adjust_after_edit(&mut self, start: usize, old_len: usize, new_len: usize) {
        if self.segments.is_empty() {
            return;
        }

        // Fast path for single segment starting at 0
        if self.segments.len() == 1 && self.segments[0].address == 0 && self.segments[0].buffer_offset == 0 {
            if self.segments[0].length != usize::MAX {
                if new_len >= old_len {
                    self.segments[0].length = self.segments[0].length.saturating_add(new_len - old_len);
                } else {
                    self.segments[0].length = self.segments[0].length.saturating_sub(old_len - new_len);
                }
            }
            return;
        }

        let old_end = start.saturating_add(old_len);
        let mut new_segments: Vec<MemorySegment> = Vec::with_capacity(self.segments.len());
        let mut current_buffer_offset = 0usize;
        let mut inserted_remaining = new_len;

        let num_segs = self.segments.len();
        for (i, seg) in self.segments.iter().enumerate() {
            let is_last_seg = i + 1 == num_segs;
            let seg_start = seg.buffer_offset;
            let seg_end = seg.end_buffer_offset();

            if old_len == 0 {
                // Pure insertion at `start`
                let insert_into_this = if is_last_seg {
                    start >= seg_start
                } else if start == 0 && seg_start == 0 {
                    true
                } else {
                    start >= seg_start && start < seg_end
                };

                let to_insert = if insert_into_this && inserted_remaining > 0 {
                    let ins = inserted_remaining;
                    inserted_remaining = 0;
                    ins
                } else {
                    0
                };

                let new_seg_len = seg.length.saturating_add(to_insert);
                new_segments.push(MemorySegment {
                    buffer_offset: current_buffer_offset,
                    address: seg.address,
                    length: new_seg_len,
                });
                current_buffer_offset = current_buffer_offset.saturating_add(new_seg_len);
            } else {
                // Replacement or deletion
                if old_end <= seg_start {
                    // Edit was completely before this segment
                    new_segments.push(MemorySegment {
                        buffer_offset: current_buffer_offset,
                        address: seg.address,
                        length: seg.length,
                    });
                    current_buffer_offset = current_buffer_offset.saturating_add(seg.length);
                } else if start >= seg_end && !is_last_seg {
                    // Edit is completely after this segment
                    new_segments.push(MemorySegment {
                        buffer_offset: current_buffer_offset,
                        address: seg.address,
                        length: seg.length,
                    });
                    current_buffer_offset = current_buffer_offset.saturating_add(seg.length);
                } else {
                    // Overlaps with this segment
                    let kept_before = start.saturating_sub(seg_start).min(seg.length);
                    let deleted_prefix = if start <= seg_start {
                        old_end.saturating_sub(seg_start).min(seg.length)
                    } else {
                        0
                    };
                    let kept_after = seg_end.saturating_sub(old_end.max(seg_start));

                    let insert_into_this = if is_last_seg {
                        start >= seg_start || (start <= seg_start && kept_after > 0)
                    } else {
                        start >= seg_start && start <= seg_end
                    };

                    let to_insert = if insert_into_this && inserted_remaining > 0 {
                        let ins = inserted_remaining;
                        inserted_remaining = 0;
                        ins
                    } else {
                        0
                    };

                    let new_seg_len = kept_before.saturating_add(to_insert).saturating_add(kept_after);
                    if new_seg_len > 0 {
                        let new_address = seg.address.saturating_add(deleted_prefix);
                        new_segments.push(MemorySegment {
                            buffer_offset: current_buffer_offset,
                            address: new_address,
                            length: new_seg_len,
                        });
                        current_buffer_offset = current_buffer_offset.saturating_add(new_seg_len);
                    }
                }
            }
        }

        if inserted_remaining > 0 {
            if let Some(last) = new_segments.last_mut() {
                last.length = last.length.saturating_add(inserted_remaining);
            } else {
                let base_addr = self.base_address();
                new_segments.push(MemorySegment {
                    buffer_offset: 0,
                    address: base_addr,
                    length: inserted_remaining,
                });
            }
        }

        if new_segments.is_empty() {
            let base_addr = self.base_address();
            new_segments.push(MemorySegment {
                buffer_offset: 0,
                address: base_addr,
                length: 0,
            });
        }

        self.segments = new_segments;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_map_single_segment() {
        let map = AddressMap::single_segment(0x1000, 100);
        assert_eq!(map.base_address(), 0x1000);
        assert_eq!(map.offset_to_address(0), 0x1000);
        assert_eq!(map.offset_to_address(50), 0x1032);
        assert_eq!(map.address_to_offset(0x1000), Some(0));
        assert_eq!(map.address_to_offset(0x1032), Some(50));
        assert!(!map.has_gaps());
    }

    #[test]
    fn test_address_map_resolve_jump_offset_and_range() {
        // Single segment with SREC base address 0x1000, length 100
        let srec_map = AddressMap::single_segment(0x1000, 100);
        assert_eq!(srec_map.resolve_jump_offset(0x1000), 0);
        assert_eq!(srec_map.resolve_jump_offset(0x1020), 32);
        assert_eq!(srec_map.resolve_jump_range(0x1000..0x1021, 100), 0..33);
        assert_eq!(srec_map.resolve_jump_range(0x1000..0x1000, 100), 0..0);

        // Gapped segments (e.g. SREC with multiple chunks)
        let gapped_map = AddressMap::from_segments(vec![
            MemorySegment {
                buffer_offset: 0,
                address: 0x1000,
                length: 16,
            },
            MemorySegment {
                buffer_offset: 16,
                address: 0x2000,
                length: 32,
            },
        ]);
        assert_eq!(gapped_map.resolve_jump_offset(0x1005), 5);
        assert_eq!(gapped_map.resolve_jump_offset(0x2000), 16);
        assert_eq!(gapped_map.resolve_jump_offset(0x2005), 21);
        assert_eq!(gapped_map.resolve_jump_range(0x2000..0x2020, 48), 16..48);

        // Plain binary file (base address 0, no gaps)
        let default_map = AddressMap::default();
        assert_eq!(default_map.resolve_jump_offset(0x20), 0x20);
        assert_eq!(default_map.resolve_jump_range(0x200..0x220, 0x1000), 0x200..0x220);
    }

    #[test]
    fn test_address_map_gaps() {
        let map = AddressMap::from_segments(vec![
            MemorySegment {
                buffer_offset: 0,
                address: 0x1000,
                length: 10,
            },
            MemorySegment {
                buffer_offset: 10,
                address: 0x2000,
                length: 20,
            },
        ]);
        assert!(map.has_gaps());
        assert_eq!(map.gap_before_offset(10), Some((0x100A, 0x2000)));
        assert_eq!(map.gap_before_offset(0), None);

        let mut breaks = BTreeSet::new();
        map.collect_segment_breaks(&mut breaks);
        assert_eq!(breaks, BTreeSet::from([10]));

        let mut empty_lines = BTreeMap::new();
        map.collect_gap_lines(&mut empty_lines);
        assert_eq!(empty_lines.get(&10), Some(&1));
    }

    #[test]
    fn test_address_map_adjust_after_edit_insert_preserves_subsequent_segment_addresses() {
        let mut map = AddressMap::from_segments(vec![
            MemorySegment {
                buffer_offset: 0,
                address: 0x00FD_0000,
                length: 10,
            },
            MemorySegment {
                buffer_offset: 10,
                address: 0x0100_0000,
                length: 10,
            },
        ]);

        // Insert 2 bytes at offset 5 (inside first segment)
        map.adjust_after_edit(5, 0, 2);

        assert_eq!(map.segments.len(), 2);
        // Segment 0 length grew by 2
        assert_eq!(map.segments[0].buffer_offset, 0);
        assert_eq!(map.segments[0].address, 0x00FD_0000);
        assert_eq!(map.segments[0].length, 12);

        // Segment 1 buffer_offset shifted by 2, address remains EXACTLY 0x0100_0000!
        assert_eq!(map.segments[1].buffer_offset, 12);
        assert_eq!(map.segments[1].address, 0x0100_0000);
        assert_eq!(map.segments[1].length, 10);

        // Verify offset_to_address mappings
        assert_eq!(map.offset_to_address(0), 0x00FD_0000);
        assert_eq!(map.offset_to_address(5), 0x00FD_0005);
        assert_eq!(map.offset_to_address(6), 0x00FD_0006);
        assert_eq!(map.offset_to_address(11), 0x00FD_000B);
        assert_eq!(map.offset_to_address(12), 0x0100_0000);
        assert_eq!(map.offset_to_address(21), 0x0100_0009);
    }

    #[test]
    fn test_address_map_adjust_after_edit_delete_preserves_subsequent_segment_addresses() {
        let mut map = AddressMap::from_segments(vec![
            MemorySegment {
                buffer_offset: 0,
                address: 0x00FD_0000,
                length: 10,
            },
            MemorySegment {
                buffer_offset: 10,
                address: 0x0100_0000,
                length: 10,
            },
        ]);

        // Delete 3 bytes at offset 4 (inside first segment)
        map.adjust_after_edit(4, 3, 0);

        assert_eq!(map.segments.len(), 2);
        assert_eq!(map.segments[0].buffer_offset, 0);
        assert_eq!(map.segments[0].address, 0x00FD_0000);
        assert_eq!(map.segments[0].length, 7);

        assert_eq!(map.segments[1].buffer_offset, 7);
        assert_eq!(map.segments[1].address, 0x0100_0000);
        assert_eq!(map.segments[1].length, 10);

        assert_eq!(map.offset_to_address(6), 0x00FD_0006);
        assert_eq!(map.offset_to_address(7), 0x0100_0000);
        assert_eq!(map.offset_to_address(16), 0x0100_0009);
    }

    #[test]
    fn test_address_map_default_extension() {
        assert_eq!(HexFormatOptions::default().record_data_length, 0);
        assert_eq!(AddressMap::default().default_extension(Path::new("firmware.dat")), "bin");
        assert_eq!(AddressMap::default().default_extension(Path::new("Untitled")), "bin");
        assert_eq!(AddressMap::single_segment(0x1000, 100).default_extension(Path::new("dump.dat")), "bin");

        assert_eq!(AddressMap::default().default_extension(Path::new("image.hex")), "hex");
        assert_eq!(AddressMap::default().default_extension(Path::new("image.mot")), "mot");
        assert_eq!(AddressMap::default().default_extension(Path::new("image.b64")), "b64");

        let srec_map = AddressMap::from_segments_with_options(
            vec![MemorySegment {
                buffer_offset: 0,
                address: 0x1000,
                length: 20,
            }],
            HexFormatOptions {
                record_data_length: 20,
                address_width: 2,
                ..Default::default()
            },
        );
        assert_eq!(srec_map.default_extension(Path::new("image.dat")), "mot");

        let hex_map = AddressMap::from_segments_with_options(
            vec![MemorySegment {
                buffer_offset: 0,
                address: 0x0800_0000,
                length: 16,
            }],
            HexFormatOptions {
                record_data_length: 16,
                address_width: 0,
                ..Default::default()
            },
        );
        assert_eq!(hex_map.default_extension(Path::new("image.dat")), "hex");
    }

    #[test]
    fn test_address_map_overflow_safety() {
        let mut high_addr = AddressMap::single_segment(usize::MAX - 4, 4);
        assert_eq!(high_addr.offset_to_address(10), usize::MAX);
        high_addr.adjust_after_edit(usize::MAX - 1, 0, 16);
        assert_eq!(high_addr.segments[0].length, 20);

        let high_buf_map = AddressMap::from_segments(vec![MemorySegment {
            buffer_offset: usize::MAX - 2,
            address: 0x1000,
            length: 10,
        }]);
        assert_eq!(high_buf_map.address_to_offset(0x1005), None);
    }
}
