//! Bounded Back / Forward jump history for binary editor navigation.

const MAX_NAVIGATION_HISTORY: usize = 100;

/// Tracks explicit cursor jump locations so users can navigate Back and Forward.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NavigationHistory {
    back: Vec<usize>,
    forward: Vec<usize>,
}

impl NavigationHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a jump from `from_offset` to `to_offset`.
    /// No-op when the jump stays at the same offset.
    pub fn record_jump(&mut self, from_offset: usize, to_offset: usize) {
        if from_offset == to_offset {
            return;
        }
        if self.back.last().copied() != Some(from_offset) {
            self.back.push(from_offset);
            if self.back.len() > MAX_NAVIGATION_HISTORY {
                self.back.remove(0);
            }
        }
        self.forward.clear();
    }

    /// Pops the previous jump location, pushing `current_offset` onto the forward stack.
    pub fn go_back(&mut self, current_offset: usize) -> Option<usize> {
        let target = self.back.pop()?;
        if self.forward.last().copied() != Some(current_offset) {
            self.forward.push(current_offset);
            if self.forward.len() > MAX_NAVIGATION_HISTORY {
                self.forward.remove(0);
            }
        }
        Some(target)
    }

    /// Pops the next jump location, pushing `current_offset` onto the back stack.
    pub fn go_forward(&mut self, current_offset: usize) -> Option<usize> {
        let target = self.forward.pop()?;
        if self.back.last().copied() != Some(current_offset) {
            self.back.push(current_offset);
            if self.back.len() > MAX_NAVIGATION_HISTORY {
                self.back.remove(0);
            }
        }
        Some(target)
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// Shifts recorded offsets after an edit so history entries stay aligned with moved bytes.
    pub fn adjust_after_edit(&mut self, start: usize, old_len: usize, new_len: usize, total_size: usize) {
        let shift = |offset: &mut usize| {
            *offset = crate::core::document::shift_offset_after_edit(*offset, start, old_len, new_len).min(total_size);
        };
        for offset in &mut self.back {
            shift(offset);
        }
        for offset in &mut self.forward {
            shift(offset);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_navigation_history_back_forward_and_branch_clear() {
        let mut history = NavigationHistory::new();
        assert!(!history.can_go_back());
        assert!(!history.can_go_forward());

        // Same-offset jump is ignored
        history.record_jump(10, 10);
        assert!(!history.can_go_back());

        history.record_jump(0, 100);
        history.record_jump(100, 250);
        assert!(history.can_go_back());
        assert!(!history.can_go_forward());

        assert_eq!(history.go_back(250), Some(100));
        assert!(history.can_go_forward());
        assert_eq!(history.go_back(100), Some(0));
        assert!(!history.can_go_back());

        assert_eq!(history.go_forward(0), Some(100));
        // Recording a new jump clears the forward stack
        history.record_jump(100, 500);
        assert!(!history.can_go_forward());
        assert_eq!(history.go_back(500), Some(100));
    }

    #[test]
    fn test_navigation_history_adjusts_after_edit_and_respects_capacity() {
        let mut history = NavigationHistory::new();
        for i in 0..150 {
            history.record_jump(i, i + 1);
        }
        assert_eq!(history.back.len(), MAX_NAVIGATION_HISTORY);
        assert_eq!(history.back.first().copied(), Some(50));

        history.adjust_after_edit(60, 0, 10, 1000);
        assert_eq!(history.back.first().copied(), Some(50));
        assert_eq!(history.back.last().copied(), Some(159));
    }
}
