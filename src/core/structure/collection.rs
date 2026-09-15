use crate::core::structure::types::ParsedField;
use std::sync::Arc;

/// A node in the persistent, append-only root field collection.
///
/// The collection uses a binary-counter forest. Appending a chunk copies only
/// the small forest of roots (`O(log n)`) and shares all existing field data.
/// This avoids copying an ever-growing `Vec<Arc<[ParsedField]>>` on every
/// progress update.
#[derive(Debug)]
enum FieldCollectionNode {
    Chunk(Arc<[ParsedField]>),
    Concat {
        left: Arc<FieldCollectionNode>,
        right: Arc<FieldCollectionNode>,
        len: usize,
    },
}

impl FieldCollectionNode {
    fn len(&self) -> usize {
        match self {
            Self::Chunk(fields) => fields.len(),
            Self::Concat { len, .. } => *len,
        }
    }
}

/// Append-only collection used for parse snapshots.
///
/// Each snapshot shares the already parsed chunks instead of cloning the
/// complete root field vector. Snapshots are persistent, so append and drop
/// remain proportional to the number of chunk roots rather than the number of
/// chunks already parsed.
#[derive(Debug, Clone, Default)]
pub struct FieldCollection {
    roots: Arc<Vec<Option<Arc<FieldCollectionNode>>>>,
    len: usize,
}

impl FieldCollection {
    /// Creates a collection containing one owned chunk.
    pub fn from_vec(fields: Vec<ParsedField>) -> Self {
        if fields.is_empty() {
            return Self::default();
        }

        let chunk: Arc<[ParsedField]> = Arc::from(fields.into_boxed_slice());
        Self::from_shared_chunks(std::slice::from_ref(&chunk))
    }

    /// Returns a new collection with an additional shared chunk.
    pub fn append_chunk(&self, fields: Vec<ParsedField>) -> Self {
        if fields.is_empty() {
            return self.clone();
        }

        let chunk: Arc<[ParsedField]> = Arc::from(fields.into_boxed_slice());
        self.append_shared_chunks(std::slice::from_ref(&chunk))
    }

    /// Returns a new collection with additional shared chunks.
    pub fn append_shared_chunks(&self, chunks: &[Arc<[ParsedField]>]) -> Self {
        let mut result = self.clone();
        for chunk in chunks.iter().filter(|chunk| !chunk.is_empty()) {
            result = result.append_node(Arc::new(FieldCollectionNode::Chunk(chunk.clone())), chunk.len());
        }
        result
    }

    fn from_shared_chunks(chunks: &[Arc<[ParsedField]>]) -> Self {
        Self::default().append_shared_chunks(chunks)
    }

    fn append_node(&self, mut node: Arc<FieldCollectionNode>, node_len: usize) -> Self {
        let mut roots = (*self.roots).clone();
        let mut root_index = 0;

        loop {
            if root_index == roots.len() {
                roots.push(None);
            }

            if let Some(left) = roots[root_index].take() {
                let len = left.len() + node.len();
                node = Arc::new(FieldCollectionNode::Concat { left, right: node, len });
                root_index += 1;
            } else {
                roots[root_index] = Some(node);
                break;
            }
        }

        Self {
            roots: Arc::new(roots),
            len: self.len + node_len,
        }
    }

    /// Returns the number of root fields.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns whether the collection has no root fields.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns a root field by index.
    pub fn get(&self, index: usize) -> Option<&ParsedField> {
        if index >= self.len {
            return None;
        }

        let mut index = index;
        for root in self.roots.iter().rev().flatten() {
            if index < root.len() {
                return Self::get_from_node(root, index);
            }
            index -= root.len();
        }
        None
    }

    fn get_from_node(mut node: &FieldCollectionNode, mut index: usize) -> Option<&ParsedField> {
        loop {
            match node {
                FieldCollectionNode::Chunk(fields) => return fields.get(index),
                FieldCollectionNode::Concat { left, right, .. } => {
                    if index < left.len() {
                        node = left.as_ref();
                    } else {
                        index -= left.len();
                        node = right.as_ref();
                    }
                }
            }
        }
    }

    /// Iterates over root fields in parse order.
    pub fn iter(&self) -> FieldCollectionIter<'_> {
        self.iter_from(0)
    }

    /// Iterates from a root-field index without walking the skipped fields.
    pub fn iter_from(&self, index: usize) -> FieldCollectionIter<'_> {
        let mut stack = Vec::new();
        let mut current = None;
        if index < self.len {
            let mut remaining = index;
            let mut selected_root = None;
            for (root_index, root) in self.roots.iter().enumerate().rev() {
                let Some(root) = root else { continue };
                if remaining < root.len() {
                    selected_root = Some(root_index);
                    break;
                }
                remaining -= root.len();
            }

            if let Some(selected_root) = selected_root {
                // Lower roots contain later fields. Push them first so the
                // selected root remains on top of the iterator stack.
                for root in self.roots[..selected_root].iter().flatten() {
                    stack.push((root.as_ref(), 0));
                }
                if let Some(root) = &self.roots[selected_root] {
                    if let FieldCollectionNode::Chunk(fields) = root.as_ref() {
                        // The common final-result representation is one flat
                        // chunk. Keep it in the iterator's current slot so
                        // iterating it does not allocate a traversal stack.
                        current = Some((fields.as_ref(), remaining));
                    } else {
                        Self::push_from_node(root.as_ref(), remaining, &mut stack);
                    }
                }
            }
        }

        FieldCollectionIter { stack, current }
    }

    fn push_from_node<'a>(mut node: &'a FieldCollectionNode, mut index: usize, stack: &mut Vec<(&'a FieldCollectionNode, usize)>) {
        loop {
            match node {
                FieldCollectionNode::Chunk(_) => {
                    stack.push((node, index));
                    return;
                }
                FieldCollectionNode::Concat { left, right, .. } => {
                    if index < left.len() {
                        stack.push((right.as_ref(), 0));
                        node = left.as_ref();
                    } else {
                        index -= left.len();
                        node = right.as_ref();
                    }
                }
            }
        }
    }
}

/// Iterator over the shared chunks in a [`FieldCollection`].
pub struct FieldCollectionIter<'a> {
    stack: Vec<(&'a FieldCollectionNode, usize)>,
    current: Option<(&'a [ParsedField], usize)>,
}

impl<'a> Iterator for FieldCollectionIter<'a> {
    type Item = &'a ParsedField;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some((fields, index)) = self.current.as_mut() {
                if let Some(field) = fields.get(*index) {
                    *index += 1;
                    return Some(field);
                }
                self.current = None;
            }

            let (node, index) = self.stack.pop()?;
            match node {
                FieldCollectionNode::Chunk(fields) => {
                    self.current = Some((fields.as_ref(), index));
                }
                FieldCollectionNode::Concat { left, right, .. } => {
                    self.stack.push((right.as_ref(), 0));
                    self.stack.push((left.as_ref(), index));
                }
            }
        }
    }
}

impl std::ops::Index<usize> for FieldCollection {
    type Output = ParsedField;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).unwrap_or_else(|| panic!("field index {index} out of bounds"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::color::RgbaColor;
    use crate::core::structure::types::FieldValue;

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
    fn field_collection_appends_shared_chunks_in_parse_order() {
        let first: Arc<[ParsedField]> = Arc::from(vec![test_field("a", 0), test_field("b", 1)].into_boxed_slice());
        let second: Arc<[ParsedField]> = Arc::from(vec![test_field("c", 2)].into_boxed_slice());
        let third: Arc<[ParsedField]> = Arc::from(vec![test_field("d", 3), test_field("e", 4), test_field("f", 5)].into_boxed_slice());

        let collection = FieldCollection::default().append_shared_chunks(&[first, second, third]);

        assert_eq!(collection.len(), 6);
        assert_eq!(collection.get(0).map(|field| field.id.as_str()), Some("a"));
        assert_eq!(collection.get(5).map(|field| field.id.as_str()), Some("f"));
        assert!(collection.get(6).is_none());

        let all_ids: Vec<_> = collection.iter().map(|field| field.id.as_str()).collect();
        assert_eq!(all_ids, ["a", "b", "c", "d", "e", "f"]);

        let tail_ids: Vec<_> = collection.iter_from(2).map(|field| field.id.as_str()).collect();
        assert_eq!(tail_ids, ["c", "d", "e", "f"]);
        assert!(collection.iter_from(collection.len()).next().is_none());

        let mut many = FieldCollection::default();
        for index in 0..97 {
            many = many.append_chunk(vec![test_field(&format!("field_{index}"), index)]);
        }
        for start in 0..=many.len() {
            let actual: Vec<_> = many.iter_from(start).map(|field| field.offset).collect();
            let expected: Vec<_> = (start..many.len()).collect();
            assert_eq!(actual, expected, "iterator start index {start}");
        }
    }
}
