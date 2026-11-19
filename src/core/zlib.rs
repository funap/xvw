use crate::core::format::HexCase;
use crate::core::new_file::MAX_BUFFER_SIZE;
use flate2::{Decompress, FlushDecompress, Status};
use std::fmt::{self, Write as _};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Minimum possible compressed byte length for a valid RFC 1950 zlib stream:
/// 2-byte header + 2-byte empty deflate block + 4-byte Adler-32 checksum.
pub const MIN_ZLIB_STREAM_LEN: usize = 8;

/// Maximum number of leading decompressed bytes retained per [`ZlibMatch`] for preview generation.
pub const ZLIB_PREVIEW_SAMPLE_BYTES: usize = 64;

/// Match `miniz_oxide`'s 32 KiB sliding window dictionary so large streams drain in fewer calls.
const SCAN_SCRATCH_LEN: usize = 32 * 1024;
const DECOMPRESS_SCRATCH_LEN: usize = 32 * 1024;
/// Upper bound on speculative initial output buffer reservation from caller-supplied size hints.
const MAX_INITIAL_DECOMPRESS_RESERVE: usize = 64 * 1024 * 1024;
/// Minimum compressed stream bytes required for a stored (`BTYPE = 0`) deflate block:
/// 2-byte zlib header + 1-byte block header + 4-byte `LEN`/`NLEN` + 4-byte Adler-32 trailer.
const MIN_STORED_BLOCK_STREAM_LEN: usize = 11;
/// Check cooperative cancellation roughly once per 4 KiB during linear byte scanning.
const CANCEL_CHECK_INTERVAL_MASK: usize = 0x0FFF;

const ZLIB_TEXT_PREVIEW_CHARS: usize = 48;
const ZLIB_HEX_PREVIEW_BYTES: usize = 16;

/// Errors returned while validating or decompressing an embedded RFC 1950 zlib stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZlibError {
    /// The requested start offset lies outside the source buffer.
    OffsetOutOfBounds { offset: usize, len: usize },
    /// The bounded input slice is shorter than [`MIN_ZLIB_STREAM_LEN`].
    InputTooShort,
    /// The two bytes at `offset` are not a valid RFC 1950 Deflate header.
    InvalidHeader { offset: usize, cmf: u8, flg: u8 },
    /// Streams with `FDICT=1` require an external dictionary and cannot be decoded standalone.
    PresetDictionaryUnsupported,
    /// The underlying inflate decoder rejected the bitstream or Adler-32 checksum.
    DecompressionFailed(String),
    /// Decompressed output exceeded [`MAX_BUFFER_SIZE`].
    OutputLimitExceeded { max_bytes: usize },
    /// Input ended before reaching the end of the Deflate stream and Adler-32 trailer.
    TruncatedStream,
    /// Cooperative cancellation was signaled before decompression finished.
    Cancelled,
}

impl fmt::Display for ZlibError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OffsetOutOfBounds { offset, len } => write!(f, "Offset 0x{offset:X} is out of bounds (buffer length {len})"),
            Self::InputTooShort => write!(f, "Slice is too short to contain a complete zlib stream"),
            Self::InvalidHeader { offset, cmf, flg } => write!(f, "Invalid zlib header {cmf:02X} {flg:02X} at offset 0x{offset:X}"),
            Self::PresetDictionaryUnsupported => write!(f, "Zlib streams with a preset dictionary (FDICT=1) are not supported"),
            Self::DecompressionFailed(err) => write!(f, "Zlib decompression error: {err}"),
            Self::OutputLimitExceeded { max_bytes } => write!(f, "Decompressed data exceeds maximum buffer limit of {max_bytes} bytes"),
            Self::TruncatedStream => write!(f, "Truncated or incomplete zlib stream"),
            Self::Cancelled => write!(f, "Zlib decompression cancelled"),
        }
    }
}

impl std::error::Error for ZlibError {}

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
    sample: [u8; ZLIB_PREVIEW_SAMPLE_BYTES],
    sample_len: u8,
}

impl ZlibMatch {
    /// Creates a [`ZlibMatch`] retaining up to [`ZLIB_PREVIEW_SAMPLE_BYTES`] of `sample`.
    pub fn from_sample(offset: usize, compressed_len: usize, decompressed_len: usize, header: [u8; 2], sample_bytes: &[u8]) -> Self {
        let mut sample = [0u8; ZLIB_PREVIEW_SAMPLE_BYTES];
        let copy_len = sample_bytes.len().min(ZLIB_PREVIEW_SAMPLE_BYTES);
        sample[..copy_len].copy_from_slice(&sample_bytes[..copy_len]);
        Self {
            offset,
            compressed_len,
            decompressed_len,
            header,
            sample,
            sample_len: copy_len as u8,
        }
    }

    /// Returns the retained prefix of decompressed payload bytes for preview rendering.
    #[inline]
    pub fn preview_sample(&self) -> &[u8] {
        &self.sample[..usize::from(self.sample_len)]
    }

    /// Formats a concise human-readable preview of this stream's decompressed payload.
    pub fn format_preview(&self) -> String {
        format_zlib_preview(self.preview_sample(), self.decompressed_len)
    }
}

/// Outcome of scanning a buffer (or segmented ranges) for embedded zlib streams.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZlibScanResult {
    /// Discovered non-empty, Adler-32 verified zlib streams in ascending offset order.
    pub matches: Vec<ZlibMatch>,
    /// `true` if additional valid streams existed beyond the configured `max_results` limit.
    pub is_truncated: bool,
    /// `true` if scanning terminated early due to cooperative cancellation.
    pub cancelled: bool,
}

/// Returns `true` if `(cmf, flg)` forms a valid RFC 1950 zlib header for Deflate (`CM == 8`, `CINFO <= 7`, `FCHECK` mod 31 == 0).
#[inline]
pub const fn is_valid_zlib_header(cmf: u8, flg: u8) -> bool {
    // Bit 7 == 0 (CINFO <= 7) and low nibble == 8 (CM == 8).
    if (cmf & 0x8F) != 0x08 {
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

    let block_hdr = slice[2];
    let bfinal = block_hdr & 0x01;
    let btype = (block_hdr >> 1) & 0b11;
    match btype {
        // Stored / uncompressed block (RFC 1951 Section 3.2.4): LEN must equal !NLEN and fit with trailer.
        0 => {
            if slice.len() < MIN_STORED_BLOCK_STREAM_LEN {
                return false;
            }
            let len = u16::from_le_bytes([slice[3], slice[4]]);
            let nlen = u16::from_le_bytes([slice[5], slice[6]]);
            if len != !nlen {
                return false;
            }
            // Final zero-length stored block produces an empty payload, which we always skip.
            if bfinal == 1 && len == 0 {
                return false;
            }
            slice.len() >= MIN_STORED_BLOCK_STREAM_LEN + len as usize
        }
        // Fixed Huffman block: always enters the decompressor.
        1 => true,
        // Dynamic Huffman block (RFC 1951 Section 3.2.7): HLIT <= 29 (257..=286), HDIST <= 29 (1..=30).
        2 => {
            let hlit = block_hdr >> 3;
            let hdist = slice[3] & 0x1F;
            hlit <= 29 && hdist <= 29
        }
        // Reserved `BTYPE == 0b11` is always invalid.
        _ => false,
    }
}

#[inline]
fn is_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|flag| flag.load(Ordering::Relaxed))
}

struct ProbeState {
    decompress: Decompress,
    scratch: Vec<u8>,
}

impl ProbeState {
    fn new() -> Self {
        Self {
            decompress: Decompress::new(true),
            scratch: vec![0u8; SCAN_SCRATCH_LEN],
        }
    }
}

enum ProbeOutcome {
    Match(ZlibMatch),
    NoMatch,
    Cancelled,
}

/// Finds all complete, non-empty, Adler-32 verified zlib streams in `data`.
#[allow(dead_code)]
pub fn find_zlib_streams(data: &[u8]) -> Vec<ZlibMatch> {
    find_zlib_streams_segmented(data, &[], usize::MAX, None).matches
}

/// Finds non-empty zlib streams in `data` while capping returned matches at `max_results`.
#[allow(dead_code)]
pub fn find_zlib_streams_limited(data: &[u8], max_results: usize) -> ZlibScanResult {
    find_zlib_streams_segmented(data, &[], max_results, None)
}

/// Finds non-empty zlib streams across `ranges` while capping returned matches at `max_results`
/// and honoring optional cooperative cancellation.
///
/// If `ranges` is empty, the entire `data` slice is scanned as one contiguous buffer.
/// When `ranges` are provided, each range is scanned independently so streams cannot falsely span address gaps.
pub fn find_zlib_streams_segmented(data: &[u8], ranges: &[Range<usize>], max_results: usize, cancel: Option<&AtomicBool>) -> ZlibScanResult {
    if max_results == 0 || data.len() < MIN_ZLIB_STREAM_LEN {
        return ZlibScanResult {
            matches: Vec::new(),
            is_truncated: false,
            cancelled: false,
        };
    }

    let scan_limit = if max_results == usize::MAX {
        None
    } else {
        Some(max_results.saturating_add(1))
    };

    let mut matches = Vec::new();
    let mut probe_state: Option<ProbeState> = None;
    let mut cancelled = false;

    let default_range = 0..data.len();
    let effective_ranges = if ranges.is_empty() { std::slice::from_ref(&default_range) } else { ranges };

    for range in effective_ranges {
        let start = range.start.min(data.len());
        let end = range.end.min(data.len());
        if end.saturating_sub(start) < MIN_ZLIB_STREAM_LEN {
            continue;
        }

        if scan_segment(data, start, end, scan_limit, cancel, &mut probe_state, &mut matches) {
            cancelled = true;
            break;
        }

        if let Some(limit) = scan_limit
            && matches.len() >= limit
        {
            break;
        }
    }

    let is_truncated = if !cancelled && max_results != usize::MAX && matches.len() > max_results {
        matches.truncate(max_results);
        true
    } else {
        false
    };

    ZlibScanResult {
        matches,
        is_truncated,
        cancelled,
    }
}

/// Scans `data[start..end]`, pushing verified matches into `matches` and returning `true` if cancelled.
fn scan_segment(
    data: &[u8],
    start: usize,
    end: usize,
    scan_limit: Option<usize>,
    cancel: Option<&AtomicBool>,
    probe_state: &mut Option<ProbeState>,
    matches: &mut Vec<ZlibMatch>,
) -> bool {
    let mut offset = start;
    while offset + MIN_ZLIB_STREAM_LEN <= end {
        if ((offset - start) & CANCEL_CHECK_INTERVAL_MASK) == 0 && is_cancelled(cancel) {
            return true;
        }

        // Fast reject 31/32 of bytes before slicing or reading subsequent header bytes.
        if (data[offset] & 0x8F) != 0x08 {
            offset += 1;
            continue;
        }

        let slice = &data[offset..end];
        if !has_decompressable_prefix(slice) {
            offset += 1;
            continue;
        }

        let state = probe_state.get_or_insert_with(ProbeState::new);
        match try_probe_zlib_at(offset, slice, state, cancel) {
            ProbeOutcome::Match(matched) => {
                let advance = matched.compressed_len;
                matches.push(matched);
                if let Some(limit) = scan_limit
                    && matches.len() >= limit
                {
                    return false;
                }
                offset += advance;
            }
            ProbeOutcome::NoMatch => {
                offset += 1;
            }
            ProbeOutcome::Cancelled => return true,
        }
    }

    false
}

fn try_probe_zlib_at(offset: usize, slice: &[u8], state: &mut ProbeState, cancel: Option<&AtomicBool>) -> ProbeOutcome {
    state.decompress.reset(true);
    let mut in_pos = 0usize;
    let mut sample = [0u8; ZLIB_PREVIEW_SAMPLE_BYTES];
    let mut sample_len = 0usize;

    loop {
        if is_cancelled(cancel) {
            return ProbeOutcome::Cancelled;
        }

        let before_in = state.decompress.total_in();
        let before_out = state.decompress.total_out();
        let had_input = in_pos < slice.len();

        // Always stream with `FlushDecompress::None`: `miniz_oxide` returns `MZError::Buf` under
        // `FlushDecompress::Finish` whenever remaining decompressed output exceeds a single scratch
        // slice (e.g. highly-compressible streams >32 KiB), whereas `None` drains cleanly and
        // still emits `Status::StreamEnd` once the final block and Adler-32 trailer complete.
        let Ok(status) = state.decompress.decompress(&slice[in_pos..], &mut state.scratch, FlushDecompress::None) else {
            return ProbeOutcome::NoMatch;
        };
        let consumed = (state.decompress.total_in() - before_in) as usize;
        let produced = (state.decompress.total_out() - before_out) as usize;
        in_pos += consumed;

        if sample_len < ZLIB_PREVIEW_SAMPLE_BYTES && produced > 0 {
            let take = produced.min(ZLIB_PREVIEW_SAMPLE_BYTES - sample_len);
            sample[sample_len..sample_len + take].copy_from_slice(&state.scratch[..take]);
            sample_len += take;
        }

        if state.decompress.total_out() > MAX_BUFFER_SIZE as u64 {
            return ProbeOutcome::NoMatch;
        }

        match status {
            Status::StreamEnd => {
                let Ok(compressed_len) = usize::try_from(state.decompress.total_in()) else {
                    return ProbeOutcome::NoMatch;
                };
                let Ok(decompressed_len) = usize::try_from(state.decompress.total_out()) else {
                    return ProbeOutcome::NoMatch;
                };
                if compressed_len < MIN_ZLIB_STREAM_LEN || decompressed_len == 0 {
                    return ProbeOutcome::NoMatch;
                }
                return ProbeOutcome::Match(ZlibMatch::from_sample(
                    offset,
                    compressed_len,
                    decompressed_len,
                    [slice[0], slice[1]],
                    &sample[..sample_len],
                ));
            }
            Status::Ok => {
                if consumed == 0 && produced == 0 {
                    return ProbeOutcome::NoMatch;
                }
            }
            Status::BufError => {
                if !had_input || (consumed == 0 && produced == 0) {
                    return ProbeOutcome::NoMatch;
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
#[allow(dead_code)]
pub fn decompress_zlib_at(data: &[u8], offset: usize, max_compressed_len: Option<usize>) -> Result<Vec<u8>, ZlibError> {
    decompress_zlib_at_cancellable(data, offset, max_compressed_len, None, None)
}

/// Decompresses the zlib stream starting at `offset` in `data` with an optional output-size hint
/// and cooperative cancellation flag.
pub fn decompress_zlib_at_cancellable(
    data: &[u8],
    offset: usize,
    max_compressed_len: Option<usize>,
    expected_decompressed_len: Option<usize>,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<u8>, ZlibError> {
    if offset >= data.len() {
        return Err(ZlibError::OffsetOutOfBounds { offset, len: data.len() });
    }

    let end = match max_compressed_len {
        Some(len) => offset.saturating_add(len).min(data.len()),
        None => data.len(),
    };
    let slice = &data[offset..end];

    if slice.len() < MIN_ZLIB_STREAM_LEN {
        return Err(ZlibError::InputTooShort);
    }
    if !is_valid_zlib_header(slice[0], slice[1]) {
        return Err(ZlibError::InvalidHeader {
            offset,
            cmf: slice[0],
            flg: slice[1],
        });
    }
    if (slice[1] & 0x20) != 0 {
        return Err(ZlibError::PresetDictionaryUnsupported);
    }

    let mut decompress = Decompress::new(true);
    let mut scratch = vec![0u8; DECOMPRESS_SCRATCH_LEN];
    let initial_cap = expected_decompressed_len.unwrap_or(0).min(MAX_INITIAL_DECOMPRESS_RESERVE).min(MAX_BUFFER_SIZE);
    let mut output = Vec::with_capacity(initial_cap);
    let mut in_pos = 0usize;

    loop {
        if is_cancelled(cancel) {
            return Err(ZlibError::Cancelled);
        }

        let before_in = decompress.total_in();
        let before_out = decompress.total_out();
        let had_input = in_pos < slice.len();

        let status = decompress
            .decompress(&slice[in_pos..], &mut scratch, FlushDecompress::None)
            .map_err(|err| ZlibError::DecompressionFailed(err.to_string()))?;
        let consumed = (decompress.total_in() - before_in) as usize;
        let produced = (decompress.total_out() - before_out) as usize;
        in_pos += consumed;

        output
            .len()
            .checked_add(produced)
            .filter(|&len| len <= MAX_BUFFER_SIZE)
            .ok_or(ZlibError::OutputLimitExceeded { max_bytes: MAX_BUFFER_SIZE })?;

        if produced > 0 {
            output.extend_from_slice(&scratch[..produced]);
        }

        match status {
            Status::StreamEnd => return Ok(output),
            Status::Ok => {
                if consumed == 0 && produced == 0 {
                    return Err(ZlibError::TruncatedStream);
                }
            }
            Status::BufError => {
                if !had_input || (consumed == 0 && produced == 0) {
                    return Err(ZlibError::TruncatedStream);
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
    fn prefilter_rejects_invalid_stored_and_dynamic_block_headers() {
        // BTYPE=0 with mismatched LEN/NLEN (0x0005 vs 0x0000)
        assert!(!has_decompressable_prefix(&[
            0x78, 0x01, 0x01, 0x05, 0x00, 0x00, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ]));
        // BTYPE=0 with BFINAL=1 and LEN=0 (valid empty stream -> skipped)
        assert!(!has_decompressable_prefix(&[0x78, 0x01, 0x01, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x01]));
        // BTYPE=0 with valid LEN=1, NLEN=0xFFFE, and enough trailing bytes
        assert!(has_decompressable_prefix(&[0x78, 0x01, 0x01, 0x01, 0x00, 0xFE, 0xFF, b'A', 0, 0, 0, 0]));
        // BTYPE=0 with valid LEN=10 but truncated slice (< 11 + 10 = 21 bytes)
        assert!(!has_decompressable_prefix(&[0x78, 0x01, 0x01, 0x0A, 0x00, 0xF5, 0xFF, 0, 0, 0, 0, 0]));
        // BTYPE=2 with HLIT=30 (0b11110 << 3 | 0b100 = 0xF4) -> invalid (> 29)
        assert!(!has_decompressable_prefix(&[0x78, 0x9C, 0xF4, 0x00, 0, 0, 0, 0]));
        // BTYPE=2 with HDIST=30 (0x1E) -> invalid (> 29)
        assert!(!has_decompressable_prefix(&[0x78, 0x9C, 0x04, 0x1E, 0, 0, 0, 0]));
        // BTYPE=2 with HLIT=29 and HDIST=29 -> accepted by prefilter
        assert!(has_decompressable_prefix(&[0x78, 0x9C, (29 << 3) | 0b100, 29, 0, 0, 0, 0]));
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
        assert_eq!(matches[0].format_preview(), "Hello embedded zlib stream number one!");

        assert_eq!(matches[1].offset, offset2);
        assert_eq!(matches[1].compressed_len, compressed2.len());
        assert_eq!(matches[1].decompressed_len, payload2.len());
        let preview2 = matches[1].format_preview();
        assert!(preview2.starts_with("89 50 4E 47"));
        assert!(preview2.ends_with('…'));

        assert_eq!(decompress_zlib_at(&blob, offset1, Some(matches[0].compressed_len)).unwrap(), payload1);
        assert_eq!(decompress_zlib_at(&blob, offset2, Some(matches[1].compressed_len)).unwrap(), payload2);
    }

    #[test]
    fn handles_multi_chunk_streams_and_all_compression_levels() {
        // Larger than SCAN_SCRATCH_LEN / DECOMPRESS_SCRATCH_LEN (32 KiB)
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

            let restored = decompress_zlib_at_cancellable(&data, 3, Some(matches[0].compressed_len), Some(matches[0].decompressed_len), None).unwrap();
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
        let preview0 = matches[0].format_preview();
        assert!(preview0.starts_with("00 00 00 00"));
        assert!(preview0.ends_with('…'));

        assert_eq!(matches[1].offset, second_offset);
        assert_eq!(matches[1].compressed_len, trailing_compressed.len());
        assert_eq!(matches[1].decompressed_len, trailing_payload.len());

        assert_eq!(decompress_zlib_at(&blob, zero_offset, Some(compressed.len())).unwrap(), zero_payload);
        assert_eq!(decompress_zlib_at(&blob, zero_offset, None).unwrap(), zero_payload);
        assert_eq!(decompress_zlib_at(&blob, second_offset, None).unwrap(), trailing_payload);

        // Case 3: Truncating even 1 byte (inside Adler-32 trailer after >32 KiB output) must still fail
        assert!(find_zlib_streams(&compressed[..compressed.len() - 1]).is_empty());
        assert_eq!(
            decompress_zlib_at(&compressed[..compressed.len() - 1], 0, None),
            Err(ZlibError::TruncatedStream)
        );
        assert_eq!(
            decompress_zlib_at(&blob, zero_offset, Some(compressed.len() - 1)),
            Err(ZlibError::TruncatedStream)
        );
    }

    #[test]
    fn rejects_corrupted_adler_truncated_and_fdict_streams() {
        let compressed = compress_zlib(b"integrity verified payload", Compression::default());

        // Corrupted Adler-32 trailer byte
        let mut bad_adler = compressed.clone();
        *bad_adler.last_mut().unwrap() ^= 0x01;
        assert!(find_zlib_streams(&bad_adler).is_empty());
        assert!(matches!(decompress_zlib_at(&bad_adler, 0, None), Err(ZlibError::DecompressionFailed(_))));

        // Truncated by 1 byte
        let truncated = &compressed[..compressed.len() - 1];
        assert!(find_zlib_streams(truncated).is_empty());
        assert_eq!(decompress_zlib_at(truncated, 0, None), Err(ZlibError::TruncatedStream));

        // Corrupted deflate payload byte
        let mut bad_deflate = compressed.clone();
        bad_deflate[3] ^= 0xFF;
        assert!(find_zlib_streams(&bad_deflate).is_empty());

        // Craft FDICT=1 header (0x78, 0xBB -> FCHECK valid)
        let mut fdict = vec![0x78, 0xBB, 0x00, 0x00, 0x00, 0x01];
        fdict.extend_from_slice(&compressed[2..]);
        assert!(find_zlib_streams(&fdict).is_empty());
        assert_eq!(decompress_zlib_at(&fdict, 0, None), Err(ZlibError::PresetDictionaryUnsupported));
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
    fn respects_max_results_exact_count_and_segmented_boundaries() {
        let s1 = compress_zlib(b"segment one", Compression::default());
        let s2 = compress_zlib(b"segment two", Compression::default());
        let s3 = compress_zlib(b"segment three", Compression::default());

        let mut blob = Vec::new();
        blob.extend_from_slice(&s1);
        let off2 = blob.len();
        blob.extend_from_slice(&s2);
        let off3 = blob.len();
        blob.extend_from_slice(&s3);

        // Truncated to 2 when 3 exist
        let limited = find_zlib_streams_limited(&blob, 2);
        assert!(limited.is_truncated);
        assert!(!limited.cancelled);
        assert_eq!(limited.matches.len(), 2);

        // Exact limit match across multiple segments must NOT report `is_truncated = true`
        let split_point = off2 + s2.len() / 2;
        let ranges = vec![0..split_point, split_point..blob.len()];
        let exact = find_zlib_streams_segmented(&blob, &ranges, 2, None);
        assert!(!exact.is_truncated);
        assert_eq!(exact.matches.len(), 2);
        assert_eq!(exact.matches[0].offset, 0);
        assert_eq!(exact.matches[1].offset, off3);

        // Limit = 1 with 1 match in segment 0 and 1 match in segment 1 MUST report `is_truncated = true`
        let cross_seg_truncated = find_zlib_streams_segmented(&blob, &ranges, 1, None);
        assert!(cross_seg_truncated.is_truncated);
        assert_eq!(cross_seg_truncated.matches.len(), 1);
        assert_eq!(cross_seg_truncated.matches[0].offset, 0);
    }

    #[test]
    fn honors_cooperative_cancellation_for_scan_and_decompress() {
        let payload = b"cancellable zlib stream payload";
        let compressed = compress_zlib(payload, Compression::default());
        let cancel = AtomicBool::new(true);

        let scan = find_zlib_streams_segmented(&compressed, &[], 10, Some(&cancel));
        assert!(scan.cancelled);
        assert!(!scan.is_truncated);
        assert!(scan.matches.is_empty());

        let decomp = decompress_zlib_at_cancellable(&compressed, 0, Some(compressed.len()), Some(payload.len()), Some(&cancel));
        assert_eq!(decomp, Err(ZlibError::Cancelled));
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
