use std::ops::Range;

use super::types::EditColumn;
use crate::app_state::InsertModeState;
use crate::core::clipboard::parse_paste_bytes;
use crate::core::editor::Editor;
use crate::core::encoding::Encoding;
use crate::core::format::{CopyFormat, format_bytes, format_hex_spaces};
use crate::core::radix::{ByteGroupSize, DisplayRadix};
use gpui_kit::{App, ClipboardItem, Entity, FocusHandle, Window};

/// Determines the range of bytes to copy for the current selection or cursor position.
///
/// In Overwrite mode, if there is no explicit selection, the range corresponds to the cursor
/// frame drawn on screen (single byte for hexadecimal, item group for other radices, or
/// character boundary in ASCII column). In Insert mode without selection, `None` is returned.
#[allow(clippy::too_many_arguments)]
pub fn effective_copy_range(
    selection: Option<Range<usize>>,
    insert_mode: bool,
    cursor_offset: usize,
    total_size: usize,
    active_column: EditColumn,
    radix: DisplayRadix,
    group_size: ByteGroupSize,
    encoding: Encoding,
    buffer_data: &[u8],
) -> Option<Range<usize>> {
    if total_size == 0 {
        return None;
    }
    if let Some(range) = selection {
        let start = range.start.min(total_size);
        let end = range.end.min(total_size);
        if start < end {
            return Some(start..end);
        }
    }
    if insert_mode || cursor_offset >= total_size {
        return None;
    }
    match active_column {
        EditColumn::Ascii => {
            let range = encoding.char_range_at(buffer_data, cursor_offset);
            let start = range.start.min(total_size);
            let end = range.end.min(total_size);
            if start < end {
                Some(start..end)
            } else {
                Some(cursor_offset..cursor_offset + 1)
            }
        }
        EditColumn::Hex => {
            if radix == DisplayRadix::Hexadecimal {
                Some(cursor_offset..cursor_offset + 1)
            } else {
                let group_bytes = group_size.byte_count();
                let start = (cursor_offset / group_bytes) * group_bytes;
                let end = (start + group_bytes).min(total_size);
                Some(start..end)
            }
        }
    }
}

/// Handles clipboard interactions (copy with various formats, cut, paste) for hex views.
pub struct ClipboardHandler;

impl ClipboardHandler {
    pub fn copy_formatted_range(
        editor: &Entity<Editor>,
        focus_handle: &FocusHandle,
        range: Option<Range<usize>>,
        format: CopyFormat,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (formatted, raw_bytes) = {
            let Some(range) = range else {
                return;
            };
            if range.is_empty() {
                return;
            }
            let editor = editor.read(cx);
            let doc = editor.document.read().expect("document read lock");
            let total = doc.buffer.len();
            if total == 0 || range.start >= total {
                (String::new(), Vec::new())
            } else {
                let clamped_range = range.start..range.end.min(total);
                let slice = doc.buffer.get_range(clamped_range.start, clamped_range.len());
                (format_bytes(slice, clamped_range.start, format, editor.options.encoding), slice.to_vec())
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

    pub fn copy_range(editor: &Entity<Editor>, focus_handle: &FocusHandle, range: Option<Range<usize>>, window: &mut Window, cx: &mut App) {
        let (formatted, raw_bytes) = {
            let Some(range) = range else {
                return;
            };
            if range.is_empty() {
                return;
            }
            let editor = editor.read(cx);
            let doc = editor.document.read().expect("document read lock");
            let total = doc.buffer.len();
            if total == 0 || range.start >= total {
                (String::new(), Vec::new())
            } else {
                let clamped_range = range.start..range.end.min(total);
                let radix = editor.options.radix;
                let group_size = if radix == DisplayRadix::Hexadecimal && clamped_range.len() == 1 {
                    ByteGroupSize::One
                } else {
                    editor.options.group_size
                };
                let is_big_endian = editor.options.is_big_endian;
                let line_starts = editor.line_starts();
                let slice = doc.buffer.get_range(clamped_range.start, clamped_range.len());
                (
                    crate::core::radix::format_display_content_with_lines(doc.buffer.data(), clamped_range, &line_starts, radix, group_size, is_big_endian),
                    slice.to_vec(),
                )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effective_copy_range_with_active_selection() {
        let data = b"Hello, World!";
        let range = effective_copy_range(
            Some(2..6),
            false,
            0,
            data.len(),
            EditColumn::Hex,
            DisplayRadix::Hexadecimal,
            ByteGroupSize::One,
            Encoding::Ascii,
            data,
        );
        assert_eq!(range, Some(2..6));
    }

    #[test]
    fn test_effective_copy_range_insert_mode_no_selection() {
        let data = b"Hello, World!";
        let range = effective_copy_range(
            None,
            true, // insert mode enabled
            2,
            data.len(),
            EditColumn::Hex,
            DisplayRadix::Hexadecimal,
            ByteGroupSize::One,
            Encoding::Ascii,
            data,
        );
        assert_eq!(range, None);
    }

    #[test]
    fn test_effective_copy_range_overwrite_hex_column_hex_radix() {
        let data = b"Hello, World!";
        let range = effective_copy_range(
            None,
            false,
            4,
            data.len(),
            EditColumn::Hex,
            DisplayRadix::Hexadecimal,
            ByteGroupSize::Two,
            Encoding::Ascii,
            data,
        );
        // In hexadecimal mode, single byte cursor frame is copied regardless of group size
        assert_eq!(range, Some(4..5));
    }

    #[test]
    fn test_effective_copy_range_overwrite_hex_column_decimal_radix() {
        let data = b"Hello, World!";
        let range = effective_copy_range(
            None,
            false,
            5,
            data.len(),
            EditColumn::Hex,
            DisplayRadix::Decimal,
            ByteGroupSize::Two,
            Encoding::Ascii,
            data,
        );
        // In decimal mode with Group2Bytes, group 4..6 is copied
        assert_eq!(range, Some(4..6));
    }

    #[test]
    fn test_effective_copy_range_overwrite_ascii_column() {
        let data = "こんにちは".as_bytes(); // Each character is 3 bytes in UTF-8
        let range = effective_copy_range(
            None,
            false,
            3, // Start of second character 'ん'
            data.len(),
            EditColumn::Ascii,
            DisplayRadix::Hexadecimal,
            ByteGroupSize::One,
            Encoding::Utf8,
            data,
        );
        assert_eq!(range, Some(3..6));
    }

    #[test]
    fn test_effective_copy_range_eof_or_empty() {
        let data = b"abc";
        let at_eof = effective_copy_range(
            None,
            false,
            3, // cursor at EOF
            data.len(),
            EditColumn::Hex,
            DisplayRadix::Hexadecimal,
            ByteGroupSize::One,
            Encoding::Ascii,
            data,
        );
        assert_eq!(at_eof, None);

        let empty: &[u8] = b"";
        let empty_result = effective_copy_range(
            None,
            false,
            0,
            empty.len(),
            EditColumn::Hex,
            DisplayRadix::Hexadecimal,
            ByteGroupSize::One,
            Encoding::Ascii,
            empty,
        );
        assert_eq!(empty_result, None);
    }
}
