use crate::core::color::RgbaColor;
use crate::core::structure::types::{FieldValue, ParsedField};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::RwLock;

#[derive(Debug, Clone)]
pub struct ActiveStructRange {
    pub start: usize,
    pub end: usize,
    #[allow(dead_code)]
    pub depth: usize,
    #[allow(dead_code)]
    pub id: String,
}

/// Lightweight field data used by byte-range queries.
///
/// The index intentionally does not retain `children` or raw byte buffers.
/// Those values remain in `ParseResult::fields` and are only cloned when a
/// snapshot itself is created.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedField {
    pub id: String,
    pub offset: usize,
    pub size: usize,
    /// Nesting depth in the parsed structure tree.
    pub depth: usize,
    /// Whether this field is a computed/positioned instance rather than a physical field.
    pub is_instance: bool,
    pub type_name: String,
    pub collapse_key: String,
    pub formatted_value: String,
    pub enum_label: Option<String>,
    pub description: Option<String>,
    pub color: Option<RgbaColor>,
    pub validation_failed: bool,
    pub expression: String,
}

impl IndexedField {
    pub(crate) fn container(field: &ParsedField, depth: usize) -> Self {
        Self {
            id: field.id.clone(),
            offset: field.offset,
            size: field.size,
            depth,
            is_instance: field.is_instance,
            type_name: field.field_type.clone(),
            collapse_key: crate::core::structure::types::struct_collapse_key(field.offset, depth, &field.id),
            formatted_value: String::new(),
            enum_label: field.enum_label.clone(),
            description: field.description.clone(),
            color: Some(field.color),
            validation_failed: false,
            expression: String::new(),
        }
    }

    pub(crate) fn leaf(field: &ParsedField, depth: usize) -> Self {
        Self {
            id: field.id.clone(),
            offset: field.offset,
            size: field.size,
            depth,
            is_instance: field.is_instance,
            type_name: field.field_type.clone(),
            collapse_key: crate::core::structure::types::struct_collapse_key(field.offset, depth, &field.id),
            formatted_value: field.format_value_compact(),
            enum_label: field.enum_label.clone(),
            description: field.description.clone(),
            color: Some(field.color),
            validation_failed: false,
            expression: field.format_expression(),
        }
    }

    /// Returns the field label used for a structure description.
    ///
    /// A switched custom type is stored as the resolved type name in
    /// `ParsedField.field_type`, while `id` remains the field name (`body` in
    /// a ZIP section). Keeping both makes the selected case visible without
    /// changing IDs used by collapse and range lookup logic.
    pub fn format_container_label(&self) -> String {
        if self.type_name.is_empty() || self.type_name == self.id {
            self.id.clone()
        } else {
            format!("{}: {}", self.id, self.type_name)
        }
    }

    /// Returns the preformatted expression used by the description column.
    pub fn format_expression(&self) -> &str {
        &self.expression
    }
}

#[derive(Debug, Clone, Default)]
pub struct StructureIndex {
    pub container_structs: Vec<IndexedField>,
    pub leaf_fields: Vec<IndexedField>,
    pub active_ranges: Vec<ActiveStructRange>,
    pub highlights: Arc<Vec<(std::ops::Range<usize>, RgbaColor)>>,
    /// Sorted physical field boundaries used by the inline line layout.
    pub field_breaks: Arc<Vec<usize>>,
    pub(crate) active_range_tree_base: usize,
    pub(crate) active_range_max_tree: Vec<usize>,
    pub(crate) leaf_prefix_max_end: Vec<usize>,
    pub(crate) leaf_tree_base: usize,
    pub(crate) leaf_max_tree: Vec<usize>,
}

impl StructureIndex {
    pub fn find_deepest_leaf_at(&self, offset: usize) -> Option<&IndexedField> {
        let idx = self.leaf_fields.partition_point(|f| f.offset <= offset);
        if idx == 0 || self.leaf_prefix_max_end.get(idx - 1).is_none_or(|&max_end| max_end <= offset) {
            return None;
        }

        // Fast path for non-overlapping leaf fields: when no earlier leaf extends past `offset`,
        // `leaf_fields[idx - 1]` is guaranteed to be the sole leaf covering `offset`.
        let prev_max = if idx >= 2 { self.leaf_prefix_max_end[idx - 2] } else { 0 };
        if prev_max <= offset {
            return Some(&self.leaf_fields[idx - 1]);
        }

        let mut best: Option<&IndexedField> = None;
        let tree = &self.leaf_max_tree;
        if tree.is_empty() {
            for (i, field) in self.leaf_fields[..idx].iter().enumerate().rev() {
                if self.leaf_prefix_max_end[i] <= offset {
                    break;
                }
                if offset < field.offset + field.size && best.is_none_or(|b| field.depth > b.depth) {
                    best = Some(field);
                }
            }
            return best;
        }

        let mut work = [(0usize, 0usize, 0usize); 64];
        let mut work_len = 1;
        work[0] = (1, 0, self.leaf_tree_base);
        while work_len > 0 {
            work_len -= 1;
            let (node, segment_start, segment_end) = work[work_len];
            if segment_start >= idx || tree[node] <= offset {
                continue;
            }

            if segment_end - segment_start == 1 {
                let field = &self.leaf_fields[segment_start];
                if offset < field.offset + field.size && best.is_none_or(|b| field.depth > b.depth) {
                    best = Some(field);
                }
                continue;
            }

            let middle = segment_start + (segment_end - segment_start) / 2;
            if work_len + 2 <= work.len() {
                // Push left first, then right so right is popped first (matching .iter().rev() order).
                work[work_len] = (node * 2, segment_start, middle);
                work[work_len + 1] = (node * 2 + 1, middle, segment_end);
                work_len += 2;
            }
        }
        best
    }

    pub fn find_container_headers_at(&self, offset: usize) -> Vec<&IndexedField> {
        let containers = &self.container_structs;
        let start_idx = containers.partition_point(|f| f.offset < offset);
        let end_idx = start_idx + containers[start_idx..].partition_point(|f| f.offset <= offset);
        containers[start_idx..end_idx]
            .iter()
            .filter(|f| f.offset == offset && !f.is_instance && f.size > 0)
            .collect()
    }
}

/// Incrementally collects the byte-range data needed by structure rendering.
///
/// The parser receives completed fields in batches. Collecting each batch
/// here avoids traversing the complete field tree again when the final parse
/// result is published. Ordering and deduplication are intentionally deferred
/// to [`Self::finish`] because instance fields may point backwards in the
/// stream.
#[derive(Debug, Default)]
pub(crate) struct StructureIndexBuilder {
    highlights: Vec<(std::ops::Range<usize>, RgbaColor)>,
    container_structs: Vec<IndexedField>,
    leaf_fields: Vec<IndexedField>,
    active_ranges: Vec<ActiveStructRange>,
    field_breaks: Vec<usize>,
    /// Interns IDs so duplicate-detection keys do not clone the full string
    /// for every parsed field.
    id_keys: HashMap<String, usize>,
    container_seen: HashSet<(usize, usize, usize)>,
    leaf_seen: HashSet<(usize, usize, usize)>,
    range_seen: HashSet<(usize, usize, usize)>,
    collect_layout: bool,
}

impl StructureIndexBuilder {
    pub(crate) fn new() -> Self {
        Self {
            collect_layout: true,
            ..Self::default()
        }
    }

    pub(crate) fn new_live() -> Self {
        Self::default()
    }

    /// Adds one newly completed root field and all of its children.
    pub(crate) fn add_field(&mut self, field: &ParsedField) {
        self.collect_index_field(field, 0);
    }

    fn id_key(&mut self, id: &str) -> usize {
        if let Some(&key) = self.id_keys.get(id) {
            return key;
        }

        let key = self.id_keys.len();
        self.id_keys.insert(id.to_owned(), key);
        key
    }

    /// Finalizes ordering and lookup metadata without revisiting the fields.
    pub(crate) fn finish(mut self) -> StructureIndex {
        self.highlights.sort_unstable_by_key(|(range, _)| range.start);
        // The lookup helpers below use `partition_point`, so both collections
        // must be ordered by file offset. Structure traversal is normally in
        // stream order, but `pos`-based instances are appended after `seq`
        // fields and can point backwards in the file.
        self.container_structs.sort_by_key(|field| field.offset);
        self.leaf_fields.sort_by_key(|field| field.offset);
        self.active_ranges.sort_unstable_by_key(|r| r.start);
        self.field_breaks.sort_unstable();
        self.field_breaks.dedup();
        let (active_range_tree_base, active_range_max_tree) = Self::build_active_range_tree(&self.active_ranges);
        let leaf_prefix_max_end = Self::build_leaf_prefix_max_end(&self.leaf_fields);
        let (leaf_tree_base, leaf_max_tree) = Self::build_leaf_max_tree(&self.leaf_fields);

        StructureIndex {
            container_structs: self.container_structs,
            leaf_fields: self.leaf_fields,
            active_ranges: self.active_ranges,
            highlights: Arc::new(self.highlights),
            field_breaks: Arc::new(self.field_breaks),
            active_range_tree_base,
            active_range_max_tree,
            leaf_prefix_max_end,
            leaf_tree_base,
            leaf_max_tree,
        }
    }

    pub(crate) fn finish_live(mut self) -> LiveIndexBatch {
        self.container_structs.sort_by_key(|field| field.offset);
        self.leaf_fields.sort_by_key(|field| field.offset);
        let leaf_prefix_max_end = Self::build_leaf_prefix_max_end(&self.leaf_fields);
        LiveIndexBatch {
            container_structs: self.container_structs,
            leaf_fields: self.leaf_fields,
            leaf_prefix_max_end,
        }
    }

    fn collect_index_field(&mut self, field: &ParsedField, depth: usize) {
        let mut work = vec![(field, depth)];

        while let Some((field, depth)) = work.pop() {
            let is_str = field.is_struct();
            if self.collect_layout && !field.is_instance && field.size > 0 {
                self.field_breaks.push(field.offset);
                self.field_breaks.push(field.offset + field.size);
            }
            if self.collect_layout && field.size > 0 && !is_str {
                self.highlights.push((field.offset..field.offset + field.size, field.color));
            }

            if is_str {
                let end = field.offset + field.size;
                let id_key = self.id_key(&field.id);
                if self.container_seen.insert((field.offset, field.size, id_key)) {
                    self.container_structs.push(IndexedField::container(field, depth));
                }
                if self.collect_layout && self.range_seen.insert((field.offset, end, id_key)) {
                    self.active_ranges.push(ActiveStructRange {
                        start: field.offset,
                        end,
                        depth,
                        id: field.id.clone(),
                    });
                }
            } else if field.size > 0 && field.children.is_empty() && !matches!(field.value, FieldValue::Struct) {
                let id_key = self.id_key(&field.id);
                if self.leaf_seen.insert((field.offset, field.size, id_key)) {
                    self.leaf_fields.push(IndexedField::leaf(field, depth));
                }
            }

            // LIFO traversal needs children in reverse order to retain the
            // same source order as the former recursive implementation.
            for child in field.children.iter().rev() {
                work.push((child, depth + 1));
            }
        }
    }

    fn build_active_range_tree(ranges: &[ActiveStructRange]) -> (usize, Vec<usize>) {
        let base = ranges.len().max(1).next_power_of_two();
        let mut tree = vec![0; base * 2];

        for (index, range) in ranges.iter().enumerate() {
            tree[base + index] = range.end;
        }
        for index in (1..base).rev() {
            tree[index] = tree[index * 2].max(tree[index * 2 + 1]);
        }

        (base, tree)
    }

    fn build_leaf_prefix_max_end(leaves: &[IndexedField]) -> Vec<usize> {
        let mut prefix = Vec::with_capacity(leaves.len());
        let mut running_max = 0;
        for leaf in leaves {
            running_max = running_max.max(leaf.offset + leaf.size);
            prefix.push(running_max);
        }
        prefix
    }

    fn build_leaf_max_tree(leaves: &[IndexedField]) -> (usize, Vec<usize>) {
        if leaves.is_empty() {
            return (0, Vec::new());
        }
        let base = leaves.len().next_power_of_two();
        let mut tree = vec![0; base * 2];
        for (index, leaf) in leaves.iter().enumerate() {
            tree[base + index] = leaf.offset + leaf.size;
        }
        for index in (1..base).rev() {
            tree[index] = tree[index * 2].max(tree[index * 2 + 1]);
        }
        (base, tree)
    }
}

/// Indexes newly parsed chunks for the live structure view.
///
/// The final parser index is intentionally built once, on the parser thread.
/// During parsing the UI only needs descriptions for the chunks that have
/// arrived so far. Keeping one small, sorted index per received chunk avoids
/// rebuilding or cloning the complete index on every progress update.
#[derive(Debug)]
pub(crate) struct LiveIndexBatch {
    container_structs: Vec<IndexedField>,
    leaf_fields: Vec<IndexedField>,
    leaf_prefix_max_end: Vec<usize>,
}

#[derive(Debug, Default)]
pub(crate) struct LiveStructureIndex {
    batches: RwLock<Vec<Arc<LiveIndexBatch>>>,
}

impl LiveStructureIndex {
    pub(crate) fn append_chunks(&self, chunks: &[Arc<[ParsedField]>]) {
        let mut batches = Vec::with_capacity(chunks.len());
        for chunk in chunks.iter().filter(|chunk| !chunk.is_empty()) {
            let mut builder = StructureIndexBuilder::new_live();
            for field in chunk.iter() {
                builder.add_field(field);
            }
            batches.push(Arc::new(builder.finish_live()));
        }

        if !batches.is_empty() {
            self.batches.write().expect("live structure index write lock").extend(batches);
        }
    }

    fn fields_starting_at(&self, start_offset: usize, len: usize, select: fn(&LiveIndexBatch) -> &[IndexedField]) -> Vec<IndexedField> {
        if len == 0 {
            return Vec::new();
        }

        let end_offset = start_offset.saturating_add(len);
        let batches = self.batches.read().expect("live structure index read lock");
        let mut fields = Vec::new();
        for batch in batches.iter() {
            let fields_in_batch = select(batch);
            let start_idx = fields_in_batch.partition_point(|field| field.offset < start_offset);
            let end_idx = start_idx + fields_in_batch[start_idx..].partition_point(|field| field.offset < end_offset);
            fields.extend(fields_in_batch[start_idx..end_idx].iter().cloned());
        }

        fields.sort_by(|left, right| (left.offset, left.depth, left.size, &left.id).cmp(&(right.offset, right.depth, right.size, &right.id)));
        fields.dedup_by(|right, left| right.offset == left.offset && right.size == left.size && right.id == left.id);
        fields
    }

    pub(crate) fn find_container_structs_starting_at(&self, start_offset: usize, len: usize) -> Vec<IndexedField> {
        self.fields_starting_at(start_offset, len, |index| &index.container_structs)
    }

    pub(crate) fn find_leaf_fields_starting_at(&self, start_offset: usize, len: usize) -> Vec<IndexedField> {
        self.fields_starting_at(start_offset, len, |index| &index.leaf_fields)
    }

    pub(crate) fn find_deepest_leaf_at(&self, offset: usize) -> Option<IndexedField> {
        let batches = self.batches.read().expect("live structure index read lock");
        let mut best: Option<IndexedField> = None;
        for batch in batches.iter() {
            let leaves = &batch.leaf_fields;
            let idx = leaves.partition_point(|f| f.offset <= offset);
            for (i, field) in leaves[..idx].iter().enumerate().rev() {
                if batch.leaf_prefix_max_end[i] <= offset {
                    break;
                }
                if offset < field.offset + field.size && best.as_ref().is_none_or(|b| field.depth > b.depth) {
                    best = Some(field.clone());
                }
            }
        }
        best
    }
}
