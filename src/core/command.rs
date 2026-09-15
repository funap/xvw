#![allow(dead_code)]

use crate::core::document::Document;

/// Description of a byte range change resulting from command execution or undo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditDelta {
    pub offset: usize,
    pub old_len: usize,
    pub new_len: usize,
}

/// A trait representing an executable and undoable command that operates directly on a [`Document`].
pub trait Command: Send + Sync {
    /// Executes the modification on the document buffer, address map, and metadata.
    /// Returns the affected byte range delta, or `None` if the command was a no-op.
    fn execute(&mut self, doc: &mut Document) -> Option<EditDelta>;

    /// Reverts the modification on the document buffer, address map, and metadata.
    /// Returns the affected byte range delta, or `None` if the command was a no-op.
    fn undo(&mut self, doc: &mut Document) -> Option<EditDelta>;

    /// Returns true when executing the command did not change the document.
    fn is_noop(&self) -> bool {
        false
    }
}

/// Replaces an arbitrary range of bytes on a [`Document`].
///
/// An empty range is an insertion and an empty replacement is a deletion, so
/// one command type covers all ordinary buffer edits.
pub struct ReplaceRangeCommand {
    pub position: usize,
    pub old: Vec<u8>,
    pub new: Vec<u8>,
    noop: bool,
}

impl ReplaceRangeCommand {
    pub fn new(position: usize, old: Vec<u8>, new: Vec<u8>) -> Self {
        let noop = old == new;
        Self { position, old, new, noop }
    }
}

impl Command for ReplaceRangeCommand {
    fn execute(&mut self, doc: &mut Document) -> Option<EditDelta> {
        if self.noop {
            return None;
        }
        let old_len = self.old.len();
        let new_len = self.new.len();
        if !doc.replace_bytes(self.position, old_len, &self.new) {
            return None;
        }
        Some(EditDelta {
            offset: self.position,
            old_len,
            new_len,
        })
    }

    fn undo(&mut self, doc: &mut Document) -> Option<EditDelta> {
        if self.noop {
            return None;
        }
        let old_len = self.old.len();
        let new_len = self.new.len();
        if !doc.replace_bytes(self.position, new_len, &self.old) {
            return None;
        }
        Some(EditDelta {
            offset: self.position,
            old_len: new_len,
            new_len: old_len,
        })
    }

    fn is_noop(&self) -> bool {
        self.noop
    }
}

/// Command to insert a single character.
pub struct InsertCharCommand {
    pub position: usize,
    pub c: u8,
    inserted: bool,
}

impl InsertCharCommand {
    pub fn new(position: usize, c: u8) -> Self {
        Self { position, c, inserted: false }
    }
}

impl Command for InsertCharCommand {
    fn execute(&mut self, doc: &mut Document) -> Option<EditDelta> {
        if self.position <= doc.buffer.len() {
            doc.buffer.insert(self.position, self.c);
            doc.address_map.adjust_after_edit(self.position, 0, 1);
            doc.adjust_metadata_after_edit(self.position, 0, 1);
            self.inserted = true;
            Some(EditDelta {
                offset: self.position,
                old_len: 0,
                new_len: 1,
            })
        } else {
            self.inserted = false;
            None
        }
    }

    fn undo(&mut self, doc: &mut Document) -> Option<EditDelta> {
        if self.inserted && self.position < doc.buffer.len() {
            if doc.buffer.remove(self.position).is_some() {
                doc.address_map.adjust_after_edit(self.position, 1, 0);
                doc.adjust_metadata_after_edit(self.position, 1, 0);
                Some(EditDelta {
                    offset: self.position,
                    old_len: 1,
                    new_len: 0,
                })
            } else {
                None
            }
        } else {
            None
        }
    }

    fn is_noop(&self) -> bool {
        !self.inserted
    }
}

/// Command to delete a single character at a specific position.
pub struct DeleteCharCommand {
    pub position: usize,
    pub deleted_char: Option<u8>,
}

impl DeleteCharCommand {
    pub fn new(position: usize) -> Self {
        Self { position, deleted_char: None }
    }
}

impl Command for DeleteCharCommand {
    fn execute(&mut self, doc: &mut Document) -> Option<EditDelta> {
        if self.position < doc.buffer.len() {
            let ch = doc.buffer.remove(self.position);
            if let Some(c) = ch {
                self.deleted_char = Some(c);
                doc.address_map.adjust_after_edit(self.position, 1, 0);
                doc.adjust_metadata_after_edit(self.position, 1, 0);
                Some(EditDelta {
                    offset: self.position,
                    old_len: 1,
                    new_len: 0,
                })
            } else {
                None
            }
        } else {
            None
        }
    }

    fn undo(&mut self, doc: &mut Document) -> Option<EditDelta> {
        if let Some(c) = self.deleted_char {
            if self.position <= doc.buffer.len() {
                doc.buffer.insert(self.position, c);
                doc.address_map.adjust_after_edit(self.position, 0, 1);
                doc.adjust_metadata_after_edit(self.position, 0, 1);
                Some(EditDelta {
                    offset: self.position,
                    old_len: 0,
                    new_len: 1,
                })
            } else {
                None
            }
        } else {
            None
        }
    }

    fn is_noop(&self) -> bool {
        self.deleted_char.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::{DeleteCharCommand, EditDelta, InsertCharCommand, ReplaceRangeCommand};
    use crate::core::buffer::Buffer;
    use crate::core::document::Document;
    use std::path::PathBuf;

    fn document_with_content(content: &[u8]) -> Document {
        Document::new(PathBuf::from("command-test.bin"), Buffer::new(content.to_vec()))
    }

    #[test]
    fn insert_command_round_trips_through_undo_and_redo() {
        let mut doc = document_with_content(b"ac");

        let delta = doc.execute_command(Box::new(InsertCharCommand::new(1, b'b')));
        assert_eq!(
            delta,
            Some(EditDelta {
                offset: 1,
                old_len: 0,
                new_len: 1
            })
        );
        assert_eq!(doc.buffer.data(), b"abc");
        assert!(doc.is_dirty());

        let undo_delta = doc.undo();
        assert_eq!(
            undo_delta,
            Some(EditDelta {
                offset: 1,
                old_len: 1,
                new_len: 0
            })
        );
        assert_eq!(doc.buffer.data(), b"ac");

        let redo_delta = doc.redo();
        assert_eq!(
            redo_delta,
            Some(EditDelta {
                offset: 1,
                old_len: 0,
                new_len: 1
            })
        );
        assert_eq!(doc.buffer.data(), b"abc");
    }

    #[test]
    fn delete_command_restores_the_deleted_byte() {
        let mut doc = document_with_content(b"abc");

        let delta = doc.execute_command(Box::new(DeleteCharCommand::new(1)));
        assert_eq!(
            delta,
            Some(EditDelta {
                offset: 1,
                old_len: 1,
                new_len: 0
            })
        );
        assert_eq!(doc.buffer.data(), b"ac");

        doc.undo();
        assert_eq!(doc.buffer.data(), b"abc");

        doc.redo();
        assert_eq!(doc.buffer.data(), b"ac");
    }

    #[test]
    fn replace_range_command_round_trips() {
        let mut doc = document_with_content(b"hello world");

        let delta = doc.execute_command(Box::new(ReplaceRangeCommand::new(6, b"world".to_vec(), b"rust".to_vec())));
        assert_eq!(
            delta,
            Some(EditDelta {
                offset: 6,
                old_len: 5,
                new_len: 4
            })
        );
        assert_eq!(doc.buffer.data(), b"hello rust");

        doc.undo();
        assert_eq!(doc.buffer.data(), b"hello world");

        doc.redo();
        assert_eq!(doc.buffer.data(), b"hello rust");
    }

    #[test]
    fn deleting_out_of_bounds_is_a_no_op() {
        let mut doc = document_with_content(b"abc");

        let delta = doc.execute_command(Box::new(DeleteCharCommand::new(3)));
        assert_eq!(delta, None);
        assert_eq!(doc.buffer.data(), b"abc");
        assert!(!doc.can_undo());
    }

    #[test]
    fn inserting_out_of_bounds_is_a_no_op() {
        let mut doc = document_with_content(b"abc");

        let delta = doc.execute_command(Box::new(InsertCharCommand::new(4, b'x')));
        assert_eq!(delta, None);
        assert_eq!(doc.buffer.data(), b"abc");
        assert!(!doc.can_undo());
        assert!(!doc.is_dirty());
    }
}
