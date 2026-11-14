#![allow(dead_code)]

/// KaitaiStream represents the binary stream reader for Kaitai Struct formats,
/// supporting bit-level reading, endian-specific integer/float reads, and substreams.
pub struct KaitaiStream<'a> {
    data: &'a [u8],
    root_data: &'a [u8],
    pos: usize,
    bits: u64,
    bits_left: usize,
}

impl<'a> KaitaiStream<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            root_data: data,
            pos: 0,
            bits: 0,
            bits_left: 0,
        }
    }

    pub fn new_with_root(data: &'a [u8], root_data: &'a [u8]) -> Self {
        Self {
            data,
            root_data,
            pos: 0,
            bits: 0,
            bits_left: 0,
        }
    }

    pub fn root_data(&self) -> &'a [u8] {
        self.root_data
    }

    pub fn pos(&self) -> u64 {
        self.pos as u64
    }

    pub fn set_pos(&mut self, pos: u64) {
        self.align_to_byte();
        self.pos = usize::try_from(pos).unwrap_or(usize::MAX).min(self.data.len());
    }

    pub fn is_eof(&self) -> bool {
        self.pos >= self.data.len() && self.bits_left == 0
    }

    pub fn size(&self) -> u64 {
        self.data.len() as u64
    }

    pub fn align_to_byte(&mut self) {
        self.bits_left = 0;
        self.bits = 0;
    }

    pub fn read_bits_int_be(&mut self, n: usize) -> Option<u64> {
        if n > 64 {
            return None;
        }
        if n == 0 {
            return Some(0);
        }

        let needed_bits = n.saturating_sub(self.bits_left);
        let needed_bytes = needed_bits.div_ceil(8);
        if needed_bytes > self.data.len().saturating_sub(self.pos) {
            return None;
        }

        let mut bits = self.bits as u128;
        let mut bits_left = self.bits_left;
        let mut pos = self.pos;

        while bits_left < n {
            let byte = self.data[pos] as u128;
            pos += 1;
            bits = (bits << 8) | byte;
            bits_left += 8;
        }

        let shift = bits_left - n;
        let mask = if n == 64 { u128::from(u64::MAX) } else { (1u128 << n) - 1 };
        let res = ((bits >> shift) & mask) as u64;
        bits_left -= n;
        let rem_mask = if bits_left == 0 {
            0
        } else if bits_left >= 128 {
            u128::MAX
        } else {
            (1u128 << bits_left) - 1
        };
        self.pos = pos;
        self.bits = (bits & rem_mask) as u64;
        self.bits_left = bits_left;
        Some(res)
    }

    pub fn read_bits_int_le(&mut self, n: usize) -> Option<u64> {
        if n > 64 {
            return None;
        }
        if n == 0 {
            return Some(0);
        }

        let needed_bits = n.saturating_sub(self.bits_left);
        let needed_bytes = needed_bits.div_ceil(8);
        if needed_bytes > self.data.len().saturating_sub(self.pos) {
            return None;
        }

        let mut bits = self.bits;
        let mut bits_left = self.bits_left;
        let mut pos = self.pos;
        let mut bits_read = 0;
        let mut res: u64 = 0;

        while bits_read < n {
            if bits_left == 0 {
                bits = self.data[pos] as u64;
                pos += 1;
                bits_left = 8;
            }

            let count = (n - bits_read).min(bits_left);
            let mask = if count == 64 { u64::MAX } else { (1u64 << count) - 1 };
            let chunk = bits & mask;

            res |= chunk << bits_read;
            bits = if count >= 64 { 0 } else { bits >> count };
            bits_left -= count;
            bits_read += count;
        }

        self.pos = pos;
        self.bits = bits;
        self.bits_left = bits_left;
        Some(res)
    }

    #[inline(always)]
    fn read_fixed<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.align_to_byte();
        let end = self.pos.checked_add(N)?;
        if end <= self.data.len() {
            let mut bytes = [0u8; N];
            bytes.copy_from_slice(&self.data[self.pos..end]);
            self.pos = end;
            Some(bytes)
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn read_u1(&mut self) -> Option<u8> {
        self.read_fixed::<1>().map(|b| b[0])
    }

    #[inline(always)]
    pub fn read_u2le(&mut self) -> Option<u16> {
        self.read_fixed::<2>().map(u16::from_le_bytes)
    }

    #[inline(always)]
    pub fn read_u2be(&mut self) -> Option<u16> {
        self.read_fixed::<2>().map(u16::from_be_bytes)
    }

    #[inline(always)]
    pub fn read_u4le(&mut self) -> Option<u32> {
        self.read_fixed::<4>().map(u32::from_le_bytes)
    }

    #[inline(always)]
    pub fn read_u4be(&mut self) -> Option<u32> {
        self.read_fixed::<4>().map(u32::from_be_bytes)
    }

    #[inline(always)]
    pub fn read_u8le(&mut self) -> Option<u64> {
        self.read_fixed::<8>().map(u64::from_le_bytes)
    }

    #[inline(always)]
    pub fn read_u8be(&mut self) -> Option<u64> {
        self.read_fixed::<8>().map(u64::from_be_bytes)
    }

    #[inline(always)]
    pub fn read_s1(&mut self) -> Option<i8> {
        self.read_u1().map(|v| v as i8)
    }

    #[inline(always)]
    pub fn read_s2le(&mut self) -> Option<i16> {
        self.read_u2le().map(|v| v as i16)
    }

    #[inline(always)]
    pub fn read_s2be(&mut self) -> Option<i16> {
        self.read_u2be().map(|v| v as i16)
    }

    #[inline(always)]
    pub fn read_s4le(&mut self) -> Option<i32> {
        self.read_u4le().map(|v| v as i32)
    }

    #[inline(always)]
    pub fn read_s4be(&mut self) -> Option<i32> {
        self.read_u4be().map(|v| v as i32)
    }

    pub fn read_s8le(&mut self) -> Option<i64> {
        self.read_u8le().map(|v| v as i64)
    }

    pub fn read_s8be(&mut self) -> Option<i64> {
        self.read_u8be().map(|v| v as i64)
    }

    pub fn read_f4le(&mut self) -> Option<f32> {
        self.read_fixed::<4>().map(f32::from_le_bytes)
    }

    pub fn read_f4be(&mut self) -> Option<f32> {
        self.read_fixed::<4>().map(f32::from_be_bytes)
    }

    pub fn read_f8le(&mut self) -> Option<f64> {
        self.read_fixed::<8>().map(f64::from_le_bytes)
    }

    pub fn read_f8be(&mut self) -> Option<f64> {
        self.read_fixed::<8>().map(f64::from_be_bytes)
    }

    #[inline(always)]
    pub fn read_bytes_slice(&mut self, size: usize) -> Option<&'a [u8]> {
        self.align_to_byte();
        let end = self.pos.checked_add(size)?;
        if end <= self.data.len() {
            let slice = &self.data[self.pos..end];
            self.pos = end;
            Some(slice)
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn read_bytes(&mut self, size: usize) -> Option<Vec<u8>> {
        self.read_bytes_slice(size).map(|s| s.to_vec())
    }

    /// Advances the parent stream and returns a bounded view over the same
    /// backing bytes. Unlike `read_bytes`, this does not allocate or copy the
    /// substream contents.
    pub fn take_substream(&mut self, size: usize) -> Option<KaitaiStream<'a>> {
        self.align_to_byte();
        let end = self.pos.checked_add(size)?;
        if end > self.data.len() {
            return None;
        }

        let start = self.pos;
        self.pos = end;
        Some(KaitaiStream::new_with_root(&self.data[start..end], self.root_data))
    }

    #[inline(always)]
    pub fn read_bytes_remaining_slice(&mut self) -> Option<&'a [u8]> {
        self.align_to_byte();
        let size = self.data.len().saturating_sub(self.pos);
        self.read_bytes_slice(size)
    }

    #[inline(always)]
    pub fn read_bytes_remaining(&mut self) -> Option<Vec<u8>> {
        self.read_bytes_remaining_slice().map(|s| s.to_vec())
    }

    /// Reads bytes until the terminator byte is found.
    ///
    /// - `include`: if true, the terminator byte is included in the result.
    /// - `consume`: if true, the stream position advances past the terminator.
    /// - `eos_error`: if true, returns None when EOF is reached without finding the terminator.
    pub fn read_bytes_term(&mut self, term: u8, include: bool, consume: bool, eos_error: bool) -> Option<Vec<u8>> {
        self.align_to_byte();
        let rem = &self.data[self.pos..];
        if let Some(idx) = rem.iter().position(|&b| b == term) {
            let result_len = if include { idx + 1 } else { idx };
            let result = rem[..result_len].to_vec();
            self.pos += if consume { idx + 1 } else { idx };
            Some(result)
        } else if eos_error {
            None
        } else {
            let result = rem.to_vec();
            self.pos = self.data.len();
            Some(result)
        }
    }

    pub fn read_bytes_term_multi(&mut self, terminator: &[u8], include: bool, consume: bool, eos_error: bool) -> Option<Vec<u8>> {
        self.align_to_byte();
        let unit_size = terminator.len();
        if unit_size == 0 {
            return Some(Vec::new());
        }

        let rem = &self.data[self.pos..];
        if let Some(idx) = rem.windows(unit_size).position(|w| w == terminator) {
            let result_len = if include { idx + unit_size } else { idx };
            let result = rem[..result_len].to_vec();
            self.pos += if consume { idx + unit_size } else { idx };
            Some(result)
        } else if eos_error {
            None
        } else {
            let result = rem.to_vec();
            self.pos = self.data.len();
            Some(result)
        }
    }

    pub fn ensure_fixed_contents(&mut self, expected: &[u8]) -> Option<Vec<u8>> {
        self.align_to_byte();
        let end = self.pos.checked_add(expected.len())?;
        if end <= self.data.len() {
            let slice = &self.data[self.pos..end];
            if slice == expected {
                self.pos = end;
                return Some(slice.to_vec());
            }
        }
        None
    }
}

/// Kaitai Struct data processing utilities for custom data transformations (XOR, rotate, zlib).
pub mod process {
    pub fn xor_one(data: &[u8], key: u8) -> Vec<u8> {
        data.iter().map(|&b| b ^ key).collect()
    }

    pub fn xor_many(data: &[u8], key: &[u8]) -> Vec<u8> {
        if key.is_empty() {
            return data.to_vec();
        }
        data.iter().enumerate().map(|(i, &b)| b ^ key[i % key.len()]).collect()
    }

    pub fn rotate_left(data: &[u8], amount: u32, group_size: usize) -> Result<Vec<u8>, String> {
        if group_size != 1 {
            return Err(format!("unable to rotate group of {} bytes yet", group_size));
        }
        let amount = amount % 8;
        let result = data.iter().map(|&b| b.rotate_left(amount)).collect();
        Ok(result)
    }

    pub fn rotate_right(data: &[u8], amount: u32, group_size: usize) -> Result<Vec<u8>, String> {
        let amount = amount % 8;
        rotate_left(data, (8 - amount) % 8, group_size)
    }

    pub fn zlib_decompress(data: &[u8]) -> Option<Vec<u8>> {
        use flate2::read::ZlibDecoder;
        use std::io::Read;
        let mut decoder = ZlibDecoder::new(data);
        let mut buf = Vec::new();
        decoder.read_to_end(&mut buf).ok().map(|_| buf)
    }
}

pub fn bytes_strip_right(data: &[u8], pad_byte: u8) -> Vec<u8> {
    let end = data.iter().rposition(|&b| b != pad_byte).map_or(0, |i| i + 1);
    data[..end].to_vec()
}

pub fn bytes_terminate(data: &[u8], term: u8, include: bool) -> Vec<u8> {
    let pos = data.iter().position(|&b| b == term);
    match pos {
        Some(i) if include => data[..=i].to_vec(),
        Some(i) => data[..i].to_vec(),
        None => data.to_vec(),
    }
}

pub fn byte_array_compare(a: &[u8], b: &[u8]) -> i32 {
    match a.cmp(b) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_readers_use_the_expected_bit_order() {
        let mut big_endian = KaitaiStream::new(&[0b1011_0010, 0b0110_0001]);
        assert_eq!(big_endian.read_bits_int_be(3), Some(0b101));
        assert_eq!(big_endian.read_bits_int_be(5), Some(0b1_0010));
        assert_eq!(big_endian.read_bits_int_be(8), Some(0b0110_0001));
        assert!(big_endian.is_eof());

        let mut little_endian = KaitaiStream::new(&[0b1011_0010, 0b0110_0001]);
        assert_eq!(little_endian.read_bits_int_le(3), Some(0b010));
        assert_eq!(little_endian.read_bits_int_le(5), Some(0b1_0110));
        assert_eq!(little_endian.read_bits_int_le(8), Some(0b0110_0001));
        assert!(little_endian.is_eof());

        assert_eq!(KaitaiStream::new(&[]).read_bits_int_be(0), Some(0));
        assert_eq!(KaitaiStream::new(&[]).read_bits_int_le(65), None);
    }

    #[test]
    fn scalar_reads_align_and_set_position_is_clamped() {
        let mut stream = KaitaiStream::new(&[0b0000_0001, 0x34, 0x12, 0x56, 0x78]);
        assert_eq!(stream.read_bits_int_be(1), Some(0));
        assert_eq!(stream.read_u2le(), Some(0x1234));
        assert_eq!(stream.read_u2be(), Some(0x5678));
        assert!(stream.is_eof());

        stream.set_pos(100);
        assert_eq!(stream.pos(), stream.size());
        assert_eq!(stream.read_u1(), None);
    }

    #[test]
    fn substream_shares_parent_bytes_without_copying() {
        let data = [1, 2, 3, 4];
        let mut parent = KaitaiStream::new(&data);
        let mut child = parent.take_substream(2).expect("substream");

        assert_eq!(parent.pos(), 2);
        assert_eq!(child.read_u2be(), Some(0x0102));
        assert_eq!(parent.read_u1(), Some(3));
    }

    #[test]
    fn terminated_reads_respect_include_consume_and_eos_error() {
        let mut stream = KaitaiStream::new(b"abc,def");
        assert_eq!(stream.read_bytes_term(b',', false, false, true), Some(b"abc".to_vec()));
        assert_eq!(stream.pos(), 3);
        assert_eq!(stream.read_u1(), Some(b','));
        assert_eq!(stream.read_bytes_remaining(), Some(b"def".to_vec()));

        let mut multi = KaitaiStream::new(b"prefix::suffix");
        assert_eq!(multi.read_bytes_term_multi(b"::", true, true, true), Some(b"prefix::".to_vec()));
        assert_eq!(multi.read_bytes_remaining(), Some(b"suffix".to_vec()));

        let mut missing = KaitaiStream::new(b"unterminated");
        assert_eq!(missing.read_bytes_term(b'!', false, true, true), None);
        assert_eq!(missing.pos(), 0);
        assert_eq!(missing.read_bytes_term(b'!', false, true, false), Some(b"unterminated".to_vec()));
        assert!(missing.is_eof());
    }

    #[test]
    fn fixed_contents_and_processing_helpers_cover_success_and_failure() {
        let mut stream = KaitaiStream::new(b"MAGICpayload");
        assert_eq!(stream.ensure_fixed_contents(b"MAGIC"), Some(b"MAGIC".to_vec()));
        assert_eq!(stream.read_bytes_remaining(), Some(b"payload".to_vec()));

        let mut mismatch = KaitaiStream::new(b"other");
        assert_eq!(mismatch.ensure_fixed_contents(b"MAGIC"), None);
        assert_eq!(mismatch.pos(), 0);

        assert_eq!(process::xor_one(&[0x00, 0xFF], 0xAA), vec![0xAA, 0x55]);
        assert_eq!(process::xor_many(&[1, 2, 3, 4], &[0x10, 0x20]), vec![0x11, 0x22, 0x13, 0x24]);
        assert_eq!(process::xor_many(&[1, 2], &[]), vec![1, 2]);
        assert_eq!(process::rotate_left(&[0b1000_0001], 1, 1), Ok(vec![0b0000_0011]));
        assert!(process::rotate_left(&[1], 1, 2).is_err());

        use flate2::{Compression, write::ZlibEncoder};
        use std::io::Write;
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(b"compressed").expect("compress test data");
        let compressed = encoder.finish().expect("finish compression");
        assert_eq!(process::zlib_decompress(&compressed), Some(b"compressed".to_vec()));
        assert_eq!(process::zlib_decompress(b"not zlib"), None);
    }

    #[test]
    fn byte_helpers_have_stable_boundary_behavior() {
        assert_eq!(bytes_strip_right(b"abc\0\0", 0), b"abc".to_vec());
        assert_eq!(bytes_strip_right(b"\0\0", 0), Vec::<u8>::new());
        assert_eq!(bytes_terminate(b"abc,def", b',', false), b"abc".to_vec());
        assert_eq!(bytes_terminate(b"abc,def", b',', true), b"abc,".to_vec());
        assert_eq!(bytes_terminate(b"abcdef", b',', false), b"abcdef".to_vec());

        assert_eq!(byte_array_compare(b"a", b"b"), -1);
        assert_eq!(byte_array_compare(b"a", b"a"), 0);
        assert_eq!(byte_array_compare(b"b", b"a"), 1);
    }

    #[test]
    fn read_bits_int_be_preserves_high_bits_with_unaligned_accumulator() {
        for prefix_bits in 1..=7 {
            let rem_bits = 8 - prefix_bits;
            // Prefix bits = all 0s so byte 0's lower `rem_bits` (all 1s) form the high bits of the 64-bit read.
            let first_byte = (1u8 << rem_bits) - 1;
            let last_byte = ((0xA5u8 >> prefix_bits) << prefix_bits) | ((1u8 << prefix_bits) - 1);
            let mut data = vec![first_byte, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, last_byte];

            let mut stream = KaitaiStream::new(&data);
            assert_eq!(stream.read_bits_int_be(prefix_bits), Some(0));

            let expected_val: u64 =
                ((u64::from(first_byte)) << (64 - rem_bits)) | ((0x0012_3456_789A_BCDE_u64) << prefix_bits) | u64::from(last_byte >> rem_bits);
            assert_eq!(stream.read_bits_int_be(64), Some(expected_val), "high bits lost for prefix_bits={prefix_bits}");
            assert_eq!(stream.read_bits_int_be(rem_bits), Some(u64::from(last_byte & ((1u8 << rem_bits) - 1))));
            assert!(stream.is_eof());

            // Also test when first remaining bit is explicitly 1 followed by zeros.
            data[0] = 1 << (rem_bits - 1);
            let mut stream2 = KaitaiStream::new(&data);
            assert_eq!(stream2.read_bits_int_be(prefix_bits), Some(0));
            let expected2: u64 = (1u64 << 63) | ((0x0012_3456_789A_BCDE_u64) << prefix_bits) | u64::from(last_byte >> rem_bits);
            assert_eq!(stream2.read_bits_int_be(64), Some(expected2));
        }
    }

    #[test]
    fn bit_readers_keep_stream_state_unchanged_on_eof() {
        let mut be = KaitaiStream::new(&[0b1011_0100, 0xF1]);
        assert_eq!(be.read_bits_int_be(3), Some(0b101));
        assert_eq!(be.pos(), 1);
        assert_eq!(be.read_bits_int_be(16), None);
        assert_eq!(be.pos(), 1);
        assert!(!be.is_eof());
        assert_eq!(be.read_bits_int_be(13), Some((0b1_0100 << 8) | 0xF1));
        assert!(be.is_eof());
        assert_eq!(be.read_bits_int_be(1), None);
        assert_eq!(be.pos(), 2);

        let mut le = KaitaiStream::new(&[0b1011_0100, 0xA5]);
        assert_eq!(le.read_bits_int_le(3), Some(0b100));
        assert_eq!(le.pos(), 1);
        assert_eq!(le.read_bits_int_le(16), None);
        assert_eq!(le.pos(), 1);
        assert!(!le.is_eof());
        assert_eq!(le.read_bits_int_le(13), Some((0xA5 << 5) | 0b1_0110));
        assert!(le.is_eof());
        assert_eq!(le.read_bits_int_le(1), None);
        assert_eq!(le.pos(), 2);
    }

    #[test]
    fn byte_and_substream_reads_prevent_size_overflow() {
        let mut stream = KaitaiStream::new(&[1, 2, 3, 4]);
        assert_eq!(stream.read_u1(), Some(1));
        assert_eq!(stream.pos(), 1);

        assert_eq!(stream.read_bytes_slice(usize::MAX), None);
        assert_eq!(stream.pos(), 1);
        assert_eq!(stream.read_bytes(usize::MAX), None);
        assert_eq!(stream.pos(), 1);
        assert!(stream.take_substream(usize::MAX).is_none());
        assert_eq!(stream.pos(), 1);
        assert_eq!(stream.ensure_fixed_contents(&[2, 3, 4, 5]), None);
        assert_eq!(stream.pos(), 1);
        assert_eq!(stream.ensure_fixed_contents(&[2, 3]), Some(vec![2, 3]));
        assert_eq!(stream.pos(), 3);
    }
}
