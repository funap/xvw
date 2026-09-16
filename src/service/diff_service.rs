use crate::core::diff::{DiffResult, compute_simple_diff};
use crate::core::document::Document;
use gpui_kit::{App, Task};
use std::sync::{Arc, RwLock};

/// A service for asynchronously computing diffs between documents.
/// A service for asynchronously computing diffs between documents.
#[derive(Debug, Clone, Default)]
pub struct DiffService;

impl DiffService {
    /// Creates a new `DiffService` instance.
    pub fn new() -> Self {
        Self
    }

    /// Asynchronously computes a simple byte diff between two documents.
    ///
    /// Locks on the documents are held only briefly to snapshot their buffers,
    /// ensuring background diff computation does not block other readers or writers.
    pub fn compute_diff(&self, left: Arc<RwLock<Document>>, right: Arc<RwLock<Document>>, cx: &App) -> Task<DiffResult> {
        let (left_buf, right_buf) = {
            let left_doc = left.read().expect("left document read lock");
            let right_doc = right.read().expect("right document read lock");
            (left_doc.buffer.clone(), right_doc.buffer.clone())
        };
        cx.background_executor()
            .spawn(async move { compute_simple_diff(left_buf.data(), right_buf.data()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::buffer::Buffer;
    use std::path::PathBuf;

    #[test]
    fn test_diff_service_new() {
        let service = DiffService::new();
        let _cloned = service.clone();
        assert_eq!(format!("{service:?}"), "DiffService");
    }

    #[test]
    fn test_compute_diff_snapshot() {
        let left_doc = Arc::new(RwLock::new(Document::new(PathBuf::from("left.bin"), Buffer::new(vec![0x01, 0x02, 0x03]))));
        let right_doc = Arc::new(RwLock::new(Document::new(PathBuf::from("right.bin"), Buffer::new(vec![0x01, 0xFF, 0x03]))));

        let left_buf = left_doc.read().unwrap().buffer.clone();
        let right_buf = right_doc.read().unwrap().buffer.clone();
        let diff = compute_simple_diff(left_buf.data(), right_buf.data());

        assert_eq!(diff.chunks.len(), 3);
    }
}
