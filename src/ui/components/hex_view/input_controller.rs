use crate::core::encoding::Encoding;
use crate::core::radix::DisplayRadix;
use crate::ui::components::hex_view::types::EditColumn;
use std::ops::Range;

/// Represents buffered single-nibble hexadecimal input waiting for a second digit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingHexInput {
    pub offset: usize,
    pub digit: u8,
}

/// A resolved byte modification ready to be applied to the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexCommit {
    pub offset: usize,
    pub value: u8,
    pub replacement_range: Option<Range<usize>>,
}

/// The state transition result of feeding a hexadecimal digit to the controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HexInputResult {
    /// The first nibble was buffered; UI should display visual feedback (e.g. `1_`).
    Pending(PendingHexInput),
    /// A complete byte was formed; the commit should be applied to the document.
    Commit(HexCommit),
    /// The input was rejected (read-only document or non-hexadecimal radix).
    Ignored,
}

/// Manages active column state and multi-step hexadecimal/ASCII input buffering.
///
/// This is a pure, framework-independent state machine with zero GPUI or Editor dependencies.
#[derive(Default, Debug, Clone)]
pub struct InputController {
    active_column: EditColumn,
    pending: Option<PendingHexInput>,
    pending_range: Option<Range<usize>>,
}

impl InputController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn active_column(&self) -> EditColumn {
        self.active_column
    }

    pub fn set_active_column(&mut self, column: EditColumn) {
        self.active_column = column;
    }

    pub fn is_hex(&self) -> bool {
        self.active_column == EditColumn::Hex
    }

    pub fn is_ascii(&self) -> bool {
        self.active_column == EditColumn::Ascii
    }

    pub fn pending_hex_digit(&self) -> Option<(usize, u8)> {
        self.pending.map(|p| (p.offset, p.digit))
    }

    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn clear_pending(&mut self) {
        self.pending = None;
        self.pending_range = None;
    }

    /// Cancels any pending input and returns `true` if a pending edit was active.
    pub fn cancel_pending(&mut self) -> bool {
        let had_pending = self.pending.is_some();
        self.clear_pending();
        had_pending
    }

    /// Handles a hexadecimal digit input.
    pub fn handle_hex_digit(
        &mut self,
        digit: u8,
        cursor_offset: usize,
        selected_range: Option<Range<usize>>,
        is_read_only: bool,
        radix: DisplayRadix,
    ) -> HexInputResult {
        if radix != DisplayRadix::Hexadecimal || is_read_only {
            return HexInputResult::Ignored;
        }

        let target_pos = if let Some(range) = selected_range {
            self.pending_range = Some(range.clone());
            self.pending = None;
            range.start
        } else {
            cursor_offset
        };

        if self.pending.as_ref().map(|p| p.offset) != Some(target_pos) {
            let pending = PendingHexInput { offset: target_pos, digit };
            self.pending = Some(pending);
            HexInputResult::Pending(pending)
        } else {
            let high = self.pending.take().map(|p| p.digit).unwrap_or(0);
            let value = (high << 4) | digit;
            let replacement_range = self.pending_range.take();
            HexInputResult::Commit(HexCommit {
                offset: target_pos,
                value,
                replacement_range,
            })
        }
    }

    /// Commits a pending single digit padded with leading zero (e.g. `'a'` -> `0x0A`),
    /// as triggered by navigation away from the byte (HexEd.it style).
    pub fn commit_pending_with_zero(&mut self, radix: DisplayRadix, is_read_only: bool) -> Option<HexCommit> {
        if radix != DisplayRadix::Hexadecimal || is_read_only {
            self.clear_pending();
            return None;
        }

        let pending = self.pending.take()?;
        let replacement_range = self.pending_range.take();
        Some(HexCommit {
            offset: pending.offset,
            value: pending.digit,
            replacement_range,
        })
    }

    /// Validates and encodes a typed ASCII character.
    pub fn encode_ascii_char(&mut self, character: char, encoding: Encoding, is_read_only: bool) -> Option<Vec<u8>> {
        if character.is_control() || is_read_only {
            return None;
        }
        self.clear_pending();
        encoding.encode_char(character)
    }
}
