use crate::core::encoding::Encoding;
use crate::core::search::{CompiledSearch, PatternByte, SearchMode, SearchParseError, TextSearchFlags, compile_search, parse_hex_pattern, parse_text_pattern};
use std::ops::Range;

/// Encapsulates search query, active match collection, and result navigation state.
#[derive(Default, Clone, Debug)]
pub struct SearchState {
    pub query: String,
    pub mode: SearchMode,
    pub flags: TextSearchFlags,
    pub results: Vec<usize>,
    pub result_ranges: Vec<Range<usize>>,
    pub current_result_index: Option<usize>,
    pub is_full_search_complete: bool,
    pub generation: usize,
}

impl SearchState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn search_pattern(&self, encoding: Encoding) -> Option<Vec<PatternByte>> {
        if self.query.is_empty() {
            return None;
        }
        match self.mode {
            SearchMode::Text => parse_text_pattern(&self.query, encoding),
            SearchMode::Hex => parse_hex_pattern(&self.query),
        }
    }

    pub fn compiled_search(&self, encoding: Encoding) -> Result<CompiledSearch, SearchParseError> {
        compile_search(&self.query, self.mode, encoding, self.flags)
    }

    pub fn set_query(&mut self, query: String) {
        if self.query != query {
            self.query = query;
            self.results.clear();
            self.result_ranges.clear();
            self.current_result_index = None;
            self.is_full_search_complete = false;
            self.generation = self.generation.wrapping_add(1);
        }
    }

    pub fn set_query_and_mode(&mut self, query: String, mode: SearchMode) {
        self.set_query_mode_and_flags(query, mode, self.flags);
    }

    pub fn set_query_mode_and_flags(&mut self, query: String, mode: SearchMode, flags: TextSearchFlags) {
        let effective_flags = if mode == SearchMode::Text { flags } else { TextSearchFlags::default() };
        if self.query != query || self.mode != mode || self.flags != effective_flags {
            self.query = query;
            self.mode = mode;
            self.flags = effective_flags;
            self.results.clear();
            self.result_ranges.clear();
            self.current_result_index = None;
            self.is_full_search_complete = false;
            self.generation = self.generation.wrapping_add(1);
        }
    }

    pub fn set_results(&mut self, results: Vec<usize>, generation: usize, is_full: bool) {
        if generation < self.generation {
            return;
        }
        if generation > self.generation {
            self.generation = generation;
        }
        if self.is_full_search_complete && !is_full {
            return;
        }
        self.results = results;
        self.result_ranges.clear();
        if is_full {
            self.is_full_search_complete = true;
        }
        if !self.results.is_empty() && self.current_result_index.is_none() {
            self.current_result_index = Some(0);
        }
    }

    pub fn set_result_ranges(&mut self, ranges: Vec<Range<usize>>, generation: usize, is_full: bool) {
        if generation < self.generation {
            return;
        }
        if generation > self.generation {
            self.generation = generation;
        }
        if self.is_full_search_complete && !is_full {
            return;
        }
        self.results = ranges.iter().map(|r| r.start).collect();
        self.result_ranges = ranges;
        if is_full {
            self.is_full_search_complete = true;
        }
        if !self.results.is_empty() && self.current_result_index.is_none() {
            self.current_result_index = Some(0);
        }
    }

    pub fn clear(&mut self) {
        self.query.clear();
        self.results.clear();
        self.result_ranges.clear();
        self.current_result_index = None;
        self.is_full_search_complete = false;
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn next_result_offset(&mut self) -> Option<usize> {
        if self.results.is_empty() {
            return None;
        }
        let next_index = if let Some(index) = self.current_result_index {
            (index + 1) % self.results.len()
        } else {
            0
        };
        self.current_result_index = Some(next_index);
        Some(self.results[next_index])
    }

    pub fn prev_result_offset(&mut self) -> Option<usize> {
        if self.results.is_empty() {
            return None;
        }
        let prev_index = if let Some(index) = self.current_result_index {
            if index == 0 { self.results.len() - 1 } else { index - 1 }
        } else {
            self.results.len() - 1
        };
        self.current_result_index = Some(prev_index);
        Some(self.results[prev_index])
    }

    pub fn current_result(&self) -> Option<usize> {
        if let Some(i) = self.current_result_index {
            self.results.get(i).copied()
        } else {
            None
        }
    }

    pub fn current_result_range(&self) -> Option<Range<usize>> {
        let i = self.current_result_index?;
        self.result_ranges.get(i).cloned()
    }

    pub fn on_encoding_changed(&mut self) {
        if self.mode == SearchMode::Text && !self.query.is_empty() {
            self.results.clear();
            self.result_ranges.clear();
            self.current_result_index = None;
            self.is_full_search_complete = false;
            self.generation = self.generation.wrapping_add(1);
        }
    }

    pub fn on_document_changed(&mut self) {
        self.results.clear();
        self.result_ranges.clear();
        self.current_result_index = None;
        self.is_full_search_complete = false;
        self.generation = self.generation.wrapping_add(1);
    }
}
