#![allow(dead_code)]

use crate::core::color::RgbaColor;
use crate::core::layout::{LineMap, build_line_map_from_sorted_events};
use crate::core::radix::DisplayRadix;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

pub use crate::core::structure::collection::FieldCollection;
pub use crate::core::structure::index::{ActiveStructRange, IndexedField, StructureIndex};
pub(crate) use crate::core::structure::index::{LiveStructureIndex, StructureIndexBuilder};

#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    Bool(bool),
    String(String),
    Bytes(Vec<u8>),
    Struct,
}

impl FieldValue {
    pub fn to_i64(&self) -> i64 {
        match self {
            FieldValue::U8(v) => *v as i64,
            FieldValue::U16(v) => *v as i64,
            FieldValue::U32(v) => *v as i64,
            FieldValue::U64(v) => *v as i64,
            FieldValue::I8(v) => *v as i64,
            FieldValue::I16(v) => *v as i64,
            FieldValue::I32(v) => *v as i64,
            FieldValue::I64(v) => *v,
            FieldValue::F32(v) => *v as i64,
            FieldValue::F64(v) => *v as i64,
            FieldValue::Bool(true) => 1,
            FieldValue::Bool(false) => 0,
            _ => 0,
        }
    }

    pub fn to_f64(&self) -> f64 {
        match self {
            FieldValue::U8(v) => *v as f64,
            FieldValue::U16(v) => *v as f64,
            FieldValue::U32(v) => *v as f64,
            FieldValue::U64(v) => *v as f64,
            FieldValue::I8(v) => *v as f64,
            FieldValue::I16(v) => *v as f64,
            FieldValue::I32(v) => *v as f64,
            FieldValue::I64(v) => *v as f64,
            FieldValue::F32(v) => *v as f64,
            FieldValue::F64(v) => *v,
            FieldValue::Bool(true) => 1.0,
            FieldValue::Bool(false) => 0.0,
            _ => 0.0,
        }
    }

    pub fn to_string_value(&self) -> String {
        match self {
            FieldValue::String(s) => s.clone(),
            FieldValue::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
            other => format!("{}", other),
        }
    }

    /// Formats numeric values in the requested radix while preserving the
    /// regular display for strings, byte arrays, structures, and non-integer
    /// values.
    pub fn format_with_radix(&self, radix: DisplayRadix) -> String {
        match self {
            FieldValue::U8(value) => Self::format_unsigned(*value as u64, radix),
            FieldValue::U16(value) => Self::format_unsigned(*value as u64, radix),
            FieldValue::U32(value) => Self::format_unsigned(*value as u64, radix),
            FieldValue::U64(value) => Self::format_unsigned(*value, radix),
            FieldValue::I8(value) => Self::format_signed(*value as i64, radix),
            FieldValue::I16(value) => Self::format_signed(*value as i64, radix),
            FieldValue::I32(value) => Self::format_signed(*value as i64, radix),
            FieldValue::I64(value) => Self::format_signed(*value, radix),
            other => other.to_string(),
        }
    }

    fn format_unsigned(value: u64, radix: DisplayRadix) -> String {
        match radix {
            DisplayRadix::Hexadecimal => format!("0x{value:X}"),
            DisplayRadix::Decimal => value.to_string(),
            DisplayRadix::Octal => format!("0o{value:o}"),
            DisplayRadix::Binary => format!("0b{value:b}"),
        }
    }

    fn format_signed(value: i64, radix: DisplayRadix) -> String {
        if radix == DisplayRadix::Decimal {
            return value.to_string();
        }

        let sign = if value.is_negative() { "-" } else { "" };
        let magnitude = value.unsigned_abs();
        match radix {
            DisplayRadix::Hexadecimal => format!("{sign}0x{magnitude:X}"),
            DisplayRadix::Octal => format!("{sign}0o{magnitude:o}"),
            DisplayRadix::Binary => format!("{sign}0b{magnitude:b}"),
            DisplayRadix::Decimal => unreachable!("decimal values return before radix formatting"),
        }
    }
}

impl std::fmt::Display for FieldValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FieldValue::U8(v) => write!(f, "{}", v),
            FieldValue::U16(v) => write!(f, "{}", v),
            FieldValue::U32(v) => write!(f, "{}", v),
            FieldValue::U64(v) => write!(f, "{}", v),
            FieldValue::I8(v) => write!(f, "{}", v),
            FieldValue::I16(v) => write!(f, "{}", v),
            FieldValue::I32(v) => write!(f, "{}", v),
            FieldValue::I64(v) => write!(f, "{}", v),
            FieldValue::F32(v) => write!(f, "{}", v),
            FieldValue::F64(v) => write!(f, "{}", v),
            FieldValue::Bool(v) => write!(f, "{}", v),
            FieldValue::String(v) => write!(f, "\"{}\"", v),
            FieldValue::Bytes(v) => write!(f, "[{} bytes]", v.len()),
            FieldValue::Struct => write!(f, "{{...}}"),
        }
    }
}

#[derive(Debug)]
pub struct ParsedField {
    pub id: String,
    pub field_type: String,
    pub offset: usize,
    pub size: usize,
    pub value: FieldValue,
    pub color: RgbaColor,
    pub description: Option<String>,
    pub children: Vec<ParsedField>,
    pub enum_label: Option<String>,
    pub is_instance: bool,
}

impl Clone for ParsedField {
    fn clone(&self) -> Self {
        struct Frame<'a> {
            source: &'a ParsedField,
            next_child: usize,
            cloned: ParsedField,
        }

        let mut frames = vec![Frame {
            source: self,
            next_child: 0,
            cloned: Self::clone_shallow(self),
        }];

        loop {
            let next_child = {
                let frame = frames.last_mut().expect("parsed-field clone stack must not be empty");
                if frame.next_child < frame.source.children.len() {
                    let index = frame.next_child;
                    frame.next_child += 1;
                    Some(index)
                } else {
                    None
                }
            };

            if let Some(index) = next_child {
                let child = {
                    let frame = frames.last().expect("parsed-field clone parent must exist");
                    &frame.source.children[index]
                };
                frames.push(Frame {
                    source: child,
                    next_child: 0,
                    cloned: Self::clone_shallow(child),
                });
                continue;
            }

            let completed = frames.pop().expect("parsed-field clone frame must exist").cloned;
            if let Some(parent) = frames.last_mut() {
                parent.cloned.children.push(completed);
            } else {
                return completed;
            }
        }
    }
}

impl ParsedField {
    fn clone_shallow(field: &Self) -> Self {
        Self {
            id: field.id.clone(),
            field_type: field.field_type.clone(),
            offset: field.offset,
            size: field.size,
            value: field.value.clone(),
            color: field.color,
            description: field.description.clone(),
            children: Vec::with_capacity(field.children.len()),
            enum_label: field.enum_label.clone(),
            is_instance: field.is_instance,
        }
    }
}

impl Drop for ParsedField {
    fn drop(&mut self) {
        // ParsedField is a recursive logical tree. Drain descendants on an
        // explicit heap-backed worklist so dropping a deeply nested Kaitai
        // result never consumes one stack frame per structure level.
        let mut pending = std::mem::take(&mut self.children);
        while let Some(mut field) = pending.pop() {
            pending.append(&mut field.children);
        }
    }
}

impl ParsedField {
    pub fn is_struct(&self) -> bool {
        !self.children.is_empty() || matches!(self.value, FieldValue::Struct)
    }

    pub fn format_expression(&self) -> String {
        if self.is_struct() {
            return self.id.clone();
        }
        if let Some(label) = &self.enum_label {
            return format!("{} = {} ({})", self.id, self.value, label);
        }
        match &self.value {
            FieldValue::String(s) => format!("{} = \"{}\"", self.id, s),
            FieldValue::Bytes(b) => format!("{} = [{} bytes]", self.id, b.len()),
            FieldValue::U8(v) => format!("{} = {:X}h ({})", self.id, v, v),
            FieldValue::U16(v) => format!("{} = {:X}h ({})", self.id, v, v),
            FieldValue::U32(v) => format!("{} = {:X}h ({})", self.id, v, v),
            FieldValue::U64(v) => format!("{} = {:X}h ({})", self.id, v, v),
            other => format!("{} = {}", self.id, other),
        }
    }

    pub fn format_comment(&self) -> Option<String> {
        if let Some(desc) = &self.description
            && !desc.is_empty()
        {
            return Some(desc.clone());
        }
        if let Some(label) = &self.enum_label {
            return Some(label.clone());
        }
        None
    }
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct ParseProgress {
    pub definition_id: String,
    /// Newly completed root fields since the previous progress notification.
    /// This is a delta, not a cumulative snapshot.
    pub fields: Arc<[ParsedField]>,
    pub parsed_offset: usize,
    pub total_bytes: usize,
    pub is_done: bool,
    /// True while the parser has reached the byte end but is preparing the
    /// final display index and line map.
    pub is_finalizing: bool,
    pub errors: Vec<ParseError>,
    pub parse_result: Option<Arc<ParseResult>>,
}

#[derive(Debug, Clone)]
pub struct ParseResult {
    pub definition_id: String,
    pub fields: FieldCollection,
    pub total_parsed_bytes: usize,
    pub errors: Vec<ParseError>,
    pub index: Arc<StructureIndex>,
    /// Background-prepared layout for the default expanded structure view.
    ///
    /// Custom joins/breaks and collapsed structures are intentionally not
    /// included. The editor falls back to the existing dynamic layout for
    /// those cases, so this cache cannot change their display semantics.
    pub structure_line_map: Option<Arc<LineMap>>,
    live_index: Option<Arc<LiveStructureIndex>>,
}

impl ParseResult {
    pub fn new(definition_id: String, fields: Vec<ParsedField>, total_parsed_bytes: usize, errors: Vec<ParseError>) -> Self {
        let fields = FieldCollection::from_vec(fields);
        let mut index_builder = StructureIndexBuilder::new();
        for field in fields.iter() {
            index_builder.add_field(field);
        }
        Self::new_with_index(definition_id, fields, total_parsed_bytes, errors, index_builder.finish())
    }

    pub(crate) fn new_with_index(
        definition_id: String,
        fields: FieldCollection,
        total_parsed_bytes: usize,
        errors: Vec<ParseError>,
        index: StructureIndex,
    ) -> Self {
        Self {
            definition_id,
            fields,
            total_parsed_bytes,
            errors,
            index: Arc::new(index),
            structure_line_map: None,
            live_index: None,
        }
    }

    /// Creates an empty result that can receive incremental parse batches.
    pub fn empty(definition_id: String) -> Self {
        Self {
            definition_id,
            fields: FieldCollection::default(),
            total_parsed_bytes: 0,
            errors: Vec::new(),
            index: Arc::new(StructureIndex::default()),
            structure_line_map: None,
            live_index: Some(Arc::new(LiveStructureIndex::default())),
        }
    }

    /// Prepares the default expanded structure line map off the UI thread.
    pub fn with_structure_line_map(mut self, total_size: usize) -> Self {
        let field_breaks = self.index.field_breaks.as_ref();
        let mut structure_headers = BTreeMap::new();
        self.collect_structure_header_lines(&mut structure_headers, &HashSet::new());
        self.structure_line_map = Some(Arc::new(build_line_map_from_sorted_events(
            total_size,
            field_breaks,
            &Default::default(),
            &structure_headers,
        )));
        self
    }

    /// Appends a field batch while sharing all previously parsed field chunks.
    pub fn append_fields(&self, fields: Vec<ParsedField>, total_parsed_bytes: usize) -> Self {
        let fields = self.fields.append_chunk(fields);
        let mut index_builder = StructureIndexBuilder::new();
        for field in fields.iter() {
            index_builder.add_field(field);
        }
        Self::new_with_index(
            self.definition_id.clone(),
            fields,
            total_parsed_bytes,
            self.errors.clone(),
            index_builder.finish(),
        )
    }

    /// Appends a field batch to a live parse snapshot.
    ///
    /// Only the new chunk is indexed. Previously received chunks keep their
    /// own small index, so the UI never rebuilds the complete parse result.
    pub fn append_fields_without_index(&self, fields: Vec<ParsedField>, total_parsed_bytes: usize) -> Self {
        let chunk: Arc<[ParsedField]> = Arc::from(fields.into_boxed_slice());
        self.append_shared_chunks_without_index(std::slice::from_ref(&chunk), total_parsed_bytes)
    }

    /// Appends shared parse chunks without rebuilding the complete byte-range index.
    pub fn append_shared_chunks_without_index(&self, chunks: &[Arc<[ParsedField]>], total_parsed_bytes: usize) -> Self {
        let fields = self.fields.append_shared_chunks(chunks);
        let live_index = self.live_index.clone().unwrap_or_else(|| Arc::new(LiveStructureIndex::default()));
        live_index.append_chunks(chunks);
        Self {
            definition_id: self.definition_id.clone(),
            fields,
            total_parsed_bytes,
            errors: self.errors.clone(),
            index: self.index.clone(),
            structure_line_map: None,
            live_index: Some(live_index),
        }
    }

    /// Returns whether this result is an incremental snapshot.
    pub fn is_live(&self) -> bool {
        self.live_index.is_some()
    }

    /// Finds container descriptions in an incremental snapshot.
    pub fn find_live_container_structs_starting_at(&self, start_offset: usize, len: usize) -> Vec<IndexedField> {
        self.live_index
            .as_ref()
            .map(|index| index.find_container_structs_starting_at(start_offset, len))
            .unwrap_or_default()
    }

    /// Finds leaf descriptions in an incremental snapshot.
    pub fn find_live_leaf_fields_starting_at(&self, start_offset: usize, len: usize) -> Vec<IndexedField> {
        self.live_index
            .as_ref()
            .map(|index| index.find_leaf_fields_starting_at(start_offset, len))
            .unwrap_or_default()
    }

    pub fn build_index(&mut self) {
        let mut index_builder = StructureIndexBuilder::new();
        for field in self.fields.iter() {
            index_builder.add_field(field);
        }
        self.index = Arc::new(index_builder.finish());
        self.structure_line_map = None;
        self.live_index = None;
    }

    pub fn to_highlights(&self) -> Vec<(std::ops::Range<usize>, RgbaColor)> {
        self.index.highlights.as_ref().clone()
    }

    pub fn collect_field_breaks(&self, breaks: &mut Vec<usize>, collapsed_structs: &std::collections::HashSet<String>) {
        if collapsed_structs.is_empty() {
            breaks.extend(self.index.field_breaks.iter().copied());
            return;
        }

        let mut work = Vec::new();
        for field in self.fields.iter() {
            work.push(field);
            while let Some(field) = work.pop() {
                // Sequence fields define physical stream boundaries and line breaks.
                // Instance fields (computed values or pos-peeks) do not break the physical stream.
                if !field.is_instance && field.size > 0 {
                    breaks.push(field.offset);
                    breaks.push(field.offset + field.size);
                }
                if !field.children.is_empty() && !collapsed_structs.contains(&field.id) {
                    for child in field.children.iter().rev() {
                        work.push(child);
                    }
                }
            }
        }
    }

    /// Adds one description-only row for each visible physical structure.
    pub fn collect_structure_header_lines(&self, empty_lines: &mut BTreeMap<usize, usize>, collapsed_structs: &HashSet<String>) {
        if collapsed_structs.is_empty() {
            for header in self.index.container_structs.iter().filter(|field| !field.is_instance && field.size > 0) {
                *empty_lines.entry(header.offset).or_default() += 1;
            }
            return;
        }

        let mut work = self.fields.iter().map(|field| (field, false)).collect::<Vec<_>>();
        while let Some((field, hidden_by_collapsed_parent)) = work.pop() {
            let is_collapsed = collapsed_structs.contains(&field.id);
            if field.is_struct() && !field.is_instance && field.size > 0 && !hidden_by_collapsed_parent && !is_collapsed {
                *empty_lines.entry(field.offset).or_default() += 1;
            }

            if !hidden_by_collapsed_parent && !is_collapsed {
                for child in field.children.iter().rev() {
                    work.push((child, false));
                }
            }
        }
    }

    /// Returns whether a visible structure starts at `offset`.
    pub fn has_structure_header_at(&self, offset: usize, collapsed_structs: &HashSet<String>) -> bool {
        self.index
            .container_structs
            .iter()
            .any(|header| header.offset == offset && header.size > 0 && !header.is_instance && !collapsed_structs.contains(&header.id))
    }

    pub fn find_container_structs_starting_at(&self, start_offset: usize, len: usize) -> &[IndexedField] {
        let end_offset = start_offset.saturating_add(len);
        let containers = &self.index.container_structs;
        let start_idx = containers.partition_point(|f| f.offset < start_offset);
        let end_idx = start_idx + containers[start_idx..].partition_point(|f| f.offset < end_offset);
        &containers[start_idx..end_idx]
    }

    pub fn find_leaf_fields_starting_at(&self, start_offset: usize, len: usize) -> &[IndexedField] {
        let end_offset = start_offset.saturating_add(len);
        let leaves = &self.index.leaf_fields;
        let start_idx = leaves.partition_point(|f| f.offset < start_offset);
        let end_idx = start_idx + leaves[start_idx..].partition_point(|f| f.offset < end_offset);
        &leaves[start_idx..end_idx]
    }

    pub fn find_active_struct_ranges(&self, start_offset: usize, len: usize) -> Vec<&ActiveStructRange> {
        let row_end = start_offset.saturating_add(len);
        let ranges = &self.index.active_ranges;
        let mut result = Vec::with_capacity(8);
        if ranges.is_empty() {
            return result;
        }

        let tree = &self.index.active_range_max_tree;
        let mut work = vec![(1, 0, self.index.active_range_tree_base)];
        while let Some((node, segment_start, segment_end)) = work.pop() {
            if segment_start >= ranges.len() || ranges[segment_start].start >= row_end || tree[node] <= start_offset {
                continue;
            }

            if segment_end - segment_start == 1 {
                let range = &ranges[segment_start];
                if range.start < row_end && range.end > start_offset {
                    result.push(range);
                }
                continue;
            }

            let middle = segment_start + (segment_end - segment_start) / 2;
            // Push right first so results remain sorted by the range start.
            work.push((node * 2 + 1, middle, segment_end));
            work.push((node * 2, segment_start, middle));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_field(id: &str, offset: usize) -> ParsedField {
        ParsedField {
            id: id.into(),
            field_type: "u1".into(),
            offset,
            size: 1,
            value: FieldValue::U8(offset as u8),
            color: RgbaColor::default(),
            description: None,
            children: Vec::new(),
            enum_label: None,
            is_instance: false,
        }
    }

    #[test]
    fn live_parse_index_exposes_only_received_chunks() {
        let mut first_container = test_field("header", 0);
        first_container.size = 2;
        first_container.value = FieldValue::Struct;
        first_container.children = vec![test_field("magic", 0)];

        let first: Arc<[ParsedField]> = Arc::from(vec![first_container].into_boxed_slice());
        let second: Arc<[ParsedField]> = Arc::from(vec![test_field("tail", 2)].into_boxed_slice());

        let partial = ParseResult::empty("live".into()).append_shared_chunks_without_index(&[first], 2);
        assert!(partial.is_live());
        assert_eq!(partial.find_live_container_structs_starting_at(0, 2)[0].id, "header");
        assert_eq!(partial.find_live_leaf_fields_starting_at(0, 2)[0].id, "magic");
        assert!(partial.find_live_leaf_fields_starting_at(2, 1).is_empty());

        let partial = partial.append_shared_chunks_without_index(&[second], 3);
        assert_eq!(partial.find_live_leaf_fields_starting_at(2, 1)[0].id, "tail");

        let complete = ParseResult::new("complete".into(), Vec::new(), 0, Vec::new());
        assert!(!complete.is_live());
    }

    #[test]
    fn field_value_conversions_cover_numeric_text_and_struct_values() {
        assert_eq!(FieldValue::U8(0x12).to_i64(), 0x12);
        assert_eq!(FieldValue::I64(-4).to_i64(), -4);
        assert_eq!(FieldValue::F32(2.75).to_i64(), 2);
        assert_eq!(FieldValue::Bool(true).to_i64(), 1);
        assert_eq!(FieldValue::Bool(false).to_f64(), 0.0);
        assert_eq!(FieldValue::U16(12).to_f64(), 12.0);
        assert_eq!(FieldValue::String("text".into()).to_string_value(), "text");
        assert_eq!(FieldValue::Bytes(vec![b'A', b'\0']).to_string_value(), "A\0");
        assert_eq!(FieldValue::Struct.to_string_value(), "{...}");
        assert_eq!(FieldValue::U16(0x2A).format_with_radix(DisplayRadix::Binary), "0b101010");
        assert_eq!(FieldValue::U16(0x2A).format_with_radix(DisplayRadix::Octal), "0o52");
        assert_eq!(FieldValue::U16(0x2A).format_with_radix(DisplayRadix::Decimal), "42");
        assert_eq!(FieldValue::U16(0x2A).format_with_radix(DisplayRadix::Hexadecimal), "0x2A");
        assert_eq!(FieldValue::I8(-10).format_with_radix(DisplayRadix::Hexadecimal), "-0xA");
    }

    #[test]
    fn parsed_field_formatting_prefers_structure_enum_and_description_details() {
        let numeric = ParsedField {
            id: "flags".into(),
            field_type: "u1".into(),
            offset: 0,
            size: 1,
            value: FieldValue::U8(0xAB),
            color: RgbaColor::default(),
            description: None,
            children: Vec::new(),
            enum_label: None,
            is_instance: false,
        };
        assert_eq!(numeric.format_expression(), "flags = ABh (171)");
        assert_eq!(numeric.format_comment(), None);

        let mut enum_field = numeric.clone();
        enum_field.enum_label = Some("enabled".into());
        enum_field.description = Some("flag description".into());
        assert_eq!(enum_field.format_expression(), "flags = 171 (enabled)");
        assert_eq!(enum_field.format_comment(), Some("flag description".into()));

        let structure = ParsedField {
            id: "header".into(),
            field_type: "header".into(),
            offset: 0,
            size: 4,
            value: FieldValue::Struct,
            color: RgbaColor::default(),
            description: None,
            children: vec![numeric],
            enum_label: None,
            is_instance: false,
        };
        assert!(structure.is_struct());
        assert_eq!(structure.format_expression(), "header");
    }

    #[test]
    fn deep_structure_index_walks_without_call_stack_growth() {
        let depth = 256;
        let mut field = test_field("leaf", 0);
        for level in (0..depth).rev() {
            field = ParsedField {
                id: format!("node_{level}"),
                field_type: "nested".into(),
                offset: 0,
                size: 1,
                value: FieldValue::Struct,
                color: RgbaColor::default(),
                description: None,
                children: vec![field],
                enum_label: None,
                is_instance: false,
            };
        }

        // The result is dropped normally at the end of the test. ParsedField's
        // custom destructor must keep that operation off the call stack too.
        let result = ParseResult::new("deep".into(), vec![field], 1, Vec::new());

        let mut collapsed = HashSet::new();
        collapsed.insert("unrelated".to_string());
        let mut breaks = Vec::new();
        result.collect_field_breaks(&mut breaks, &collapsed);
        assert_eq!(breaks.len(), (depth + 1) * 2);

        let ranges = result.find_active_struct_ranges(0, 1);
        assert_eq!(ranges.len(), depth);
        assert_eq!(ranges.first().map(|range| range.depth), Some(0));
        assert_eq!(ranges.last().map(|range| range.depth), Some(depth - 1));

        let cloned = result.fields.get(0).expect("deep root field").clone();
        assert_eq!(cloned.children.len(), 1);
    }
}
