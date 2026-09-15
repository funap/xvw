use crate::core::color::RgbaColor;
use crate::core::structure::types::{FieldValue, ParsedField};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::RwLock;

#[derive(Debug, Clone)]
pub struct ActiveStructRange {
    pub start: usize,
    pub end: usize,
    pub depth: usize,
    pub id: String,
}

/// Lightweight field data used by byte-range queries.
///
/// The index intentionally does not retain `children` or raw byte buffers.
/// Those values remain in `ParseResult::fields` and are only cloned when a
/// snapshot itself is created.
#[derive(Debug, Clone)]
pub struct IndexedField {
    pub id: String,
    pub offset: usize,
    pub size: usize,
    /// Nesting depth in the parsed structure tree.
    pub depth: usize,
    /// Whether this field is a computed/positioned instance rather than a physical field.
    pub is_instance: bool,
    type_name: String,
    expression: String,
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
            type_name: String::new(),
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

        StructureIndex {
            container_structs: self.container_structs,
            leaf_fields: self.leaf_fields,
            active_ranges: self.active_ranges,
            highlights: Arc::new(self.highlights),
            field_breaks: Arc::new(self.field_breaks),
            active_range_tree_base,
            active_range_max_tree,
        }
    }

    pub(crate) fn finish_live(mut self) -> LiveIndexBatch {
        self.container_structs.sort_by_key(|field| field.offset);
        self.leaf_fields.sort_by_key(|field| field.offset);
        LiveIndexBatch {
            container_structs: self.container_structs,
            leaf_fields: self.leaf_fields,
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
}
