use std::ops::Range;
use std::sync::Arc;

#[derive(Clone)]
pub enum BufferData {
    Memory(Arc<Vec<u8>>),
    Mmap(Arc<memmap2::Mmap>),
}

impl BufferData {
    pub fn as_slice(&self) -> &[u8] {
        match self {
            BufferData::Memory(vec) => vec.as_slice(),
            BufferData::Mmap(mmap) => mmap.as_ref(),
        }
    }

    pub fn make_mut(&mut self) -> &mut Vec<u8> {
        match self {
            BufferData::Memory(vec) => Arc::make_mut(vec),
            BufferData::Mmap(mmap) => {
                let vec = mmap.as_ref().to_vec();
                *self = BufferData::Memory(Arc::new(vec));
                if let BufferData::Memory(v) = self { Arc::make_mut(v) } else { unreachable!() }
            }
        }
    }
}

/// A buffer to hold the contents of a file.
#[allow(dead_code)]
#[derive(Clone)]
pub struct Buffer {
    data: BufferData,
}

#[allow(dead_code)]
impl Buffer {
    /// Creates a new Buffer with the given data.
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            data: BufferData::Memory(Arc::new(data)),
        }
    }

    /// Creates a new Buffer from a memory-mapped file.
    pub fn from_mmap(mmap: memmap2::Mmap) -> Self {
        Self {
            data: BufferData::Mmap(Arc::new(mmap)),
        }
    }

    /// Creates an empty Buffer.
    pub fn empty() -> Self {
        Self {
            data: BufferData::Memory(Arc::new(Vec::new())),
        }
    }

    /// Returns the length of the buffer.
    pub fn len(&self) -> usize {
        self.data.as_slice().len()
    }

    /// Returns true if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.data.as_slice().is_empty()
    }

    /// Returns a slice of the buffer's data.
    pub fn data(&self) -> &[u8] {
        self.data.as_slice()
    }

    /// Returns a slice of the buffer's data in the given range.
    /// If the range is out of bounds, it is clamped to the valid range.
    pub fn get_range(&self, start: usize, len: usize) -> &[u8] {
        let slice = self.data.as_slice();
        let start = start.min(slice.len());
        let end = start.saturating_add(len).min(slice.len());
        &slice[start..end]
    }

    /// Replaces one byte and returns its previous value.
    pub fn replace(&mut self, index: usize, byte: u8) -> Option<u8> {
        let vec = self.data.make_mut();
        let old = *vec.get(index)?;
        vec[index] = byte;
        Some(old)
    }

    /// Inserts a byte at the specified index.
    pub fn insert(&mut self, index: usize, byte: u8) {
        let vec = self.data.make_mut();
        if index <= vec.len() {
            vec.insert(index, byte);
        }
    }

    /// Inserts all `bytes` at `index`, clamping the index to the end of the buffer.
    pub fn insert_bytes(&mut self, index: usize, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let vec = self.data.make_mut();
        let index = index.min(vec.len());
        vec.splice(index..index, bytes.iter().copied());
    }

    /// Removes a byte at the specified index and returns it.
    pub fn remove(&mut self, index: usize) -> Option<u8> {
        let vec = self.data.make_mut();
        if index < vec.len() { Some(vec.remove(index)) } else { None }
    }

    /// Removes and returns the bytes in `range`, clamped to the buffer.
    pub fn remove_range(&mut self, range: Range<usize>) -> Vec<u8> {
        let vec = self.data.make_mut();
        let start = range.start.min(vec.len());
        let end = range.end.min(vec.len()).max(start);
        vec.drain(start..end).collect()
    }

    /// Replaces the bytes in `range` and returns the bytes that were removed.
    pub fn replace_range(&mut self, range: Range<usize>, bytes: &[u8]) -> Vec<u8> {
        let vec = self.data.make_mut();
        let start = range.start.min(vec.len());
        let end = range.end.min(vec.len()).max(start);
        let old = vec[start..end].to_vec();
        vec.splice(start..end, bytes.iter().copied());
        old
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let data = vec![1, 2, 3];
        let buffer = Buffer::new(data.clone());

        assert_eq!(buffer.data(), &data[..]);
        assert_eq!(buffer.len(), 3);
        assert!(!buffer.is_empty());
    }

    #[test]
    fn test_empty() {
        let buffer = Buffer::empty();
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn test_get_range() {
        let data = vec![10, 20, 30, 40, 50];
        let buffer = Buffer::new(data);

        // Valid range
        assert_eq!(buffer.get_range(1, 3), &[20, 30, 40]);

        // Range extending beyond end
        assert_eq!(buffer.get_range(3, 10), &[40, 50]);

        // Start beyond end
        assert_eq!(buffer.get_range(10, 5), &[] as &[u8]);

        // Empty range
        assert_eq!(buffer.get_range(0, 0), &[] as &[u8]);
    }

    #[test]
    fn test_buffer_insert_remove() {
        let mut buffer = Buffer::new(vec![1, 3]);
        buffer.insert(1, 2);
        assert_eq!(buffer.data(), &[1, 2, 3]);

        assert_eq!(buffer.remove(1), Some(2));
        assert_eq!(buffer.data(), &[1, 3]);

        assert_eq!(buffer.remove(10), None);
    }

    #[test]
    fn test_buffer_range_edits() {
        let mut buffer = Buffer::new(vec![1, 2, 3, 4]);

        assert_eq!(buffer.replace(1, 9), Some(2));
        assert_eq!(buffer.data(), &[1, 9, 3, 4]);

        buffer.insert_bytes(2, &[7, 8]);
        assert_eq!(buffer.data(), &[1, 9, 7, 8, 3, 4]);

        assert_eq!(buffer.remove_range(1..4), vec![9, 7, 8]);
        assert_eq!(buffer.data(), &[1, 3, 4]);

        assert_eq!(buffer.replace_range(1..3, &[5, 6, 7]), vec![3, 4]);
        assert_eq!(buffer.data(), &[1, 5, 6, 7]);
    }
}
