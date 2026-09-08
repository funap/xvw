use crate::core::buffer::Buffer;
use crate::core::editor::Editor;
use crate::core::search::{self, SearchMode, SearchOptions};
use gpui_kit::{App, AsyncApp, Entity, Task};
use std::ops::Range;
use std::sync::Arc;

/// A service for executing asynchronous text and hex search queries across document buffers.
#[derive(Clone, Default)]
pub struct SearchService;

impl SearchService {
    /// Creates a new SearchService instance.
    pub fn new() -> Self {
        Self
    }

    /// Searches for a query in the given buffer based on the search options.
    /// Returns a Task that executes the search in the background.
    pub fn search(&self, buffer: Arc<Buffer>, query: String, options: SearchOptions, cx: &App) -> Task<Vec<usize>> {
        self.search_with_segments(buffer, query, options, Vec::new(), cx)
    }

    /// Searches for a query in the given buffer respecting memory segment boundaries.
    pub fn search_with_segments(&self, buffer: Arc<Buffer>, query: String, options: SearchOptions, segments: Vec<Range<usize>>, cx: &App) -> Task<Vec<usize>> {
        cx.background_executor().spawn(async move {
            if query.is_empty() {
                return Vec::new();
            }

            match options.mode {
                SearchMode::Text => {
                    if let Some(pattern) = search::parse_text_pattern(&query, options.encoding) {
                        search::find_occurrences_segmented(buffer.data(), &pattern, options.limit, &segments, options.range)
                    } else {
                        Vec::new()
                    }
                }
                SearchMode::Hex => {
                    if let Some(pattern) = search::parse_hex_pattern(&query) {
                        search::find_occurrences_segmented(buffer.data(), &pattern, options.limit, &segments, options.range)
                    } else {
                        Vec::new()
                    }
                }
            }
        })
    }

    /// Performs a search and updates the provided Editor entity with the results.
    pub fn perform_search(&self, editor: Entity<Editor>, query: String, options: SearchOptions, generation: usize, is_full: bool, cx: &App) -> Task<()> {
        let (buffer_data, segments) = {
            let editor_read = editor.read(cx);
            let document = editor_read.document.read().expect("document read lock");
            (Arc::new(document.buffer.clone()), document.address_map.segment_ranges())
        };

        let search_task = self.search_with_segments(buffer_data, query, options, segments, cx);
        let editor_weak = editor.downgrade();

        cx.spawn(move |cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let results = search_task.await;
                if let Some(editor) = editor_weak.upgrade() {
                    editor.update(&mut cx, |editor, cx| {
                        editor.search_state_mut().set_results(results, generation, is_full);
                        cx.notify();
                    });
                }
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

    #[test]
    fn test_search_service_new() {
        let service = SearchService::new();
        let _cloned = service.clone();
    }
}
