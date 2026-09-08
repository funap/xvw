use crate::app_state::InsertModeState;
use crate::core::editor::Editor;
use crate::core::encoding::Encoding;
use crate::core::radix::DisplayRadix;
use crate::ui::components::hex_view::types::EditColumn;
use gpui_kit::*;
use std::ops::Range;

/// Manages active column editing, nibble input buffering, and text/byte deletions.
#[derive(Default, Debug, Clone)]
pub struct InputController {
    pub active_column: EditColumn,
    pub pending_hex_digit: Option<(usize, u8)>,
    pub pending_hex_range: Option<Range<usize>>,
    pub hex_nibble: u8,
}

impl InputController {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear_pending(&mut self) {
        self.pending_hex_digit = None;
        self.pending_hex_range = None;
        self.hex_nibble = 0;
    }

    pub fn is_hex(&self) -> bool {
        self.active_column == EditColumn::Hex
    }

    pub fn is_ascii(&self) -> bool {
        self.active_column == EditColumn::Ascii
    }

    pub fn handle_hex_digit(&mut self, digit: u8, editor: &Entity<Editor>, radix: DisplayRadix, cx: &mut App) -> Option<bool> {
        if radix != DisplayRadix::Hexadecimal || editor.read(cx).is_read_only() {
            return None;
        }

        let selected_range = {
            let ed = editor.read(cx);
            ed.has_selection().then(|| ed.edit_range()).flatten()
        };
        if let Some(range) = selected_range {
            editor.update(cx, |ed, _| {
                ed.set_cursor_offset_exact(range.start);
            });
            self.pending_hex_range = Some(range);
            self.pending_hex_digit = None;
            self.hex_nibble = 0;
        }

        let position = editor.read(cx).cursor.offset;
        if self.hex_nibble == 0 {
            self.pending_hex_digit = Some((position, digit));
            self.hex_nibble = 1;
            return None;
        }

        let high = self
            .pending_hex_digit
            .filter(|(pending_position, _)| *pending_position == position)
            .map(|(_, high)| high)
            .unwrap_or_else(|| editor.read(cx).value_at_cursor().unwrap_or(0) >> 4);
        let value = (high << 4) | digit;
        let insert_mode = InsertModeState::is_enabled(cx);
        let replacement_range = self.pending_hex_range.take();
        let changed = editor.update(cx, |ed, editor_cx| {
            let changed = if let Some(range) = replacement_range {
                if insert_mode {
                    let cursor_after = range.start.saturating_add(1);
                    ed.replace_range_with_cursor(range, vec![value], cursor_after)
                } else {
                    ed.replace_range(range, vec![value])
                }
            } else if insert_mode {
                ed.insert_bytes(position, vec![value])
            } else {
                ed.replace_byte(position, value)
            };
            editor_cx.notify();
            changed
        });
        self.clear_pending();
        Some(changed)
    }

    pub fn handle_ascii_character(&mut self, character: char, editor: &Entity<Editor>, encoding: Encoding, cx: &mut App) -> Option<bool> {
        if character.is_control() || editor.read(cx).is_read_only() {
            return None;
        }

        let replacement = encoding.encode_char(character)?;
        self.clear_pending();
        let insert_mode = InsertModeState::is_enabled(cx);
        let changed = editor.update(cx, |ed, editor_cx| {
            let has_selection = ed.has_selection();
            let changed = if insert_mode && !has_selection {
                let position = ed.cursor.offset;
                ed.insert_bytes(position, replacement)
            } else if has_selection {
                let range = ed.edit_range().expect("selection has an edit range");
                if insert_mode {
                    let cursor_after = range.start.saturating_add(replacement.len());
                    ed.replace_range_with_cursor(range, replacement, cursor_after)
                } else {
                    ed.replace_range(range, replacement)
                }
            } else {
                let position = ed.cursor.offset;
                let range = position..position.saturating_add(replacement.len()).min(ed.total_size());
                ed.replace_range(range, replacement)
            };
            editor_cx.notify();
            changed
        });
        Some(changed)
    }

    pub fn delete_backward(&mut self, editor: &Entity<Editor>, cx: &mut App) -> Option<bool> {
        if editor.read(cx).is_read_only() {
            return None;
        }
        self.clear_pending();
        let changed = editor.update(cx, |ed, editor_cx| {
            let changed = ed.delete_backward();
            if changed {
                editor_cx.notify();
            }
            changed
        });
        Some(changed)
    }

    pub fn delete_forward(&mut self, editor: &Entity<Editor>, cx: &mut App) -> Option<bool> {
        if editor.read(cx).is_read_only() {
            return None;
        }
        self.clear_pending();
        let changed = editor.update(cx, |ed, editor_cx| {
            let changed = ed.delete_forward();
            if changed {
                editor_cx.notify();
            }
            changed
        });
        Some(changed)
    }
}
