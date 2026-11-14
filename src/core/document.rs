#![allow(dead_code)]

use crate::core::address_map::AddressMap;
use crate::core::bookmark::BookmarkStore;
use crate::core::buffer::Buffer;
use crate::core::command::{Command, EditDelta};
use crate::core::format::FileFormat;
use crate::core::history::History;
use crate::core::layout::CustomLayoutRules;
use crate::core::structure::{KsyDefinition, ParseResult};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use crate::core::bookmark::FoldedBookmarkSummary;

/// Encapsulates metadata, annotations, structural analysis results,
/// and formatting configuration associated with a [`Document`].
#[derive(Debug, Clone, Default)]
pub struct DocumentMetadata {
    pub bookmarks: BookmarkStore,
    pub custom_layout: CustomLayoutRules,
    pub ksy_definition: Option<Arc<KsyDefinition>>,
    pub parse_result: Option<Arc<ParseResult>>,
    /// Monotonically increasing version counter used to invalidate derived layout caches.
    pub layout_version: usize,
}

impl DocumentMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    /// Increments the layout version counter.
    #[inline]
    pub fn bump_layout_version(&mut self) {
        self.layout_version = self.layout_version.wrapping_add(1);
    }
}

/// Pure helper calculating shifted offset after byte range replacement.
#[inline]
pub fn shift_offset_after_edit(offset: usize, start: usize, old_len: usize, new_len: usize) -> usize {
    let old_end = start.saturating_add(old_len);
    if old_len == 0 {
        if offset >= start { offset.saturating_add(new_len) } else { offset }
    } else if offset <= start {
        offset
    } else if offset >= old_end {
        if new_len >= old_len {
            offset.saturating_add(new_len - old_len)
        } else {
            offset.saturating_sub(old_len - new_len)
        }
    } else {
        start.saturating_add(new_len)
    }
}

/// Represents a document processing unit that bundles a file buffer, edit history, and metadata.
pub struct Document {
    pub path: PathBuf,
    pub buffer: Buffer,
    pub history: History,
    pub last_saved_version: usize,
    read_only: bool,
    pub address_map: AddressMap,
    pub format: FileFormat,
    pub metadata: DocumentMetadata,
}

impl Document {
    pub fn new(path: PathBuf, buffer: Buffer) -> Self {
        Self {
            path,
            buffer,
            history: History::new(),
            last_saved_version: 0,
            read_only: false,
            address_map: AddressMap::default(),
            format: FileFormat::Binary,
            metadata: DocumentMetadata::default(),
        }
    }

    /// Creates a document that starts in read-only mode.
    pub fn new_read_only(path: PathBuf, buffer: Buffer) -> Self {
        let mut document = Self::new(path, buffer);
        document.read_only = true;
        document
    }

    /// Sets the file format for this document and returns self.
    pub fn with_format(mut self, format: FileFormat) -> Self {
        self.format = format;
        self
    }

    /// Sets the address map for this document and returns self.
    pub fn with_address_map(mut self, address_map: AddressMap) -> Self {
        self.address_map = address_map;
        self
    }

    /// Returns the base address of the document.
    pub fn base_address(&self) -> usize {
        self.address_map.base_address()
    }

    /// Converts a linear buffer offset to its physical memory address.
    pub fn offset_to_address(&self, offset: usize) -> usize {
        self.address_map.offset_to_address(offset)
    }

    /// Converts a physical memory address to a linear buffer offset.
    pub fn address_to_offset(&self, address: usize) -> Option<usize> {
        self.address_map.address_to_offset(address)
    }

    /// Returns the maximum contiguous slice of bytes starting at `offset` up to `count` bytes.
    ///
    /// If `address_map` defines memory segments, the returned slice will not exceed the current
    /// segment's boundary, ensuring that unmapped address gaps are not bridged.
    pub fn read_contiguous_bytes(&self, offset: usize, count: usize) -> &[u8] {
        let data = self.buffer.data();
        if offset >= data.len() || count == 0 {
            return &[];
        }

        let max_end = if self.address_map.segments.is_empty() {
            data.len()
        } else {
            match self.address_map.segment_at_offset(offset) {
                Some(seg) => seg.end_buffer_offset(),
                None => return &[],
            }
        };

        let end = offset.saturating_add(count).min(max_end).min(data.len());
        if offset < end { &data[offset..end] } else { &[] }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns a reference to the underlying buffer.
    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    /// Returns the length of the document buffer in bytes.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Returns true if the document buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Returns a slice of the document's byte data.
    pub fn data(&self) -> &[u8] {
        self.buffer.data()
    }

    /// Returns a reference to the document's edit history.
    pub fn history(&self) -> &History {
        &self.history
    }

    /// Returns a reference to the document's address map.
    pub fn address_map(&self) -> &AddressMap {
        &self.address_map
    }

    /// Returns a reference to the document's metadata.
    pub fn metadata(&self) -> &DocumentMetadata {
        &self.metadata
    }

    /// Returns a mutable reference to the document's metadata.
    pub fn metadata_mut(&mut self) -> &mut DocumentMetadata {
        &mut self.metadata
    }

    /// Changes the file path used by subsequent save operations.
    pub fn set_path(&mut self, path: PathBuf) {
        self.path = path;
    }

    /// Atomically replaces bytes, adjusting address map and layout metadata.
    /// Does nothing and returns false if the document is read-only.
    pub fn replace_bytes(&mut self, offset: usize, old_len: usize, new_bytes: &[u8]) -> bool {
        if self.read_only {
            return false;
        }
        let new_len = new_bytes.len();
        self.buffer.replace_range(offset..offset.saturating_add(old_len), new_bytes);
        self.address_map.adjust_after_edit(offset, old_len, new_len);
        self.adjust_metadata_after_edit(offset, old_len, new_len);
        true
    }

    /// Returns true if the document has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.history.state_id() != self.last_saved_version
    }

    /// Returns whether editing and normal saves are currently disabled.
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Changes whether the document accepts edits and normal saves.
    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    /// Toggles read-only mode and returns the new state.
    pub fn toggle_read_only(&mut self) -> bool {
        self.read_only = !self.read_only;
        self.read_only
    }

    /// Marks the document as saved, updating the last saved version.
    pub fn mark_as_saved(&mut self) {
        self.last_saved_version = self.history.state_id();
    }

    /// Returns the current layout version of the document.
    pub fn layout_version(&self) -> usize {
        self.metadata.layout_version
    }

    /// Increments the layout version, invalidating any cached line maps.
    pub fn bump_layout_version(&mut self) {
        self.metadata.bump_layout_version();
    }

    /// Adjusts layout breaks, joins, empty lines, and bookmarks after a byte range edit.
    pub fn adjust_metadata_after_edit(&mut self, start: usize, old_len: usize, new_len: usize) {
        let shift = |offset: usize| shift_offset_after_edit(offset, start, old_len, new_len);

        self.bump_layout_version();
        self.metadata.custom_layout.adjust_after_edit(start, old_len, new_len, shift);
        self.metadata.bookmarks.adjust_after_edit(start, old_len, new_len, shift);
    }

    /// Executes a command on the document, recording it in history if it modified the document.
    pub fn execute_command(&mut self, mut command: Box<dyn Command>) -> Option<EditDelta> {
        if self.read_only {
            return None;
        }
        let delta = command.execute(self)?;
        if command.is_noop() {
            return None;
        }
        self.history.push(command);
        Some(delta)
    }

    /// Undoes the last command in history, reverting the document modification.
    pub fn undo(&mut self) -> Option<EditDelta> {
        if self.read_only {
            return None;
        }
        let mut command = self.history.pop_undo()?;
        if command.is_noop() {
            self.history.discard_pending_redo();
            return None;
        }
        let delta = command.undo(self);
        if delta.is_none() || command.is_noop() {
            if command.is_noop() {
                self.history.discard_pending_redo();
            } else {
                self.history.restore_undo(command);
            }
            return None;
        }
        self.history.push_redo(command);
        delta
    }

    /// Redoes the last undone command in history, re-applying the modification.
    pub fn redo(&mut self) -> Option<EditDelta> {
        if self.read_only {
            return None;
        }
        let mut command = self.history.pop_redo()?;
        if command.is_noop() {
            self.history.discard_pending_undo();
            return None;
        }
        let delta = command.execute(self);
        if delta.is_none() || command.is_noop() {
            if command.is_noop() {
                self.history.discard_pending_undo();
            } else {
                self.history.restore_redo(command);
            }
            return None;
        }
        self.history.push_undo(command);
        delta
    }

    /// Returns whether an undo operation is available.
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Returns whether a redo operation is available.
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Computes the active folded regions (as a start -> end map).
    ///
    /// Hidden bookmarks and unbookmarked gaps (when `hide_unbookmarked` is enabled)
    /// are calculated as separate, non-overlapping folded intervals.
    pub fn computed_folded_regions(&self) -> Arc<BTreeMap<usize, usize>> {
        self.metadata.bookmarks.computed_folded_regions(self.buffer.len())
    }

    /// Returns summary details for a folded region starting at `offset`.
    pub fn fold_bookmark_summary_at(&self, offset: usize) -> Option<FoldedBookmarkSummary> {
        self.metadata.bookmarks.fold_bookmark_summary_at(offset, self.buffer.len())
    }
}

impl Drop for Document {
    fn drop(&mut self) {
        let old_definition = self.metadata.ksy_definition.take();
        let old_parse_result = self.metadata.parse_result.take();
        if old_definition.is_some() || old_parse_result.is_some() {
            crate::core::dealloc::discard_in_background((old_definition, old_parse_result));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::command::{Command, EditDelta};

    struct MockCommand;

    impl Command for MockCommand {
        fn execute(&mut self, _doc: &mut Document) -> Option<EditDelta> {
            Some(EditDelta {
                offset: 0,
                old_len: 0,
                new_len: 1,
            })
        }
        fn undo(&mut self, _doc: &mut Document) -> Option<EditDelta> {
            Some(EditDelta {
                offset: 0,
                old_len: 1,
                new_len: 0,
            })
        }
    }

    #[test]
    fn test_is_dirty() {
        let mut doc = Document::new(PathBuf::from("test"), Buffer::empty());

        // Initially clean
        assert!(!doc.is_dirty());

        // Simulate modification (push command to history)
        doc.history.push(Box::new(MockCommand));
        assert!(doc.is_dirty());

        // Simulate save
        doc.mark_as_saved();
        assert!(!doc.is_dirty());

        // Modify again
        doc.history.push(Box::new(MockCommand));
        assert!(doc.is_dirty());

        // Undo
        // Note: history.pop_undo() returns the command and removes it from undo stack
        doc.history.pop_undo();
        assert!(!doc.is_dirty());
    }

    #[test]
    fn test_read_only_state() {
        let mut doc = Document::new_read_only(PathBuf::from("test"), Buffer::empty());
        assert!(doc.is_read_only());

        doc.set_read_only(false);
        assert!(!doc.is_read_only());
        assert!(doc.toggle_read_only());
        assert!(doc.is_read_only());
    }

    #[test]
    fn test_new_read_only_with_format_and_address_map() {
        let doc = Document::new_read_only(PathBuf::from("test.b64"), Buffer::empty())
            .with_format(FileFormat::Base64)
            .with_address_map(AddressMap::default());
        assert!(doc.is_read_only());
        assert_eq!(doc.format, FileFormat::Base64);
    }

    #[test]
    fn test_read_contiguous_bytes_with_segments() {
        use crate::core::address_map::{AddressMap, MemorySegment};

        let data = b"Hello 0Hello 1".to_vec();
        let map = AddressMap::from_segments(vec![
            MemorySegment {
                buffer_offset: 0,
                address: 0x0000,
                length: 7,
            },
            MemorySegment {
                buffer_offset: 7,
                address: 0x1000,
                length: 7,
            },
        ]);
        let doc = Document::new(PathBuf::from("test.mot"), Buffer::new(data)).with_address_map(map);

        // Near end of segment 0 (offset 5, reading 8 bytes): must clamp to segment 0 boundary (offset 7)
        let bytes = doc.read_contiguous_bytes(5, 8);
        assert_eq!(bytes, b" 0");

        // Within segment 1: reading 4 bytes
        let bytes = doc.read_contiguous_bytes(7, 4);
        assert_eq!(bytes, b"Hell");

        // Out of bounds
        assert!(doc.read_contiguous_bytes(14, 8).is_empty());
    }

    #[test]
    fn test_document_execute_command_undo_redo() {
        use crate::core::command::ReplaceRangeCommand;

        let mut doc = Document::new(PathBuf::from("test.bin"), Buffer::new(b"hello world".to_vec()));
        assert!(!doc.can_undo());
        assert!(!doc.can_redo());

        // Replace "world" with "rust"
        let delta = doc.execute_command(Box::new(ReplaceRangeCommand::new(6, b"world".to_vec(), b"rust".to_vec())));
        assert_eq!(
            delta,
            Some(EditDelta {
                offset: 6,
                old_len: 5,
                new_len: 4,
            })
        );
        assert_eq!(doc.buffer.data(), b"hello rust");
        assert!(doc.can_undo());
        assert!(!doc.can_redo());
        assert!(doc.is_dirty());

        // Undo
        let undo_delta = doc.undo();
        assert_eq!(
            undo_delta,
            Some(EditDelta {
                offset: 6,
                old_len: 4,
                new_len: 5,
            })
        );
        assert_eq!(doc.buffer.data(), b"hello world");
        assert!(!doc.can_undo());
        assert!(doc.can_redo());
        assert!(!doc.is_dirty());

        // Redo
        let redo_delta = doc.redo();
        assert_eq!(
            redo_delta,
            Some(EditDelta {
                offset: 6,
                old_len: 5,
                new_len: 4,
            })
        );
        assert_eq!(doc.buffer.data(), b"hello rust");
        assert!(doc.can_undo());
        assert!(!doc.can_redo());
        assert!(doc.is_dirty());
    }

    #[test]
    fn test_read_only_invariant_blocks_replace_bytes() {
        let mut doc = Document::new_read_only(PathBuf::from("readonly.bin"), Buffer::new(b"immutable".to_vec()));
        assert!(doc.is_read_only());

        // Attempting direct replace_bytes should fail and return false
        assert!(!doc.replace_bytes(0, 9, b"corrupted"));
        assert_eq!(doc.data(), b"immutable");

        // Attempting execute_command should also return None
        use crate::core::command::ReplaceRangeCommand;
        assert!(
            doc.execute_command(Box::new(ReplaceRangeCommand::new(0, b"immutable".to_vec(), b"corrupted".to_vec())))
                .is_none()
        );
        assert_eq!(doc.data(), b"immutable");
    }

    #[test]
    fn test_document_accessors() {
        let doc = Document::new(PathBuf::from("test.bin"), Buffer::new(b"test data".to_vec()));
        assert_eq!(doc.len(), 9);
        assert!(!doc.is_empty());
        assert_eq!(doc.data(), b"test data");
        assert_eq!(doc.buffer().len(), 9);
    }

    #[test]
    fn test_undo_redo_empty_and_read_only_preserve_state() {
        use crate::core::command::ReplaceRangeCommand;

        let mut doc = Document::new(PathBuf::from("test.bin"), Buffer::new(b"hello".to_vec()));
        assert_eq!(doc.undo(), None);
        assert_eq!(doc.redo(), None);
        assert!(!doc.can_undo());
        assert!(!doc.can_redo());
        assert!(!doc.is_dirty());

        doc.execute_command(Box::new(ReplaceRangeCommand::new(0, b"h".to_vec(), b"H".to_vec())))
            .expect("first edit");
        doc.mark_as_saved();
        assert!(!doc.is_dirty());

        doc.execute_command(Box::new(ReplaceRangeCommand::new(1, b"e".to_vec(), b"E".to_vec())))
            .expect("second edit");
        assert_eq!(doc.data(), b"HEllo");
        assert!(doc.is_dirty());

        doc.set_read_only(true);
        assert_eq!(doc.undo(), None);
        assert_eq!(doc.data(), b"HEllo");
        assert!(doc.can_undo());
        assert!(!doc.can_redo());
        assert!(doc.is_dirty());

        doc.set_read_only(false);
        assert!(doc.undo().is_some());
        assert_eq!(doc.data(), b"Hello");
        assert!(!doc.is_dirty());
        assert!(doc.can_redo());

        doc.set_read_only(true);
        assert_eq!(doc.redo(), None);
        assert_eq!(doc.data(), b"Hello");
        assert!(doc.can_redo());
        assert!(!doc.is_dirty());

        doc.set_read_only(false);
        assert!(doc.redo().is_some());
        assert_eq!(doc.data(), b"HEllo");
        assert!(!doc.can_redo());
        assert!(doc.is_dirty());
    }

    #[test]
    fn test_execute_noop_command_does_not_pollute_history() {
        use crate::core::command::ReplaceRangeCommand;

        struct NoopCommandWithDelta;
        impl Command for NoopCommandWithDelta {
            fn execute(&mut self, _doc: &mut Document) -> Option<EditDelta> {
                Some(EditDelta {
                    offset: 0,
                    old_len: 0,
                    new_len: 0,
                })
            }

            fn undo(&mut self, _doc: &mut Document) -> Option<EditDelta> {
                None
            }

            fn is_noop(&self) -> bool {
                true
            }
        }

        let mut doc = Document::new(PathBuf::from("test.bin"), Buffer::new(b"abc".to_vec()));
        doc.execute_command(Box::new(ReplaceRangeCommand::new(0, b"a".to_vec(), b"z".to_vec())))
            .expect("edit");
        assert!(doc.undo().is_some());
        assert!(!doc.can_undo());
        assert!(doc.can_redo());
        assert!(!doc.is_dirty());

        // Same-bytes ReplaceRangeCommand and explicit `is_noop() == true` commands must not record
        // history entries or clear existing redo history.
        assert_eq!(doc.execute_command(Box::new(ReplaceRangeCommand::new(0, b"a".to_vec(), b"a".to_vec()))), None);
        assert_eq!(doc.execute_command(Box::new(NoopCommandWithDelta)), None);
        assert!(!doc.can_undo());
        assert!(doc.can_redo());
        assert!(!doc.is_dirty());
    }

    #[test]
    fn test_failed_undo_and_redo_preserve_history_stack_and_dirty_state() {
        use crate::core::command::ReplaceRangeCommand;
        use std::sync::atomic::{AtomicBool, Ordering};

        struct ConditionalReplaceCommand {
            allow_execute: Arc<AtomicBool>,
            allow_undo: Arc<AtomicBool>,
        }

        impl Command for ConditionalReplaceCommand {
            fn execute(&mut self, doc: &mut Document) -> Option<EditDelta> {
                if !self.allow_execute.load(Ordering::SeqCst) {
                    return None;
                }
                if !doc.replace_bytes(0, 1, b"B") {
                    return None;
                }
                Some(EditDelta {
                    offset: 0,
                    old_len: 1,
                    new_len: 1,
                })
            }

            fn undo(&mut self, doc: &mut Document) -> Option<EditDelta> {
                if !self.allow_undo.load(Ordering::SeqCst) {
                    return None;
                }
                if !doc.replace_bytes(0, 1, b"A") {
                    return None;
                }
                Some(EditDelta {
                    offset: 0,
                    old_len: 1,
                    new_len: 1,
                })
            }
        }

        let allow_execute = Arc::new(AtomicBool::new(true));
        let allow_undo = Arc::new(AtomicBool::new(false));
        let mut doc = Document::new(PathBuf::from("test.bin"), Buffer::new(b"_tail".to_vec()));

        doc.execute_command(Box::new(ReplaceRangeCommand::new(0, b"_".to_vec(), b"A".to_vec())))
            .expect("base edit");
        doc.mark_as_saved();
        assert!(!doc.is_dirty());

        doc.execute_command(Box::new(ConditionalReplaceCommand {
            allow_execute: Arc::clone(&allow_execute),
            allow_undo: Arc::clone(&allow_undo),
        }))
        .expect("conditional edit");
        assert_eq!(doc.data(), b"Btail");
        assert!(doc.is_dirty());

        // Failing undo must restore the command on the undo stack and keep `is_dirty()` true.
        assert_eq!(doc.undo(), None);
        assert_eq!(doc.data(), b"Btail");
        assert!(doc.can_undo());
        assert!(!doc.can_redo());
        assert!(doc.is_dirty());

        // Allowing undo now reverts the document cleanly back to the saved state.
        allow_undo.store(true, Ordering::SeqCst);
        assert!(doc.undo().is_some());
        assert_eq!(doc.data(), b"Atail");
        assert!(!doc.is_dirty());
        assert!(doc.can_redo());

        // Failing redo must keep the entry on the redo stack and preserve `!is_dirty()`.
        allow_execute.store(false, Ordering::SeqCst);
        assert_eq!(doc.redo(), None);
        assert_eq!(doc.data(), b"Atail");
        assert!(doc.can_redo());
        assert!(!doc.is_dirty());

        // Allowing redo re-applies the command and marks the document dirty again.
        allow_execute.store(true, Ordering::SeqCst);
        assert!(doc.redo().is_some());
        assert_eq!(doc.data(), b"Btail");
        assert!(!doc.can_redo());
        assert!(doc.is_dirty());
    }

    #[test]
    fn test_stale_noop_entries_discarded_cleanly_on_undo_and_redo() {
        struct StaleNoop;
        impl Command for StaleNoop {
            fn execute(&mut self, _doc: &mut Document) -> Option<EditDelta> {
                unreachable!("is_noop commands should not be executed on redo")
            }

            fn undo(&mut self, _doc: &mut Document) -> Option<EditDelta> {
                unreachable!("is_noop commands should not be undone")
            }

            fn is_noop(&self) -> bool {
                true
            }
        }

        let mut doc = Document::new(PathBuf::from("test.bin"), Buffer::new(b"abc".to_vec()));
        doc.history.push(Box::new(MockCommand));
        doc.mark_as_saved();
        doc.history.push(Box::new(StaleNoop));

        assert_eq!(doc.undo(), None);
        assert!(!doc.can_redo());
        assert!(!doc.is_dirty());

        doc.history.push_redo(Box::new(StaleNoop));
        assert_eq!(doc.redo(), None);
        assert!(!doc.can_redo());
        assert!(!doc.is_dirty());
        assert_eq!(doc.history.version(), 1);
    }
}
