use crate::core::diff::{DiffResult, compute_simple_diff};
use crate::core::document::Document;
use gpui_kit::{App, Task};
use std::sync::{Arc, RwLock};

/// A service for asynchronously computing diffs between documents.
#[derive(Clone, Default)]
pub struct DiffService;

impl DiffService {
    /// Creates a new DiffService instance.
    pub fn new() -> Self {
        Self
    }

    /// Asynchronously computes a simple byte diff between two documents.
    pub fn compute_diff(&self, left: Arc<RwLock<Document>>, right: Arc<RwLock<Document>>, cx: &App) -> Task<DiffResult> {
        cx.background_executor().spawn(async move {
            let left_doc = left.read().expect("left document read lock");
            let right_doc = right.read().expect("right document read lock");
            let left_data = left_doc.buffer.data();
            let right_data = right_doc.buffer.data();
            compute_simple_diff(left_data, right_data)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_service_new() {
        let service = DiffService::new();
        let _cloned = service.clone();
    }
}
