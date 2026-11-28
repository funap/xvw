/// Bounded MRU input history with Up/Down navigation and draft preservation.
#[derive(Clone, Debug)]
pub(super) struct InputHistory {
    entries: Vec<String>,
    cursor: Option<usize>,
    draft: String,
    max_entries: usize,
}

impl InputHistory {
    pub(super) fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            cursor: None,
            draft: String::new(),
            max_entries: max_entries.max(1),
        }
    }

    /// Records a non-empty entry at the most-recent position, deduplicating prior identical entries.
    pub(super) fn push(&mut self, value: &str) {
        if value.is_empty() {
            return;
        }
        if let Some(pos) = self.entries.iter().position(|existing| existing == value) {
            self.entries.remove(pos);
        }
        self.entries.push(value.to_string());
        if self.entries.len() > self.max_entries {
            let overflow = self.entries.len() - self.max_entries;
            self.entries.drain(0..overflow);
        }
        self.cursor = None;
        self.draft.clear();
    }

    /// Resets history browsing when the user manually edits the input text.
    pub(super) fn reset_navigation(&mut self) {
        self.cursor = None;
        self.draft.clear();
    }

    /// Navigates to the previous (older) history entry, preserving `current_input` as the draft.
    pub(super) fn prev(&mut self, current_input: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        let next_idx = match self.cursor {
            None => {
                self.draft = current_input.to_string();
                self.entries.len() - 1
            }
            Some(idx) => idx.saturating_sub(1),
        };
        self.cursor = Some(next_idx);
        self.entries.get(next_idx).map(String::as_str)
    }

    /// Navigates to the next (newer) history entry, restoring the saved draft past the newest entry.
    pub(super) fn next(&mut self) -> Option<&str> {
        let idx = self.cursor?;
        if idx + 1 < self.entries.len() {
            let next_idx = idx + 1;
            self.cursor = Some(next_idx);
            self.entries.get(next_idx).map(String::as_str)
        } else {
            self.cursor = None;
            Some(self.draft.as_str())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::InputHistory;

    #[test]
    fn input_history_navigates_preserves_draft_and_deduplicates() {
        let mut history = InputHistory::new(3);
        assert_eq!(history.prev("draft"), None);
        assert_eq!(history.next(), None);

        history.push("0x10");
        history.push("0x20");
        history.push("0x30");

        assert_eq!(history.prev("0x4"), Some("0x30"));
        assert_eq!(history.prev("ignored"), Some("0x20"));
        assert_eq!(history.prev("ignored"), Some("0x10"));
        // Clamps at oldest entry
        assert_eq!(history.prev("ignored"), Some("0x10"));

        assert_eq!(history.next(), Some("0x20"));
        assert_eq!(history.next(), Some("0x30"));
        // Restores draft past newest
        assert_eq!(history.next(), Some("0x4"));
        assert_eq!(history.next(), None);

        // Deduplicating moves existing entry to most recent and enforces max_entries
        history.push("0x10");
        history.push("0x40");
        assert_eq!(history.prev(""), Some("0x40"));
        assert_eq!(history.prev(""), Some("0x10"));
        assert_eq!(history.prev(""), Some("0x30"));
        // Oldest ("0x20") was evicted
        assert_eq!(history.prev(""), Some("0x30"));
    }

    #[test]
    fn input_history_ignores_empty_and_resets_navigation() {
        let mut history = InputHistory::new(5);
        history.push("");
        assert_eq!(history.prev("abc"), None);

        history.push("first");
        assert_eq!(history.prev("typing"), Some("first"));
        history.reset_navigation();
        assert_eq!(history.next(), None);
    }
}
