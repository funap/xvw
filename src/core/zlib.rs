use crate::core::format::HexCase;
use crate::core::new_file::MAX_BUFFER_SIZE;
use flate2::{Decompress, FlushDecompress, Status};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Minimum possible compressed byte length for a valid RFC 1950 zlib stream:
/// 2-byte header + 2-byte empty deflate block + 4-byte Adler-32 checksum.
pub const MIN_ZLIB_STREAM_LEN: usize = 8;

const SCAN_SCRATCH_LEN: usize = 16 * 1024;
const DECOMPRESS_SCRATCH_LEN: usize = 32 * 1024;
const ZLIB_PREVIEW_SAMPLE_BYTES: usize = 64;
const ZLIB_TEXT_PREVIEW_CHARS: usize = 48;
const ZLIB_HEX_PREVIEW_BYTES: usize = 16;

/// A validated RFC 1950 zlib stream found in a byte buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZlibMatch {
    /// The buffer byte offset at which the zlib stream starts.
    pub offset: usize,
    /// The compressed stream byte length (header + deflate payload + Adler-32 trailer).
    pub compressed_len: usize,
    /// The exact uncompressed payload byte length.
    pub decompressed_len: usize,
    /// The two-byte zlib header `(CMF, FLG)`.
    pub header: [u8; 2],
    /// A concise human-readable preview of the decompressed payload.
    pub preview: String,
}

/// Returns `true` if `(cmf, flg)` forms a valid RFC 1950 zlib header for Deflate (`CM == 8`, `CINFO <= 7`, `FCHECK` mod 31 == 0).
#[inline]
pub const fn is_valid_zlib_header(cmf: u8, flg: u8) -> bool {
    let cm = cmf & 0x0F;
    let cinfo = cmf >> 4;
    if cm != 8 || cinfo > 7 {
        return false;
    }
    (((cmf as u16) << 8) | (flg as u16)).is_multiple_of(31)
}

#[inline]
const fn has_decompressable_prefix(slice: &[u8]) -> bool {
    if slice.len() < MIN_ZLIB_STREAM_LEN {
        return false;
    }
    let cmf = slice[0];
    let flg = slice[1];
    if !is_valid_zlib_header(cmf, flg) {
        return false;
    }
    // Preset dictionary streams (`FDICT == 1`) require an external dictionary and cannot be decoded standalone.
    if (flg & 0x20) != 0 {
        return false;
    }
    // Reject deflate blocks with reserved `BTYPE == 0b11` before entering the decompressor.
    ((slice[2] >> 1) & 0b11) != 0b11
}

/// Finds all complete, non-empty, Adler-32 verified zlib streams in `data`.
#[allow(dead_code)]
pub fn find_zlib_streams(data: &[u8]) -> Vec<ZlibMatch> {
    find_zlib_streams_impl(data, None)
}

/// Finds non-empty zlib streams in `data` while capping returned matches at `max_results`.
///
/// The returned boolean is `true` when additional matches were omitted because `max_results` was reached.
pub fn find_zlib_streams_limited(data: &[u8], max_results: usize) -> (Vec<ZlibMatch>, bool) {
    if max_results == usize::MAX {
        return (find_zlib_streams(data), false);
    }

    let scan_limit = max_results.saturating_add(1);
    let mut matches = find_zlib_streams_impl(data, Some(scan_limit));
    let is_truncated = matches.len() > max_results;
    if is_truncated {
        matches.truncate(max_results);
    }
    (matches, is_truncated)
}

/// Finds non-empty zlib streams across multiple buffer segments while capping returned matches at `max_results`.
///
/// If `ranges` is empty, the entire `data` slice is scanned as one contiguous buffer.
/// When `ranges` are provided, each range is scanned independently so streams cannot falsely span address gaps.
pub fn find_zlib_streams_segmented_limited(data: &[u8], ranges: &[std::ops::Range<usize>], max_results: usize) -> (Vec<ZlibMatch>, bool) {
    if ranges.is_empty() {
        return find_zlib_streams_limited(data, max_results);
    }

    if max_results == 0 || data.len() < MIN_ZLIB_STREAM_LEN {
        return (Vec::new(), false);
    }

    let mut all_matches = Vec::new();
    let mut is_truncated = false;

    for range in ranges {
        let start = range.start.min(data.len());
        let end = range.end.min(data.len());
        if end.saturating_sub(start) < MIN_ZLIB_STREAM_LEN {
            continue;
        }

        let slice = &data[start..end];
        let remaining = if max_results == usize::MAX {
            usize::MAX
        } else {
            let rem = max_results.saturating_sub(all_matches.len());
            if rem == 0 {
                is_truncated = true;
                break;
            }
            rem
        };

        let (mut seg_matches, seg_truncated) = find_zlib_streams_limited(slice, remaining);
        for m in &mut seg_matches {
            m.offset += start;
        }
        all_matches.extend(seg_matches);

        if seg_truncated || (max_results != usize::MAX && all_matches.len() >= max_results) {
            is_truncated = true;
            break;
        }
    }

    (all_matches, is_truncated)
}

fn find_zlib_streams_impl(data: &[u8], max_results: Option<usize>) -> Vec<ZlibMatch> {
    if data.len() < MIN_ZLIB_STREAM_LEN || max_results == Some(0) {
        return Vec::new();
    }

    let mut matches = Vec::new();
    let mut decompress = Decompress::new(true);
    let mut scratch = vec![0u8; SCAN_SCRATCH_LEN];
    let mut offset = 0usize;

    while offset + MIN_ZLIB_STREAM_LEN <= data.len() {
        let slice = &data[offset..];
        if !has_decompressable_prefix(slice) {
            offset += 1;
            continue;
        }

        if let Some((compressed_len, decompressed_len, preview)) = try_probe_zlib_at(slice, &mut decompress, &mut scratch) {
            matches.push(ZlibMatch {
                offset,
                compressed_len,
                decompressed_len,
                header: [slice[0], slice[1]],
                preview,
            });

            if let Some(limit) = max_results
                && matches.len() >= limit
            {
                break;
            }

            offset += compressed_len;
        } else {
            offset += 1;
        }
    }

    matches
}

fn try_probe_zlib_at(slice: &[u8], decompress: &mut Decompress, scratch: &mut [u8]) -> Option<(usize, usize, String)> {
    decompress.reset(true);
    let mut in_pos = 0usize;
    let mut sample = [0u8; ZLIB_PREVIEW_SAMPLE_BYTES];
    let mut sample_len = 0usize;

    loop {
        let before_in = decompress.total_in();
        let before_out = decompress.total_out();
        let had_input = in_pos < slice.len();

        // Always stream with `FlushDecompress::None`: `miniz_oxide` returns `MZError::Buf` under
        // `FlushDecompress::Finish` whenever remaining decompressed output exceeds a single scratch
        // slice (e.g. highly-compressible streams >16-32 KiB), whereas `None` drains cleanly and
        // still emits `Status::StreamEnd` once the final block and Adler-32 trailer complete.
        let status = decompress.decompress(&slice[in_pos..], scratch, FlushDecompress::None).ok()?;
        let consumed = (decompress.total_in() - before_in) as usize;
        let produced = (decompress.total_out() - before_out) as usize;
        in_pos += consumed;

        if sample_len < ZLIB_PREVIEW_SAMPLE_BYTES && produced > 0 {
            let take = produced.min(ZLIB_PREVIEW_SAMPLE_BYTES - sample_len);
            sample[sample_len..sample_len + take].copy_from_slice(&scratch[..take]);
            sample_len += take;
        }

        if decompress.total_out() > MAX_BUFFER_SIZE as u64 {
            return None;
        }

        match status {
            Status::StreamEnd => {
                let compressed_len = usize::try_from(decompress.total_in()).ok()?;
                let decompressed_len = usize::try_from(decompress.total_out()).ok()?;
                if compressed_len < MIN_ZLIB_STREAM_LEN || decompressed_len == 0 {
                    return None;
                }
                let preview = format_zlib_preview(&sample[..sample_len], decompressed_len);
                return Some((compressed_len, decompressed_len, preview));
            }
            Status::Ok => {
                if consumed == 0 && produced == 0 {
                    return None;
                }
            }
            Status::BufError => {
                if !had_input || (consumed == 0 && produced == 0) {
                    return None;
                }
            }
        }
    }
}

/// Decompresses the zlib stream starting at `offset` in `data`.
///
/// When `max_compressed_len` is `Some(len)`, input is bounded to `[offset..offset + len]`
/// clamped to `data.len()`. Rejects invalid headers, `FDICT=1` preset dictionary streams,
/// truncated streams, Adler-32 mismatches, and outputs exceeding [`MAX_BUFFER_SIZE`].
pub fn decompress_zlib_at(data: &[u8], offset: usize, max_compressed_len: Option<usize>) -> Result<Vec<u8>, String> {
    if offset >= data.len() {
        return Err(format!("Offset 0x{offset:X} is out of bounds"));
    }

    let end = match max_compressed_len {
        Some(len) => offset.saturating_add(len).min(data.len()),
        None => data.len(),
    };
    let slice = &data[offset..end];

    if slice.len() < MIN_ZLIB_STREAM_LEN {
        return Err("Slice is too short to contain a complete zlib stream".to_string());
    }
    if !is_valid_zlib_header(slice[0], slice[1]) {
        return Err(format!("Invalid zlib header {:02X} {:02X} at offset 0x{offset:X}", slice[0], slice[1]));
    }
    if (slice[1] & 0x20) != 0 {
        return Err("Zlib streams with a preset dictionary (FDICT=1) are not supported".to_string());
    }

    let mut decompress = Decompress::new(true);
    let mut scratch = vec![0u8; DECOMPRESS_SCRATCH_LEN];
    let mut output = Vec::new();
    let mut in_pos = 0usize;

    loop {
        let before_in = decompress.total_in();
        let before_out = decompress.total_out();
        let had_input = in_pos < slice.len();

        let status = decompress
            .decompress(&slice[in_pos..], &mut scratch, FlushDecompress::None)
            .map_err(|err| format!("Zlib decompression error: {err}"))?;
        let consumed = (decompress.total_in() - before_in) as usize;
        let produced = (decompress.total_out() - before_out) as usize;
        in_pos += consumed;

        let new_len = output
            .len()
            .checked_add(produced)
            .filter(|&len| len <= MAX_BUFFER_SIZE)
            .ok_or_else(|| format!("Decompressed data exceeds maximum buffer limit of {MAX_BUFFER_SIZE} bytes"))?;

        if produced > 0 {
            output.reserve(new_len.saturating_sub(output.len()));
            output.extend_from_slice(&scratch[..produced]);
        }

        match status {
            Status::StreamEnd => return Ok(output),
            Status::Ok => {
                if consumed == 0 && produced == 0 {
                    return Err("Truncated or incomplete zlib stream".to_string());
                }
            }
            Status::BufError => {
                if !had_input || (consumed == 0 && produced == 0) {
                    return Err("Truncated or incomplete zlib stream".to_string());
                }
            }
        }
    }
}

/// Formats a preview string from a prefix sample of decompressed bytes.
///
/// Renders printable single-line UTF-8 text when the sample is valid printable text
/// containing non-whitespace characters, or uppercase spaced hex bytes for binary payloads.
pub fn format_zlib_preview(sample: &[u8], total_decompressed_len: usize) -> String {
    if sample.is_empty() || total_decompressed_len == 0 {
        return String::new();
    }

    let valid_text = match std::str::from_utf8(sample) {
        Ok(s) => Some(s),
        Err(err) => {
            let up_to = err.valid_up_to();
            if err.error_len().is_none() && up_to > 0 {
                std::str::from_utf8(&sample[..up_to]).ok()
            } else {
                None
            }
        }
    };

    if let Some(text) = valid_text
        && !sample.contains(&0)
        && text.chars().all(|ch| !ch.is_control() || matches!(ch, '\n' | '\r' | '\t'))
        && text.chars().any(|ch| !ch.is_whitespace())
    {
        let mut out = String::new();
        let mut emitted_chars = 0usize;
        let mut emitted_bytes = 0usize;
        let mut clipped_chars = false;
        let mut pending_space = false;

        for (byte_idx, ch) in text.char_indices() {
            if matches!(ch, '\n' | '\r' | '\t' | ' ') {
                if !out.is_empty() {
                    pending_space = true;
                }
                emitted_bytes = byte_idx + ch.len_utf8();
                continue;
            }

            if pending_space {
                if emitted_chars >= ZLIB_TEXT_PREVIEW_CHARS {
                    clipped_chars = true;
                    break;
                }
                out.push(' ');
                emitted_chars += 1;
                pending_space = false;
            }

            if emitted_chars >= ZLIB_TEXT_PREVIEW_CHARS {
                clipped_chars = true;
                break;
            }

            out.push(ch);
            emitted_chars += 1;
            emitted_bytes = byte_idx + ch.len_utf8();
        }

        if !out.is_empty() {
            if clipped_chars || total_decompressed_len > emitted_bytes {
                out.push('…');
            }
            return out;
        }
    }

    let shown = sample.len().min(ZLIB_HEX_PREVIEW_BYTES);
    let mut hex = String::with_capacity(shown * 3 + 3);
    for (idx, byte) in sample[..shown].iter().enumerate() {
        if idx > 0 {
            hex.push(' ');
        }
        let _ = write!(&mut hex, "{byte:02X}");
    }
    if total_decompressed_len > shown {
        hex.push('…');
    }
    hex
}

/// Builds a unique virtual path for a newly created decompressed document.
///
/// The filename includes the source file stem and stream address (`<stem>_zlib_0x<addr>.bin`),
/// appending `_2`, `_3`, ... until `exists(&candidate)` returns `false` so existing disk files
/// and open editor tabs are never collided with.
pub fn build_decompressed_document_path_with(source_path: &Path, address: usize, hex_case: HexCase, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let raw_stem = source_path.file_stem().and_then(|s| s.to_str()).unwrap_or("").trim();
    let prefix = if raw_stem.is_empty() {
        "zlib".to_string()
    } else {
        format!("{raw_stem}_zlib")
    };
    let parent = source_path.parent().filter(|p| !p.as_os_str().is_empty());
    let addr_str = hex_case.format_prefixed(address);

    for suffix_idx in 0usize.. {
        let filename = if suffix_idx == 0 {
            format!("{prefix}_{addr_str}.bin")
        } else {
            format!("{prefix}_{addr_str}_{}.bin", suffix_idx + 1)
        };
        let candidate = match parent {
            Some(dir) => dir.join(filename),
            None => PathBuf::from(filename),
        };
        if !exists(&candidate) {
            return candidate;
        }
    }

    unreachable!("unbounded suffix iterator always finds a unique path")
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::ZlibEncoder};
    use std::collections::HashSet;
    use std::io::Write as _;

    fn compress_zlib(data: &[u8], level: Compression) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), level);
        encoder.write_all(data).expect("zlib encode write");
        encoder.finish().expect("zlib encode finish")
    }

    #[test]
    fn validates_rfc1950_headers() {
        // Common zlib headers: 78 01 (no/low), 78 5E (fast), 78 9C (default), 78 DA (best)
        assert!(is_valid_zlib_header(0x78, 0x01));
        assert!(is_valid_zlib_header(0x78, 0x5E));
        assert!(is_valid_zlib_header(0x78, 0x9C));
        assert!(is_valid_zlib_header(0x78, 0xDA));
        // Smaller window sizes (CINFO = 0..7)
        assert!(is_valid_zlib_header(0x08, 0x1D));
        assert!(is_valid_zlib_header(0x48, 0x0D));
        // FDICT=1 still passes RFC 1950 header checksum formula (rejected later by prefix/decompress)
        assert!(is_valid_zlib_header(0x78, 0xBB));

        // Invalid CM (!= 8), CINFO (> 7), or FCHECK
        assert!(!is_valid_zlib_header(0x79, 0x9C));
        assert!(!is_valid_zlib_header(0x88, 0x00));
        assert!(!is_valid_zlib_header(0x78, 0x9D));
    }

    #[test]
    fn finds_embedded_and_adjacent_zlib_streams_with_exact_bounds() {
        let payload1 = b"Hello embedded zlib stream number one!";
        let payload2 = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x10";
        let compressed1 = compress_zlib(payload1, Compression::default());
        let compressed2 = compress_zlib(payload2, Compression::best());

        let mut blob = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let offset1 = blob.len();
        blob.extend_from_slice(&compressed1);
        // Back-to-back stream with zero padding between streams!
        let offset2 = blob.len();
        blob.extend_from_slice(&compressed2);
        blob.extend_from_slice(&[0xFF, 0x00, 0x12, 0x34, 0x78, 0x9C]);

        let matches = find_zlib_streams(&blob);
        assert_eq!(matches.len(), 2);

        assert_eq!(matches[0].offset, offset1);
        assert_eq!(matches[0].compressed_len, compressed1.len());
        assert_eq!(matches[0].decompressed_len, payload1.len());
        assert_eq!(matches[0].header, [compressed1[0], compressed1[1]]);
        assert_eq!(matches[0].preview, "Hello embedded zlib stream number one!");

        assert_eq!(matches[1].offset, offset2);
        assert_eq!(matches[1].compressed_len, compressed2.len());
        assert_eq!(matches[1].decompressed_len, payload2.len());
        assert!(matches[1].preview.starts_with("89 50 4E 47"));
        assert!(matches[1].preview.ends_with('…'));

        assert_eq!(decompress_zlib_at(&blob, offset1, Some(matches[0].compressed_len)).unwrap(), payload1);
        assert_eq!(decompress_zlib_at(&blob, offset2, Some(matches[1].compressed_len)).unwrap(), payload2);
    }

    #[test]
    fn handles_multi_chunk_streams_and_all_compression_levels() {
        // Larger than SCAN_SCRATCH_LEN (16 KiB) and DECOMPRESS_SCRATCH_LEN (32 KiB)
        let mut large_payload = Vec::with_capacity(50_000);
        for i in 0..50_000usize {
            large_payload.push(b'A' + (i % 26) as u8);
        }

        for level in [Compression::none(), Compression::fast(), Compression::default(), Compression::best()] {
            let compressed = compress_zlib(&large_payload, level);
            let mut data = vec![0x11, 0x22, 0x33];
            data.extend_from_slice(&compressed);
            data.extend_from_slice(&[0x44, 0x55]);

            let matches = find_zlib_streams(&data);
            assert_eq!(matches.len(), 1, "failed for compression level {level:?}");
            assert_eq!(matches[0].offset, 3);
            assert_eq!(matches[0].compressed_len, compressed.len());
            assert_eq!(matches[0].decompressed_len, large_payload.len());

            let restored = decompress_zlib_at(&data, 3, Some(matches[0].compressed_len)).unwrap();
            assert_eq!(restored, large_payload);
        }
    }

    #[test]
    fn handles_highly_compressible_streams_exceeding_internal_window() {
        // Highly compressible payloads consume all compressed input well before draining all
        // 32 KiB miniz_oxide output dictionary chunks.
        let zero_payload = vec![0u8; 128 * 1024];
        let compressed = compress_zlib(&zero_payload, Compression::best());
        assert!(compressed.len() < 1024);

        // Case 1: Stream ends cleanly at exact EOF (zero trailing bytes)
        let eof_matches = find_zlib_streams(&compressed);
        assert_eq!(eof_matches.len(), 1);
        assert_eq!(eof_matches[0].offset, 0);
        assert_eq!(eof_matches[0].compressed_len, compressed.len());
        assert_eq!(eof_matches[0].decompressed_len, zero_payload.len());
        assert_eq!(decompress_zlib_at(&compressed, 0, Some(compressed.len())).unwrap(), zero_payload);
        assert_eq!(decompress_zlib_at(&compressed, 0, None).unwrap(), zero_payload);

        // Case 2: Preceded by junk, followed immediately by another valid stream + bogus 78 9C trailer
        let trailing_payload = b"stream right after 128 KiB zeros";
        let trailing_compressed = compress_zlib(trailing_payload, Compression::default());
        let mut blob = vec![0xAB, 0xCD, 0x78, 0x9C];
        let zero_offset = blob.len();
        blob.extend_from_slice(&compressed);
        let second_offset = blob.len();
        blob.extend_from_slice(&trailing_compressed);
        blob.extend_from_slice(&[0xAB, 0xCD, 0x78, 0x9C]);

        let matches = find_zlib_streams(&blob);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].offset, zero_offset);
        assert_eq!(matches[0].compressed_len, compressed.len());
        assert_eq!(matches[0].decompressed_len, zero_payload.len());
        assert!(matches[0].preview.starts_with("00 00 00 00"));
        assert!(matches[0].preview.ends_with('…'));

        assert_eq!(matches[1].offset, second_offset);
        assert_eq!(matches[1].compressed_len, trailing_compressed.len());
        assert_eq!(matches[1].decompressed_len, trailing_payload.len());

        assert_eq!(decompress_zlib_at(&blob, zero_offset, Some(compressed.len())).unwrap(), zero_payload);
        assert_eq!(decompress_zlib_at(&blob, zero_offset, None).unwrap(), zero_payload);
        assert_eq!(decompress_zlib_at(&blob, second_offset, None).unwrap(), trailing_payload);

        // Case 3: Truncating even 1 byte (inside Adler-32 trailer after >32 KiB output) must still fail
        assert!(find_zlib_streams(&compressed[..compressed.len() - 1]).is_empty());
        assert!(decompress_zlib_at(&compressed[..compressed.len() - 1], 0, None).is_err());
        assert!(decompress_zlib_at(&blob, zero_offset, Some(compressed.len() - 1)).is_err());
    }

    #[test]
    fn rejects_corrupted_adler_truncated_and_fdict_streams() {
        let compressed = compress_zlib(b"integrity verified payload", Compression::default());

        // Corrupted Adler-32 trailer byte
        let mut bad_adler = compressed.clone();
        *bad_adler.last_mut().unwrap() ^= 0x01;
        assert!(find_zlib_streams(&bad_adler).is_empty());
        assert!(decompress_zlib_at(&bad_adler, 0, None).is_err());

        // Truncated by 1 byte
        let truncated = &compressed[..compressed.len() - 1];
        assert!(find_zlib_streams(truncated).is_empty());
        assert!(decompress_zlib_at(truncated, 0, None).is_err());

        // Corrupted deflate payload byte
        let mut bad_deflate = compressed.clone();
        bad_deflate[3] ^= 0xFF;
        assert!(find_zlib_streams(&bad_deflate).is_empty());

        // Craft FDICT=1 header (0x78, 0xBB -> FCHECK valid)
        let mut fdict = vec![0x78, 0xBB, 0x00, 0x00, 0x00, 0x01];
        fdict.extend_from_slice(&compressed[2..]);
        assert!(find_zlib_streams(&fdict).is_empty());
        let err = decompress_zlib_at(&fdict, 0, None).unwrap_err();
        assert!(err.contains("FDICT=1"));
    }

    #[test]
    fn skips_empty_zero_byte_zlib_streams() {
        let empty = compress_zlib(b"", Compression::default());
        let one_byte = compress_zlib(b"x", Compression::default());
        let large = compress_zlib(b"abcdefghij", Compression::default());

        let mut blob = Vec::new();
        blob.extend_from_slice(&empty);
        let one_off = blob.len();
        blob.extend_from_slice(&one_byte);
        let large_off = blob.len();
        blob.extend_from_slice(&large);

        let matches = find_zlib_streams(&blob);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].offset, one_off);
        assert_eq!(matches[0].decompressed_len, 1);
        assert_eq!(matches[1].offset, large_off);
        assert_eq!(matches[1].decompressed_len, 10);
    }

    #[test]
    fn respects_max_results_and_segmented_boundaries() {
        let s1 = compress_zlib(b"segment one", Compression::default());
        let s2 = compress_zlib(b"segment two", Compression::default());
        let s3 = compress_zlib(b"segment three", Compression::default());

        let mut blob = Vec::new();
        blob.extend_from_slice(&s1);
        let off2 = blob.len();
        blob.extend_from_slice(&s2);
        let off3 = blob.len();
        blob.extend_from_slice(&s3);

        // Truncated to 2
        let (limited, truncated) = find_zlib_streams_limited(&blob, 2);
        assert!(truncated);
        assert_eq!(limited.len(), 2);

        // Split s2 across two segments so it cannot match when scanned segment-by-segment
        let split_point = off2 + s2.len() / 2;
        let ranges = vec![0..split_point, split_point..blob.len()];
        let (seg_matches, seg_truncated) = find_zlib_streams_segmented_limited(&blob, &ranges, 10);
        assert!(!seg_truncated);
        assert_eq!(seg_matches.len(), 2);
        assert_eq!(seg_matches[0].offset, 0);
        assert_eq!(seg_matches[1].offset, off3);
    }

    #[test]
    fn formats_utf8_and_binary_previews_safely() {
        // Multi-byte UTF-8 boundary clipped cleanly inside sample
        let utf8 = "こんにちは世界".as_bytes();
        let preview = format_zlib_preview(&utf8[..utf8.len() - 1], 100);
        assert_eq!(preview, "こんにちは世…");

        // Long text clipped at ZLIB_TEXT_PREVIEW_CHARS without splitting multi-byte chars
        let long_jp = "あ".repeat(60);
        let jp_preview = format_zlib_preview(long_jp.as_bytes(), long_jp.len());
        assert_eq!(jp_preview.chars().count(), ZLIB_TEXT_PREVIEW_CHARS + 1);
        assert!(jp_preview.ends_with('…'));

        // Control/whitespace normalization
        let multiline = b"hello\r\n  world\t!";
        assert_eq!(format_zlib_preview(multiline, multiline.len()), "hello world !");

        // Pure whitespace falls back to hex so table rows don't look blank
        assert_eq!(format_zlib_preview(b"    ", 4), "20 20 20 20");
    }

    #[test]
    fn builds_unique_virtual_decompressed_paths() {
        let mut taken: HashSet<PathBuf> = HashSet::new();
        taken.insert(PathBuf::from("/tmp/firmware_zlib_0x1040.bin"));
        taken.insert(PathBuf::from("/tmp/firmware_zlib_0x1040_2.bin"));

        let next = build_decompressed_document_path_with(Path::new("/tmp/firmware.bin"), 0x1040, HexCase::Lower, |p| taken.contains(p));
        assert_eq!(next, PathBuf::from("/tmp/firmware_zlib_0x1040_3.bin"));

        let upper = build_decompressed_document_path_with(Path::new("Untitled-1.bin"), 0x2A, HexCase::Upper, |_| false);
        assert_eq!(upper, PathBuf::from("Untitled-1_zlib_0x2A.bin"));

        let empty_source = build_decompressed_document_path_with(Path::new(""), 0, HexCase::Lower, |_| false);
        assert_eq!(empty_source, PathBuf::from("zlib_0x0.bin"));
    }
}
