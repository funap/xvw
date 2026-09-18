pub fn sum8(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, &x| acc.wrapping_add(x))
}

pub fn sum16(data: &[u8]) -> u16 {
    data.iter().fold(0u16, |acc, &x| acc.wrapping_add(x as u16))
}

pub fn sum32(data: &[u8]) -> u32 {
    data.iter().fold(0u32, |acc, &x| acc.wrapping_add(x as u32))
}

pub fn sum64(data: &[u8]) -> u64 {
    data.iter().fold(0u64, |acc, &x| acc.wrapping_add(x as u64))
}

pub fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

pub fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &byte in data {
        let mut x = ((crc >> 8) ^ byte as u16) & 0xFF;
        x ^= x >> 4;
        crc = (crc << 8) ^ (x << 12) ^ (x << 5) ^ x;
    }
    crc
}

pub fn crc16_arc(data: &[u8]) -> u16 {
    let mut crc = 0x0000u16;
    for &byte in data {
        crc ^= byte as u16;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

const fn make_crc32_table() -> [u32; 256] {
    const POLY: u32 = 0xEDB88320;
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ POLY;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

const CRC32_TABLE: [u32; 256] = make_crc32_table();

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFFFFFFu32;
    for &byte in data {
        let idx = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32_TABLE[idx];
    }
    !crc
}

pub fn md5(data: &[u8]) -> [u8; 16] {
    let s: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4,
        11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let k: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be,
        0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
        0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c,
        0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
        0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1, 0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1,
        0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
    ];

    let mut h0 = 0x67452301u32;
    let mut h1 = 0xefcdab89u32;
    let mut h2 = 0x98badcfeu32;
    let mut h3 = 0x10325476u32;

    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while !(msg.len() + 8).is_multiple_of(64) {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());

    for chunk in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 16];
        for i in 0..16 {
            w[i] = u32::from_le_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;

        for i in 0..64 {
            let (f, g) = if i < 16 {
                ((b & c) | (!b & d), i)
            } else if i < 32 {
                ((d & b) | (!d & c), (5 * i + 1) % 16)
            } else if i < 48 {
                (b ^ c ^ d, (3 * i + 5) % 16)
            } else {
                (c ^ (b | !d), (7 * i) % 16)
            };

            let temp = d;
            d = c;
            c = b;
            let val = a.wrapping_add(f).wrapping_add(k[i]).wrapping_add(w[g]);
            let shift = s[i];
            b = b.wrapping_add(val.rotate_left(shift));
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
    }

    let mut result = [0u8; 16];
    result[0..4].copy_from_slice(&h0.to_le_bytes());
    result[4..8].copy_from_slice(&h1.to_le_bytes());
    result[8..12].copy_from_slice(&h2.to_le_bytes());
    result[12..16].copy_from_slice(&h3.to_le_bytes());
    result
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Category grouping for checksum and hash algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChecksumCategory {
    Sums,
    Crc,
    Crypto,
}

impl ChecksumCategory {
    /// List of all supported categories in presentation order.
    pub const ALL: &'static [ChecksumCategory] = &[ChecksumCategory::Sums, ChecksumCategory::Crc, ChecksumCategory::Crypto];

    /// Returns the UI display label for this category.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Sums => "Basic Sums",
            Self::Crc => "CRCs & Adler",
            Self::Crypto => "Cryptographic Hashes",
        }
    }
}

/// Supported checksum and cryptographic hash algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChecksumAlgorithm {
    Sum8,
    Sum16,
    Sum32,
    Sum64,
    Adler32,
    Crc16Ccitt,
    Crc16Arc,
    Crc32,
    Md5,
    Sha256,
}

impl ChecksumAlgorithm {
    /// List of all supported checksum algorithms in presentation order.
    pub const ALL: &'static [ChecksumAlgorithm] = &[
        ChecksumAlgorithm::Sum8,
        ChecksumAlgorithm::Sum16,
        ChecksumAlgorithm::Sum32,
        ChecksumAlgorithm::Sum64,
        ChecksumAlgorithm::Adler32,
        ChecksumAlgorithm::Crc16Ccitt,
        ChecksumAlgorithm::Crc16Arc,
        ChecksumAlgorithm::Crc32,
        ChecksumAlgorithm::Md5,
        ChecksumAlgorithm::Sha256,
    ];

    /// Returns the category for this algorithm.
    pub const fn category(&self) -> ChecksumCategory {
        match self {
            Self::Sum8 | Self::Sum16 | Self::Sum32 | Self::Sum64 => ChecksumCategory::Sums,
            Self::Adler32 | Self::Crc16Ccitt | Self::Crc16Arc | Self::Crc32 => ChecksumCategory::Crc,
            Self::Md5 | Self::Sha256 => ChecksumCategory::Crypto,
        }
    }

    /// Returns the UI display label for this algorithm.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Sum8 => "Sum 8-bit",
            Self::Sum16 => "Sum 16-bit",
            Self::Sum32 => "Sum 32-bit",
            Self::Sum64 => "Sum 64-bit",
            Self::Adler32 => "Adler-32",
            Self::Crc16Ccitt => "CRC-16 (CCITT)",
            Self::Crc16Arc => "CRC-16 (ARC)",
            Self::Crc32 => "CRC-32",
            Self::Md5 => "MD5",
            Self::Sha256 => "SHA-256",
        }
    }
}

/// Consolidated checksum and hash computation results for a byte range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChecksumResults {
    pub sum8: u8,
    pub sum16: u16,
    pub sum32: u32,
    pub sum64: u64,
    pub crc16_ccitt: u16,
    pub crc16_arc: u16,
    pub crc32: u32,
    pub adler32: u32,
    pub md5: [u8; 16],
    pub sha256: [u8; 32],
    pub data_len: usize,
    pub range_start: usize,
    pub range_end: usize,
}

impl ChecksumResults {
    /// Computes all checksums and hashes for the given slice.
    pub fn compute(data: &[u8], range_start: usize, range_end: usize) -> Self {
        Self {
            sum8: sum8(data),
            sum16: sum16(data),
            sum32: sum32(data),
            sum64: sum64(data),
            adler32: adler32(data),
            crc16_ccitt: crc16_ccitt(data),
            crc16_arc: crc16_arc(data),
            crc32: crc32(data),
            md5: md5(data),
            sha256: sha256(data),
            data_len: data.len(),
            range_start,
            range_end,
        }
    }

    /// Formats the result of a specific algorithm for UI display (hexadecimal representation).
    pub fn format_display(&self, algo: ChecksumAlgorithm) -> String {
        self.format_hex(algo)
    }

    /// Formats the raw hex value of a specific algorithm (typically used for copying to clipboard).
    pub fn format_hex(&self, algo: ChecksumAlgorithm) -> String {
        match algo {
            ChecksumAlgorithm::Sum8 => format!("0x{:02X}", self.sum8),
            ChecksumAlgorithm::Sum16 => format!("0x{:04X}", self.sum16),
            ChecksumAlgorithm::Sum32 => format!("0x{:08X}", self.sum32),
            ChecksumAlgorithm::Sum64 => format!("0x{:016X}", self.sum64),
            ChecksumAlgorithm::Adler32 => format!("0x{:08X}", self.adler32),
            ChecksumAlgorithm::Crc16Ccitt => format!("0x{:04X}", self.crc16_ccitt),
            ChecksumAlgorithm::Crc16Arc => format!("0x{:04X}", self.crc16_arc),
            ChecksumAlgorithm::Crc32 => format!("0x{:08X}", self.crc32),
            ChecksumAlgorithm::Md5 => self.md5.iter().map(|b| format!("{:02x}", b)).collect(),
            ChecksumAlgorithm::Sha256 => self.sha256.iter().map(|b| format!("{:02x}", b)).collect(),
        }
    }

    /// Formats all checksum results as a multi-line report.
    pub fn format_all(&self) -> String {
        format!(
            "Sum 8-bit:       {}\n\
             Sum 16-bit:      {}\n\
             Sum 32-bit:      {}\n\
             Sum 64-bit:      {}\n\
             Adler-32:        {}\n\
             CRC-16 (CCITT):  {}\n\
             CRC-16 (ARC):    {}\n\
             CRC-32:          {}\n\
             MD5:             {}\n\
             SHA-256:         {}",
            self.format_display(ChecksumAlgorithm::Sum8),
            self.format_display(ChecksumAlgorithm::Sum16),
            self.format_display(ChecksumAlgorithm::Sum32),
            self.format_display(ChecksumAlgorithm::Sum64),
            self.format_display(ChecksumAlgorithm::Adler32),
            self.format_display(ChecksumAlgorithm::Crc16Ccitt),
            self.format_display(ChecksumAlgorithm::Crc16Arc),
            self.format_display(ChecksumAlgorithm::Crc32),
            self.format_hex(ChecksumAlgorithm::Md5),
            self.format_hex(ChecksumAlgorithm::Sha256)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sums() {
        let data = b"Hello, world!";
        assert_eq!(sum8(data), (1161 % 256) as u8);
        assert_eq!(sum16(data), 1161u16);
        assert_eq!(sum32(data), 1161u32);
        assert_eq!(sum64(data), 1161u64);
    }

    #[test]
    fn test_adler32() {
        assert_eq!(adler32(b"Wikipedia"), 0x11E60398);
        assert_eq!(adler32(b"Hello, world!"), 0x205E048A);
    }

    #[test]
    fn test_crc16_ccitt() {
        assert_eq!(crc16_ccitt(b"123456789"), 0x29B1);
    }

    #[test]
    fn test_crc16_arc() {
        assert_eq!(crc16_arc(b"123456789"), 0xBB3D);
    }

    #[test]
    fn test_crc32() {
        assert_eq!(crc32(b"123456789"), 0xCBF43926);
    }

    #[test]
    fn test_md5() {
        let res = md5(b"The quick brown fox jumps over the lazy dog");
        let hex_str: String = res.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(hex_str, "9e107d9d372bb6826bd81d3542a419d6");
    }

    #[test]
    fn test_sha256() {
        let res = sha256(b"The quick brown fox jumps over the lazy dog");
        let hex_str: String = res.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(hex_str, "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592");
    }

    #[test]
    fn test_checksum_results_compute() {
        let data = b"Hello, world!";
        let results = ChecksumResults::compute(data, 0, 13);
        assert_eq!(results.sum8, (1161 % 256) as u8);
        assert_eq!(results.sum16, 1161);
        assert_eq!(results.adler32, 0x205E048A);
        assert_eq!(results.data_len, 13);
        assert_eq!(results.range_start, 0);
        assert_eq!(results.range_end, 13);
    }

    #[test]
    fn test_checksum_algorithm_and_formatting() {
        let data = b"Hello, world!";
        let results = ChecksumResults::compute(data, 0, 13);

        assert_eq!(ChecksumAlgorithm::ALL.len(), 10);
        for &algo in ChecksumAlgorithm::ALL {
            assert!(!algo.label().is_empty());
            assert!(!results.format_display(algo).is_empty());
            assert!(!results.format_hex(algo).is_empty());
        }

        let all_str = results.format_all();
        assert!(all_str.contains("Sum 8-bit:"));
        assert!(all_str.contains("SHA-256:"));
    }

    #[test]
    fn test_checksum_categories() {
        assert_eq!(ChecksumCategory::ALL.len(), 3);
        assert_eq!(ChecksumAlgorithm::Sum8.category(), ChecksumCategory::Sums);
        assert_eq!(ChecksumAlgorithm::Crc32.category(), ChecksumCategory::Crc);
        assert_eq!(ChecksumAlgorithm::Sha256.category(), ChecksumCategory::Crypto);
    }
}
