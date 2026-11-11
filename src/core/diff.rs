use std::cmp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffChunk {
    Equal { offset: usize, length: usize },
    Modified { offset: usize, length: usize },
}

#[derive(Debug, Clone)]
pub struct DiffResult {
    pub chunks: Vec<DiffChunk>,
    pub total_differences: usize,
}

impl DiffResult {
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            total_differences: 0,
        }
    }

    pub fn add_chunk(&mut self, chunk: DiffChunk) {
        if let DiffChunk::Modified { length, .. } = &chunk {
            self.total_differences += length;
        }
        self.chunks.push(chunk);
    }
}

impl Default for DiffResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Diff comparison mode specifying how positions are matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffAlignment {
    /// Compares sequential offsets directly from 0 to max(len).
    #[default]
    LinearOffset,
}

pub fn compute_simple_diff(left: &[u8], right: &[u8]) -> DiffResult {
    compute_aligned_diff(left, right, DiffAlignment::LinearOffset)
}

/// Computes differences between two byte slices according to alignment mode.
pub fn compute_aligned_diff(left: &[u8], right: &[u8], alignment: DiffAlignment) -> DiffResult {
    match alignment {
        DiffAlignment::LinearOffset => {
            let mut result = DiffResult::new();
            let min_len = cmp::min(left.len(), right.len());
            let max_len = cmp::max(left.len(), right.len());

            let mut offset = 0;
            while offset < max_len {
                let mut equal_start = offset;

                while equal_start < min_len && left[equal_start] == right[equal_start] {
                    equal_start += 1;
                }

                if equal_start > offset {
                    result.add_chunk(DiffChunk::Equal {
                        offset,
                        length: equal_start - offset,
                    });
                    offset = equal_start;
                }

                if offset >= min_len {
                    break;
                }

                let mut modified_start = offset;
                while modified_start < min_len && left[modified_start] != right[modified_start] {
                    modified_start += 1;
                }

                if modified_start > offset {
                    result.add_chunk(DiffChunk::Modified {
                        offset,
                        length: modified_start - offset,
                    });
                    offset = modified_start;
                }
            }

            if left.len() != right.len() {
                result.add_chunk(DiffChunk::Modified {
                    offset: min_len,
                    length: max_len - min_len,
                });
            }

            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_files() {
        let left = b"Hello World";
        let right = b"Hello World";
        let result = compute_simple_diff(left, right);

        assert_eq!(result.total_differences, 0);
        assert_eq!(result.chunks.len(), 1);
        assert_eq!(result.chunks[0], DiffChunk::Equal { offset: 0, length: 11 });
    }

    #[test]
    fn test_completely_different() {
        let left = b"AAAA";
        let right = b"BBBB";
        let result = compute_simple_diff(left, right);

        assert_eq!(result.total_differences, 4);
        assert_eq!(result.chunks.len(), 1);
        assert_eq!(result.chunks[0], DiffChunk::Modified { offset: 0, length: 4 });
    }

    #[test]
    fn test_partial_difference() {
        let left = b"Hello World";
        let right = b"Hello Rust!";
        let result = compute_simple_diff(left, right);

        assert!(result.total_differences > 0);
        assert!(result.chunks.len() >= 2);
    }

    #[test]
    fn test_different_lengths() {
        let left = b"Short";
        let right = b"Short and long";
        let result = compute_simple_diff(left, right);

        assert_eq!(result.total_differences, 9);
    }

    #[test]
    fn test_empty_files() {
        let left = b"";
        let right = b"";
        let result = compute_simple_diff(left, right);

        assert_eq!(result.total_differences, 0);
        assert_eq!(result.chunks.len(), 0);
    }

    #[test]
    fn test_empty_left_non_empty_right() {
        let left = b"";
        let right = b"12345";
        let result = compute_simple_diff(left, right);

        assert_eq!(result.total_differences, 5);
        assert_eq!(result.chunks.len(), 1);
        assert_eq!(result.chunks[0], DiffChunk::Modified { offset: 0, length: 5 });
    }

    #[test]
    fn test_interleaving_modifications() {
        let left = b"A_B_C_D";
        let right = b"A!B!C!D";
        let result = compute_simple_diff(left, right);

        assert_eq!(result.total_differences, 3);
        assert_eq!(result.chunks.len(), 7);
    }
}
