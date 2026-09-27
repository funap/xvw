//! Export helpers for structure-analysis results.
//!
//! The parser's field tree is intentionally kept out of the UI export code.
//! These functions only read an immutable [`ParseResult`] snapshot, so callers
//! can run them on a background executor while parsing or rendering continues.

use super::types::{FieldValue, ParseResult, ParsedField};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

pub const DEFAULT_STRUCTURE_YAML_SHA_THRESHOLD: usize = 32;
pub const MAX_STRUCTURE_YAML_SHA_THRESHOLD: usize = 10_000_000;
pub const DEFAULT_STRUCTURE_YAML_INCLUDE_OFFSETS: bool = true;

/// Threshold in bytes at or above which binary data is serialized as a SHA-256 digest in YAML export.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StructureYamlShaThreshold(pub usize);

impl Default for StructureYamlShaThreshold {
    fn default() -> Self {
        Self(DEFAULT_STRUCTURE_YAML_SHA_THRESHOLD)
    }
}

impl std::ops::Deref for StructureYamlShaThreshold {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<usize> for StructureYamlShaThreshold {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

/// Setting for whether to include field byte offsets in YAML structure export.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StructureYamlIncludeOffsets(pub bool);

impl Default for StructureYamlIncludeOffsets {
    fn default() -> Self {
        Self(DEFAULT_STRUCTURE_YAML_INCLUDE_OFFSETS)
    }
}

impl std::ops::Deref for StructureYamlIncludeOffsets {
    type Target = bool;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<bool> for StructureYamlIncludeOffsets {
    fn from(value: bool) -> Self {
        Self(value)
    }
}

/// Options controlling structure YAML export formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct YamlExportOptions {
    /// Minimum byte count at which binary data is serialized as a SHA-256 digest
    /// rather than raw lower-case hex digits.
    pub binary_sha256_threshold: usize,
    /// Whether to include field byte offsets in the exported YAML.
    /// Disabling offsets produces cleaner diffs when variable-sized fields shift
    /// subsequent byte positions.
    pub include_offsets: bool,
}

impl Default for YamlExportOptions {
    fn default() -> Self {
        Self {
            binary_sha256_threshold: DEFAULT_STRUCTURE_YAML_SHA_THRESHOLD,
            include_offsets: DEFAULT_STRUCTURE_YAML_INCLUDE_OFFSETS,
        }
    }
}

/// Formats binary data for YAML structure export.
///
/// If `data.len() >= sha256_threshold`, outputs `"sha256:<hex_digest>"`.
/// Otherwise, outputs the raw bytes as lower-case hexadecimal digits.
/// For binary fields exceeding 16 bytes, lines are wrapped every 16 bytes
/// (32 hex characters) to facilitate line-based text diffing.
pub fn format_bytes_for_yaml(data: &[u8], sha256_threshold: usize) -> String {
    if data.len() >= sha256_threshold {
        let digest = crate::core::checksum::sha256(data);
        let mut s = String::with_capacity(7 + 64);
        s.push_str("sha256:");
        for byte in digest {
            let _ = write!(s, "{byte:02x}");
        }
        s
    } else if data.len() > 16 {
        let mut s = String::with_capacity(data.len() * 2 + (data.len() / 16) + 1);
        for (i, byte) in data.iter().enumerate() {
            let _ = write!(s, "{byte:02x}");
            if (i + 1) % 16 == 0 && (i + 1) < data.len() {
                s.push('\n');
            }
        }
        s
    } else {
        let mut s = String::with_capacity(data.len() * 2);
        for byte in data {
            let _ = write!(s, "{byte:02x}");
        }
        s
    }
}

/// Formats a structure-analysis snapshot as readable, Wireshark-like indented text.
///
/// Traversal is iterative rather than recursive. A malformed or deliberately
/// deep definition therefore cannot overflow the stack while being copied.
pub fn format_parse_result_as_text(result: &ParseResult) -> String {
    let mut output = String::with_capacity(256);

    let root_fields: Vec<_> = result.fields.iter().collect();
    let mut stack = Vec::with_capacity(root_fields.len());
    for field in root_fields.into_iter().rev() {
        stack.push((field, 0usize));
    }

    while let Some((field, depth)) = stack.pop() {
        let indent = "    ".repeat(depth);
        let is_container = field.is_struct() || !field.children.is_empty();
        let instance_marker = if field.is_instance { " [instance]" } else { "" };

        if is_container {
            if !field.field_type.is_empty() && field.field_type != "struct" {
                let _ = writeln!(output, "{}{}: {}{}", indent, field.id, field.field_type, instance_marker);
            } else {
                let _ = writeln!(output, "{}{}{}", indent, field.id, instance_marker);
            }
        } else {
            let _ = writeln!(output, "{}{}: {}{}", indent, field.id, format_field_value(field), instance_marker);
        }

        if let Some(description) = non_empty(field.description.as_deref()) {
            let _ = writeln!(output, "{}    [{}]", indent, description);
        }

        for child in field.children.iter().rev() {
            stack.push((child, depth + 1));
        }
    }

    if !result.errors.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str("[Parse errors]\n");
        for error in &result.errors {
            let _ = writeln!(output, "    [Offset 0x{:X}: {}]", error.offset, error.message);
        }
    }

    output
}

/// Formats a structure-analysis snapshot as a YAML document using default export options.
#[allow(dead_code)]
pub fn format_parse_result_as_yaml(result: &ParseResult) -> Result<String, serde_yaml::Error> {
    format_parse_result_as_yaml_with_options(result, YamlExportOptions::default())
}

/// Formats a structure-analysis snapshot as a YAML document using custom export options.
pub fn format_parse_result_as_yaml_with_options(result: &ParseResult, options: YamlExportOptions) -> Result<String, serde_yaml::Error> {
    let mut field_count = 0;
    let mut fields = Vec::with_capacity(result.fields.len());
    for field in result.fields.iter() {
        fields.push(convert_field_to_yaml(field, &mut field_count, options));
    }

    let document = YamlStructureExport {
        format_version: 1,
        definition_id: result.definition_id.clone(),
        status: if result.is_live() { "in_progress" } else { "complete" }.to_string(),
        parsed_bytes: result.total_parsed_bytes,
        root_field_count: result.fields.len(),
        field_count,
        error_count: result.errors.len(),
        fields,
        errors: result
            .errors
            .iter()
            .map(|error| YamlParseError {
                message: error.message.clone(),
                offset: error.offset,
            })
            .collect(),
    };

    serde_yaml::to_string(&document)
}

fn convert_field_to_yaml(root: &ParsedField, field_count: &mut usize, options: YamlExportOptions) -> YamlField {
    struct Frame<'a> {
        field: &'a ParsedField,
        next_child: usize,
        converted: YamlField,
    }

    let create_yaml_field = |field: &ParsedField| -> YamlField {
        let value = if field.is_struct() && !field.children.is_empty() {
            None
        } else {
            Some(format_yaml_field_value(field, options.binary_sha256_threshold))
        };

        let field_type = if field.field_type.is_empty() {
            if field.children.is_empty() {
                "value".to_string()
            } else {
                "struct".to_string()
            }
        } else {
            field.field_type.clone()
        };

        let offset = if options.include_offsets { Some(field.offset) } else { None };

        YamlField {
            id: field.id.clone(),
            field_type,
            offset,
            size: field.size,
            value,
            is_instance: field.is_instance,
            description: field.description.clone().filter(|value| !value.is_empty()),
            enum_label: field.enum_label.clone(),
            children: Vec::with_capacity(field.children.len()),
        }
    };

    *field_count += 1;
    let mut frames = vec![Frame {
        field: root,
        next_child: 0,
        converted: create_yaml_field(root),
    }];

    loop {
        let next_child = {
            let frame = frames.last_mut().expect("yaml frame stack must not be empty");
            if frame.next_child < frame.field.children.len() {
                let idx = frame.next_child;
                frame.next_child += 1;
                Some(idx)
            } else {
                None
            }
        };

        if let Some(idx) = next_child {
            let child = {
                let frame = frames.last().expect("parent frame must exist");
                &frame.field.children[idx]
            };
            *field_count += 1;
            frames.push(Frame {
                field: child,
                next_child: 0,
                converted: create_yaml_field(child),
            });
            continue;
        }

        let completed = frames.pop().expect("frame must exist").converted;
        if let Some(parent) = frames.last_mut() {
            parent.converted.children.push(completed);
        } else {
            return completed;
        }
    }
}

fn format_yaml_field_value(field: &ParsedField, sha256_threshold: usize) -> String {
    let mut value = match &field.value {
        FieldValue::U8(value) => format!("{:X}h ({value})", value),
        FieldValue::U16(value) => format!("{:X}h ({value})", value),
        FieldValue::U32(value) => format!("{:X}h ({value})", value),
        FieldValue::U64(value) => format!("{:X}h ({value})", value),
        FieldValue::I8(value) => value.to_string(),
        FieldValue::I16(value) => value.to_string(),
        FieldValue::I32(value) => value.to_string(),
        FieldValue::I64(value) => value.to_string(),
        FieldValue::F32(value) => value.to_string(),
        FieldValue::F64(value) => value.to_string(),
        FieldValue::Bool(value) => value.to_string(),
        FieldValue::String(value) => format!("{value:?}"),
        FieldValue::Bytes(value) => format_bytes_for_yaml(value, sha256_threshold),
        FieldValue::Struct => "{...}".to_string(),
    };

    if let Some(label) = &field.enum_label {
        let _ = write!(value, " ({label})");
    }
    value
}

fn format_field_value(field: &ParsedField) -> String {
    let mut value = match &field.value {
        FieldValue::U8(value) => format!("{:X}h ({value})", value),
        FieldValue::U16(value) => format!("{:X}h ({value})", value),
        FieldValue::U32(value) => format!("{:X}h ({value})", value),
        FieldValue::U64(value) => format!("{:X}h ({value})", value),
        FieldValue::I8(value) => value.to_string(),
        FieldValue::I16(value) => value.to_string(),
        FieldValue::I32(value) => value.to_string(),
        FieldValue::I64(value) => value.to_string(),
        FieldValue::F32(value) => value.to_string(),
        FieldValue::F64(value) => value.to_string(),
        FieldValue::Bool(value) => value.to_string(),
        FieldValue::String(value) => format!("{value:?}"),
        FieldValue::Bytes(value) => format!("[{} bytes]", value.len()),
        FieldValue::Struct => "{...}".to_string(),
    };

    if let Some(label) = &field.enum_label {
        let _ = write!(value, " ({label})");
    }
    value
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

#[derive(Debug, Serialize)]
pub struct YamlStructureExport {
    pub format_version: u8,
    pub definition_id: String,
    pub status: String,
    pub parsed_bytes: usize,
    pub root_field_count: usize,
    pub field_count: usize,
    pub error_count: usize,
    pub fields: Vec<YamlField>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<YamlParseError>,
}

#[derive(Debug, Serialize)]
pub struct YamlField {
    pub id: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
    pub size: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_instance: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_label: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<YamlField>,
}

#[derive(Debug, Serialize)]
pub struct YamlParseError {
    pub message: String,
    pub offset: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(id: &str, field_type: &str, offset: usize, value: FieldValue) -> ParsedField {
        ParsedField {
            id: id.to_string(),
            field_type: field_type.to_string(),
            offset,
            size: 1,
            value,
            color: crate::core::color::RgbaColor::default(),
            description: None,
            children: Vec::new(),
            enum_label: None,
            is_instance: false,
        }
    }

    #[test]
    fn text_export_preserves_hierarchy_and_metadata() {
        let mut header = field("header", "local_file_header", 0, FieldValue::Struct);
        header.size = 3;
        header.description = Some("Local header".to_string());
        header.children.push(field("magic", "u2", 0, FieldValue::U16(0x4B50)));

        let mut flags_field = field("flags", "u2", 2, FieldValue::U16(0x0002));
        flags_field.enum_label = Some("Don't fragment".to_string());
        header.children.push(flags_field);

        let mut instance_field = field("calculated_crc", "u4", 0, FieldValue::U32(0x12345678));
        instance_field.is_instance = true;
        header.children.push(instance_field);

        let result = ParseResult::new(
            "local_file".to_string(),
            vec![header],
            3,
            vec![crate::core::structure::types::ParseError {
                offset: 10,
                message: "unexpected EOF".to_string(),
            }],
        );
        let text = format_parse_result_as_text(&result);

        assert_eq!(
            text,
            "header: local_file_header\n    [Local header]\n    magic: 4B50h (19280)\n    flags: 2h (2) (Don't fragment)\n    calculated_crc: 12345678h (305419896) [instance]\n\n[Parse errors]\n    [Offset 0xA: unexpected EOF]\n"
        );
    }

    #[test]
    fn yaml_export_preserves_tree_structure() {
        let mut root = field("body", "local_file", 0, FieldValue::Struct);
        let mut child = field("size", "u4", 0, FieldValue::U32(8));
        child.enum_label = Some("Eight bytes".to_string());
        root.children.push(child);

        let result = ParseResult::new("pk_section".to_string(), vec![root], 4, Vec::new());

        let yaml = format_parse_result_as_yaml(&result).expect("structure YAML should serialize");
        let value: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("structure YAML should parse");
        assert_eq!(value["definition_id"].as_str(), Some("pk_section"));
        assert_eq!(value["field_count"].as_u64(), Some(2));
        assert_eq!(value["fields"][0]["id"].as_str(), Some("body"));
        assert_eq!(value["fields"][0]["type"].as_str(), Some("local_file"));
        assert_eq!(value["fields"][0]["children"][0]["id"].as_str(), Some("size"));
        assert_eq!(value["fields"][0]["children"][0]["type"].as_str(), Some("u4"));
        assert_eq!(value["fields"][0]["children"][0]["value"].as_str(), Some("8h (8) (Eight bytes)"));
        assert_eq!(value["fields"][0]["children"][0]["enum_label"].as_str(), Some("Eight bytes"));
    }

    #[test]
    fn format_bytes_for_yaml_hex_and_sha256() {
        let sample = [0x01, 0x02, 0x03, 0x04, 0x0a, 0x0b, 0x0c, 0x0d, 0x0f];
        // Below threshold: hex string
        assert_eq!(format_bytes_for_yaml(&sample, 10), "010203040a0b0c0d0f");
        assert_eq!(format_bytes_for_yaml(&sample, 32), "010203040a0b0c0d0f");

        // At or above threshold: sha256:<digest>
        let sha = format_bytes_for_yaml(&sample, 9);
        assert!(sha.starts_with("sha256:"));
        assert_eq!(sha.len(), 7 + 64);

        let expected_digest = crate::core::checksum::sha256(&sample);
        let expected_sha = format!("sha256:{}", expected_digest.iter().map(|b| format!("{b:02x}")).collect::<String>());
        assert_eq!(sha, expected_sha);

        // Empty slice below threshold: empty string
        assert_eq!(format_bytes_for_yaml(&[], 1), "");
        // Empty slice at threshold 0: sha256 of empty
        let empty_sha = format_bytes_for_yaml(&[], 0);
        assert!(empty_sha.starts_with("sha256:"));
    }

    #[test]
    fn yaml_export_formats_binary_fields_based_on_threshold() {
        let small_bytes = vec![0x01, 0x02, 0x03, 0x04];
        let large_bytes = vec![0xaa; 40];

        let mut root = field("packet", "packet_t", 0, FieldValue::Struct);
        root.children.push(field("small", "bytes", 0, FieldValue::Bytes(small_bytes.clone())));
        root.children.push(field("large", "bytes", 4, FieldValue::Bytes(large_bytes.clone())));

        let result = ParseResult::new("test_bin".to_string(), vec![root], 44, Vec::new());

        // Default threshold is 32: small is hex, large is sha256
        let yaml_default = format_parse_result_as_yaml(&result).expect("serialize with default options");
        let val_default: serde_yaml::Value = serde_yaml::from_str(&yaml_default).expect("parse yaml");
        assert_eq!(val_default["fields"][0]["children"][0]["value"].as_str(), Some("01020304"));
        let large_val = val_default["fields"][0]["children"][1]["value"].as_str().unwrap();
        assert!(large_val.starts_with("sha256:"));

        // Custom threshold = 2: small is also sha256
        let yaml_threshold_2 = format_parse_result_as_yaml_with_options(
            &result,
            YamlExportOptions {
                binary_sha256_threshold: 2,
                include_offsets: true,
            },
        )
        .expect("serialize with threshold 2");
        let val_threshold_2: serde_yaml::Value = serde_yaml::from_str(&yaml_threshold_2).expect("parse yaml");
        let small_val = val_threshold_2["fields"][0]["children"][0]["value"].as_str().unwrap();
        assert!(small_val.starts_with("sha256:"));

        // Custom threshold = 100: large is also hex, formatted with 16-byte lines
        let yaml_threshold_100 = format_parse_result_as_yaml_with_options(
            &result,
            YamlExportOptions {
                binary_sha256_threshold: 100,
                include_offsets: true,
            },
        )
        .expect("serialize with threshold 100");
        let val_threshold_100: serde_yaml::Value = serde_yaml::from_str(&yaml_threshold_100).expect("parse yaml");
        assert_eq!(val_threshold_100["fields"][0]["children"][0]["value"].as_str(), Some("01020304"));
        let expected_large_hex = format!("{}\n{}\n{}", "aa".repeat(16), "aa".repeat(16), "aa".repeat(8));
        assert_eq!(
            val_threshold_100["fields"][0]["children"][1]["value"].as_str(),
            Some(expected_large_hex.as_str())
        );
    }

    #[test]
    fn yaml_export_omits_offsets_when_disabled() {
        let mut root = field("block", "block_t", 100, FieldValue::Struct);
        root.children.push(field("data", "bytes", 100, FieldValue::Bytes(vec![0x11, 0x22])));

        let result = ParseResult::new("test_offsets".to_string(), vec![root], 102, Vec::new());

        // When include_offsets is true (default)
        let yaml_with_offsets = format_parse_result_as_yaml(&result).expect("serialize with offsets");
        let val_with_offsets: serde_yaml::Value = serde_yaml::from_str(&yaml_with_offsets).expect("parse");
        assert_eq!(val_with_offsets["fields"][0]["offset"].as_u64(), Some(100));
        assert_eq!(val_with_offsets["fields"][0]["children"][0]["offset"].as_u64(), Some(100));

        // When include_offsets is false
        let yaml_without_offsets = format_parse_result_as_yaml_with_options(
            &result,
            YamlExportOptions {
                binary_sha256_threshold: 32,
                include_offsets: false,
            },
        )
        .expect("serialize without offsets");
        let val_without_offsets: serde_yaml::Value = serde_yaml::from_str(&yaml_without_offsets).expect("parse");
        assert!(val_without_offsets["fields"][0].get("offset").is_none());
        assert!(val_without_offsets["fields"][0]["children"][0].get("offset").is_none());
        assert_eq!(val_without_offsets["fields"][0]["children"][0]["value"].as_str(), Some("1122"));
    }
}
