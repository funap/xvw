#![allow(dead_code)]

pub mod cursor;
pub mod layout_engine;
pub mod search_state;
pub mod structure_state;
pub mod view_options;

pub use cursor::{CursorModel, CursorState};
pub use layout_engine::LayoutEngine;
pub use search_state::SearchState;
pub use structure_state::EditorStructureState;
pub use view_options::ViewOptions;

use crate::core::bookmark::{BookmarkColor, BookmarkItem};
use crate::core::color::RgbaColor;
use crate::core::command::{Command, ReplaceRangeCommand};
use crate::core::document::Document;
use crate::core::encoding::Encoding;
use crate::core::radix::{ByteGroupSize, DisplayRadix};
use crate::core::selection::Selection;
use crate::core::structure::{ParseResult, ParsedField};
use std::collections::BTreeSet;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;
use std::sync::RwLock;

pub use crate::core::bookmark::FoldedBookmarkSummary;
pub use crate::core::layout::LineMap;

/// Represents the editor.
pub struct Editor {
    // Shared document containing buffer, history, and metadata
    pub document: Arc<RwLock<Document>>,
    pub cursor: CursorModel,
    pub layout: LayoutEngine,
    pub options: ViewOptions,
    pub search_state: SearchState,
    pub structure: EditorStructureState,
}

/// Read-only session providing scoped queries into document bookmarks and visibility.
pub struct BookmarkSessionRef<'a> {
    document: &'a Arc<RwLock<Document>>,
}

impl<'a> BookmarkSessionRef<'a> {
    pub fn snapshot(&self) -> Vec<BookmarkItem> {
        self.document.read().expect("document read lock").metadata.bookmarks.snapshot()
    }

    pub fn by_id(&self, id: &str) -> Option<BookmarkItem> {
        self.document.read().expect("document read lock").metadata.bookmarks.by_id_cloned(id)
    }

    pub fn custom_bookmarks_for_rendering(&self) -> Vec<(Range<usize>, RgbaColor)> {
        self.document
            .read()
            .expect("document read lock")
            .metadata
            .bookmarks
            .custom_bookmarks_for_rendering()
    }

    pub fn export_to_file(&self, path: &Path) -> anyhow::Result<()> {
        let doc = self.document.read().expect("document read lock");
        doc.metadata.bookmarks.export_to_file(path, Some(doc.path()))
    }

    pub fn is_color_hidden(&self, color: BookmarkColor) -> bool {
        self.document.read().expect("document read lock").metadata.bookmarks.is_color_hidden(color)
    }

    pub fn is_id_hidden(&self, id: &str) -> bool {
        self.document.read().expect("document read lock").metadata.bookmarks.is_id_hidden(id)
    }

    pub fn is_item_hidden(&self, item: &BookmarkItem) -> bool {
        self.document.read().expect("document read lock").metadata.bookmarks.is_item_hidden(item)
    }

    pub fn is_hide_unbookmarked(&self) -> bool {
        self.document.read().expect("document read lock").metadata.bookmarks.is_hide_unbookmarked()
    }
}

/// Mutable session providing scoped modifications to document bookmarks, automatically updating layout cache.
pub struct BookmarkSessionMut<'a> {
    document: &'a Arc<RwLock<Document>>,
    layout: &'a LayoutEngine,
}

impl<'a> BookmarkSessionMut<'a> {
    pub fn add(&mut self, item: BookmarkItem) -> String {
        let mut doc = self.document.write().expect("document write lock");
        let total = doc.buffer.len();
        doc.bump_layout_version();
        let id = doc.metadata.bookmarks.add(item, total);
        self.layout.invalidate();
        id
    }

    pub fn add_custom(&mut self, range: Range<usize>, color: RgbaColor) {
        let mut doc = self.document.write().expect("document write lock");
        let total = doc.buffer.len();
        doc.bump_layout_version();
        doc.metadata.bookmarks.add_custom(range, color, total);
        self.layout.invalidate();
    }

    pub fn update_comment(&mut self, id: &str, comment: impl Into<String>) -> bool {
        let mut doc = self.document.write().expect("document write lock");
        doc.metadata.bookmarks.update_comment(id, comment)
    }

    pub fn update_color(&mut self, id: &str, color: BookmarkColor) -> bool {
        let mut doc = self.document.write().expect("document write lock");
        doc.metadata.bookmarks.update_color(id, color)
    }

    pub fn update_range(&mut self, id: &str, offset: usize, size: usize) -> bool {
        let mut doc = self.document.write().expect("document write lock");
        let total = doc.buffer.len();
        doc.bump_layout_version();
        let ok = doc.metadata.bookmarks.update_range(id, offset, size, total);
        self.layout.invalidate();
        ok
    }

    pub fn remove_by_id(&mut self, id: &str) -> bool {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        let ok = doc.metadata.bookmarks.remove_by_id(id);
        self.layout.invalidate();
        ok
    }

    pub fn remove_by_index(&mut self, index: usize) -> Option<BookmarkItem> {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        let item = doc.metadata.bookmarks.remove_by_index(index);
        self.layout.invalidate();
        item
    }

    pub fn clear_custom(&mut self, range: Range<usize>) {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.bookmarks.clear_custom(range);
        self.layout.invalidate();
    }

    pub fn clear_all(&mut self) {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.bookmarks.clear_all();
        self.layout.invalidate();
    }

    pub fn import_from_file(&mut self, path: &Path) -> anyhow::Result<usize> {
        let mut doc = self.document.write().expect("document write lock");
        let total = doc.buffer.len();
        doc.bump_layout_version();
        let count = doc.metadata.bookmarks.import_from_file(path, total)?;
        self.layout.invalidate();
        Ok(count)
    }

    pub fn toggle_color(&mut self, color: BookmarkColor) {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.bookmarks.toggle_color(color);
        self.layout.invalidate();
    }

    pub fn show_color(&mut self, color: BookmarkColor) {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.bookmarks.show_color(color);
        self.layout.invalidate();
    }

    pub fn hide_color(&mut self, color: BookmarkColor) {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.bookmarks.hide_color(color);
        self.layout.invalidate();
    }

    pub fn show_only_color(&mut self, target_color: BookmarkColor) {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.bookmarks.show_only_color(target_color);
        self.layout.invalidate();
    }

    pub fn show_all(&mut self) {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.bookmarks.show_all();
        self.layout.invalidate();
    }

    pub fn hide_all(&mut self) {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.bookmarks.hide_all();
        self.layout.invalidate();
    }

    pub fn toggle_item_visibility(&mut self, id: &str) {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.bookmarks.toggle_item_visibility(id);
        self.layout.invalidate();
    }

    pub fn unfold_at(&mut self, offset: usize, folded_regions: &std::collections::BTreeMap<usize, usize>) -> bool {
        let mut doc = self.document.write().expect("document write lock");
        if doc.metadata.bookmarks.unfold_at(offset, folded_regions) {
            doc.bump_layout_version();
            self.layout.invalidate();
            true
        } else {
            false
        }
    }

    pub fn toggle_hide_unbookmarked(&mut self) {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.bookmarks.toggle_hide_unbookmarked();
        self.layout.invalidate();
    }

    pub fn set_hide_unbookmarked(&mut self, hide: bool) {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.bookmarks.set_hide_unbookmarked(hide);
        self.layout.invalidate();
    }
}

/// Read-only session providing scoped queries into custom layout rules (breaks, joins, empty lines).
pub struct CustomLayoutSessionRef<'a> {
    document: &'a Arc<RwLock<Document>>,
}

impl<'a> CustomLayoutSessionRef<'a> {
    pub fn has_break(&self, offset: usize) -> bool {
        self.document.read().expect("document read lock").metadata.custom_layout.has_break(offset)
    }

    pub fn breaks_count(&self) -> usize {
        self.document.read().expect("document read lock").metadata.custom_layout.breaks_count()
    }

    pub fn breaks_snapshot(&self) -> BTreeSet<usize> {
        self.document.read().expect("document read lock").metadata.custom_layout.breaks_snapshot()
    }

    pub fn has_join(&self, offset: usize) -> bool {
        self.document.read().expect("document read lock").metadata.custom_layout.has_join(offset)
    }

    pub fn empty_lines_at(&self, offset: usize) -> usize {
        self.document.read().expect("document read lock").metadata.custom_layout.empty_lines_at(offset)
    }

    pub fn count(&self, folded_count: usize) -> usize {
        self.document
            .read()
            .expect("document read lock")
            .metadata
            .custom_layout
            .custom_layout_count(folded_count)
    }
}

/// Mutable session providing scoped modifications to custom layout rules, automatically updating layout cache.
pub struct CustomLayoutSessionMut<'a> {
    document: &'a Arc<RwLock<Document>>,
    layout: &'a LayoutEngine,
    line_starts: LineMap,
    total_size: usize,
    cursor_offset: usize,
    selection_range: Option<Range<usize>>,
}

impl<'a> CustomLayoutSessionMut<'a> {
    pub fn add_break(&mut self, offset: usize) {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.custom_layout.add_break(offset, &self.line_starts, self.total_size);
        self.layout.invalidate();
    }

    pub fn remove_break(&mut self, offset: usize) -> bool {
        let mut doc = self.document.write().expect("document write lock");
        if doc.metadata.custom_layout.remove_break(offset) {
            doc.bump_layout_version();
            self.layout.invalidate();
            true
        } else {
            false
        }
    }

    pub fn toggle_break(&mut self, offset: usize) {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.custom_layout.toggle_break(offset, &self.line_starts, self.total_size);
        self.layout.invalidate();
    }

    pub fn add_empty_line(&mut self, offset: usize) {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.custom_layout.add_empty_line(offset, self.total_size);
        self.layout.invalidate();
    }

    pub fn remove_empty_line(&mut self, offset: usize) -> bool {
        let mut doc = self.document.write().expect("document write lock");
        if doc.metadata.custom_layout.remove_empty_line(offset) {
            doc.bump_layout_version();
            self.layout.invalidate();
            true
        } else {
            false
        }
    }

    pub fn join_line(&mut self) {
        if let Some(range) = self.selection_range.clone() {
            self.join_range(range);
            return;
        }
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.custom_layout.join_line(&self.line_starts, self.cursor_offset);
        self.layout.invalidate();
    }

    pub fn join_range(&mut self, range: Range<usize>) {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.custom_layout.join_range(range, &self.line_starts, self.total_size);
        self.layout.invalidate();
    }

    pub fn clear_breaks(&mut self) {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.custom_layout.clear_breaks();
        self.layout.invalidate();
    }

    pub fn clear_all(&mut self) {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.custom_layout.clear_all();
        self.layout.invalidate();
    }
}

impl Editor {
    pub fn new(document: Arc<RwLock<Document>>) -> Self {
        let cached_layout_version = document.read().expect("document read lock").layout_version();

        Self {
            document,
            cursor: CursorModel::default(),
            layout: LayoutEngine::new(cached_layout_version),
            options: ViewOptions::default(),
            search_state: SearchState::default(),
            structure: EditorStructureState::default(),
        }
    }

    /// Read-only session for querying bookmarks and bookmark visibility.
    pub fn bookmarks(&self) -> BookmarkSessionRef<'_> {
        BookmarkSessionRef { document: &self.document }
    }

    /// Mutable session for bookmark mutations, automatically updating layout cache.
    pub fn bookmarks_mut(&mut self) -> BookmarkSessionMut<'_> {
        BookmarkSessionMut {
            document: &self.document,
            layout: &self.layout,
        }
    }

    /// Read-only session for querying custom breaks, joins, and empty lines.
    pub fn custom_layout(&self) -> CustomLayoutSessionRef<'_> {
        CustomLayoutSessionRef { document: &self.document }
    }

    /// Mutable session for custom layout mutations, automatically updating layout cache.
    pub fn custom_layout_mut(&mut self) -> CustomLayoutSessionMut<'_> {
        let line_starts = self.line_starts();
        let total_size = self.total_size();
        let cursor_offset = self.cursor.offset;
        let selection_range = self.selection_range();
        CustomLayoutSessionMut {
            document: &self.document,
            layout: &self.layout,
            line_starts,
            total_size,
            cursor_offset,
            selection_range,
        }
    }

    pub fn cursor(&self) -> &CursorModel {
        &self.cursor
    }

    pub fn cursor_mut(&mut self) -> &mut CursorModel {
        &mut self.cursor
    }

    pub fn options(&self) -> &ViewOptions {
        &self.options
    }

    pub fn options_mut(&mut self) -> &mut ViewOptions {
        &mut self.options
    }

    pub fn search_state(&self) -> &SearchState {
        &self.search_state
    }

    pub fn search_state_mut(&mut self) -> &mut SearchState {
        &mut self.search_state
    }

    pub fn structure(&self) -> &EditorStructureState {
        &self.structure
    }

    pub fn structure_mut(&mut self) -> &mut EditorStructureState {
        &mut self.structure
    }

    pub fn document(&self) -> &Arc<RwLock<Document>> {
        &self.document
    }

    pub fn layout(&self) -> &LayoutEngine {
        &self.layout
    }

    pub fn bytes_per_row(&self) -> usize {
        self.layout.bytes_per_row()
    }

    pub fn set_bytes_per_row(&mut self, bytes_per_row: usize) {
        self.layout.set_bytes_per_row(bytes_per_row);
    }

    pub fn total_size(&self) -> usize {
        let binding = self.document.read().expect("document read lock");
        let buffer = &binding.buffer;
        buffer.len()
    }

    pub fn is_read_only(&self) -> bool {
        self.document.read().expect("document read lock").is_read_only()
    }

    pub fn set_read_only(&mut self, read_only: bool) {
        self.document.write().expect("document write lock").set_read_only(read_only);
    }

    pub fn toggle_read_only(&mut self) -> bool {
        self.document.write().expect("document write lock").toggle_read_only()
    }

    pub fn find_line_index(offset: usize, line_starts: &LineMap) -> usize {
        LayoutEngine::find_line_index(offset, line_starts)
    }

    pub fn find_line_index_in_slice(offset: usize, line_starts: &[usize]) -> usize {
        LayoutEngine::find_line_index_in_slice(offset, line_starts)
    }

    pub fn prev_data_line(idx: usize, line_starts: &LineMap, folded_regions: &std::collections::BTreeMap<usize, usize>) -> Option<usize> {
        LayoutEngine::prev_data_line(idx, line_starts, folded_regions)
    }

    pub fn next_data_line(idx: usize, line_starts: &LineMap, total_size: usize, folded_regions: &std::collections::BTreeMap<usize, usize>) -> Option<usize> {
        LayoutEngine::next_data_line(idx, line_starts, total_size, folded_regions)
    }

    pub fn value_at_cursor(&self) -> Option<u8> {
        let binding = self.document.read().expect("document read lock");
        let buffer = &binding.buffer;
        buffer.data().get(self.cursor.offset).copied()
    }

    pub fn read_bytes_at_cursor(&self, count: usize) -> Vec<u8> {
        let binding = self.document.read().expect("document read lock");
        binding.read_contiguous_bytes(self.cursor.offset, count).to_vec()
    }

    pub fn set_encoding(&mut self, encoding: Encoding) {
        if self.options.encoding != encoding {
            self.options.encoding = encoding;
            self.search_state.on_encoding_changed();
        }
    }

    pub fn set_radix(&mut self, radix: DisplayRadix) {
        self.options.radix = radix;
    }

    pub fn set_group_size(&mut self, group_size: ByteGroupSize) {
        self.options.group_size = group_size;
        self.cursor.set_group_size(group_size, self.total_size());
    }

    pub fn set_is_big_endian(&mut self, is_big_endian: bool) {
        self.options.is_big_endian = is_big_endian;
    }

    pub fn toggle_byte_order(&mut self) {
        self.options.is_big_endian = !self.options.is_big_endian;
    }

    pub fn selection_range(&self) -> Option<Range<usize>> {
        self.cursor.selection_range(self.total_size())
    }

    pub fn selection(&self) -> Selection {
        self.cursor.selection(self.total_size())
    }

    pub fn has_selection(&self) -> bool {
        self.cursor.has_selection(self.total_size())
    }

    pub fn set_selection(&mut self, anchor: usize, active: usize) {
        self.cursor.set_selection(anchor, active, self.total_size());
    }

    pub fn set_selection_range(&mut self, range: Range<usize>) {
        self.cursor.set_selection_range(range, self.total_size());
    }

    pub fn clear_selection(&mut self) {
        self.cursor.clear_selection(self.total_size());
    }

    pub fn insert_cursor_offset(&self) -> usize {
        self.cursor.insert_cursor_offset(self.total_size())
    }

    pub fn selected_range_or_cursor(&self) -> Option<Range<usize>> {
        self.cursor.selected_range_or_cursor(self.total_size())
    }

    pub fn edit_range(&self) -> Option<Range<usize>> {
        self.cursor.edit_range(self.total_size())
    }

    pub fn set_cursor_offset(&mut self, offset: usize) {
        self.cursor.set_cursor_offset(offset, self.total_size());
    }

    pub fn set_cursor_offset_exact(&mut self, offset: usize) {
        self.cursor.set_cursor_offset_exact(offset, self.total_size());
    }

    pub(crate) fn cursor_state(&self) -> CursorState {
        self.cursor.cursor_state()
    }

    pub(crate) fn restore_cursor_state(&mut self, state: CursorState) {
        self.cursor.restore_cursor_state(state, self.total_size());
    }

    pub fn adjust_after_edit(&mut self, start: usize, old_len: usize, new_len: usize) {
        self.cursor.adjust_after_edit(start, old_len, new_len, self.total_size());
        self.layout.invalidate();
    }

    pub fn move_left(&mut self) {
        self.cursor.move_left(self.total_size());
    }

    pub fn move_left_for_insert(&mut self) {
        self.cursor.move_left_for_insert(self.total_size());
    }

    pub fn move_right_for_insert(&mut self) {
        self.cursor.move_right_for_insert(self.total_size());
    }

    pub fn move_right(&mut self) {
        self.cursor.move_right(self.total_size());
    }

    pub fn move_up(&mut self) {
        let line_map = self.line_starts();
        let folded = self.computed_folded_regions();
        self.cursor.move_up(&line_map, &folded, self.total_size());
    }

    pub fn move_down(&mut self) {
        let line_map = self.line_starts();
        let folded = self.computed_folded_regions();
        self.cursor.move_down(&line_map, &folded, self.total_size());
    }

    pub fn move_down_for_insert(&mut self) {
        let line_map = self.line_starts();
        let folded = self.computed_folded_regions();
        self.cursor.move_down_for_insert(&line_map, &folded, self.total_size());
    }

    pub fn select_left(&mut self) {
        self.cursor.select_left(self.total_size());
    }

    pub fn select_left_for_insert(&mut self) {
        self.cursor.select_left_for_insert(self.total_size());
    }

    pub fn select_right(&mut self) {
        self.cursor.select_right(self.total_size());
    }

    pub fn select_right_for_insert(&mut self) {
        self.cursor.select_right_for_insert(self.total_size());
    }

    pub fn select_up_for_insert(&mut self) {
        let line_map = self.line_starts();
        let folded = self.computed_folded_regions();
        self.cursor.select_up_for_insert(&line_map, &folded, self.total_size());
    }

    pub fn select_up(&mut self) {
        let line_map = self.line_starts();
        let folded = self.computed_folded_regions();
        self.cursor.select_up(&line_map, &folded, self.total_size());
    }

    pub fn select_down_for_insert(&mut self) {
        let line_map = self.line_starts();
        let folded = self.computed_folded_regions();
        self.cursor.select_down_for_insert(&line_map, &folded, self.total_size());
    }

    pub fn select_down(&mut self) {
        let line_map = self.line_starts();
        let folded = self.computed_folded_regions();
        self.cursor.select_down(&line_map, &folded, self.total_size());
    }

    pub fn select_all(&mut self) {
        self.cursor.select_all(self.total_size());
    }

    pub fn go_to_beginning(&mut self) {
        self.cursor.go_to_beginning(self.total_size());
    }

    pub fn go_to_end(&mut self) {
        self.cursor.go_to_end(self.total_size());
    }

    pub fn go_to_offset(&mut self, offset: usize, extend_selection: bool) {
        let target = offset.min(self.total_size());
        self.auto_unfold_if_needed(target);
        self.cursor.go_to_offset(offset, extend_selection, self.total_size());
    }

    /// Jumps to and selects the specified byte offset range, unfolding folded regions if needed.
    pub fn go_to_range(&mut self, range: Range<usize>) {
        let start = range.start.min(self.total_size());
        self.auto_unfold_if_needed(start);
        if range.end > range.start {
            self.auto_unfold_if_needed(range.end.saturating_sub(1));
        }
        self.set_selection_range(range);
    }

    pub fn page_up(&mut self, visible_rows: usize) {
        let line_map = self.line_starts();
        self.cursor.page_up(visible_rows, &line_map, self.total_size());
    }

    pub fn page_down(&mut self, visible_rows: usize) {
        let line_map = self.line_starts();
        self.cursor.page_down(visible_rows, &line_map, self.total_size());
    }

    pub fn home(&mut self) {
        self.cursor.home(self.total_size());
    }

    pub fn end(&mut self) {
        self.cursor.end(self.total_size());
    }

    pub fn select_page_up(&mut self, visible_rows: usize) {
        let line_map = self.line_starts();
        self.cursor.select_page_up(visible_rows, &line_map, self.total_size());
    }

    pub fn select_page_down(&mut self, visible_rows: usize) {
        let line_map = self.line_starts();
        self.cursor.select_page_down(visible_rows, &line_map, self.total_size());
    }

    pub fn select_home_for_insert(&mut self) {
        self.cursor.select_home_for_insert(self.total_size());
    }

    pub fn select_home(&mut self) {
        self.cursor.select_home(self.total_size());
    }

    pub fn select_end(&mut self) {
        self.cursor.select_end(self.total_size());
    }

    pub fn select_end_for_insert(&mut self) {
        self.cursor.select_end_for_insert(self.total_size());
    }

    pub fn start_drag(&mut self, byte_pos: usize) {
        self.cursor.start_drag(byte_pos, self.total_size());
    }

    pub fn continue_drag(&mut self, anchor_pos: usize, byte_pos: usize) {
        self.cursor.continue_drag(anchor_pos, byte_pos, self.total_size());
    }

    pub fn line_starts(&self) -> LineMap {
        let doc = self.document.read().expect("document read lock");
        self.layout.line_starts(
            &doc,
            self.structure.show_inline_structure_view,
            self.structure.is_parsing,
            &self.structure.collapsed_struct_ids,
        )
    }

    pub fn has_custom_layout(&self) -> bool {
        let doc = self.document.read().expect("document read lock");
        LayoutEngine::has_custom_layout(&doc, self.structure.show_inline_structure_view, self.structure.is_parsing)
    }

    pub fn has_custom_layout_doc(&self, doc: &Document) -> bool {
        LayoutEngine::has_custom_layout(doc, self.structure.show_inline_structure_view, self.structure.is_parsing)
    }
    pub fn parse_result(&self) -> Option<Arc<ParseResult>> {
        self.document.read().expect("document read lock").metadata.parse_result.clone()
    }

    pub fn ksy_definition(&self) -> Option<Arc<crate::core::structure::KsyDefinition>> {
        self.document.read().expect("document read lock").metadata.ksy_definition.clone()
    }

    pub fn replace_range(&mut self, range: Range<usize>, replacement: Vec<u8>) -> bool {
        let total = self.total_size();
        let start = range.start.min(total);
        let end = range.end.min(total).max(start);
        let old_len = end - start;
        let new_total = total - old_len + replacement.len();
        let cursor_after = if replacement.is_empty() {
            start.min(new_total)
        } else {
            let next = start.saturating_add(replacement.len());
            next.min(new_total)
        };
        self.replace_range_with_cursor(start..end, replacement, cursor_after)
    }

    /// Replaces a range and explicitly chooses the resulting cursor offset.
    pub fn replace_range_with_cursor(&mut self, range: Range<usize>, replacement: Vec<u8>, cursor_after: usize) -> bool {
        if self.is_read_only() {
            return false;
        }
        let total = self.total_size();
        let start = range.start.min(total);
        let end = range.end.min(total).max(start);
        let old = self.document.read().expect("document read lock").buffer.get_range(start, end - start).to_vec();
        if old == replacement {
            self.set_cursor_offset_exact(cursor_after);
            return false;
        }

        let success = self.execute_command(Box::new(ReplaceRangeCommand::new(start, old, replacement)));
        if success {
            self.set_cursor_offset_exact(cursor_after);
        }
        success
    }

    /// Inserts bytes at `position` and advances the cursor after them.
    pub fn insert_bytes(&mut self, position: usize, bytes: Vec<u8>) -> bool {
        let position = position.min(self.total_size());
        let cursor_after = position.saturating_add(bytes.len());
        self.replace_range_with_cursor(position..position, bytes, cursor_after)
    }

    /// Replaces the byte at `position` without changing the buffer length.
    pub fn replace_byte(&mut self, position: usize, byte: u8) -> bool {
        let total = self.total_size();
        if position >= total {
            return false;
        }
        let cursor_after = (position + 1).min(total);
        self.replace_range_with_cursor(position..position + 1, vec![byte], cursor_after)
    }

    /// Deletes the selected bytes, or the byte at the cursor when there is no
    /// selection.
    pub fn delete_forward(&mut self) -> bool {
        let Some(range) = self.edit_range() else {
            return false;
        };
        let has_selection = self.has_selection();
        let cursor_after = if has_selection {
            range.start
        } else {
            range.start.min(self.total_size().saturating_sub(1))
        };
        self.replace_range_with_cursor(range, Vec::new(), cursor_after)
    }

    /// Deletes the selected bytes, or the byte immediately before the cursor
    /// when there is no selection.
    pub fn delete_backward(&mut self) -> bool {
        let total = self.total_size();
        if total == 0 {
            return false;
        }

        let has_selection = self.has_selection();
        let range = if has_selection {
            self.selection_range().expect("a non-empty selection has a range")
        } else if self.cursor.offset > 0 {
            self.cursor.offset - 1..self.cursor.offset
        } else {
            return false;
        };
        // Backspace removes the byte immediately before the caret. The caret
        // should remain at the deletion start, which is one position left of
        // its original location—not one additional position to the left.
        let cursor_after = range.start;
        self.replace_range_with_cursor(range, Vec::new(), cursor_after)
    }

    pub fn search_pattern(&self) -> Option<Vec<crate::core::search::PatternByte>> {
        self.search_state.search_pattern(self.options.encoding)
    }

    /// Finds and navigates to the next occurrence starting after the current cursor offset,
    /// wrapping around if necessary. Updates cursor offset and returns the match offset.
    pub fn find_and_navigate_next(&mut self) -> Option<usize> {
        let pattern = self.search_pattern()?;
        let next_offset = {
            let doc = self.document.read().ok()?;
            let data = doc.buffer.data();
            let from_offset = self.cursor.offset.saturating_add(1);
            crate::core::search::find_next_occurrence(data, &pattern, from_offset)?
        };
        self.auto_unfold_if_needed(next_offset);
        self.cursor.offset = next_offset;
        Some(next_offset)
    }

    /// Finds and navigates to the previous occurrence starting before the current cursor offset,
    /// wrapping around if necessary. Updates cursor offset and returns the match offset.
    pub fn find_and_navigate_prev(&mut self) -> Option<usize> {
        let pattern = self.search_pattern()?;
        let prev_offset = {
            let doc = self.document.read().ok()?;
            let data = doc.buffer.data();
            crate::core::search::find_prev_occurrence(data, &pattern, self.cursor.offset)?
        };
        self.auto_unfold_if_needed(prev_offset);
        self.cursor.offset = prev_offset;
        Some(prev_offset)
    }

    pub fn next_search_result(&mut self) -> Option<usize> {
        if let Some(offset) = self.search_state.next_result_offset() {
            self.auto_unfold_if_needed(offset);
            self.cursor.offset = offset;
            Some(offset)
        } else {
            self.find_and_navigate_next()
        }
    }

    /// Returns the active folded intervals [start, end) computed from hidden bookmarks or unbookmarked gaps.
    /// Overlapping and adjacent intervals are merged into disjoint union ranges.
    pub fn computed_folded_regions(&self) -> std::collections::BTreeMap<usize, usize> {
        if let Ok(doc) = self.document.read() {
            doc.computed_folded_regions()
        } else {
            std::collections::BTreeMap::new()
        }
    }

    /// Returns summary details for a folded region starting at `offset`.
    pub fn fold_bookmark_summary_at(&self, offset: usize) -> Option<FoldedBookmarkSummary> {
        self.document.read().ok()?.fold_bookmark_summary_at(offset)
    }
    pub fn unfold_bookmark_at(&mut self, offset: usize) -> bool {
        let folded = self.computed_folded_regions();
        self.bookmarks_mut().unfold_at(offset, &folded)
    }

    pub fn auto_unfold_if_needed(&mut self, target_offset: usize) -> bool {
        self.unfold_bookmark_at(target_offset)
    }

    pub fn is_folded(&self, offset: usize) -> bool {
        let regions = self.computed_folded_regions();
        for (&s, &e) in regions.iter() {
            if offset >= s && offset < e {
                return true;
            }
        }
        false
    }

    pub fn fold_end_at(&self, start: usize) -> Option<usize> {
        let regions = self.computed_folded_regions();
        regions.get(&start).copied()
    }

    pub fn fold_containing(&self, offset: usize) -> Option<(usize, usize)> {
        let regions = self.computed_folded_regions();
        for (&s, &e) in regions.iter() {
            if offset >= s && offset < e {
                return Some((s, e));
            }
        }
        None
    }

    /// Returns the base address of the document.
    pub fn base_address(&self) -> usize {
        self.document.read().expect("document read lock").base_address()
    }

    /// Converts a buffer offset to its physical memory address.
    pub fn offset_to_address(&self, offset: usize) -> usize {
        self.document.read().expect("document read lock").offset_to_address(offset)
    }

    /// Returns the physical memory address of the current cursor position.
    pub fn cursor_address(&self) -> usize {
        self.offset_to_address(self.cursor.offset)
    }

    /// Converts a physical memory address to a buffer offset.
    pub fn address_to_offset(&self, address: usize) -> Option<usize> {
        self.document.read().expect("document read lock").address_to_offset(address)
    }

    /// Returns true if the document has multiple discontinuous memory segments with address gaps.
    pub fn has_address_gaps(&self) -> bool {
        self.document.read().expect("document read lock").address_map.has_gaps()
    }

    pub fn toggle_struct_collapsed(&mut self, struct_id: &str) {
        self.structure.toggle_collapsed(struct_id);
        self.layout.invalidate();
    }

    pub fn toggle_inline_structure_view(&mut self) {
        self.structure.toggle_inline_view();
        self.layout.invalidate();
    }

    pub fn custom_layout_count(&self) -> usize {
        self.custom_layout().count(self.computed_folded_regions().len())
    }

    pub fn prev_search_result(&mut self) -> Option<usize> {
        if let Some(offset) = self.search_state.prev_result_offset() {
            self.auto_unfold_if_needed(offset);
            self.cursor.offset = offset;
            Some(offset)
        } else {
            self.find_and_navigate_prev()
        }
    }

    pub fn current_search_result(&self) -> Option<usize> {
        self.search_state.current_result()
    }

    pub fn execute_command(&mut self, command: Box<dyn Command>) -> bool {
        if self.is_read_only() {
            return false;
        }
        let delta = self.document.write().expect("document write lock").execute_command(command);
        if let Some(delta) = delta {
            self.adjust_after_edit(delta.offset, delta.old_len, delta.new_len);
            self.document_changed();
            true
        } else {
            false
        }
    }

    pub fn undo(&mut self) -> bool {
        if self.is_read_only() {
            return false;
        }
        let delta = self.document.write().expect("document write lock").undo();
        if let Some(delta) = delta {
            self.adjust_after_edit(delta.offset, delta.old_len, delta.new_len);
            self.set_cursor_offset_exact(delta.offset);
            self.document_changed();
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if self.is_read_only() {
            return false;
        }
        let delta = self.document.write().expect("document write lock").redo();
        if let Some(delta) = delta {
            self.adjust_after_edit(delta.offset, delta.old_len, delta.new_len);
            self.set_cursor_offset_exact(delta.offset.saturating_add(delta.new_len));
            self.document_changed();
            true
        } else {
            false
        }
    }

    /// Returns whether this editor has an undoable command.
    pub fn can_undo(&self) -> bool {
        self.document.read().expect("document read lock").can_undo()
    }

    /// Returns whether this editor has a redoable command.
    pub fn can_redo(&self) -> bool {
        self.document.read().expect("document read lock").can_redo()
    }

    pub fn set_kaitai_definition(&mut self, ksy: Arc<crate::core::structure::KsyDefinition>) {
        self.cancel_structure_parsing();
        self.structure.is_async = false;
        self.structure.reparse_requested = false;
        {
            let mut doc = self.document.write().expect("document write lock");
            doc.bump_layout_version();
            doc.metadata.ksy_definition = Some(ksy);
        }
        self.reparse_structure();
    }

    pub fn set_ksy_definition(&mut self, ksy: Arc<crate::core::structure::KsyDefinition>) {
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.ksy_definition = Some(ksy);
    }

    pub fn set_parse_result(&mut self, result: ParseResult) {
        self.structure.progress_offset = result.total_parsed_bytes;
        self.structure.is_finalizing = false;
        {
            let mut doc = self.document.write().expect("document write lock");
            doc.bump_layout_version();
            doc.metadata.parse_result = Some(Arc::new(result));
        }
        self.layout.invalidate();
    }

    /// Starts a new partial structure result that can receive parse batches.
    pub fn begin_partial_parse_result(&mut self, definition_id: String) {
        let old = {
            let mut doc = self.document.write().expect("document write lock");
            doc.bump_layout_version();
            doc.metadata.parse_result.replace(Arc::new(ParseResult::empty(definition_id)))
        };
        if let Some(old_res) = old {
            crate::core::dealloc::discard_in_background(old_res);
        }
        self.structure.is_finalizing = false;
        self.layout.invalidate();
    }

    /// Appends a batch of parsed root fields without cloning earlier batches.
    pub fn append_parse_fields(&mut self, definition_id: String, fields: Vec<ParsedField>, offset: usize, total_size: usize) {
        let chunk: Arc<[ParsedField]> = Arc::from(fields.into_boxed_slice());
        self.append_parse_chunks(definition_id, vec![chunk], offset, total_size);
    }

    /// Appends shared parse chunks without cloning their fields.
    pub fn append_parse_chunks(&mut self, definition_id: String, chunks: Vec<Arc<[ParsedField]>>, offset: usize, total_size: usize) {
        self.structure.progress_offset = offset;
        self.structure.total_size = total_size;
        if chunks.iter().all(|chunk| chunk.is_empty()) {
            return;
        }

        let next = if let Some(current) = self.parse_result() {
            Arc::new(current.append_shared_chunks_without_index(&chunks, offset))
        } else {
            Arc::new(ParseResult::empty(definition_id).append_shared_chunks_without_index(&chunks, offset))
        };
        let mut doc = self.document.write().expect("document write lock");
        doc.bump_layout_version();
        doc.metadata.parse_result = Some(next);
    }

    pub fn set_parse_result_arc(&mut self, result: Arc<ParseResult>) {
        self.structure.progress_offset = result.total_parsed_bytes;
        self.structure.is_finalizing = false;
        let old = {
            let mut doc = self.document.write().expect("document write lock");
            doc.bump_layout_version();
            doc.metadata.parse_result.replace(result)
        };
        if let Some(old_res) = old {
            crate::core::dealloc::discard_in_background(old_res);
        }
        self.layout.invalidate();
    }

    pub fn update_parse_progress(&mut self, offset: usize, total_size: usize, intermediate_result: Option<ParseResult>) {
        self.structure.progress_offset = offset;
        self.structure.total_size = total_size;
        if let Some(res) = intermediate_result {
            let mut doc = self.document.write().expect("document write lock");
            doc.bump_layout_version();
            doc.metadata.parse_result = Some(Arc::new(res));
            self.layout.invalidate();
        }
    }

    pub fn invalidate_line_map(&self) {
        self.layout.invalidate();
    }

    pub fn reparse_structure(&mut self) {
        if let Some(ksy) = self.ksy_definition() {
            let (buffer, ksy_clone) = {
                let buffer_lock = self.document.read().expect("document read lock");
                (buffer_lock.buffer.clone(), (*ksy).clone())
            };
            let mut stream = crate::core::structure::KaitaiStream::new(buffer.data());
            let interpreter = crate::core::structure::KaitaiInterpreter::new(ksy_clone);
            let result = interpreter.parse(&mut stream);
            self.set_parse_result(result);
        }
    }

    /// Returns the current deferred edit request without consuming it.
    ///
    /// The UI uses the generation as a debounce token. A request remains
    /// pending until [`Self::take_structure_reparse_request`] starts it, so a
    /// newer edit can invalidate an older timer without racing the parser.
    pub fn pending_structure_reparse(&self) -> Option<(Arc<crate::core::structure::KsyDefinition>, usize)> {
        self.structure.pending_reparse(self.ksy_definition().as_ref())
    }

    /// Takes a deferred edit request if it still belongs to `generation`.
    pub fn take_structure_reparse_request(&mut self, generation: usize) -> Option<Arc<crate::core::structure::KsyDefinition>> {
        self.structure.take_reparse_request(generation, self.ksy_definition().as_ref())
    }

    fn document_changed(&mut self) {
        self.layout.invalidate();
        self.search_state.on_document_changed();

        if self.structure.is_async && self.ksy_definition().is_some() {
            self.cancel_structure_parsing();
            self.structure.reparse_requested = true;
            self.structure.is_parsing = true;
            self.structure.is_finalizing = false;
            self.structure.progress_offset = 0;
            self.structure.total_size = self.total_size();

            if let Some(ksy) = self.ksy_definition() {
                self.begin_partial_parse_result(ksy.meta.id.clone());
            }
        } else {
            self.reparse_structure();
        }
    }

    pub fn cancel_structure_parsing(&mut self) {
        self.structure.cancel();
    }

    pub fn clear_structure_definition(&mut self) {
        self.cancel_structure_parsing();
        self.structure.is_async = false;
        self.structure.reparse_requested = false;
        let (old_definition, old) = {
            let mut doc = self.document.write().expect("document write lock");
            doc.bump_layout_version();
            (doc.metadata.ksy_definition.take(), doc.metadata.parse_result.take())
        };
        if old_definition.is_some() || old.is_some() {
            crate::core::dealloc::discard_in_background((old_definition, old));
        }
        self.structure.reset_progress();
        self.layout.invalidate();
    }
}

#[cfg(test)]
mod tests;
