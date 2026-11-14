#![allow(dead_code)]

use crate::core::command::Command;

/// Manages the history of commands for undo/redo functionality.
#[derive(Default)]
pub struct History {
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
    next_state_id: usize,
    current_state_id: usize,
    pending_redo_state_id: Option<usize>,
    pending_undo_state_id: Option<usize>,
}

struct HistoryEntry {
    command: Box<dyn Command>,
    state_id: usize,
}

impl History {
    /// Creates a new empty history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Pushes a new command onto the undo stack and clears the redo stack.
    /// This should be called when a new command is executed.
    pub fn push(&mut self, command: Box<dyn Command>) {
        self.next_state_id = self.next_state_id.wrapping_add(1).max(1);
        self.current_state_id = self.next_state_id;
        self.undo_stack.push(HistoryEntry {
            command,
            state_id: self.current_state_id,
        });
        self.redo_stack.clear();
        self.pending_redo_state_id = None;
        self.pending_undo_state_id = None;
    }

    /// Pops the last command from the undo stack.
    pub fn pop_undo(&mut self) -> Option<Box<dyn Command>> {
        let entry = self.undo_stack.pop()?;
        self.current_state_id = self.undo_stack.last().map_or(0, |entry| entry.state_id);
        self.pending_redo_state_id = Some(entry.state_id);
        Some(entry.command)
    }

    /// Pushes a command onto the redo stack.
    pub fn push_redo(&mut self, command: Box<dyn Command>) {
        let state_id = self.pending_redo_state_id.take().unwrap_or_else(|| {
            self.next_state_id = self.next_state_id.wrapping_add(1).max(1);
            self.next_state_id
        });
        self.redo_stack.push(HistoryEntry { command, state_id });
    }

    /// Restores a command popped via [`Self::pop_undo`] back onto the undo stack with its original state ID.
    pub fn restore_undo(&mut self, command: Box<dyn Command>) {
        let state_id = self.pending_redo_state_id.take().unwrap_or_else(|| {
            self.next_state_id = self.next_state_id.wrapping_add(1).max(1);
            self.next_state_id
        });
        self.current_state_id = state_id;
        self.undo_stack.push(HistoryEntry { command, state_id });
    }

    /// Discards the pending redo state ID after a popped undo command is dropped as a no-op.
    pub fn discard_pending_redo(&mut self) {
        self.pending_redo_state_id = None;
    }

    /// Pops the last command from the redo stack.
    pub fn pop_redo(&mut self) -> Option<Box<dyn Command>> {
        let entry = self.redo_stack.pop()?;
        self.pending_undo_state_id = Some(entry.state_id);
        Some(entry.command)
    }

    /// Pushes a command back onto the undo stack (used during redo).
    pub fn push_undo(&mut self, command: Box<dyn Command>) {
        let state_id = self.pending_undo_state_id.take().unwrap_or_else(|| {
            self.next_state_id = self.next_state_id.wrapping_add(1).max(1);
            self.next_state_id
        });
        self.current_state_id = state_id;
        self.undo_stack.push(HistoryEntry { command, state_id });
    }

    /// Restores a command popped via [`Self::pop_redo`] back onto the redo stack with its original state ID.
    pub fn restore_redo(&mut self, command: Box<dyn Command>) {
        let state_id = self.pending_undo_state_id.take().unwrap_or_else(|| {
            self.next_state_id = self.next_state_id.wrapping_add(1).max(1);
            self.next_state_id
        });
        self.redo_stack.push(HistoryEntry { command, state_id });
    }

    /// Discards the pending undo state ID after a popped redo command is dropped as a no-op.
    pub fn discard_pending_undo(&mut self) {
        self.pending_undo_state_id = None;
    }

    /// Clears the history.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.current_state_id = 0;
        self.pending_redo_state_id = None;
        self.pending_undo_state_id = None;
    }

    /// Returns the current version of the history (length of the undo stack).
    pub fn version(&self) -> usize {
        self.undo_stack.len()
    }

    /// Returns an identity for the current content state.
    ///
    /// Unlike [`Self::version`], this remains unique when a new edit is made
    /// after undoing an earlier edit. Documents use it to determine whether
    /// the current bytes are the same history state that was last saved.
    pub fn state_id(&self) -> usize {
        self.current_state_id
    }

    /// Returns whether an undo operation is available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Returns whether a redo operation is available.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyCommand;
    impl Command for DummyCommand {
        fn execute(&mut self, _: &mut crate::core::document::Document) -> Option<crate::core::command::EditDelta> {
            None
        }
        fn undo(&mut self, _: &mut crate::core::document::Document) -> Option<crate::core::command::EditDelta> {
            None
        }
    }

    #[test]
    fn test_history_push_pop() {
        let mut history = History::new();
        assert_eq!(history.version(), 0);

        history.push(Box::new(DummyCommand));
        assert_eq!(history.version(), 1);

        history.push_redo(Box::new(DummyCommand));
        assert!(history.pop_redo().is_some());

        history.push(Box::new(DummyCommand));
        assert!(history.pop_redo().is_none()); // push clears redo stack

        assert!(history.pop_undo().is_some());
        assert_eq!(history.version(), 1);

        history.clear();
        assert_eq!(history.version(), 0);
    }

    #[test]
    fn test_history_restore_preserves_state_ids() {
        let mut history = History::new();
        history.push(Box::new(DummyCommand));
        let first_id = history.state_id();
        history.push(Box::new(DummyCommand));
        let second_id = history.state_id();
        assert_ne!(first_id, second_id);

        // Simulated failed undo restores the entry and its state_id onto the undo stack.
        let popped = history.pop_undo().expect("undo entry");
        assert_eq!(history.state_id(), first_id);
        history.restore_undo(popped);
        assert_eq!(history.state_id(), second_id);
        assert_eq!(history.version(), 2);
        assert!(!history.can_redo());

        // Move second entry cleanly to redo, then simulate a failed redo restore.
        let popped = history.pop_undo().expect("undo entry");
        history.push_redo(popped);
        assert_eq!(history.state_id(), first_id);

        let redo_cmd = history.pop_redo().expect("redo entry");
        history.restore_redo(redo_cmd);
        assert_eq!(history.state_id(), first_id);
        assert!(history.can_redo());

        // Succeeding redo afterwards must restore exact `second_id`.
        let redo_cmd = history.pop_redo().expect("redo entry");
        history.push_undo(redo_cmd);
        assert_eq!(history.state_id(), second_id);
        assert!(!history.can_redo());
    }
}
