use crate::core::buffer::Buffer;
use crate::core::editor::Editor;
use crate::core::search::{self, SearchMode, SearchOptions};
use gpui_kit::{App, Entity, Task};
use std::ops::Range;

/// Pure helper executing segmented text or hex search queries on a byte buffer.
pub fn execute_search(data: &[u8], query: &str, options: &SearchOptions, segments: &[Range<usize>]) -> Vec<usize> {
    if query.is_empty() {
        return Vec::new();
    }

    match options.mode {
        SearchMode::Text => {
            if let Some(pattern) = search::parse_text_pattern(query, options.encoding) {
                search::find_occurrences_segmented(data, &pattern, options.limit, segments, options.range.clone())
            } else {
                Vec::new()
            }
        }
        SearchMode::Hex => {
            if let Some(pattern) = search::parse_hex_pattern(query) {
                search::find_occurrences_segmented(data, &pattern, options.limit, segments, options.range.clone())
            } else {
                Vec::new()
            }
        }
    }
}

/// A service for executing asynchronous text and hex search queries across document buffers.
#[derive(Debug, Clone, Default)]
pub struct SearchService;

#[allow(dead_code)]
impl SearchService {
    /// Creates a new `SearchService` instance.
    pub fn new() -> Self {
        Self
    }

    /// Searches for a query in the given buffer based on the search options.
    /// Returns a Task that executes the search in the background.
    pub fn search(&self, buffer: Buffer, query: String, options: SearchOptions, cx: &App) -> Task<Vec<usize>> {
        self.search_with_segments(buffer, query, options, Vec::new(), cx)
    }

    /// Searches for a query in the given buffer respecting memory segment boundaries.
    pub fn search_with_segments(&self, buffer: Buffer, query: String, options: SearchOptions, segments: Vec<Range<usize>>, cx: &App) -> Task<Vec<usize>> {
        cx.background_executor()
            .spawn(async move { execute_search(buffer.data(), &query, &options, &segments) })
    }

    /// Performs a search and updates the provided Editor entity with the results.
    pub fn perform_search(&self, editor: Entity<Editor>, query: String, options: SearchOptions, generation: usize, is_full: bool, cx: &App) -> Task<()> {
        let (buffer_data, segments) = {
            let editor_read = editor.read(cx);
            let document = editor_read.document.read().expect("document read lock");
            (document.buffer.clone(), document.address_map.segment_ranges())
        };

        let search_task = self.search_with_segments(buffer_data, query, options, segments, cx);
        let editor_weak = editor.downgrade();

        cx.spawn(async move |cx| {
            let results = search_task.await;
            if let Some(editor) = editor_weak.upgrade() {
                editor.update(cx, |editor, cx| {
                    editor.search_state_mut().set_results(results, generation, is_full);
                    cx.notify();
                });
            }
        })
    }

    /// Performs an incremental search: immediate viewport search followed by background full search.
    pub fn incremental_search(&self, editor: Entity<Editor>, query: String, mode: SearchMode, viewport_range: Range<usize>, cx: &App) -> (Task<()>, Task<()>) {
        let (generation, encoding) = {
            let ed = editor.read(cx);
            (ed.search_state.generation, ed.options.encoding)
        };

        let viewport_options = SearchOptions {
            mode,
            encoding,
            limit: crate::core::search::SearchLimit::Unlimited,
            range: Some(viewport_range),
        };
        let viewport_task = self.perform_search(editor.clone(), query.clone(), viewport_options, generation, false, cx);

        let full_options = SearchOptions {
            mode,
            encoding,
            limit: crate::core::search::SearchLimit::Unlimited,
            range: None,
        };
        let full_task = self.perform_search(editor, query, full_options, generation, true, cx);

        (viewport_task, full_task)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::encoding::Encoding;

    #[test]
    fn test_search_service_new() {
        let service = SearchService::new();
        let _cloned = service.clone();
        assert_eq!(format!("{service:?}"), "SearchService");
    }

    #[test]
    fn test_execute_search_empty_query() {
        let data = b"Hello, World!";
        let options = SearchOptions {
            mode: SearchMode::Text,
            encoding: Encoding::Utf8,
            limit: crate::core::search::SearchLimit::Unlimited,
            range: None,
        };
        let results = execute_search(data, "", &options, &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn test_execute_search_text() {
        let data = b"Hello, World! Hello!";
        let options = SearchOptions {
            mode: SearchMode::Text,
            encoding: Encoding::Utf8,
            limit: crate::core::search::SearchLimit::Unlimited,
            range: None,
        };
        let results = execute_search(data, "Hello", &options, &[]);
        assert_eq!(results, vec![0, 14]);
    }

    #[test]
    fn test_execute_search_hex() {
        let data = [0xAA, 0xBB, 0xCC, 0xAA, 0xBB];
        let options = SearchOptions {
            mode: SearchMode::Hex,
            encoding: Encoding::Utf8,
            limit: crate::core::search::SearchLimit::Unlimited,
            range: None,
        };
        let results = execute_search(&data, "AA BB", &options, &[]);
        assert_eq!(results, vec![0, 3]);
    }
}
