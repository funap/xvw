use super::HexView;
use super::actions::*;
use super::clipboard_handler::{ClipboardHandler, effective_copy_range};
use super::input_controller::{HexCommit, HexInputResult};
use super::types::*;
use crate::actions::{
    AddCustomBreak, BookmarkBlue, BookmarkCyan, BookmarkGreen, BookmarkOrange, BookmarkPink, BookmarkPurple, BookmarkRed, BookmarkYellow, ClearAllBookmarks,
    ClearAllCustomBreaks, ClearBookmark, Copy, CopyAsBase64, CopyAsBinary, CopyAsCppArray, CopyAsEscapedString, CopyAsHexDump, CopyAsHexSpaces,
    CopyAsHexStream, CopyAsJsonArray, CopyAsPrintableText, CopyAsRustArray, Cut, HideAllBookmarks, JoinLine, Paste, Redo, RemoveCustomBreakBackward,
    RemoveCustomBreakForward, RemoveCustomBreaksInRange, SearchNext, SearchPrev, SetByteOrderBigEndian, SetByteOrderLittleEndian, SetGroupSize1, SetGroupSize2,
    SetGroupSize4, SetGroupSize8, SetRadixBin, SetRadixDec, SetRadixHex, SetRadixOct, ShowAllBookmarks, ToggleByteOrder, ToggleHideUnbookmarked, ToggleSearch,
    Undo, UnfoldBookmarkAtCursor,
};
use crate::app_state::InsertModeState;
use crate::core::bookmark::BookmarkColor;
use crate::core::editor::Editor;
use crate::core::format::CopyFormat;
use crate::core::radix::{ByteGroupSize, DisplayRadix};
use crate::ui::color::BookmarkColorExt;
use gpui_kit::{App, Context, Hsla, KeyDownEvent, PromptLevel, Window};

impl HexView {
    pub(super) fn clear_pending_hex_input(&mut self) {
        self.input.clear_pending();
    }

    pub(super) fn apply_hex_commit(&mut self, commit: HexCommit, advance_cursor: bool, cx: &mut Context<Self>) -> bool {
        let insert_mode = InsertModeState::is_enabled(cx);
        let (group_size, is_big_endian) = {
            let ed = self.editor.read(cx);
            (ed.options.group_size, ed.options.is_big_endian)
        };
        let changed = self.editor.update(cx, |ed, editor_cx| {
            let total = ed.total_size();
            let changed = if let Some(range) = commit.replacement_range {
                let cursor_after = if advance_cursor { range.start.saturating_add(1) } else { commit.offset };
                ed.replace_range_with_cursor(range, vec![commit.value], cursor_after)
            } else if insert_mode {
                let pos = commit.offset.min(total);
                let cursor_after = if advance_cursor { pos.saturating_add(1) } else { pos };
                ed.replace_range_with_cursor(pos..pos, vec![commit.value], cursor_after)
            } else if commit.offset >= total {
                let cursor_after = if advance_cursor { total.saturating_add(1) } else { total };
                ed.replace_range_with_cursor(total..total, vec![commit.value], cursor_after)
            } else {
                let next_cursor = if advance_cursor {
                    crate::core::radix::next_visual_byte(commit.offset, total, group_size, is_big_endian)
                } else {
                    commit.offset
                };
                ed.replace_range_with_cursor(commit.offset..commit.offset + 1, vec![commit.value], next_cursor)
            };
            editor_cx.notify();
            changed
        });
        self.edit_changed(changed, cx);
        changed
    }

    pub(super) fn commit_pending_hex_with_zero(&mut self, cx: &mut Context<Self>) {
        if !self.input.has_pending() {
            return;
        }
        let is_read_only = self.editor.read(cx).is_read_only();
        if let Some(commit) = self.input.commit_pending_with_zero(self.radix, is_read_only) {
            self.apply_hex_commit(commit, false, cx);
        }
    }

    pub(super) fn edit_column_is_hex(&self) -> bool {
        self.input.is_hex()
    }

    pub(super) fn edit_column_is_ascii(&self) -> bool {
        self.input.is_ascii()
    }

    pub(super) fn edit_changed(&mut self, changed: bool, cx: &mut Context<Self>) {
        self.pause_cursor_blink(cx);
        if changed {
            self.notify_document_changed(cx);
        }
        cx.notify();
    }

    pub(super) fn pause_cursor_blink(&mut self, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.cursor_blink.update(cx, |cursor, cx| {
            if insert_mode {
                cursor.pause(cx);
            } else {
                cursor.stop(cx);
            }
        });
    }

    pub(super) fn handle_hex_digit(&mut self, digit: u8, window: &mut Window, cx: &mut Context<Self>) {
        if self.radix != DisplayRadix::Hexadecimal || self.editor.read(cx).is_read_only() {
            return;
        }

        cx.focus_self(window);
        let (cursor_offset, selected_range, is_read_only) = {
            let ed = self.editor.read(cx);
            let sel = ed.has_selection().then(|| ed.edit_range()).flatten();
            (ed.cursor.offset, sel, ed.is_read_only())
        };

        if let Some(ref range) = selected_range {
            self.editor.update(cx, |ed, cx| {
                ed.set_cursor_offset_exact(range.start);
                cx.notify();
            });
        }

        match self.input.handle_hex_digit(digit, cursor_offset, selected_range, is_read_only, self.radix) {
            HexInputResult::Pending(_) => {
                cx.notify();
            }
            HexInputResult::Commit(commit) => {
                self.apply_hex_commit(commit, true, cx);
            }
            HexInputResult::Ignored => {}
        }
    }

    pub(super) fn handle_ascii_character(&mut self, character: char, window: &mut Window, cx: &mut Context<Self>) {
        if character.is_control() || self.editor.read(cx).is_read_only() {
            return;
        }

        cx.focus_self(window);
        let is_read_only = self.editor.read(cx).is_read_only();
        let Some(replacement) = self.input.encode_ascii_char(character, self.encoding, is_read_only) else {
            return;
        };

        let insert_mode = InsertModeState::is_enabled(cx);
        let changed = self.editor.update(cx, |ed, editor_cx| {
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
                let total = ed.total_size();
                if position >= total {
                    let cursor_after = total.saturating_add(replacement.len());
                    ed.replace_range_with_cursor(total..total, replacement, cursor_after)
                } else {
                    let range = position..position.saturating_add(replacement.len()).min(total);
                    ed.replace_range(range, replacement)
                }
            };
            editor_cx.notify();
            changed
        });
        self.edit_changed(changed, cx);
    }

    pub(super) fn delete_backward_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::core::editor::OverwriteDeleteBehavior;
        use gpui_kit::component::WindowExt as _;
        use gpui_kit::component::notification::Notification;

        cx.focus_self(window);
        if self.edit_column_is_hex() && self.input.cancel_pending() {
            cx.notify();
            return;
        }
        self.input.clear_pending();
        if self.editor.read(cx).is_read_only() {
            return;
        }
        let insert_mode = InsertModeState::is_enabled(cx);
        let overwrite_behavior = cx.try_global::<OverwriteDeleteBehavior>().copied().unwrap_or_default();
        if !insert_mode && overwrite_behavior == OverwriteDeleteBehavior::Disallow {
            window.push_notification(
                Notification::warning("Overwrite mode protects file size — switch to Insert mode or enable Zero-Fill in Settings"),
                cx,
            );
            return;
        }
        let changed = self.editor.update(cx, |ed, editor_cx| {
            let changed = if insert_mode { ed.delete_backward() } else { ed.zero_fill_backward() };
            if changed {
                editor_cx.notify();
            }
            changed
        });
        self.edit_changed(changed, cx);
    }

    pub(super) fn delete_forward_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::core::editor::OverwriteDeleteBehavior;
        use gpui_kit::component::WindowExt as _;
        use gpui_kit::component::notification::Notification;

        cx.focus_self(window);
        self.input.clear_pending();
        if self.editor.read(cx).is_read_only() {
            return;
        }
        let insert_mode = InsertModeState::is_enabled(cx);
        let overwrite_behavior = cx.try_global::<OverwriteDeleteBehavior>().copied().unwrap_or_default();
        if !insert_mode && overwrite_behavior == OverwriteDeleteBehavior::Disallow {
            window.push_notification(
                Notification::warning("Overwrite mode protects file size — switch to Insert mode or enable Zero-Fill in Settings"),
                cx,
            );
            return;
        }
        let changed = self.editor.update(cx, |ed, editor_cx| {
            let changed = if insert_mode { ed.delete_forward() } else { ed.zero_fill_forward() };
            if changed {
                editor_cx.notify();
            }
            changed
        });
        self.edit_changed(changed, cx);
    }

    pub(super) fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.pause_cursor_blink(cx);
        let modifiers = event.keystroke.modifiers;
        if modifiers.control || modifiers.platform || modifiers.alt || modifiers.function {
            return;
        }

        match event.keystroke.key.as_str() {
            "backspace" => {
                self.delete_backward_key(window, cx);
                return;
            }
            "delete" => {
                self.delete_forward_key(window, cx);
                return;
            }
            "escape" => {
                if self.input.cancel_pending() {
                    cx.notify();
                    return;
                }
                if self.editor.read(cx).has_selection() {
                    self.editor.update(cx, |editor, cx| {
                        editor.clear_selection();
                        cx.notify();
                    });
                    cx.emit(HexViewEvent::SelectionChanged { start: None, end: None });
                    cx.notify();
                    return;
                }
                cx.emit(HexViewEvent::EscapePressed);
                return;
            }
            _ => {}
        }

        let Some(key_char) = event.keystroke.key_char.as_deref() else {
            return;
        };
        let mut characters = key_char.chars();
        let Some(character) = characters.next() else {
            return;
        };
        if characters.next().is_some() {
            return;
        }

        if self.edit_column_is_hex()
            && let Some(digit) = hex_digit(character)
        {
            self.handle_hex_digit(digit, window, cx);
        } else if self.edit_column_is_ascii() {
            self.handle_ascii_character(character, window, cx);
        }
    }

    pub fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        if self.editor.read(cx).is_read_only() {
            return;
        }
        self.clear_pending_hex_input();
        let changed = self.editor.update(cx, |editor, editor_cx| {
            let changed = editor.undo();
            if changed {
                editor_cx.notify();
            }
            changed
        });
        self.edit_changed(changed, cx);
    }

    pub fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        if self.editor.read(cx).is_read_only() {
            return;
        }
        self.clear_pending_hex_input();
        let changed = self.editor.update(cx, |editor, editor_cx| {
            let changed = editor.redo();
            if changed {
                editor_cx.notify();
            }
            changed
        });
        self.edit_changed(changed, cx);
    }

    pub fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_pending_hex_input();
        let changed = ClipboardHandler::cut(&self.editor, &self.focus_handle, window, cx);
        self.edit_changed(changed, cx);
    }

    pub fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_pending_hex_input();
        let changed = ClipboardHandler::paste(&self.editor, &self.focus_handle, window, cx);
        self.edit_changed(changed, cx);
    }

    pub(super) fn can_handle_vi_action(&self, cx: &App) -> bool {
        self.edit_column_is_hex() || self.editor.read(cx).is_read_only()
    }

    pub(super) fn vi_move_left(&mut self, _: &ViMoveLeft, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            self.handle_move_left(window, cx);
        } else {
            cx.propagate();
        }
    }

    pub(super) fn vi_move_right(&mut self, _: &ViMoveRight, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            self.handle_move_right(window, cx);
        } else {
            cx.propagate();
        }
    }

    pub(super) fn vi_move_up(&mut self, _: &ViMoveUp, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            self.exec_move(window, cx, |editor| editor.move_up());
        } else {
            cx.propagate();
        }
    }

    pub(super) fn vi_move_down(&mut self, _: &ViMoveDown, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            let insert_mode = InsertModeState::is_enabled(cx);
            self.exec_move(window, cx, move |editor| {
                if insert_mode {
                    editor.move_down_for_insert();
                } else {
                    editor.move_down();
                }
            });
        } else {
            cx.propagate();
        }
    }

    pub(super) fn vi_select_left(&mut self, _: &ViSelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            self.handle_select_left(window, cx);
        } else {
            cx.propagate();
        }
    }

    pub(super) fn vi_select_right(&mut self, _: &ViSelectRight, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            self.handle_select_right(window, cx);
        } else {
            cx.propagate();
        }
    }

    pub(super) fn vi_select_up(&mut self, _: &ViSelectUp, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            let insert_mode = InsertModeState::is_enabled(cx);
            self.exec_select(window, cx, move |editor| {
                if insert_mode {
                    editor.select_up_for_insert();
                } else {
                    editor.select_up();
                }
            });
        } else {
            cx.propagate();
        }
    }

    pub(super) fn vi_select_down(&mut self, _: &ViSelectDown, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            let insert_mode = InsertModeState::is_enabled(cx);
            self.exec_select(window, cx, move |editor| {
                if insert_mode {
                    editor.select_down_for_insert();
                } else {
                    editor.select_down();
                }
            });
        } else {
            cx.propagate();
        }
    }

    pub(super) fn exec_move(&mut self, window: &mut Window, cx: &mut Context<Self>, f: impl FnOnce(&mut Editor)) {
        cx.focus_self(window);
        self.pause_cursor_blink(cx);
        self.commit_pending_hex_with_zero(cx);
        self.editor.update(cx, |editor, cx| {
            f(editor);
            cx.notify();
        });
        let cursor_offset = self.editor.read(cx).cursor.offset;
        cx.emit(HexViewEvent::CursorMoved(cursor_offset));
    }

    pub(super) fn exec_select(&mut self, window: &mut Window, cx: &mut Context<Self>, f: impl FnOnce(&mut Editor)) {
        cx.focus_self(window);
        self.pause_cursor_blink(cx);
        self.commit_pending_hex_with_zero(cx);
        self.editor.update(cx, |editor, cx| {
            f(editor);
            cx.notify();
        });
        let (start, end) = {
            let editor = self.editor.read(cx);
            let range = editor.selection_range();
            (range.as_ref().map(|range| range.start), range.as_ref().map(|range| range.end.saturating_sub(1)))
        };
        cx.emit(HexViewEvent::SelectionChanged { start, end });
    }

    pub(super) fn handle_move_left(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.is_ascii() {
            let encoding = self.encoding;
            let insert_mode = InsertModeState::is_enabled(cx);
            let action = {
                let editor = self.editor.read(cx);
                if let Ok(doc) = editor.document.read() {
                    let buf = doc.buffer.data();
                    let current = editor.cursor.offset;
                    let target = encoding.prev_char_boundary(buf, current);
                    let char_range = encoding.char_range_at(buf, target);
                    Some((target, char_range))
                } else {
                    None
                }
            };
            self.exec_move(window, cx, |editor| {
                if let Some((target, char_range)) = action {
                    if insert_mode {
                        editor.set_cursor_offset_exact(target);
                    } else if char_range.end > char_range.start {
                        editor.set_selection_range(char_range);
                    } else {
                        editor.set_cursor_offset_exact(target);
                    }
                } else if insert_mode {
                    editor.move_left_for_insert();
                } else {
                    editor.move_left();
                }
            });
        } else {
            let insert_mode = InsertModeState::is_enabled(cx);
            self.exec_move(window, cx, move |editor| {
                if insert_mode {
                    editor.move_left_for_insert();
                } else {
                    let cur = editor.cursor.offset;
                    let prev = crate::core::radix::prev_visual_byte(cur, editor.total_size(), editor.options.group_size, editor.options.is_big_endian);
                    editor.set_cursor_offset_exact(prev);
                }
            });
        }
    }

    pub(super) fn handle_move_right(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.is_ascii() {
            let encoding = self.encoding;
            let insert_mode = InsertModeState::is_enabled(cx);
            let action = {
                let editor = self.editor.read(cx);
                if let Ok(doc) = editor.document.read() {
                    let buf = doc.buffer.data();
                    let current = editor.cursor.offset;
                    let target = encoding.next_char_boundary(buf, current);
                    let char_range = encoding.char_range_at(buf, target);
                    let buf_len = buf.len();
                    Some((target, char_range, buf_len))
                } else {
                    None
                }
            };
            self.exec_move(window, cx, |editor| {
                if let Some((target, char_range, buf_len)) = action {
                    if target < buf_len {
                        if insert_mode {
                            editor.set_cursor_offset_exact(target);
                        } else if char_range.end > char_range.start {
                            editor.set_selection_range(char_range);
                        } else {
                            editor.set_cursor_offset_exact(target);
                        }
                    } else {
                        editor.set_cursor_offset_exact(buf_len);
                    }
                } else if insert_mode {
                    editor.move_right_for_insert();
                } else {
                    editor.move_right();
                }
            });
        } else {
            let insert_mode = InsertModeState::is_enabled(cx);
            self.exec_move(window, cx, move |editor| {
                if insert_mode {
                    editor.move_right_for_insert();
                } else {
                    let cur = editor.cursor.offset;
                    let next = crate::core::radix::next_visual_byte(cur, editor.total_size(), editor.options.group_size, editor.options.is_big_endian);
                    editor.set_cursor_offset_exact(next);
                }
            });
        }
    }

    pub(super) fn handle_select_left(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.is_ascii() {
            let encoding = self.encoding;
            let insert_mode = InsertModeState::is_enabled(cx);
            let drag_params = {
                let editor = self.editor.read(cx);
                if let Ok(doc) = editor.document.read() {
                    let buf = doc.buffer.data();
                    let anchor = if editor.has_selection() {
                        editor.selection().anchor()
                    } else {
                        editor.cursor.offset
                    };
                    let active = if editor.has_selection() {
                        editor.selection().active()
                    } else {
                        editor.cursor.offset
                    };
                    let target = encoding.prev_char_boundary(buf, active);
                    Some((anchor, target))
                } else {
                    None
                }
            };
            self.exec_select(window, cx, move |editor| {
                if let Some((anchor, target)) = drag_params {
                    editor.continue_drag(anchor, target);
                } else if insert_mode {
                    editor.select_left_for_insert();
                } else {
                    editor.select_left();
                }
            });
        } else {
            let insert_mode = InsertModeState::is_enabled(cx);
            self.exec_select(window, cx, move |editor| {
                if insert_mode {
                    editor.select_left_for_insert();
                } else {
                    editor.select_left();
                }
            });
        }
    }

    pub(super) fn handle_select_right(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.is_ascii() {
            let encoding = self.encoding;
            let insert_mode = InsertModeState::is_enabled(cx);
            let drag_params = {
                let editor = self.editor.read(cx);
                if let Ok(doc) = editor.document.read() {
                    let buf = doc.buffer.data();
                    let anchor = if editor.has_selection() {
                        editor.selection().anchor()
                    } else {
                        editor.cursor.offset
                    };
                    let active = if editor.has_selection() {
                        editor.selection().active()
                    } else {
                        editor.cursor.offset
                    };
                    let target = encoding.next_char_boundary(buf, active);
                    Some((anchor, target))
                } else {
                    None
                }
            };
            self.exec_select(window, cx, move |editor| {
                if let Some((anchor, target)) = drag_params {
                    editor.continue_drag(anchor, target);
                } else if insert_mode {
                    editor.select_right_for_insert();
                } else {
                    editor.select_right();
                }
            });
        } else {
            let insert_mode = InsertModeState::is_enabled(cx);
            self.exec_select(window, cx, move |editor| {
                if insert_mode {
                    editor.select_right_for_insert();
                } else {
                    editor.select_right();
                }
            });
        }
    }

    pub(super) fn move_left(&mut self, _: &MoveLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_move_left(window, cx);
    }

    pub(super) fn move_right(&mut self, _: &MoveRight, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_move_right(window, cx);
    }

    pub(super) fn move_up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        self.exec_move(window, cx, |e| e.move_up());
    }

    pub(super) fn move_down(&mut self, _: &MoveDown, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_move(window, cx, move |editor| {
            if insert_mode {
                editor.move_down_for_insert();
            } else {
                editor.move_down();
            }
        });
    }

    pub(super) fn select_left(&mut self, _: &SelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_select_left(window, cx);
    }

    pub(super) fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_select_right(window, cx);
    }

    pub(super) fn select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_select(window, cx, move |editor| {
            if insert_mode {
                editor.select_up_for_insert();
            } else {
                editor.select_up();
            }
        });
    }

    pub(super) fn select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_select(window, cx, move |editor| {
            if insert_mode {
                editor.select_down_for_insert();
            } else {
                editor.select_down();
            }
        });
    }

    pub(super) fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        self.exec_select(window, cx, |e| e.select_all());
    }

    pub(super) fn page_up(&mut self, _: &PageUp, window: &mut Window, cx: &mut Context<Self>) {
        let visible_rows = if let Some(bounds) = self.list_bounds.get() {
            (f32::from(bounds.size.height) / ROW_HEIGHT).floor() as usize
        } else {
            30
        };
        let count = visible_rows.saturating_sub(2).max(1);
        self.exec_move(window, cx, |e| {
            for _ in 0..count {
                e.move_up();
            }
        });
    }

    pub(super) fn page_down(&mut self, _: &PageDown, window: &mut Window, cx: &mut Context<Self>) {
        let visible_rows = if let Some(bounds) = self.list_bounds.get() {
            (f32::from(bounds.size.height) / ROW_HEIGHT).floor() as usize
        } else {
            30
        };
        let count = visible_rows.saturating_sub(2).max(1);
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_move(window, cx, |e| {
            for _ in 0..count {
                if insert_mode {
                    e.move_down_for_insert();
                } else {
                    e.move_down();
                }
            }
        });
    }

    pub(super) fn home(&mut self, _: &Home, window: &mut Window, cx: &mut Context<Self>) {
        self.exec_move(window, cx, |e| e.move_to_line_start());
    }

    pub(super) fn end(&mut self, _: &End, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_move(window, cx, move |e| e.move_to_line_end(insert_mode));
    }

    pub(super) fn move_group_left(&mut self, _: &MoveGroupLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.exec_move(window, cx, |e| e.move_group_left());
    }

    pub(super) fn move_group_right(&mut self, _: &MoveGroupRight, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_move(window, cx, move |e| e.move_group_right(insert_mode));
    }

    pub(super) fn select_group_left(&mut self, _: &SelectGroupLeft, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_select(window, cx, move |e| e.select_group_left(insert_mode));
    }

    pub(super) fn select_group_right(&mut self, _: &SelectGroupRight, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_select(window, cx, move |e| e.select_group_right(insert_mode));
    }

    pub fn toggle_edit_column(&mut self, _: &crate::actions::ToggleEditColumn, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        let is_struct_mode = {
            let ed = self.editor.read(cx);
            ed.structure.show_inline_structure_view && ed.parse_result().is_some()
        };
        if !self.show_ascii || is_struct_mode {
            return;
        }
        self.commit_pending_hex_with_zero(cx);
        let next = if self.input.is_hex() { EditColumn::Ascii } else { EditColumn::Hex };
        self.input.set_active_column(next);
        self.pause_cursor_blink(cx);
        cx.notify();
    }

    pub(super) fn select_page_up(&mut self, _: &SelectPageUp, window: &mut Window, cx: &mut Context<Self>) {
        let visible_rows = if let Some(bounds) = self.list_bounds.get() {
            (f32::from(bounds.size.height) / ROW_HEIGHT).floor() as usize
        } else {
            30
        };
        let count = visible_rows.saturating_sub(2).max(1);
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_select(window, cx, move |editor| {
            for _ in 0..count {
                if insert_mode {
                    editor.select_up_for_insert();
                } else {
                    editor.select_up();
                }
            }
        });
    }

    pub(super) fn select_page_down(&mut self, _: &SelectPageDown, window: &mut Window, cx: &mut Context<Self>) {
        let visible_rows = if let Some(bounds) = self.list_bounds.get() {
            (f32::from(bounds.size.height) / ROW_HEIGHT).floor() as usize
        } else {
            30
        };
        let count = visible_rows.saturating_sub(2).max(1);
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_select(window, cx, move |editor| {
            for _ in 0..count {
                if insert_mode {
                    editor.select_down_for_insert();
                } else {
                    editor.select_down();
                }
            }
        });
    }

    pub(super) fn select_home(&mut self, _: &SelectHome, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_select(window, cx, move |editor| {
            editor.select_to_line_start(insert_mode);
        });
    }

    pub(super) fn select_end(&mut self, _: &SelectEnd, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_select(window, cx, move |editor| {
            editor.select_to_line_end(insert_mode);
        });
    }

    pub(super) fn trigger_search(&mut self, _: &TriggerSearch, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            window.dispatch_action(Box::new(ToggleSearch), cx);
        } else {
            cx.propagate();
        }
    }

    pub(super) fn trigger_search_next(&mut self, _: &TriggerSearchNext, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            window.dispatch_action(Box::new(SearchNext), cx);
        } else {
            cx.propagate();
        }
    }

    pub(super) fn trigger_search_prev(&mut self, _: &TriggerSearchPrev, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            window.dispatch_action(Box::new(SearchPrev), cx);
        } else {
            cx.propagate();
        }
    }

    pub(super) fn notify_document_changed(&self, cx: &mut App) {
        let path = self.editor.read(cx).document.read().ok().map(|d| d.path().to_path_buf());
        if let Some(path) = path {
            let service = crate::app_state::AppState::global(cx).document_service.clone();
            service.notify_document_changed(&path, cx);
        }
    }

    pub fn add_custom_break(&mut self, _: &AddCustomBreak, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.cursor_reveal_pending = true;
        self.editor.update(cx, |editor, cx| {
            let offset = editor.cursor.offset;
            if offset > 0 {
                editor.custom_layout_mut().toggle_break(offset);
            }
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn remove_custom_breaks_in_range(&mut self, _: &RemoveCustomBreaksInRange, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.cursor_reveal_pending = true;
        self.editor.update(cx, |editor, cx| {
            if let Some(range) = editor.selection_range() {
                editor.custom_layout_mut().remove_breaks_in_range(range);
            } else {
                let offset = editor.cursor.offset;
                let line_starts = editor.line_starts();
                let line_idx = match line_starts.binary_search(&offset) {
                    Ok(idx) => idx,
                    Err(idx) => idx.saturating_sub(1),
                };
                let line_start = line_starts.get(line_idx).unwrap_or(0);
                if editor.custom_layout().has_break(line_start) {
                    editor.custom_layout_mut().remove_break(line_start);
                }
            }
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn remove_custom_break_backward(&mut self, _: &RemoveCustomBreakBackward, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.editor.update(cx, |editor, cx| {
            let offset = editor.cursor.offset;
            if offset > 0 && editor.custom_layout().has_break(offset - 1) {
                editor.custom_layout_mut().remove_break(offset - 1);
            }
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn remove_custom_break_forward(&mut self, _: &RemoveCustomBreakForward, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.editor.update(cx, |editor, cx| {
            let offset = editor.cursor.offset;
            if editor.custom_layout().has_break(offset) {
                editor.custom_layout_mut().remove_break(offset);
            }
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn join_line(&mut self, _: &JoinLine, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.cursor_reveal_pending = true;
        self.editor.update(cx, |editor, cx| {
            editor.custom_layout_mut().join_line();
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn clear_all_custom_breaks(&mut self, _: &ClearAllCustomBreaks, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.editor.update(cx, |editor, cx| {
            editor.custom_layout_mut().clear_all();
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn show_all_bookmarks(&mut self, _: &ShowAllBookmarks, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.cursor_reveal_pending = true;
        self.editor.update(cx, |editor, cx| {
            editor.bookmarks_mut().show_all();
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn hide_all_bookmarks(&mut self, _: &HideAllBookmarks, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.cursor_reveal_pending = true;
        self.editor.update(cx, |editor, cx| {
            editor.bookmarks_mut().hide_all();
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn toggle_bookmark_color(&mut self, color: crate::core::bookmark::BookmarkColor, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.cursor_reveal_pending = true;
        self.editor.update(cx, |editor, cx| {
            editor.bookmarks_mut().toggle_color(color);
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn show_only_bookmark_color(&mut self, color: crate::core::bookmark::BookmarkColor, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.cursor_reveal_pending = true;
        self.editor.update(cx, |editor, cx| {
            editor.bookmarks_mut().show_only_color(color);
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn toggle_hide_unbookmarked(&mut self, _: &ToggleHideUnbookmarked, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.cursor_reveal_pending = true;
        self.editor.update(cx, |editor, cx| {
            editor.bookmarks_mut().toggle_hide_unbookmarked();
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn unfold_bookmark_at_cursor(&mut self, _: &UnfoldBookmarkAtCursor, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.cursor_reveal_pending = true;
        self.editor.update(cx, |editor, cx| {
            let offset = editor.cursor.offset;
            editor.unfold_bookmark_at(offset);
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn copy_range(&self, cx: &App) -> Option<std::ops::Range<usize>> {
        let editor = self.editor.read(cx);
        let doc = editor.document.read().ok()?;
        let selection = editor.selection_range();
        let insert_mode = InsertModeState::is_enabled(cx);
        effective_copy_range(
            selection,
            insert_mode,
            editor.cursor.offset,
            doc.buffer.len(),
            self.input.active_column(),
            self.radix,
            self.group_size,
            self.encoding,
            doc.buffer.data(),
        )
    }

    pub(super) fn copy_formatted(&self, format: CopyFormat, window: &mut Window, cx: &mut Context<Self>) {
        let range = self.copy_range(cx);
        ClipboardHandler::copy_formatted_range(&self.editor, &self.focus_handle, range, format, window, cx);
    }

    pub fn copy(&mut self, _: &Copy, window: &mut Window, cx: &mut Context<Self>) {
        let range = self.copy_range(cx);
        ClipboardHandler::copy_range(&self.editor, &self.focus_handle, range, window, cx);
    }

    pub fn copy_as_hexdump(&mut self, _: &CopyAsHexDump, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::HexDump, window, cx);
    }

    pub fn copy_as_cpp_array(&mut self, _: &CopyAsCppArray, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::CppArray, window, cx);
    }

    pub fn copy_as_hex_stream(&mut self, _: &CopyAsHexStream, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::HexStream, window, cx);
    }

    pub fn copy_as_hex_spaces(&mut self, _: &CopyAsHexSpaces, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::HexWithSpaces, window, cx);
    }

    pub fn copy_as_printable_text(&mut self, _: &CopyAsPrintableText, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::PrintableText, window, cx);
    }

    pub fn copy_as_base64(&mut self, _: &CopyAsBase64, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::Base64, window, cx);
    }

    pub fn copy_as_escaped_string(&mut self, _: &CopyAsEscapedString, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::EscapedString, window, cx);
    }

    pub fn copy_as_binary(&mut self, _: &CopyAsBinary, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::Binary, window, cx);
    }

    pub fn copy_as_rust_array(&mut self, _: &CopyAsRustArray, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::RustArray, window, cx);
    }

    pub fn copy_as_json_array(&mut self, _: &CopyAsJsonArray, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::JsonArray, window, cx);
    }

    pub(super) fn apply_bookmark(&mut self, color: Option<Hsla>, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
        self.editor.update(cx, |editor, cx| {
            if let Some(range) = editor.selected_range_or_cursor() {
                if let Some(color) = color {
                    editor.bookmarks_mut().add_custom(range, color.into());
                } else {
                    editor.bookmarks_mut().clear_custom(range);
                }
                cx.notify();
            }
        });
        self.notify_document_changed(cx);
    }

    pub fn bookmark_red(&mut self, _: &BookmarkRed, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_bookmark(Some(BookmarkColor::Red.to_hsla()), window, cx);
    }

    pub fn bookmark_orange(&mut self, _: &BookmarkOrange, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_bookmark(Some(BookmarkColor::Orange.to_hsla()), window, cx);
    }

    pub fn bookmark_yellow(&mut self, _: &BookmarkYellow, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_bookmark(Some(BookmarkColor::Yellow.to_hsla()), window, cx);
    }

    pub fn bookmark_green(&mut self, _: &BookmarkGreen, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_bookmark(Some(BookmarkColor::Green.to_hsla()), window, cx);
    }

    pub fn bookmark_cyan(&mut self, _: &BookmarkCyan, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_bookmark(Some(BookmarkColor::Cyan.to_hsla()), window, cx);
    }

    pub fn bookmark_blue(&mut self, _: &BookmarkBlue, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_bookmark(Some(BookmarkColor::Blue.to_hsla()), window, cx);
    }

    pub fn bookmark_purple(&mut self, _: &BookmarkPurple, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_bookmark(Some(BookmarkColor::Purple.to_hsla()), window, cx);
    }

    pub fn bookmark_pink(&mut self, _: &BookmarkPink, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_bookmark(Some(BookmarkColor::Pink.to_hsla()), window, cx);
    }

    pub fn clear_bookmark(&mut self, _: &ClearBookmark, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_bookmark(None, window, cx);
    }

    pub fn clear_all_bookmarks(&mut self, _: &ClearAllBookmarks, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.editor.read(cx).bookmarks().snapshot().len();
        if count == 0 {
            return;
        }

        let prompt = window.prompt(
            PromptLevel::Warning,
            "Clear all bookmarks?",
            Some(&format!(
                "Are you sure you want to clear all {} bookmark{} and comments? This action cannot be undone.",
                count,
                if count == 1 { "" } else { "s" }
            )),
            &["Clear All", "Cancel"],
            cx,
        );

        let editor = self.editor.clone();
        let doc_path = editor.read(cx).document.read().ok().map(|d| d.path().to_path_buf());
        cx.spawn_in(window, async move |_this, window| {
            if let Ok(0) = prompt.await {
                window
                    .update(|_, cx| {
                        editor.update(cx, |editor, cx| {
                            editor.bookmarks_mut().clear_all();
                            cx.notify();
                        });
                        if let Some(ref path) = doc_path {
                            let service = crate::app_state::AppState::global(cx).document_service.clone();
                            service.notify_document_changed(path, cx);
                        }
                    })
                    .ok();
            }
        })
        .detach();
    }

    pub(super) fn set_radix_hex(&mut self, _: &SetRadixHex, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_radix(DisplayRadix::Hexadecimal);
            cx.notify();
        });
    }

    pub(super) fn set_radix_dec(&mut self, _: &SetRadixDec, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_radix(DisplayRadix::Decimal);
            cx.notify();
        });
    }

    pub(super) fn set_radix_oct(&mut self, _: &SetRadixOct, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_radix(DisplayRadix::Octal);
            cx.notify();
        });
    }

    pub(super) fn set_radix_bin(&mut self, _: &SetRadixBin, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_radix(DisplayRadix::Binary);
            cx.notify();
        });
    }

    pub(super) fn set_group_size_1(&mut self, _: &SetGroupSize1, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_group_size(ByteGroupSize::One);
            cx.notify();
        });
    }

    pub(super) fn set_group_size_2(&mut self, _: &SetGroupSize2, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_group_size(ByteGroupSize::Two);
            cx.notify();
        });
    }

    pub(super) fn set_group_size_4(&mut self, _: &SetGroupSize4, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_group_size(ByteGroupSize::Four);
            cx.notify();
        });
    }

    pub(super) fn set_group_size_8(&mut self, _: &SetGroupSize8, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_group_size(ByteGroupSize::Eight);
            cx.notify();
        });
    }

    pub(super) fn set_byte_order_le(&mut self, _: &SetByteOrderLittleEndian, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_is_big_endian(false);
            cx.notify();
        });
    }

    pub(super) fn set_byte_order_be(&mut self, _: &SetByteOrderBigEndian, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_is_big_endian(true);
            cx.notify();
        });
    }

    pub(super) fn toggle_byte_order(&mut self, _: &ToggleByteOrder, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.toggle_byte_order();
            cx.notify();
        });
    }
}

fn hex_digit(character: char) -> Option<u8> {
    match character {
        '0'..='9' => Some(character as u8 - b'0'),
        'a'..='f' => Some(character as u8 - b'a' + 10),
        'A'..='F' => Some(character as u8 - b'A' + 10),
        _ => None,
    }
}
