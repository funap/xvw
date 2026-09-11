use crate::app_state::InsertModeState;
use crate::core::clipboard::parse_paste_bytes;
use crate::core::editor::Editor;
use crate::core::format::{CopyFormat, format_bytes, format_hex_spaces};
use gpui_kit::*;

/// Handles clipboard interactions (copy with various formats, cut, paste) for hex views.
pub struct ClipboardHandler;

impl ClipboardHandler {
    pub fn copy_formatted(editor: &Entity<Editor>, focus_handle: &FocusHandle, format: CopyFormat, window: &mut Window, cx: &mut App) {
        let (formatted, raw_bytes) = {
            let editor = editor.read(cx);
            let selected_range = editor.selected_range_or_cursor();
            let doc = editor.document.read().expect("document read lock");
            let total = doc.buffer.len();
            if total == 0 {
                (String::new(), Vec::new())
            } else {
                let (start_offset, slice) = if let Some(range) = selected_range {
                    (range.start, doc.buffer.get_range(range.start, range.len()))
                } else {
                    let off = editor.cursor.offset.min(total.saturating_sub(1));
                    (off, doc.buffer.get_range(off, 1))
                };
                (format_bytes(slice, start_offset, format, editor.options.encoding), slice.to_vec())
            }
        };

        focus_handle.focus(window, cx);
        let item = if raw_bytes.is_empty() {
            ClipboardItem::new_string(formatted)
        } else {
            let raw = format_hex_spaces(&raw_bytes);
            ClipboardItem::new_string_with_metadata(formatted, format!("xvw-bytes:{raw}"))
        };
        cx.write_to_clipboard(item);
    }

    pub fn copy(editor: &Entity<Editor>, focus_handle: &FocusHandle, window: &mut Window, cx: &mut App) {
        let (formatted, raw_bytes) = {
            let editor = editor.read(cx);
            let selected_range = editor.selected_range_or_cursor();
            let doc = editor.document.read().expect("document read lock");
            let total = doc.buffer.len();
            if total == 0 {
                (String::new(), Vec::new())
            } else if let Some(range) = selected_range {
                let radix = editor.options.radix;
                let group_size = editor.options.group_size;
                let is_big_endian = editor.options.is_big_endian;
                let line_starts = editor.line_starts();
                let slice = doc.buffer.get_range(range.start, range.len());
                (
                    crate::core::radix::format_display_content_with_lines(doc.buffer.data(), range, &line_starts, radix, group_size, is_big_endian),
                    slice.to_vec(),
                )
            } else {
                (String::new(), Vec::new())
            }
        };

        focus_handle.focus(window, cx);
        let item = if raw_bytes.is_empty() {
            ClipboardItem::new_string(formatted)
        } else {
            let raw = format_hex_spaces(&raw_bytes);
            ClipboardItem::new_string_with_metadata(formatted, format!("xvw-bytes:{raw}"))
        };
        cx.write_to_clipboard(item);
    }

    pub fn cut(editor: &Entity<Editor>, focus_handle: &FocusHandle, window: &mut Window, cx: &mut App) -> bool {
        focus_handle.focus(window, cx);
        if editor.read(cx).is_read_only() {
            return false;
        }
        let (range, bytes) = {
            let editor = editor.read(cx);
            let Some(range) = editor.edit_range() else {
                return false;
            };
            let bytes = editor
                .document
                .read()
                .expect("document read lock")
                .buffer
                .get_range(range.start, range.len())
                .to_vec();
            (range, bytes)
        };
        if bytes.is_empty() {
            return false;
        }

        let clipboard_text = format_hex_spaces(&bytes);
        cx.write_to_clipboard(ClipboardItem::new_string_with_metadata(
            clipboard_text.clone(),
            format!("xvw-bytes:{clipboard_text}"),
        ));

        editor.update(cx, |editor, editor_cx| {
            let remaining = editor.total_size().saturating_sub(range.len());
            let cursor_after = range.start.min(remaining.saturating_sub(1));
            let changed = editor.replace_range_with_cursor(range, Vec::new(), cursor_after);
            if changed {
                editor_cx.notify();
            }
            changed
        })
    }

    pub fn paste(editor: &Entity<Editor>, focus_handle: &FocusHandle, window: &mut Window, cx: &mut App) -> bool {
        focus_handle.focus(window, cx);
        if editor.read(cx).is_read_only() {
            return false;
        }
        let Some(item) = cx.read_from_clipboard() else {
            return false;
        };
        let bytes = item
            .metadata()
            .and_then(|metadata| metadata.strip_prefix("xvw-bytes:"))
            .and_then(parse_paste_bytes)
            .or_else(|| item.text().and_then(|text| parse_paste_bytes(&text)));
        let Some(bytes) = bytes else {
            return false;
        };
        if bytes.is_empty() {
            return false;
        }

        let insert_mode = InsertModeState::is_enabled(cx);
        editor.update(cx, |editor, editor_cx| {
            let has_selection = editor.has_selection();
            let changed = if has_selection {
                let range = editor.edit_range().expect("selection has an edit range");
                if insert_mode {
                    let cursor_after = range.start.saturating_add(bytes.len());
                    editor.replace_range_with_cursor(range, bytes, cursor_after)
                } else {
                    editor.replace_range(range, bytes)
                }
            } else if insert_mode {
                let position = editor.cursor.offset;
                editor.insert_bytes(position, bytes)
            } else {
                let position = editor.cursor.offset;
                let range = position..position.saturating_add(bytes.len()).min(editor.total_size());
                editor.replace_range(range, bytes)
            };
            if changed {
                editor_cx.notify();
            }
            changed
        })
    }
}
