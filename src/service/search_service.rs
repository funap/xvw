use crate::core::buffer::Buffer;
use crate::core::editor::Editor;
use crate::core::search::{self, SearchMode, SearchOptions, SearchParseError, TextSearchFlags};
use gpui_kit::{App, Entity, Task};
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Pure helper executing segmented text, hex, case-insensitive, or regex search queries on a byte buffer.
#[allow(dead_code)]
pub fn execute_search(data: &[u8], query: &str, options: &SearchOptions, segments: &[Range<usize>]) -> Vec<usize> {
    let Ok(compiled) = search::compile_search(query, options.mode, options.encoding, options.flags) else {
        return Vec::new();
    };
    search::find_ranges_segmented(data, &compiled, options.limit, segments, options.range.clone(), None)
        .into_iter()
        .map(|r| r.start)
        .collect()
}

/// A service for executing asynchronous text and hex search queries across document buffers.
#[derive(Debug, Clone, Default)]
pub struct SearchService;

/// Query parameters for searching with a limit and optional cancellation.
pub struct LimitedSearchQuery {
    pub buffer: Buffer,
    pub query: String,
    pub mode: SearchMode,
    pub encoding: crate::core::encoding::Encoding,
    pub flags: TextSearchFlags,
    pub segments: Vec<Range<usize>>,
    pub max_results: usize,
    pub cancel: Option<Arc<AtomicBool>>,
}

/// Result of a bounded background search: `Ok((matched_ranges, is_truncated))` or `Err(SearchParseError)`.
pub type LimitedSearchResult = Result<(Vec<Range<usize>>, bool), SearchParseError>;

impl SearchService {
    /// Creates a new `SearchService` instance.
    pub fn new() -> Self {
        Self
    }

    /// Searches for a query in the given buffer based on the search options.
    /// Returns a Task that executes the search in the background.
    #[allow(dead_code)]
    pub fn search(&self, buffer: Buffer, query: String, options: SearchOptions, cx: &App) -> Task<Vec<usize>> {
        self.search_with_segments(buffer, query, options, Vec::new(), cx)
    }

    /// Searches for a query in the given buffer respecting memory segment boundaries.
    #[allow(dead_code)]
    pub fn search_with_segments(&self, buffer: Buffer, query: String, options: SearchOptions, segments: Vec<Range<usize>>, cx: &App) -> Task<Vec<usize>> {
        cx.background_executor()
            .spawn(async move { execute_search(buffer.data(), &query, &options, &segments) })
    }

    /// Searches for a query with truncation limit and cancellation (for SearchPanel & SearchBar match counting).
    /// Returns a Task yielding `Ok((ranges, is_truncated))` or `Err(SearchParseError)`.
    pub fn search_with_limit(&self, params: LimitedSearchQuery, cx: &App) -> Task<LimitedSearchResult> {
        cx.background_executor().spawn(async move {
            let compiled = search::compile_search(&params.query, params.mode, params.encoding, params.flags)?;
            let raw_ranges = search::find_ranges_segmented(
                params.buffer.data(),
                &compiled,
                search::SearchLimit::Count(params.max_results + 1),
                &params.segments,
                None,
                params.cancel.as_deref(),
            );
            let truncated = raw_ranges.len() > params.max_results;
            let capped_ranges = if truncated {
                raw_ranges.into_iter().take(params.max_results).collect()
            } else {
                raw_ranges
            };
            Ok((capped_ranges, truncated))
        })
    }

    /// Performs a search and updates the provided Editor entity with the results.
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn incremental_search(&self, editor: Entity<Editor>, query: String, mode: SearchMode, viewport_range: Range<usize>, cx: &App) -> (Task<()>, Task<()>) {
        let (generation, encoding, flags) = {
            let ed = editor.read(cx);
            (ed.search_state.generation, ed.options.encoding, ed.search_state.flags)
        };

        let viewport_options = SearchOptions {
            mode,
            encoding,
            flags,
            limit: search::SearchLimit::Unlimited,
            range: Some(viewport_range),
        };
        let viewport_task = self.perform_search(editor.clone(), query.clone(), viewport_options, generation, false, cx);

        let full_options = SearchOptions {
            mode,
            encoding,
            flags,
            limit: search::SearchLimit::Unlimited,
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
        let options = SearchOptions::new(SearchMode::Text).with_encoding(Encoding::Utf8);
        let results = execute_search(data, "", &options, &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn test_execute_search_text() {
        let data = b"Hello, World! Hello!";
        let options = SearchOptions::new(SearchMode::Text).with_encoding(Encoding::Utf8);
        let results = execute_search(data, "Hello", &options, &[]);
        assert_eq!(results, vec![0, 14]);
    }

    #[test]
    fn test_execute_search_case_insensitive_and_regex() {
        let data = b"Hello, world! HELLO!";
        let ci_options = SearchOptions::new(SearchMode::Text).with_encoding(Encoding::Utf8).with_flags(TextSearchFlags {
            case_insensitive: true,
            regex: false,
        });
        assert_eq!(execute_search(data, "hello", &ci_options, &[]), vec![0, 14]);

        let re_options = SearchOptions::new(SearchMode::Text).with_encoding(Encoding::Utf8).with_flags(TextSearchFlags {
            case_insensitive: true,
            regex: true,
        });
        assert_eq!(execute_search(data, r"h[a-z]+o", &re_options, &[]), vec![0, 14]);
    }

    #[test]
    fn test_execute_search_hex() {
        let data = [0xAA, 0xBB, 0xCC, 0xAA, 0xBB];
        let options = SearchOptions::new(SearchMode::Hex).with_encoding(Encoding::Utf8);
        let results = execute_search(&data, "AA BB", &options, &[]);
        assert_eq!(results, vec![0, 3]);
    }
}
