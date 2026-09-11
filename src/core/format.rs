pub mod base64;
pub mod copy;
pub mod display;
pub mod intel_hex;
pub mod raw_binary;
pub mod srec;

pub use base64::*;
pub use copy::*;
pub use display::*;
pub use intel_hex::*;
pub use raw_binary::*;
pub use srec::*;

use crate::core::address_map::{AddressMap, HexFormatOptions, MemorySegment};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Identifies the file format used when opening or importing a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileFormat {
    /// Raw binary file (opened without record-format decoding).
    #[default]
    Binary,
    /// Intel HEX file format.
    IntelHex,
    /// Motorola S-Record file format.
    #[serde(alias = "motorola_s19", alias = "motorola_s28", alias = "motorola_s37")]
    MotorolaSrec,
    /// Generic / auto-detected Motorola S-Record or Intel HEX format.
    HexOrMot,
    /// Base64 encoded file format.
    Base64,
}

impl FileFormat {
    /// Returns true if this format represents an imported hex/record format rather than raw binary.
    pub fn is_import(&self) -> bool {
        matches!(self, Self::IntelHex | Self::MotorolaSrec | Self::HexOrMot | Self::Base64)
    }

    /// Short badge label for displaying in lists (e.g. Recents).
    pub fn badge_text(&self) -> &'static str {
        match self {
            Self::Binary => "",
            Self::IntelHex => "HEX",
            Self::MotorolaSrec => "SREC",
            Self::HexOrMot => "HEX",
            Self::Base64 => "B64",
        }
    }

    /// Human-readable label describing the format.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Binary => "Binary",
            Self::IntelHex => "Intel HEX",
            Self::MotorolaSrec => "Motorola S-Record",
            Self::HexOrMot => "Motorola S-Record / Intel HEX",
            Self::Base64 => "Base64",
        }
    }
}

impl From<HexFormat> for FileFormat {
    fn from(format: HexFormat) -> Self {
        match format {
            HexFormat::MotorolaS19 | HexFormat::MotorolaS28 | HexFormat::MotorolaS37 => Self::MotorolaSrec,
            HexFormat::IntelHex => Self::IntelHex,
        }
    }
}

/// Identifies the format of the parsed hex/mot file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HexFormat {
    MotorolaS19,
    MotorolaS28,
    MotorolaS37,
    IntelHex,
}

impl HexFormat {
    pub fn label(&self) -> &'static str {
        match self {
            HexFormat::MotorolaS19 => "Motorola S-Record (S19)",
            HexFormat::MotorolaS28 => "Motorola S-Record (S28)",
            HexFormat::MotorolaS37 => "Motorola S-Record (S37)",
            HexFormat::IntelHex => "Intel HEX",
        }
    }
}

/// Result of importing a Motorola S-Record or Intel HEX file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexImportResult {
    /// Combined contiguous data buffer of all segments
    pub data: Vec<u8>,
    /// Address map containing all memory segments and physical addresses
    pub address_map: AddressMap,
    /// Format of the imported file
    pub format: HexFormat,
    /// Optional execution start address / entry point
    pub entry_point: Option<usize>,
    /// Optional header string from S0 record
    pub header: Option<String>,
}

/// Error type for parsing S-Record or Intel HEX files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HexImportError {
    EmptyInput,
    InvalidRecordStart { line: usize, char: char },
    InvalidHexDigits { line: usize, content: String },
    LineTooShort { line: usize },
    ChecksumMismatch { line: usize, expected: u8, actual: u8 },
    UnsupportedRecordType { line: usize, record_type: String },
    NoDataRecords,
    UnknownFormat,
}

impl fmt::Display for HexImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HexImportError::EmptyInput => write!(f, "File is empty"),
            HexImportError::InvalidRecordStart { line, char } => {
                write!(f, "Line {}: invalid record start character '{}'", line, char)
            }
            HexImportError::InvalidHexDigits { line, content } => {
                write!(f, "Line {}: invalid hex string '{}'", line, content)
            }
            HexImportError::LineTooShort { line } => write!(f, "Line {}: line is too short", line),
            HexImportError::ChecksumMismatch { line, expected, actual } => {
                write!(
                    f,
                    "Line {}: checksum mismatch (calculated: 0x{:02X}, in file: 0x{:02X})",
                    line, expected, actual
                )
            }
            HexImportError::UnsupportedRecordType { line, record_type } => {
                write!(f, "Line {}: unsupported record type '{}'", line, record_type)
            }
            HexImportError::NoDataRecords => write!(f, "No data records found in file"),
            HexImportError::UnknownFormat => write!(f, "Unrecognized format (not valid Motorola S-Record or Intel HEX)"),
        }
    }
}

impl std::error::Error for HexImportError {}

/// Parse a two-character hex slice into a `u8`.
pub(crate) fn parse_hex_byte(s: &str, line_idx: usize) -> Result<u8, HexImportError> {
    u8::from_str_radix(s, 16).map_err(|_| HexImportError::InvalidHexDigits {
        line: line_idx + 1,
        content: s.to_string(),
    })
}

/// Assembles sorted data chunks into contiguous MemorySegments and compact buffer payload.
pub(crate) fn assemble_chunks_into_result(
    mut raw_chunks: Vec<(usize, Vec<u8>)>,
    format: HexFormat,
    entry_point: Option<usize>,
    header: Option<String>,
    format_options: HexFormatOptions,
) -> Result<HexImportResult, HexImportError> {
    // Sort chunks by starting address
    raw_chunks.sort_by_key(|(addr, _)| *addr);

    // Merge contiguous and overlapping chunks into distinct memory segments
    struct MergedBlock {
        address: usize,
        data: Vec<u8>,
    }

    let mut merged_blocks: Vec<MergedBlock> = Vec::new();

    for (chunk_addr, chunk_data) in raw_chunks {
        if let Some(last) = merged_blocks.last_mut() {
            let last_end = last.address + last.data.len();
            if chunk_addr <= last_end {
                // Overlap or strictly contiguous
                let offset_in_last = chunk_addr - last.address;
                let needed_len = offset_in_last + chunk_data.len();
                if needed_len > last.data.len() {
                    last.data.resize(needed_len, 0xFF);
                }
                last.data[offset_in_last..offset_in_last + chunk_data.len()].copy_from_slice(&chunk_data);
                continue;
            }
        }
        merged_blocks.push(MergedBlock {
            address: chunk_addr,
            data: chunk_data,
        });
    }

    if merged_blocks.is_empty() {
        return Err(HexImportError::NoDataRecords);
    }

    let total_bytes: usize = merged_blocks.iter().map(|b| b.data.len()).sum();
    let mut combined_data = Vec::with_capacity(total_bytes);
    let mut segments = Vec::with_capacity(merged_blocks.len());

    let mut current_buffer_offset = 0;
    for block in merged_blocks {
        let len = block.data.len();
        segments.push(MemorySegment {
            buffer_offset: current_buffer_offset,
            address: block.address,
            length: len,
        });
        combined_data.extend_from_slice(&block.data);
        current_buffer_offset += len;
    }

    Ok(HexImportResult {
        data: combined_data,
        address_map: AddressMap::from_segments_with_options(segments, format_options),
        format,
        entry_point,
        header,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_format_properties() {
        assert!(!FileFormat::Binary.is_import());
        assert!(FileFormat::IntelHex.is_import());
        assert!(FileFormat::MotorolaSrec.is_import());
        assert!(FileFormat::HexOrMot.is_import());
        assert!(FileFormat::Base64.is_import());

        assert_eq!(FileFormat::Binary.badge_text(), "");
        assert_eq!(FileFormat::IntelHex.badge_text(), "HEX");
        assert_eq!(FileFormat::MotorolaSrec.badge_text(), "SREC");
        assert_eq!(FileFormat::HexOrMot.badge_text(), "HEX");
        assert_eq!(FileFormat::Base64.badge_text(), "B64");

        assert_eq!(FileFormat::Base64.label(), "Base64");
    }

    #[test]
    fn test_file_format_serde() {
        let json = serde_json::to_string(&FileFormat::Base64).unwrap();
        assert_eq!(json, "\"base64\"");
        let deserialized: FileFormat = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, FileFormat::Base64);
    }
}
