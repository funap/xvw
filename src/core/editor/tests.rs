use super::*;
use crate::core::command::InsertCharCommand;
use std::sync::Arc;

fn create_editor_with_content(content: &[u8]) -> Editor {
    let buffer = crate::core::buffer::Buffer::new(content.to_vec());
    let document = Arc::new(RwLock::new(Document::new(std::path::PathBuf::from("test"), buffer)));
    Editor::new(document)
}

#[test]
fn test_initialization() {
    let editor = create_editor_with_content(b"Hello");
    assert_eq!(editor.total_size(), 5);
    assert_eq!(editor.cursor.offset, 0);
    assert!(!editor.has_selection());
}

#[test]
fn test_cursor_movement() {
    let mut editor = create_editor_with_content(b"123");

    // Move right
    editor.move_right();
    assert_eq!(editor.cursor.offset, 1);

    // Move left
    editor.move_left();
    assert_eq!(editor.cursor.offset, 0);

    // Boundary checks
    editor.move_left();
    assert_eq!(editor.cursor.offset, 0);

    editor.end();
    assert_eq!(editor.cursor.offset, 3);
    editor.move_right();
    assert_eq!(editor.cursor.offset, 3);

    editor.go_to_beginning();
    assert_eq!(editor.cursor.offset, 0);
    editor.go_to_end();
    assert_eq!(editor.cursor.offset, 3);
}

#[test]
fn test_selection() {
    let mut editor = create_editor_with_content(b"12345");

    // Select Right
    editor.select_right();
    assert_eq!(editor.selection(), Selection::new(0, 1));
    assert_eq!(editor.cursor.offset, 1);
    assert_eq!(editor.insert_cursor_offset(), 1);
    assert_eq!(editor.selected_range_or_cursor(), Some(0..1));
    assert_eq!(editor.edit_range(), Some(0..1));

    // Clear selection on move
    editor.move_right();
    assert!(!editor.has_selection());

    // Select All
    editor.select_all();
    assert_eq!(editor.selection(), Selection::new(0, 5));
    assert_eq!(editor.insert_cursor_offset(), 5);
    assert_eq!(editor.selected_range_or_cursor(), Some(0..5));

    editor.set_cursor_offset_exact(4);
    editor.select_end_for_insert();
    assert_eq!(editor.cursor.offset, 5);
    assert_eq!(editor.selection(), Selection::new(4, 5));
}

#[test]
fn test_overwrite_selection_select_down_and_up() {
    let mut editor = create_editor_with_content(&[0u8; 48]);
    assert_eq!(editor.cursor.offset, 0);

    // Shift+Down from offset 0 selects exactly 16 bytes (one row) to cursor offset 16
    editor.select_down();
    assert_eq!(editor.cursor.offset, 16);
    assert_eq!(editor.selection(), Selection::new(0, 16));
    assert_eq!(editor.edit_range(), Some(0..16));

    // Shift+Down again selects 32 bytes (two rows) to cursor offset 32
    editor.select_down();
    assert_eq!(editor.cursor.offset, 32);
    assert_eq!(editor.selection(), Selection::new(0, 32));
    assert_eq!(editor.edit_range(), Some(0..32));

    // Shift+Up shrinks the selection back to 16 bytes
    editor.select_up();
    assert_eq!(editor.cursor.offset, 16);
    assert_eq!(editor.selection(), Selection::new(0, 16));
    assert_eq!(editor.edit_range(), Some(0..16));

    // Shift+Up collapses the selection
    editor.select_up();
    assert_eq!(editor.cursor.offset, 0);
    assert_eq!(editor.selection(), Selection::new(0, 0));
    assert_eq!(editor.edit_range(), Some(0..1));
}

#[test]
fn test_overwrite_selection_select_left_and_right() {
    let mut editor = create_editor_with_content(&[0u8; 10]);
    editor.set_cursor_offset_exact(5);

    // Shift+Left from offset 5 selects 1 byte (4..5) with cursor at 4
    editor.select_left();
    assert_eq!(editor.cursor.offset, 4);
    assert_eq!(editor.selection(), Selection::new(5, 4));
    assert_eq!(editor.edit_range(), Some(4..5));

    // Shift+Right shrinks back
    editor.select_right();
    assert_eq!(editor.cursor.offset, 5);
    assert_eq!(editor.selection(), Selection::new(5, 5));
}

#[test]
fn test_insert_selection_moves_by_one_display_group_and_collapses_at_anchor() {
    let mut editor = create_editor_with_content(b"12345");
    editor.set_cursor_offset_exact(2);

    editor.select_left_for_insert();
    assert_eq!(editor.edit_range(), Some(1..2));
    assert_eq!(editor.selection(), Selection::new(2, 1));
    assert_eq!(editor.cursor.offset, 1);
    assert_eq!(editor.insert_cursor_offset(), 1);

    editor.select_left_for_insert();
    assert_eq!(editor.edit_range(), Some(0..2));
    assert_eq!(editor.selection(), Selection::new(2, 0));
    assert_eq!(editor.cursor.offset, 0);

    editor.select_right_for_insert();
    assert_eq!(editor.edit_range(), Some(1..2));
    assert_eq!(editor.selection(), Selection::new(2, 1));
    assert_eq!(editor.cursor.offset, 1);

    editor.select_right_for_insert();
    assert!(!editor.has_selection());
    assert_eq!(editor.selection(), Selection::collapsed(2));
    assert_eq!(editor.cursor.offset, 2);
    assert_eq!(editor.edit_range(), Some(2..3));
}

#[test]
fn test_insert_selection_moves_by_four_byte_groups() {
    let mut editor = create_editor_with_content(&[0u8; 10]);
    editor.set_group_size(ByteGroupSize::Four);
    editor.set_is_big_endian(true);

    editor.select_right_for_insert();
    assert_eq!(editor.selection(), Selection::new(0, 4));
    assert_eq!(editor.cursor.offset, 4);

    editor.select_right_for_insert();
    assert_eq!(editor.selection(), Selection::new(0, 8));
    assert_eq!(editor.cursor.offset, 8);

    // The final partial group ends at EOF instead of jumping past it.
    editor.select_right_for_insert();
    assert_eq!(editor.selection(), Selection::new(0, 10));
    assert_eq!(editor.cursor.offset, 10);

    editor.select_left_for_insert();
    assert_eq!(editor.selection(), Selection::new(0, 8));
    assert_eq!(editor.cursor.offset, 8);

    editor.select_left_for_insert();
    assert_eq!(editor.selection(), Selection::new(0, 4));
    assert_eq!(editor.cursor.offset, 4);

    editor.select_left_for_insert();
    assert_eq!(editor.selection(), Selection::new(0, 0));
    assert_eq!(editor.cursor.offset, 0);

    // A caret inside a group follows the same boundary as an unmodified
    // left-arrow move, and Shift+Right uses the matching next boundary.
    editor.set_cursor_offset_exact(5);
    editor.select_left_for_insert();
    assert_eq!(editor.selection(), Selection::new(5, 0));
    assert_eq!(editor.cursor.offset, 0);

    editor.select_right_for_insert();
    assert_eq!(editor.selection(), Selection::new(5, 4));
    assert_eq!(editor.cursor.offset, 4);
}

#[test]
fn test_insert_selection_select_down_and_up_standard_lines() {
    let mut editor = create_editor_with_content(&[0u8; 48]);
    // Default 16-byte lines: [0..16], [16..32], [32..48]
    assert_eq!(editor.line_starts(), vec![0, 16, 32]);

    // Start at offset 0 (beginning of line 0)
    editor.set_cursor_offset_exact(0);
    editor.select_down_for_insert();
    assert_eq!(editor.selection(), Selection::new(0, 16));
    assert_eq!(editor.cursor.offset, 16);
    assert_eq!(editor.insert_cursor_offset(), 16);
    assert_eq!(editor.selection_range(), Some(0..16));

    // Select down again to line 2 (offset 32)
    editor.select_down_for_insert();
    assert_eq!(editor.selection(), Selection::new(0, 32));
    assert_eq!(editor.cursor.offset, 32);
    assert_eq!(editor.insert_cursor_offset(), 32);
    assert_eq!(editor.selection_range(), Some(0..32));

    // Select down at last line reaches EOF (48)
    editor.select_down_for_insert();
    assert_eq!(editor.selection(), Selection::new(0, 48));
    assert_eq!(editor.cursor.offset, 48);
    assert_eq!(editor.insert_cursor_offset(), 48);

    // Select up contracts selection back to line 2 (32)
    editor.select_up_for_insert();
    assert_eq!(editor.selection(), Selection::new(0, 32));
    assert_eq!(editor.cursor.offset, 32);

    // Select up contracts back to line 1 (16)
    editor.select_up_for_insert();
    assert_eq!(editor.selection(), Selection::new(0, 16));
    assert_eq!(editor.cursor.offset, 16);

    // Select up contracts back to anchor (0) -> collapsed
    editor.select_up_for_insert();
    assert_eq!(editor.selection(), Selection::new(0, 0));
    assert_eq!(editor.cursor.offset, 0);
    assert!(!editor.has_selection());

    // Select up at first line stays at 0
    editor.select_up_for_insert();
    assert_eq!(editor.selection(), Selection::new(0, 0));
    assert_eq!(editor.cursor.offset, 0);
}

#[test]
fn test_insert_selection_select_down_with_join_line() {
    let mut editor = create_editor_with_content(&[0u8; 64]);
    // Join line 0 and line 1 -> line 0 is 32 bytes (0..32), line 1 is 16 bytes (32..48), line 2 is 16 bytes (48..64)
    editor.set_cursor_offset_exact(5);
    editor.custom_layout_mut().join_line();
    assert_eq!(editor.line_starts(), vec![0, 32, 48]);

    // Start at offset 5 (column 5 of 32-byte line 0)
    editor.select_down_for_insert();
    // Cursor moves straight down to column 5 of line 1 (offset 32 + 5 = 37)
    assert_eq!(editor.selection(), Selection::new(5, 37));
    assert_eq!(editor.cursor.offset, 37);
    assert_eq!(editor.insert_cursor_offset(), 37);
    assert_eq!(editor.selection_range(), Some(5..37));

    // Start at offset 20 (column 20 of 32-byte line 0)
    editor.clear_selection();
    editor.set_cursor_offset_exact(20);
    editor.select_down_for_insert();
    // Line 1 is 16 bytes (32..48). Column 20 clamps to line 1 length 16 -> offset 32 + 16 = 48
    assert_eq!(editor.selection(), Selection::new(20, 48));
    assert_eq!(editor.cursor.offset, 48);
    assert_eq!(editor.insert_cursor_offset(), 48);
    assert_eq!(editor.selection_range(), Some(20..48));

    // When line 0 is 16 bytes and line 1 is joined to be 32 bytes (48 total bytes)
    let mut editor2 = create_editor_with_content(&[0u8; 48]);
    editor2.set_cursor_offset_exact(16);
    editor2.custom_layout_mut().join_line(); // lines: [0..16], [16..48]
    assert_eq!(editor2.line_starts(), vec![0, 16]);

    // Caret at offset 10 in line 0 (16 bytes) moving down to joined line 1 (32 bytes)
    editor2.set_cursor_offset_exact(10);
    editor2.select_down_for_insert();
    // Moves straight down to column 10 of line 1 (16 + 10 = 26)
    assert_eq!(editor2.selection(), Selection::new(10, 26));
    assert_eq!(editor2.cursor.offset, 26);
    assert_eq!(editor2.insert_cursor_offset(), 26);
    assert_eq!(editor2.selection_range(), Some(10..26));

    // Select up from 26 moves back up to line 0 column 10 (0 + 10 = 10)
    editor2.select_up_for_insert();
    assert_eq!(editor2.selection(), Selection::new(10, 10));
    assert_eq!(editor2.cursor.offset, 10);
    assert!(!editor2.has_selection());
}

#[test]
fn test_insert_selection_select_down_and_up_with_custom_breaks() {
    let mut editor = create_editor_with_content(&[0u8; 32]);
    editor.custom_layout_mut().add_break(10); // Lines: [0..10], [10..26], [26..32]
    assert_eq!(editor.line_starts(), vec![0, 10, 26]);

    editor.set_cursor_offset_exact(4);
    editor.select_down_for_insert();
    // Line 1 start is 10, offset_in_line is 4 -> active = 14
    assert_eq!(editor.selection(), Selection::new(4, 14));
    assert_eq!(editor.cursor.offset, 14);

    editor.select_down_for_insert();
    // Line 2 start is 26, offset_in_line is 4 -> active = 30
    assert_eq!(editor.selection(), Selection::new(4, 30));
    assert_eq!(editor.cursor.offset, 30);

    editor.select_up_for_insert();
    assert_eq!(editor.selection(), Selection::new(4, 14));
    assert_eq!(editor.cursor.offset, 14);

    editor.select_up_for_insert();
    assert_eq!(editor.selection(), Selection::new(4, 4));
    assert_eq!(editor.cursor.offset, 4);
    assert!(!editor.has_selection());
}

#[test]
fn test_insert_selection_select_home() {
    let mut editor = create_editor_with_content(&[0u8; 32]);
    editor.set_cursor_offset_exact(18);
    editor.select_home_for_insert();
    assert_eq!(editor.selection(), Selection::new(18, 0));
    assert_eq!(editor.cursor.offset, 0);
    assert_eq!(editor.insert_cursor_offset(), 0);
    assert_eq!(editor.selection_range(), Some(0..18));
}

#[test]
fn test_selection_caret_direction_and_collapse() {
    let mut editor = create_editor_with_content(b"12345");
    editor.set_selection(3, 1);
    editor.cursor.offset = 1;
    assert_eq!(editor.insert_cursor_offset(), 1);

    editor.move_right_for_insert();
    assert_eq!(editor.cursor.offset, 3);
    assert!(!editor.has_selection());

    editor.set_selection(1, 3);
    editor.cursor.offset = 3;
    editor.move_left_for_insert();
    assert_eq!(editor.cursor.offset, 1);
    assert!(!editor.has_selection());
}

#[test]
fn test_arrow_keys_after_select_down_move_from_active_cursor() {
    let mut editor = create_editor_with_content(&[0u8; 128]);
    assert_eq!(editor.cursor.offset, 0);

    // 1. SHIFT+Down (0 -> 16)
    editor.select_down();
    assert_eq!(editor.cursor.offset, 16);
    assert_eq!(editor.selection(), Selection::new(0, 16));

    // 2. Down key: moves from 16 to 32 and clears selection
    editor.move_down();
    assert_eq!(editor.cursor.offset, 32);
    assert!(!editor.has_selection());

    // 3. SHIFT+Down again (32 -> 48)
    editor.select_down();
    assert_eq!(editor.cursor.offset, 48);
    assert_eq!(editor.selection(), Selection::new(32, 48));

    // 4. Up key: moves from 48 to 32 and clears selection
    editor.move_up();
    assert_eq!(editor.cursor.offset, 32);
    assert!(!editor.has_selection());

    // 5. SHIFT+Down again (32 -> 48)
    editor.select_down();
    assert_eq!(editor.cursor.offset, 48);

    // 6. Right key: moves from 48 to 49 and clears selection
    editor.move_right();
    assert_eq!(editor.cursor.offset, 49);
    assert!(!editor.has_selection());

    // 7. Reset to 16 and SHIFT+Down (16 -> 32)
    editor.cursor.offset = 16;
    editor.clear_selection();
    editor.select_down();
    assert_eq!(editor.cursor.offset, 32);

    // 8. Left key: moves from 32 to 31 and clears selection
    editor.move_left();
    assert_eq!(editor.cursor.offset, 31);
    assert!(!editor.has_selection());
}

#[test]
fn test_select_right_reaches_eof_and_drag_updates_caret() {
    let mut editor = create_editor_with_content(b"12345");
    editor.set_cursor_offset_exact(4);
    editor.select_right();
    assert_eq!(editor.selection(), Selection::new(4, 5));
    assert_eq!(editor.cursor.offset, 5);
    assert_eq!(editor.insert_cursor_offset(), 5);

    editor.start_drag(1);
    editor.continue_drag(1, 3);
    assert_eq!(editor.cursor.offset, 3);
    assert_eq!(editor.selection_range(), Some(1..4));
}

#[test]
fn test_drag_selection_step_by_step_forward_and_backward() {
    let mut editor = create_editor_with_content(b"0123456789");

    // Forward dragging from offset 0
    editor.continue_drag(0, 0);
    assert_eq!(editor.selection_range(), Some(0..1));
    assert_eq!(editor.cursor.offset, 0);

    editor.continue_drag(0, 1);
    assert_eq!(editor.selection_range(), Some(0..2));
    assert_eq!(editor.cursor.offset, 1);

    editor.continue_drag(0, 2);
    assert_eq!(editor.selection_range(), Some(0..3));
    assert_eq!(editor.cursor.offset, 2);

    editor.continue_drag(0, 3);
    assert_eq!(editor.selection_range(), Some(0..4));
    assert_eq!(editor.cursor.offset, 3);

    // Backward dragging from offset 5
    editor.continue_drag(5, 5);
    assert_eq!(editor.selection_range(), Some(5..6));
    assert_eq!(editor.cursor.offset, 5);

    editor.continue_drag(5, 4);
    assert_eq!(editor.selection_range(), Some(4..6));
    assert_eq!(editor.cursor.offset, 4);

    editor.continue_drag(5, 3);
    assert_eq!(editor.selection_range(), Some(3..6));
    assert_eq!(editor.cursor.offset, 3);

    // Reversing direction from anchor 5 to offset 7
    editor.continue_drag(5, 7);
    assert_eq!(editor.selection_range(), Some(5..8));
    assert_eq!(editor.cursor.offset, 7);

    // Multi-byte group size (Two bytes)
    editor.set_group_size(crate::core::radix::ByteGroupSize::Two);
    editor.continue_drag(0, 0);
    assert_eq!(editor.selection_range(), Some(0..2));
    assert_eq!(editor.cursor.offset, 0);

    editor.continue_drag(0, 2);
    assert_eq!(editor.selection_range(), Some(0..4));
    assert_eq!(editor.cursor.offset, 2);

    editor.continue_drag(4, 2);
    assert_eq!(editor.selection_range(), Some(2..6));
    assert_eq!(editor.cursor.offset, 2);
}

#[test]
fn test_selected_range_or_cursor_includes_selection_end() {
    let mut editor = create_editor_with_content(b"12345");

    editor.set_selection(1, 4);
    assert_eq!(editor.selected_range_or_cursor(), Some(1..4));

    editor.set_selection(4, 1);
    assert_eq!(editor.selected_range_or_cursor(), Some(1..4));

    editor.set_cursor_offset_exact(4);
    editor.set_selection(4, 4);
    assert!(!editor.has_selection());
    assert_eq!(editor.selected_range_or_cursor(), Some(4..5));
}

#[test]
fn test_set_selection_range() {
    let mut editor = create_editor_with_content(b"0123456789");

    // Multi-byte range selection
    editor.set_selection_range(2..6);
    assert!(editor.has_selection());
    assert_eq!(editor.selection_range(), Some(2..6));
    assert_eq!(editor.cursor.offset, 2);

    // 1-byte range selection
    editor.set_selection_range(5..6);
    assert!(editor.has_selection());
    assert_eq!(editor.selection_range(), Some(5..6));
    assert_eq!(editor.cursor.offset, 5);

    // Empty range
    editor.set_selection_range(3..3);
    assert!(!editor.has_selection());
    assert_eq!(editor.selection_range(), None);
    assert_eq!(editor.cursor.offset, 3);

    // Out-of-bounds clamping
    editor.set_selection_range(8..20);
    assert!(editor.has_selection());
    assert_eq!(editor.selection_range(), Some(8..10));
    assert_eq!(editor.cursor.offset, 8);
}

#[test]
fn test_search_navigation() {
    let mut editor = create_editor_with_content(b"test match test");
    editor.search_state.results = vec![0, 11];

    // Ensure we handle no current index gracefully
    assert_eq!(editor.current_search_result(), None);

    // Next: 0 -> 11
    editor.next_search_result();
    assert_eq!(editor.current_search_result(), Some(0));
    assert_eq!(editor.cursor.offset, 0);

    editor.next_search_result();
    assert_eq!(editor.current_search_result(), Some(11));

    // Wrap around
    editor.next_search_result();
    assert_eq!(editor.current_search_result(), Some(0));

    // Prev
    editor.prev_search_result();
    assert_eq!(editor.current_search_result(), Some(11));
}

#[test]
fn test_search_on_demand_navigation() {
    let mut editor = create_editor_with_content(b"test match test");
    editor
        .search_state_mut()
        .set_query_and_mode("test".to_string(), crate::core::search::SearchMode::Text);
    // results is empty because inline search does not scan the whole file
    assert!(editor.search_state.results.is_empty());

    // Next from offset 0 finds next match at offset 11
    editor.cursor.offset = 0;
    let next = editor.find_and_navigate_next();
    assert_eq!(next, Some(11));
    assert_eq!(editor.cursor.offset, 11);

    // Next from offset 11 wraps to offset 0
    let next = editor.find_and_navigate_next();
    assert_eq!(next, Some(0));
    assert_eq!(editor.cursor.offset, 0);

    // Prev from offset 0 wraps to offset 11
    let prev = editor.find_and_navigate_prev();
    assert_eq!(prev, Some(11));
    assert_eq!(editor.cursor.offset, 11);

    // Prev from offset 11 finds offset 0
    let prev = editor.find_and_navigate_prev();
    assert_eq!(prev, Some(0));
    assert_eq!(editor.cursor.offset, 0);
}

#[test]
fn test_search_generation_and_race_condition() {
    let mut editor = create_editor_with_content(b"test match test");
    assert_eq!(editor.search_state.generation, 0);

    // 1. Verification of query changes incrementing generation
    editor.search_state_mut().set_query("foo".to_string());
    assert_eq!(editor.search_state.generation, 1);

    editor.search_state_mut().set_query("foo".to_string());
    assert_eq!(editor.search_state.generation, 1); // No change

    editor.search_state_mut().set_query("bar".to_string());
    assert_eq!(editor.search_state.generation, 2);

    // 2. Discarding older queries (generation < current_generation)
    editor.search_state_mut().set_results(vec![0], 1, true);
    assert!(editor.search_state.results.is_empty());

    // 3. Allowing same generation results
    editor.search_state_mut().set_results(vec![1, 2], 2, true);
    assert_eq!(editor.search_state.results, vec![1, 2]);

    // 4. Overwriting or syncing generation if generation > current
    editor.search_state_mut().set_results(vec![3, 4], 3, true);
    assert_eq!(editor.search_state.results, vec![3, 4]);
    assert_eq!(editor.search_state.generation, 3);
    assert!(editor.search_state.is_full_search_complete);

    // 5. Preventing partial viewport search results from overwriting full search results within the same generation
    editor.search_state_mut().set_results(vec![3], 3, false); // partial results for same generation
    assert_eq!(editor.search_state.results, vec![3, 4]); // results remain full-search results

    // 6. Discarding all results and incrementing generation upon clear_search
    editor.search_state_mut().clear();
    assert_eq!(editor.search_state.generation, 4);
    assert!(editor.search_state.results.is_empty());
    assert!(!editor.search_state.is_full_search_complete);

    // Try setting results with an older generation (3)
    editor.search_state_mut().set_results(vec![5], 3, true);
    assert!(editor.search_state.results.is_empty());
}

#[test]
fn test_search_with_encoding() {
    use crate::core::encoding::Encoding;
    use crate::core::search::SearchMode;

    // Shift-JIS buffer: "こんにちは" at offset 0
    let sjis_data = [0x82, 0xB1, 0x82, 0xF1, 0x82, 0xC9, 0x82, 0xBF, 0x82, 0xCD];
    let mut editor = create_editor_with_content(&sjis_data);
    editor.set_encoding(Encoding::ShiftJis);

    editor.search_state_mut().set_query_and_mode("にち".to_string(), SearchMode::Text);
    let pattern = editor.search_pattern().expect("valid pattern");
    assert_eq!(pattern.len(), 4); // "にち" is 4 bytes in Shift-JIS

    let next = editor.find_and_navigate_next();
    assert_eq!(next, Some(4));
    assert_eq!(editor.cursor.offset, 4);

    // Switch to UTF-8 encoding: query "にち" in UTF-8 does not match the Shift-JIS bytes
    editor.set_encoding(Encoding::Utf8);
    assert_eq!(editor.find_and_navigate_next(), None);
}

#[test]
fn test_shared_document() {
    let buffer = crate::core::buffer::Buffer::new(b"".to_vec());
    let document = Arc::new(RwLock::new(Document::new(std::path::PathBuf::from("test"), buffer)));
    let mut editor1 = Editor::new(document.clone());
    let mut editor2 = Editor::new(document.clone());

    // Insert in editor1
    let cmd1 = Box::new(InsertCharCommand::new(0, b'A'));
    editor1.execute_command(cmd1);

    // Verify editor2 sees change
    assert_eq!(editor2.total_size(), 1);

    // Undo in editor2
    editor2.undo();
    assert_eq!(editor1.total_size(), 0);
}

#[test]
fn test_read_only_rejects_edits_and_history_changes() {
    let document = Arc::new(RwLock::new(Document::new_read_only(
        std::path::PathBuf::from("read-only.bin"),
        crate::core::buffer::Buffer::new(b"abc".to_vec()),
    )));
    let mut editor = Editor::new(document.clone());

    assert!(editor.is_read_only());
    assert!(!editor.replace_byte(0, b'X'));
    assert!(!editor.insert_bytes(1, vec![b'Y']));
    assert!(!editor.delete_forward());
    assert!(!editor.undo());
    assert!(!editor.redo());
    assert_eq!(document.read().unwrap().buffer.data(), b"abc");
    assert!(!document.read().unwrap().is_dirty());
}

#[test]
fn test_shared_document_independent_cursors_and_offsets() {
    let data = (0..256).map(|i| i as u8).collect::<Vec<_>>();
    let buffer = crate::core::buffer::Buffer::new(data);
    let document = Arc::new(RwLock::new(Document::new(std::path::PathBuf::from("binary.bin"), buffer)));
    let mut editor1 = Editor::new(document.clone());
    let mut editor2 = Editor::new(document.clone());

    // Set different offsets and selections (simulating vertical/horizontal split panes)
    editor1.set_cursor_offset(0x00);
    editor1.set_selection(0x00, 0x10);

    editor2.set_cursor_offset(0x80);
    editor2.set_selection(0x80, 0xA0);

    assert_eq!(editor1.cursor.offset, 0x00);
    assert_eq!(editor1.selection(), Selection::new(0x00, 0x10));

    assert_eq!(editor2.cursor.offset, 0x80);
    assert_eq!(editor2.selection(), Selection::new(0x80, 0xA0));

    // Both editors access identical underlying bytes
    assert_eq!(editor1.total_size(), 256);
    assert_eq!(editor2.total_size(), 256);

    // Edit byte in editor1 at offset 0
    let cmd = Box::new(InsertCharCommand::new(0, 0xFF));
    editor1.execute_command(cmd);

    // Both editors see updated buffer size
    assert_eq!(editor1.total_size(), 257);
    assert_eq!(editor2.total_size(), 257);
}

#[test]
fn test_undo_redo() {
    let mut editor = create_editor_with_content(b"");

    // Insert 'A'
    let cmd = Box::new(InsertCharCommand::new(0, b'A'));
    editor.execute_command(cmd);
    assert_eq!(editor.total_size(), 1);

    // Undo
    editor.undo();
    assert_eq!(editor.total_size(), 0);

    // Redo
    editor.redo();
    assert_eq!(editor.total_size(), 1);
}

#[test]
fn test_replace_range_selection_and_undo_redo() {
    let mut editor = create_editor_with_content(b"abcdef");
    editor.cursor.offset = 1;
    editor.set_selection(4, 1);

    assert_eq!(editor.edit_range(), Some(1..4));
    assert!(editor.replace_range(editor.edit_range().expect("selection range"), b"XYZ".to_vec()));
    assert_eq!(editor.document.read().unwrap().buffer.data(), b"aXYZef");
    assert_eq!(editor.cursor.offset, 4);
    assert!(!editor.has_selection());

    assert!(editor.undo());
    assert_eq!(editor.document.read().unwrap().buffer.data(), b"abcdef");
    assert_eq!(editor.cursor.offset, 1);
    assert!(!editor.has_selection());

    assert!(editor.redo());
    assert_eq!(editor.document.read().unwrap().buffer.data(), b"aXYZef");
    assert_eq!(editor.cursor.offset, 4);
    assert!(!editor.has_selection());
}

#[test]
fn test_overwrite_replacement_preserves_size_over_multibyte_utf8() {
    // "あいう" in UTF-8 is 9 bytes: [0xE3, 0x81, 0x82, 0xE3, 0x81, 0x84, 0xE3, 0x81, 0x86]
    let mut editor = create_editor_with_content("あいう".as_bytes());
    assert_eq!(editor.total_size(), 9);
    assert_eq!(editor.cursor.offset, 0);

    // Typing single-byte ASCII 'a' (0x61) at offset 0 in overwrite mode replaces exactly 1 byte
    let pos = editor.cursor.offset;
    let replacement = b"a".to_vec();
    let range = pos..pos.saturating_add(replacement.len()).min(editor.total_size());
    assert!(editor.replace_range(range, replacement));
    assert_eq!(editor.total_size(), 9);
    assert_eq!(editor.cursor.offset, 1);
    assert_eq!(editor.document.read().unwrap().buffer.data()[0], b'a');

    // Typing 'b' at offset 1
    let pos = editor.cursor.offset;
    let replacement = b"b".to_vec();
    let range = pos..pos.saturating_add(replacement.len()).min(editor.total_size());
    assert!(editor.replace_range(range, replacement));
    assert_eq!(editor.total_size(), 9);
    assert_eq!(editor.cursor.offset, 2);

    // Typing 'c' at offset 2
    let pos = editor.cursor.offset;
    let replacement = b"c".to_vec();
    let range = pos..pos.saturating_add(replacement.len()).min(editor.total_size());
    assert!(editor.replace_range(range, replacement));
    assert_eq!(editor.total_size(), 9);
    assert_eq!(editor.cursor.offset, 3);
    assert_eq!(&editor.document.read().unwrap().buffer.data()[0..3], b"abc");

    // Overwriting with a 3-byte UTF-8 character "え" at offset 3
    let pos = editor.cursor.offset;
    let replacement = "え".as_bytes().to_vec();
    let range = pos..pos.saturating_add(replacement.len()).min(editor.total_size());
    assert!(editor.replace_range(range, replacement));
    assert_eq!(editor.total_size(), 9);
    assert_eq!(editor.cursor.offset, 6);
    assert_eq!(editor.document.read().unwrap().buffer.data(), "abcえう".as_bytes());
}

#[test]
fn test_replace_byte_identical_advances_cursor_without_dirtying_or_history() {
    let mut editor = create_editor_with_content(b"abc");
    assert_eq!(editor.cursor.offset, 0);

    // Replacing byte at offset 0 with identical value 'a'
    assert!(!editor.replace_byte(0, b'a'));
    assert_eq!(editor.cursor.offset, 1);
    assert_eq!(editor.document.read().unwrap().buffer.data(), b"abc");
    assert!(!editor.document.read().unwrap().is_dirty());
    assert!(!editor.undo());

    // Replacing byte at offset 1 with identical value 'b'
    assert!(!editor.replace_byte(1, b'b'));
    assert_eq!(editor.cursor.offset, 2);
    assert_eq!(editor.document.read().unwrap().buffer.data(), b"abc");
    assert!(!editor.document.read().unwrap().is_dirty());
    assert!(!editor.undo());
}

#[test]
fn test_replace_range_identical_advances_cursor_and_clears_selection() {
    let mut editor = create_editor_with_content(b"abcdef");
    editor.set_selection(1, 3);
    assert!(editor.has_selection());

    // Replacing selection 1..3 with identical bytes b"bc"
    assert!(!editor.replace_range(1..3, b"bc".to_vec()));
    assert_eq!(editor.cursor.offset, 3);
    assert!(!editor.has_selection());
    assert_eq!(editor.document.read().unwrap().buffer.data(), b"abcdef");
    assert!(!editor.document.read().unwrap().is_dirty());
    assert!(!editor.undo());
}

#[test]
fn test_replace_identical_read_only_does_not_advance_cursor() {
    let document = Arc::new(RwLock::new(Document::new_read_only(
        std::path::PathBuf::from("read-only.bin"),
        crate::core::buffer::Buffer::new(b"abc".to_vec()),
    )));
    let mut editor = Editor::new(document);
    editor.set_cursor_offset(0);

    assert!(!editor.replace_byte(0, b'a'));
    assert_eq!(editor.cursor.offset, 0);

    assert!(!editor.replace_range(0..1, vec![b'a']));
    assert_eq!(editor.cursor.offset, 0);
}

#[test]
fn test_replace_byte_identical_at_end_of_buffer() {
    let mut editor = create_editor_with_content(b"abc");
    editor.set_cursor_offset(2);
    assert_eq!(editor.cursor.offset, 2);

    assert!(!editor.replace_byte(2, b'c'));
    assert_eq!(editor.cursor.offset, 3);
    assert_eq!(editor.document.read().unwrap().buffer.data(), b"abc");
}

#[test]
fn test_insert_delete_and_selection_backspace_cursor() {
    let mut editor = create_editor_with_content(b"abcd");
    editor.set_cursor_offset(2);

    assert!(editor.insert_bytes(2, b"XY".to_vec()));
    assert_eq!(editor.document.read().unwrap().buffer.data(), b"abXYcd");
    assert_eq!(editor.cursor.offset, 4);

    assert!(editor.undo());
    assert_eq!(editor.document.read().unwrap().buffer.data(), b"abcd");
    assert_eq!(editor.cursor.offset, 2);

    editor.set_selection(1, 3);
    editor.cursor.offset = 2;
    assert!(editor.delete_backward());
    assert_eq!(editor.document.read().unwrap().buffer.data(), b"ad");
    assert_eq!(editor.cursor.offset, 1);
    assert!(!editor.has_selection());
}

#[test]
fn test_backspace_moves_cursor_to_deletion_start() {
    let mut editor = create_editor_with_content(b"abcd");
    editor.set_cursor_offset_exact(2);

    assert!(editor.delete_backward());
    assert_eq!(editor.document.read().unwrap().buffer.data(), b"acd");
    assert_eq!(editor.cursor.offset, 1);
    assert!(!editor.has_selection());
}

#[test]
fn test_insert_at_eof_keeps_the_insertion_cursor_at_eof() {
    let mut editor = create_editor_with_content(b"abcd");
    editor.set_cursor_offset_exact(editor.total_size());

    assert!(editor.insert_bytes(editor.cursor.offset, vec![b'X']));
    assert_eq!(editor.document.read().unwrap().buffer.data(), b"abcdX");
    assert_eq!(editor.cursor.offset, 5);
    assert!(editor.edit_range().is_none());

    editor.move_left();
    assert_eq!(editor.cursor.offset, 4);
    editor.move_right_for_insert();
    assert_eq!(editor.cursor.offset, 5);

    assert!(editor.undo());
    assert_eq!(editor.document.read().unwrap().buffer.data(), b"abcd");
    assert_eq!(editor.cursor.offset, 4);

    assert!(editor.redo());
    assert_eq!(editor.document.read().unwrap().buffer.data(), b"abcdX");
    assert_eq!(editor.cursor.offset, 5);
}

#[test]
fn test_edit_adjusts_layout_and_bookmark_offsets() {
    let mut editor = create_editor_with_content(&[0; 32]);
    editor.custom_layout_mut().add_break(8);
    editor.bookmarks_mut().add_custom(4..12, RgbaColor::from_hsla_f32(0.0, 1.0, 0.5, 0.5));
    let bookmark_id = editor.bookmarks().snapshot()[0].id.clone();

    assert!(editor.insert_bytes(4, vec![1, 2]));
    assert!(editor.custom_layout().has_break(10));
    let bookmark_range = editor
        .bookmarks()
        .snapshot()
        .into_iter()
        .find(|item| item.id == bookmark_id)
        .map(|item| item.range())
        .expect("bookmark remains after insertion");
    assert_eq!(bookmark_range, 6..14);

    assert!(editor.undo());
    assert!(editor.custom_layout().has_break(8));
    let (bookmark_range, bookmark_color) = editor
        .bookmarks()
        .snapshot()
        .into_iter()
        .find(|item| item.id == bookmark_id)
        .map(|item| (item.range(), item.color))
        .expect("bookmark remains after undo");
    assert_eq!(bookmark_range, 4..12);
    assert_eq!(bookmark_color, crate::core::bookmark::BookmarkColor::Red);
}

#[test]
fn test_dirty_state_survives_new_branch_after_undo() {
    let mut editor = create_editor_with_content(b"ab");

    assert!(editor.insert_bytes(1, vec![b'X']));
    editor.document.write().unwrap().mark_as_saved();
    assert!(!editor.document.read().unwrap().is_dirty());

    assert!(editor.undo());
    assert!(editor.document.read().unwrap().is_dirty());

    assert!(editor.insert_bytes(1, vec![b'Y']));
    assert!(editor.document.read().unwrap().is_dirty());
    assert!(!editor.can_redo());
}

#[test]
fn test_line_starts_with_custom_breaks() {
    let mut editor = create_editor_with_content(&[0; 32]);
    // Default: 0, 16
    assert_eq!(editor.line_starts(), vec![0, 16]);

    // Add custom break at 10
    editor.custom_layout_mut().add_break(10);
    // Should be 0, 10, 26
    // current=0 -> push 0. next_custom=10, next_default=16. 10 < 16, so current=10.
    // current=10 -> push 10. next_custom=None, next_default=26. current=26.
    // current=26 -> push 26. next_custom=None, next_default=42. current=42 (>= 32, loop ends).
    assert_eq!(editor.line_starts(), vec![0, 10, 26]);

    // Add custom break at 5
    editor.custom_layout_mut().add_break(5);
    // 0, 5, 10, 26
    assert_eq!(editor.line_starts(), vec![0, 5, 10, 26]);
}

#[test]
fn test_move_up_down_with_custom_breaks() {
    let mut editor = create_editor_with_content(&[0; 32]);
    editor.custom_layout_mut().add_break(10); // Lines: [0..10], [10..26], [26..32]

    editor.set_cursor_offset(5);
    editor.move_down();
    // Move from line 0 pos 5 to line 1 pos 5 (offset 10 + 5 = 15)
    assert_eq!(editor.cursor.offset, 15);

    editor.move_down();
    // Move from line 1 pos 5 to line 2 pos 5 (offset 26 + 5 = 31)
    assert_eq!(editor.cursor.offset, 31);

    editor.move_up();
    assert_eq!(editor.cursor.offset, 15);

    editor.move_up();
    assert_eq!(editor.cursor.offset, 5);

    // Test clamping to line length
    editor.set_cursor_offset(28); // Line 2, pos 2 (28-26)
    editor.move_up();
    // Line 1 is 16 bytes long. pos 2 is valid. 10 + 2 = 12.
    assert_eq!(editor.cursor.offset, 12);

    editor.set_cursor_offset(20); // Line 1, pos 10
    editor.move_down();
    // Line 2 is 6 bytes long. pos 10 is too far. Clamp to EOF position 6. 26 + 6 = 32.
    assert_eq!(editor.cursor.offset, 32);
}

#[test]
fn test_join_line_creates_long_rows() {
    let mut editor = create_editor_with_content(&[0; 48]);
    // Default: 0, 16, 32
    assert_eq!(editor.line_starts(), vec![0, 16, 32]);

    // Join line 0 and line 1 (remove 16-byte boundary at offset 16)
    editor.set_cursor_offset(5); // On line 0
    editor.custom_layout_mut().join_line();
    // Now offset 16 is in custom_joins, so line_starts should skip it
    // current=0 -> push 0. next_pos=16, but 16 is in joins, so next_pos=32. current=32.
    // current=32 -> push 32. next_pos=48, not in joins. current=48 (>= 48, loop ends).
    assert_eq!(editor.line_starts(), vec![0, 32]);
}

#[test]
fn test_join_line_removes_custom_break() {
    let mut editor = create_editor_with_content(&[0; 32]);
    editor.custom_layout_mut().add_break(10);
    // Lines: [0..10], [10..26], [26..32]
    assert_eq!(editor.line_starts(), vec![0, 10, 26]);

    // Join line 0 with line 1 (removes custom break at 10)
    editor.set_cursor_offset(3);
    editor.custom_layout_mut().join_line();
    // Custom break at 10 removed, back to default 16-byte lines
    assert_eq!(editor.line_starts(), vec![0, 16]);
}

#[test]
fn test_join_line_multiple_joins() {
    let mut editor = create_editor_with_content(&[0; 64]);
    // Default: 0, 16, 32, 48
    assert_eq!(editor.line_starts(), vec![0, 16, 32, 48]);

    // Join all into one big line
    editor.set_cursor_offset(0);
    editor.custom_layout_mut().join_line(); // joins 0+16 -> skip 16
    editor.custom_layout_mut().join_line(); // joins 0+32 -> skip 32
    editor.custom_layout_mut().join_line(); // joins 0+48 -> skip 48
    // All boundaries joined, single line
    assert_eq!(editor.line_starts(), vec![0]);
}

#[test]
fn test_clear_all_custom_breaks() {
    let mut editor = create_editor_with_content(&[0; 48]);
    editor.custom_layout_mut().add_break(5);
    editor.custom_layout_mut().add_break(10);
    editor.set_cursor_offset(0);
    editor.custom_layout_mut().join_line(); // join some lines

    assert!(editor.has_custom_layout());
    assert!(editor.custom_layout_count() > 0);

    editor.custom_layout_mut().clear_all();
    assert!(!editor.has_custom_layout());
    assert_eq!(editor.custom_layout_count(), 0);
    // Back to default
    assert_eq!(editor.line_starts(), vec![0, 16, 32]);
}

#[test]
fn test_custom_break_overrides_join() {
    let mut editor = create_editor_with_content(&[0; 48]);
    // Join at 16
    editor.set_cursor_offset(0);
    editor.custom_layout_mut().join_line();
    assert_eq!(editor.line_starts(), vec![0, 32]);

    // Adding a custom break at 16 should remove the join
    editor.custom_layout_mut().add_break(16);
    assert_eq!(editor.line_starts(), vec![0, 16, 32]);
}

#[test]
fn test_sparse_line_map_large_offsets() {
    let buffer = crate::core::buffer::Buffer::new(vec![0; 100_000]);
    let document = Arc::new(RwLock::new(Document::new(std::path::PathBuf::from("test"), buffer)));
    let mut editor = Editor::new(document);

    let starts = editor.line_starts();
    assert!(matches!(starts, LineMap::Standard { .. }));
    assert_eq!(starts.len(), 100_000_usize.div_ceil(16));

    editor.custom_layout_mut().add_break(50_000);
    editor.custom_layout_mut().add_break(50_010);

    let starts = editor.line_starts();
    assert!(matches!(starts, LineMap::Sparse(_)));

    if let LineMap::Sparse(ref sparse) = starts {
        assert!(sparse.segments.len() <= 5);
    }

    assert_eq!(starts.get(0), Some(0));
    assert_eq!(starts.get(100), Some(1600));

    assert_eq!(starts.binary_search(&0), Ok(0));
    assert_eq!(starts.binary_search(&1600), Ok(100));

    let line_idx = Editor::find_line_index(50_000, &starts);
    assert_eq!(starts.get(line_idx), Some(50_000));
    assert_eq!(starts.get(line_idx + 1), Some(50_010));
}

#[test]
fn test_double_empty_line() {
    // Enterを2回押すと empty_lines[offset] = 2 になる
    // 2行分の空行が正しく生成されることを確認するリグレッションテスト
    let mut editor = create_editor_with_content(&[0; 32]);
    // デフォルト: [0..16], [16..32]
    assert_eq!(editor.line_starts(), vec![0, 16]);

    // offset 16 に空行を1つ追加
    editor.custom_layout_mut().add_empty_line(16);
    // [0..16], [空], [16..32] の3行
    assert_eq!(editor.line_starts(), vec![0, 16, 16]);
    assert_eq!(editor.line_starts().len(), 3);

    // offset 16 にさらに空行をもう1つ追加（2回目のEnter）
    editor.custom_layout_mut().add_empty_line(16);
    // [0..16], [空1], [空2], [16..32] の4行
    // 修正前はここで3行しか返らずバグになっていた
    assert_eq!(editor.line_starts(), vec![0, 16, 16, 16]);
    assert_eq!(editor.line_starts().len(), 4);

    // offset 0 にも2回空行を追加
    editor.custom_layout_mut().add_empty_line(0);
    editor.custom_layout_mut().add_empty_line(0);
    // [空1@0], [空2@0], [0..16], [空1@16], [空2@16], [16..32] の6行
    assert_eq!(editor.line_starts(), vec![0, 0, 0, 16, 16, 16]);
    assert_eq!(editor.line_starts().len(), 6);
}

#[test]
fn test_split_mega_line_preserves_end() {
    // delete×3 で 64バイトのメガ行を作り、オフセット5で改行したとき
    // [0..5] と [5..64] の2行になることを確認するリグレッションテスト
    let mut editor = create_editor_with_content(&[0; 64]);
    assert_eq!(editor.line_starts(), vec![0, 16, 32, 48]);

    // delete×3 → 64バイトのメガ行
    editor.set_cursor_offset(0);
    editor.custom_layout_mut().join_line();
    editor.custom_layout_mut().join_line();
    editor.custom_layout_mut().join_line();
    assert_eq!(editor.line_starts(), vec![0]);

    // オフセット5で改行 → [0..5] と [5..64]
    editor.custom_layout_mut().add_break(5);
    assert_eq!(editor.line_starts(), vec![0, 5]);
    assert_eq!(editor.line_starts().len(), 2);

    // 48バイト行のケース（user報告のシナリオ）:
    // 別バッファ: 48バイト, delete×2 → 48バイト行, オフセット5で改行
    let mut editor2 = create_editor_with_content(&[0; 48]);
    assert_eq!(editor2.line_starts(), vec![0, 16, 32]);
    editor2.set_cursor_offset(0);
    editor2.custom_layout_mut().join_line();
    editor2.custom_layout_mut().join_line();
    assert_eq!(editor2.line_starts(), vec![0]); // 48バイト行

    editor2.custom_layout_mut().add_break(5);
    // [0..5] (5バイト) と [5..48] (43バイト)
    assert_eq!(editor2.line_starts(), vec![0, 5]);
    assert_eq!(editor2.line_starts().len(), 2);

    // 追加ケース: 32バイトのメガ行をオフセット7で分割
    let mut editor3 = create_editor_with_content(&[0; 32]);
    editor3.set_cursor_offset(0);
    editor3.custom_layout_mut().join_line();
    assert_eq!(editor3.line_starts(), vec![0]); // 32バイト行
    editor3.custom_layout_mut().add_break(7);
    // [0..7] と [7..32]
    assert_eq!(editor3.line_starts(), vec![0, 7]);
}

#[test]
fn test_split_mega_line_mid_join() {
    // delete で 32バイトのメガ行を作り、オフセット18（結合境界16の後）で改行したとき
    // [0..18] と [18..32] の2行になることを確認するリグレッションテスト
    // 修正前は join@16 が削除されて [0..16], [16..18], [18..32] の3行になっていた
    let mut editor = create_editor_with_content(&[0; 32]);
    assert_eq!(editor.line_starts(), vec![0, 16]);

    // delete → 32バイトのメガ行
    editor.set_cursor_offset(0);
    editor.custom_layout_mut().join_line();
    assert_eq!(editor.line_starts(), vec![0]);

    // オフセット18で改行 → [0..18] と [18..32]
    editor.custom_layout_mut().add_break(18);
    assert_eq!(editor.line_starts(), vec![0, 18]);
    assert_eq!(editor.line_starts().len(), 2);

    // 64バイトのメガ行をオフセット18で分割
    let mut editor2 = create_editor_with_content(&[0; 64]);
    editor2.set_cursor_offset(0);
    editor2.custom_layout_mut().join_line();
    editor2.custom_layout_mut().join_line();
    editor2.custom_layout_mut().join_line();
    assert_eq!(editor2.line_starts(), vec![0]); // 64バイトのメガ行

    editor2.custom_layout_mut().add_break(18);
    // [0..18] と [18..64]
    assert_eq!(editor2.line_starts(), vec![0, 18]);
    assert_eq!(editor2.line_starts().len(), 2);
}

#[test]
fn test_editor_empty_lines_and_breaks() {
    use crate::core::buffer::Buffer;
    use std::path::PathBuf;
    let doc = Arc::new(RwLock::new(Document::new(PathBuf::from("test.bin"), Buffer::new(vec![0; 100]))));
    let mut editor = Editor::new(doc);

    editor.custom_layout_mut().add_empty_line(10);
    assert_eq!(editor.custom_layout().empty_lines_at(10), 1);

    editor.custom_layout_mut().add_empty_line(10);
    assert_eq!(editor.custom_layout().empty_lines_at(10), 2);

    assert!(editor.custom_layout_mut().remove_empty_line(10));
    assert_eq!(editor.custom_layout().empty_lines_at(10), 1);

    assert!(editor.custom_layout_mut().remove_empty_line(10));
    assert_eq!(editor.custom_layout().empty_lines_at(10), 0);

    editor.custom_layout_mut().toggle_break(20);
    assert!(editor.custom_layout().has_break(20));

    editor.custom_layout_mut().toggle_break(20);
    assert!(!editor.custom_layout().has_break(20));
}

#[test]
fn test_custom_bookmarks() {
    let mut editor = create_editor_with_content(b"01234567890123456789"); // 20 bytes
    let red = RgbaColor::from_hsla_f32(0.0, 1.0, 0.5, 0.5);
    let blue = RgbaColor::from_hsla_f32(0.6, 1.0, 0.5, 0.5);

    // Add red bookmark on 0..10
    editor.bookmarks_mut().add_custom(0..10, red);
    assert_eq!(editor.bookmarks().snapshot().len(), 1);
    assert_eq!(editor.bookmarks().snapshot()[0].range(), 0..10);
    assert_eq!(editor.bookmarks().snapshot()[0].color, BookmarkColor::Red);

    // Update comment
    let id = editor.bookmarks().snapshot()[0].id.clone();
    assert!(editor.bookmarks_mut().update_comment(&id, "Header block"));
    assert_eq!(editor.bookmarks().snapshot()[0].comment, "Header block");

    // Add blue bookmark on 5..15
    editor.bookmarks_mut().add_custom(5..15, blue);
    assert_eq!(editor.bookmarks().snapshot().len(), 2);
    assert_eq!(editor.bookmarks().snapshot()[0].range(), 0..5);
    assert_eq!(editor.bookmarks().snapshot()[1].range(), 5..15);
    assert_eq!(editor.bookmarks().snapshot()[1].color, BookmarkColor::Blue);

    // Clear sub-range 3..7
    editor.bookmarks_mut().clear_custom(3..7);
    assert_eq!(editor.bookmarks().snapshot().len(), 2);
    assert_eq!(editor.bookmarks().snapshot()[0].range(), 0..3);
    assert_eq!(editor.bookmarks().snapshot()[1].range(), 7..15);

    // Clear all
    editor.bookmarks_mut().clear_all();
    assert!(editor.bookmarks().snapshot().is_empty());
}

#[test]
fn test_editor_bookmarks_crud_and_file_io() {
    let mut editor = create_editor_with_content(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"); // 36 bytes

    // Add bookmarks
    let item1 = BookmarkItem::new(0, 4, BookmarkColor::Red, "Magic bytes");
    let id1 = item1.id.clone();
    editor.bookmarks_mut().add(item1);

    let item2 = BookmarkItem::new(10, 8, BookmarkColor::Green, "Payload");
    let id2 = item2.id.clone();
    editor.bookmarks_mut().add(item2);

    assert_eq!(editor.bookmarks().snapshot().len(), 2);

    // Update comment
    assert!(editor.bookmarks_mut().update_comment(&id1, "ELF Magic"));
    assert_eq!(editor.bookmarks().snapshot()[0].comment, "ELF Magic");

    // Update color
    assert!(editor.bookmarks_mut().update_color(&id1, BookmarkColor::Cyan));
    assert_eq!(editor.bookmarks().snapshot()[0].color, BookmarkColor::Cyan);

    // Update range
    assert!(editor.bookmarks_mut().update_range(&id2, 12, 10));
    assert_eq!(editor.bookmarks().snapshot()[1].offset, 12);
    assert_eq!(editor.bookmarks().snapshot()[1].size, 10);

    // Test export and import
    let temp_file = std::env::temp_dir().join("editor_bookmarks_test.bookmark.yaml");
    editor.bookmarks().export_to_file(&temp_file).unwrap();
    assert!(temp_file.exists());

    // Create new editor and import
    let mut editor2 = create_editor_with_content(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");
    let count = editor2.bookmarks_mut().import_from_file(&temp_file).unwrap();
    assert_eq!(count, 2);
    assert_eq!(editor2.bookmarks().snapshot().len(), 2);
    assert_eq!(editor2.bookmarks().snapshot()[0].comment, "ELF Magic");
    assert_eq!(editor2.bookmarks().snapshot()[0].color, BookmarkColor::Cyan);
    assert_eq!(editor2.bookmarks().snapshot()[1].offset, 12);
    assert_eq!(editor2.bookmarks().snapshot()[1].size, 10);

    // Remove by id
    assert!(editor.bookmarks_mut().remove_by_id(&id1));
    assert_eq!(editor.bookmarks().snapshot().len(), 1);
    assert_eq!(editor.bookmarks().snapshot()[0].id, id2);

    // Remove by index
    let removed = editor.bookmarks_mut().remove_by_index(0);
    assert!(removed.is_some());
    assert!(editor.bookmarks().snapshot().is_empty());

    let _ = std::fs::remove_file(temp_file);
}

#[test]
fn test_import_and_add_bookmark_no_id_collision() {
    let mut editor = create_editor_with_content(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");
    let item1 = BookmarkItem::new(0, 4, BookmarkColor::Red, "Magic bytes");
    editor.bookmarks_mut().add(item1);

    let temp_file = std::env::temp_dir().join("collision_test.bookmark.yaml");
    editor.bookmarks().export_to_file(&temp_file).unwrap();

    let mut editor2 = create_editor_with_content(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");
    editor2.bookmarks_mut().import_from_file(&temp_file).unwrap();
    assert_eq!(editor2.bookmarks().snapshot().len(), 1);

    // Now add a new bookmark
    let new_item = BookmarkItem::new(10, 4, BookmarkColor::Yellow, "New bookmark");
    let added_id = editor2.bookmarks_mut().add(new_item);

    // Must have 2 distinct IDs
    assert_eq!(editor2.bookmarks().snapshot().len(), 2);
    assert_ne!(editor2.bookmarks().snapshot()[0].id, editor2.bookmarks().snapshot()[1].id);
    assert_eq!(editor2.bookmarks().snapshot()[1].id, added_id);

    // Editing one must not affect the other
    assert!(editor2.bookmarks_mut().update_comment(&added_id, "Updated new comment"));
    assert_eq!(editor2.bookmarks().snapshot()[0].comment, "Magic bytes");
    assert_eq!(editor2.bookmarks().snapshot()[1].comment, "Updated new comment");

    // Deleting the new one must leave the first intact
    assert!(editor2.bookmarks_mut().remove_by_id(&added_id));
    assert_eq!(editor2.bookmarks().snapshot().len(), 1);
    assert_eq!(editor2.bookmarks().snapshot()[0].comment, "Magic bytes");

    let _ = std::fs::remove_file(temp_file);
}

#[test]
fn test_shared_bookmarks_across_split_editors() {
    use crate::core::buffer::Buffer;
    use std::path::PathBuf;
    let doc = Arc::new(RwLock::new(Document::new(PathBuf::from("shared.bin"), Buffer::new(vec![0xAA; 128]))));
    let mut editor1 = Editor::new(doc.clone());
    let editor2 = Editor::new(doc.clone());

    // Both start empty
    assert_eq!(editor1.bookmarks().snapshot().len(), 0);
    assert_eq!(editor2.bookmarks().snapshot().len(), 0);

    // Add bookmark in editor1
    let hl = BookmarkItem::new(0, 16, BookmarkColor::Red, "Header");
    let id = editor1.bookmarks_mut().add(hl);

    // editor2 immediately sees the bookmark from shared instance
    assert_eq!(editor2.bookmarks().snapshot().len(), 1);
    assert_eq!(editor2.bookmarks().snapshot()[0].comment, "Header");
    assert_eq!(editor2.bookmarks().snapshot()[0].color, BookmarkColor::Red);

    // Update comment in editor1
    assert!(editor1.bookmarks_mut().update_comment(&id, "Updated Header"));
    assert_eq!(editor2.bookmarks().snapshot()[0].comment, "Updated Header");

    // Clear in editor1
    editor1.bookmarks_mut().clear_all();
    assert_eq!(editor2.bookmarks().snapshot().len(), 0);
}

#[test]
fn test_shared_custom_breaks_across_split_editors() {
    use crate::core::buffer::Buffer;
    use std::path::PathBuf;
    let doc = Arc::new(RwLock::new(Document::new(PathBuf::from("shared.bin"), Buffer::new(vec![0xBB; 128]))));
    let mut editor1 = Editor::new(doc.clone());
    let editor2 = Editor::new(doc.clone());

    editor1.custom_layout_mut().add_break(20);
    assert!(editor2.custom_layout().has_break(20));

    editor1.custom_layout_mut().remove_break(20);
    assert!(!editor2.custom_layout().has_break(20));
}

#[test]
fn test_editor_radix_group_size_and_endian() {
    let mut editor = create_editor_with_content(b"Hello World 12345678");
    assert_eq!(editor.options.radix, DisplayRadix::Hexadecimal);
    assert_eq!(editor.options.group_size, ByteGroupSize::One);
    assert!(!editor.options.is_big_endian);

    editor.set_radix(DisplayRadix::Decimal);
    assert_eq!(editor.options.radix, DisplayRadix::Decimal);

    editor.set_group_size(ByteGroupSize::Four);
    assert_eq!(editor.options.group_size, ByteGroupSize::Four);

    editor.set_is_big_endian(true);
    assert!(editor.options.is_big_endian);

    editor.toggle_byte_order();
    assert!(!editor.options.is_big_endian);
}

#[test]
fn test_editor_grouping_cursor_movement_and_selection() {
    let mut editor = create_editor_with_content(&[0u8; 64]);
    editor.set_group_size(ByteGroupSize::Four);
    assert_eq!(editor.cursor.offset, 0);

    // Move right by 4 bytes (1 group)
    editor.move_right();
    assert_eq!(editor.cursor.offset, 4);
    editor.move_right();
    assert_eq!(editor.cursor.offset, 8);

    // Move left by 4 bytes
    editor.move_left();
    assert_eq!(editor.cursor.offset, 4);

    // Selection right by 4 bytes
    editor.select_right();
    assert_eq!(editor.selection(), Selection::new(4, 8));
    assert_eq!(editor.cursor.offset, 8);
    assert_eq!(editor.selected_range_or_cursor(), Some(4..8));

    // Selection left
    editor.select_left();
    assert_eq!(editor.cursor.offset, 4);
    assert_eq!(editor.selected_range_or_cursor(), Some(4..8));

    // Group 2 bytes
    editor.set_group_size(ByteGroupSize::Two);
    editor.go_to_beginning();
    assert_eq!(editor.cursor.offset, 0);
    editor.move_right();
    assert_eq!(editor.cursor.offset, 2);
    assert_eq!(editor.selected_range_or_cursor(), Some(2..4));

    // Move down across 16-byte rows
    editor.move_down();
    assert_eq!(editor.cursor.offset, 18);
    editor.move_up();
    assert_eq!(editor.cursor.offset, 2);
}

#[test]
fn test_join_line_with_selection_multiple_lines() {
    let mut editor = create_editor_with_content(&[0u8; 64]);
    assert_eq!(editor.line_starts().len(), 4); // 0, 16, 32, 48

    // Select 0..32 (first 2 lines: bytes 0..=31)
    editor.set_selection(0, 32);

    editor.custom_layout_mut().join_line();

    let line_starts = editor.line_starts();
    assert_eq!(line_starts.len(), 3);
    assert_eq!(line_starts.get(0), Some(0));
    assert_eq!(line_starts.get(1), Some(32));
    assert_eq!(line_starts.get(2), Some(48));
}

#[test]
fn test_join_line_with_selection_arbitrary_sub_range() {
    let mut editor = create_editor_with_content(&[0u8; 100]);
    // Initially 16-byte chunks: 0, 16, 32, 48, 64, 80, 96

    // Select bytes 10..=49 (range 10..50, 40 bytes)
    editor.set_selection(10, 50);

    editor.custom_layout_mut().join_line();

    let line_starts = editor.line_starts();
    assert_eq!(line_starts.get(0), Some(0)); // 0..10
    assert_eq!(line_starts.get(1), Some(10)); // 10..50 (joined line!)
    assert_eq!(line_starts.get(2), Some(50)); // 50..66
}

#[test]
fn test_join_line_with_selection_cleans_custom_breaks() {
    let mut editor = create_editor_with_content(&[0u8; 32]);
    editor.custom_layout_mut().add_break(4);
    editor.custom_layout_mut().add_break(8);
    editor.custom_layout_mut().add_break(12);

    // Select bytes 0..=15 (0..16)
    editor.set_selection(0, 16);

    editor.custom_layout_mut().join_line();

    let line_starts = editor.line_starts();
    assert_eq!(line_starts.get(0), Some(0)); // 0..16
    assert_eq!(line_starts.get(1), Some(16)); // 16..32
}

#[test]
fn test_go_to_offset() {
    let mut editor = create_editor_with_content(&[0u8; 64]);
    assert_eq!(editor.cursor.offset, 0);

    // Jump to offset 30 without extending selection
    editor.go_to_offset(30, false);
    assert_eq!(editor.cursor.offset, 30);
    assert!(!editor.has_selection());

    // Jump beyond total size clamps to total_size
    editor.go_to_offset(100, false);
    assert_eq!(editor.cursor.offset, 64);
    assert!(!editor.has_selection());
}

#[test]
fn test_go_to_offset_extend_selection() {
    let mut editor = create_editor_with_content(&[0u8; 64]);
    editor.set_cursor_offset(10);

    // Extend selection from 10 to 40
    editor.go_to_offset(40, true);
    assert_eq!(editor.cursor.offset, 40);
    assert_eq!(editor.selection_range(), Some(10..40));

    // Further extend selection to 50
    editor.go_to_offset(50, true);
    assert_eq!(editor.cursor.offset, 50);
    assert_eq!(editor.selection_range(), Some(10..50));
}

#[test]
fn test_go_to_range() {
    let mut editor = create_editor_with_content(&[0u8; 64]);
    assert_eq!(editor.cursor.offset, 0);

    // Jump to range 10..40
    editor.go_to_range(10..40);
    assert_eq!(editor.cursor.offset, 10);
    assert!(editor.has_selection());
    assert_eq!(editor.selection_range(), Some(10..40));

    // Jump to range clamped to total size
    editor.go_to_range(50..100);
    assert_eq!(editor.cursor.offset, 50);
    assert_eq!(editor.selection_range(), Some(50..64));
}

#[test]
fn test_editor_insert_bytes_updates_address_map() {
    use crate::core::address_map::{AddressMap, MemorySegment};
    use crate::core::buffer::Buffer;
    use crate::core::document::Document;
    use std::path::PathBuf;

    let map = AddressMap::from_segments(vec![
        MemorySegment {
            buffer_offset: 0,
            address: 0x00FD_0000,
            length: 10,
        },
        MemorySegment {
            buffer_offset: 10,
            address: 0x0100_0000,
            length: 10,
        },
    ]);

    let doc = Document::new(PathBuf::from("test.mot"), Buffer::new(vec![0u8; 20])).with_address_map(map);
    let mut editor = Editor::new(Arc::new(RwLock::new(doc)));

    assert_eq!(editor.offset_to_address(10), 0x0100_0000);

    // Insert 2 bytes at offset 5 (inside first segment)
    editor.insert_bytes(5, vec![0xAA, 0xBB]);

    assert_eq!(editor.total_size(), 22);
    // First segment mapping
    assert_eq!(editor.offset_to_address(0), 0x00FD_0000);
    assert_eq!(editor.offset_to_address(5), 0x00FD_0005);
    assert_eq!(editor.offset_to_address(6), 0x00FD_0006);
    assert_eq!(editor.offset_to_address(11), 0x00FD_000B);
    // Second segment mapping shifted in buffer to offset 12, address STILL 0x0100_0000!
    assert_eq!(editor.offset_to_address(12), 0x0100_0000);
    assert_eq!(editor.offset_to_address(21), 0x0100_0009);

    // Undo
    editor.undo();
    assert_eq!(editor.total_size(), 20);
    assert_eq!(editor.offset_to_address(10), 0x0100_0000);
}

#[test]
fn test_bookmark_visibility_basic_and_line_starts() {
    use crate::core::bookmark::{BookmarkColor, BookmarkItem};
    let content = vec![0u8; 100];
    let mut editor = create_editor_with_content(&content);
    assert_eq!(editor.line_starts().len(), 7); // 100 / 16 ceil = 7

    let bm = BookmarkItem::new(16, 48, BookmarkColor::Red, "Header");
    editor.bookmarks_mut().add(bm);

    // Initially visible
    assert_eq!(editor.line_starts().len(), 7);
    assert!(!editor.is_folded(16));

    // Hide Red bookmarks
    editor.bookmarks_mut().hide_color(BookmarkColor::Red);
    assert!(editor.bookmarks().is_color_hidden(BookmarkColor::Red));
    assert!(editor.is_folded(16));
    assert!(editor.is_folded(20));
    assert!(editor.is_folded(63));
    assert!(!editor.is_folded(15));
    assert!(!editor.is_folded(64));
    assert_eq!(editor.fold_end_at(16), Some(64));
    assert_eq!(editor.fold_containing(30), Some((16, 64)));

    let summary = editor.fold_bookmark_summary_at(16).unwrap();
    assert_eq!(summary.start_offset, 16);
    assert_eq!(summary.end_offset, 64);
    assert_eq!(summary.size, 48);
    assert_eq!(summary.color, BookmarkColor::Red);
    assert_eq!(summary.comment, "Header");

    let line_starts = editor.line_starts();
    assert_eq!(line_starts.len(), 5);
    assert_eq!(line_starts.get(0), Some(0));
    assert_eq!(line_starts.get(1), Some(16)); // Folded bookmark row
    assert_eq!(line_starts.get(2), Some(64));
    assert_eq!(line_starts.get(3), Some(80));
    assert_eq!(line_starts.get(4), Some(96));

    assert_eq!(Editor::find_line_index(0, &line_starts), 0);
    assert_eq!(Editor::find_line_index(15, &line_starts), 0);
    assert_eq!(Editor::find_line_index(16, &line_starts), 1);
    assert_eq!(Editor::find_line_index(30, &line_starts), 1);
    assert_eq!(Editor::find_line_index(63, &line_starts), 1);
    assert_eq!(Editor::find_line_index(64, &line_starts), 2);
}

#[test]
fn test_bookmark_visibility_overlapping_and_merging() {
    use crate::core::bookmark::{BookmarkColor, BookmarkItem};
    let content = vec![0u8; 100];
    let mut editor = create_editor_with_content(&content);

    for bm in [
        BookmarkItem::new(20, 20, BookmarkColor::Red, "Part 1"),    // [20, 40)
        BookmarkItem::new(35, 25, BookmarkColor::Orange, "Part 2"), // [35, 60)
        BookmarkItem::new(70, 10, BookmarkColor::Blue, "Part 3"),   // [70, 80)
    ] {
        editor.bookmarks_mut().add(bm);
    }

    // Hide Red: [20, 40)
    editor.bookmarks_mut().hide_color(BookmarkColor::Red);
    assert_eq!(editor.fold_containing(25), Some((20, 40)));
    assert!(!editor.is_folded(50));

    // Hide Orange: [20, 40) and [35, 60) merge into [20, 60)
    editor.bookmarks_mut().hide_color(BookmarkColor::Orange);
    assert_eq!(editor.fold_containing(25), Some((20, 60)));
    assert_eq!(editor.fold_containing(55), Some((20, 60)));

    // Hide Blue: disjoint [70, 80)
    editor.bookmarks_mut().hide_color(BookmarkColor::Blue);
    assert_eq!(editor.fold_containing(25), Some((20, 60)));
    assert_eq!(editor.fold_containing(75), Some((70, 80)));
    assert!(!editor.is_folded(65));
}

#[test]
fn test_bookmark_visibility_show_only_and_show_all() {
    use crate::core::bookmark::{BookmarkColor, BookmarkItem};
    let content = vec![0u8; 100];
    let mut editor = create_editor_with_content(&content);

    for bm in [
        BookmarkItem::new(10, 20, BookmarkColor::Red, "RedSec"),
        BookmarkItem::new(40, 20, BookmarkColor::Blue, "BlueSec"),
        BookmarkItem::new(70, 20, BookmarkColor::Green, "GreenSec"),
    ] {
        editor.bookmarks_mut().add(bm);
    }

    // Show only Blue: Red and Green are hidden, Blue remains visible
    editor.bookmarks_mut().show_only_color(BookmarkColor::Blue);
    assert!(editor.bookmarks().is_color_hidden(BookmarkColor::Red));
    assert!(editor.bookmarks().is_color_hidden(BookmarkColor::Green));
    assert!(!editor.bookmarks().is_color_hidden(BookmarkColor::Blue));

    assert!(editor.is_folded(15));
    assert!(!editor.is_folded(45));
    assert!(editor.is_folded(75));

    // Show all
    editor.bookmarks_mut().show_all();
    assert!(!editor.is_folded(15));
    assert!(!editor.is_folded(45));
    assert!(!editor.is_folded(75));

    // Hide all
    editor.bookmarks_mut().hide_all();
    assert!(editor.is_folded(15));
    assert!(editor.is_folded(45));
    assert!(editor.is_folded(75));
}

#[test]
fn test_bookmark_visibility_individual_toggle() {
    use crate::core::bookmark::{BookmarkColor, BookmarkItem};
    let content = vec![0u8; 100];
    let mut editor = create_editor_with_content(&content);

    let bm1 = BookmarkItem::new(10, 20, BookmarkColor::Yellow, "BM 1");
    let bm2 = BookmarkItem::new(50, 20, BookmarkColor::Yellow, "BM 2");
    let id1 = editor.bookmarks_mut().add(bm1);
    let id2 = editor.bookmarks_mut().add(bm2);

    // Toggle individual bookmark 1
    editor.bookmarks_mut().toggle_item_visibility(&id1);
    assert!(editor.bookmarks().is_id_hidden(&id1));
    assert!(!editor.bookmarks().is_id_hidden(&id2));
    assert!(editor.is_folded(15));
    assert!(!editor.is_folded(55));

    // Toggle individual bookmark 1 back
    editor.bookmarks_mut().toggle_item_visibility(&id1);
    assert!(!editor.bookmarks().is_id_hidden(&id1));
    assert!(!editor.is_folded(15));
}

#[test]
fn test_bookmark_visibility_cursor_navigation_skips_fold() {
    use crate::core::bookmark::{BookmarkColor, BookmarkItem};
    let content = vec![0u8; 64];
    let mut editor = create_editor_with_content(&content);

    let bm = BookmarkItem::new(16, 32, BookmarkColor::Cyan, "Middle");
    editor.bookmarks_mut().add(bm);
    editor.bookmarks_mut().hide_color(BookmarkColor::Cyan);

    // Initial cursor at 0
    assert_eq!(editor.cursor.offset, 0);

    // Move down: should skip 16..48 fold row and land on 48
    editor.move_down();
    assert_eq!(editor.cursor.offset, 48);

    // Move up: should skip back to 0
    editor.move_up();
    assert_eq!(editor.cursor.offset, 0);
}

#[test]
fn test_bookmark_visibility_auto_unfold_on_goto_and_search() {
    use crate::core::bookmark::{BookmarkColor, BookmarkItem};
    let mut data = vec![0u8; 100];
    data[30..36].copy_from_slice(b"TARGET");
    let mut editor = create_editor_with_content(&data);

    let bm = BookmarkItem::new(20, 30, BookmarkColor::Purple, "Section");
    editor.bookmarks_mut().add(bm);
    editor.bookmarks_mut().hide_color(BookmarkColor::Purple);
    assert!(editor.is_folded(30));

    // go_to_offset should auto-unfold
    editor.go_to_offset(30, false);
    assert_eq!(editor.cursor.offset, 30);
    assert!(!editor.is_folded(30));

    // Re-hide Purple
    editor.bookmarks_mut().hide_color(BookmarkColor::Purple);
    assert!(editor.is_folded(30));

    // Search navigation should auto-unfold
    editor
        .search_state_mut()
        .set_query_and_mode("TARGET".to_string(), crate::core::search::SearchMode::Text);
    let match_offset = editor.find_and_navigate_next();
    assert_eq!(match_offset, Some(30));
    assert_eq!(editor.cursor.offset, 30);
    assert!(!editor.is_folded(30));
}

#[test]
fn test_bookmark_visibility_adjust_after_edit() {
    use crate::core::bookmark::{BookmarkColor, BookmarkItem};
    let content = vec![0u8; 100];
    let mut editor = create_editor_with_content(&content);

    let bm = BookmarkItem::new(40, 20, BookmarkColor::Pink, "Payload");
    editor.bookmarks_mut().add(bm);
    editor.bookmarks_mut().hide_color(BookmarkColor::Pink);
    assert_eq!(editor.fold_containing(50), Some((40, 60)));

    // Insert 5 bytes before the fold -> bookmark shifts to 45..65
    editor.insert_bytes(10, vec![0xAA; 5]);
    assert_eq!(editor.fold_containing(50), Some((45, 65)));

    // Remove 5 bytes before the fold -> bookmark shifts back to 40..60
    editor.replace_range(10..15, vec![]);
    assert_eq!(editor.fold_containing(50), Some((40, 60)));
}

#[test]
fn test_bookmark_visibility_hide_unbookmarked() {
    use crate::core::bookmark::{BookmarkColor, BookmarkItem};
    let content = vec![0u8; 100];
    let mut editor = create_editor_with_content(&content);

    // Add 2 bookmarks: [20..36) (Red), [60..80) (Blue)
    for bm in [
        BookmarkItem::new(20, 16, BookmarkColor::Red, "Header"),
        BookmarkItem::new(60, 20, BookmarkColor::Blue, "Data"),
    ] {
        editor.bookmarks_mut().add(bm);
    }

    // Enable hide_unbookmarked
    editor.bookmarks_mut().toggle_hide_unbookmarked();
    assert!(editor.bookmarks().is_hide_unbookmarked());

    // Computed folds should be the unbookmarked gaps:
    // [0..20), [36..60), [80..100)
    let folds = editor.computed_folded_regions();
    assert_eq!(folds.len(), 3);
    assert_eq!(folds.get(&0), Some(&20));
    assert_eq!(folds.get(&36), Some(&60));
    assert_eq!(folds.get(&80), Some(&100));

    assert!(editor.is_folded(10));
    assert!(!editor.is_folded(25)); // Inside first bookmark
    assert!(editor.is_folded(45));
    assert!(!editor.is_folded(70)); // Inside second bookmark
    assert!(editor.is_folded(90));

    let summary0 = editor.fold_bookmark_summary_at(0).unwrap();
    assert_eq!(summary0.comment, "Unbookmarked");
    assert!(summary0.is_unbookmarked);

    // Check line_starts in hide_unbookmarked mode:
    // Line 0: Fold [0..20)
    // Line 1: Data row 20..36 (16 bytes = 1 hex row)
    // Line 2: Fold [36..60)
    // Line 3: Data row 60..76
    // Line 4: Data row 76..80
    // Line 5: Fold [80..100)
    let line_starts = editor.line_starts();
    assert_eq!(line_starts.len(), 6);
    assert_eq!(line_starts.get(0), Some(0));
    assert_eq!(line_starts.get(1), Some(20));
    assert_eq!(line_starts.get(2), Some(36));
    assert_eq!(line_starts.get(3), Some(60));
    assert_eq!(line_starts.get(4), Some(76));
    assert_eq!(line_starts.get(5), Some(80));

    // Now also hide Red bookmark: [20..36) becomes folded as its own Red fold banner,
    // separate from unbookmarked gaps [0..20) and [36..60)!
    editor.bookmarks_mut().hide_color(BookmarkColor::Red);
    let folds2 = editor.computed_folded_regions();
    assert_eq!(folds2.len(), 4);
    assert_eq!(folds2.get(&0), Some(&20));
    assert_eq!(folds2.get(&20), Some(&36));
    assert_eq!(folds2.get(&36), Some(&60));
    assert_eq!(folds2.get(&80), Some(&100));

    let summary_gap0 = editor.fold_bookmark_summary_at(0).unwrap();
    assert!(summary_gap0.is_unbookmarked);

    let summary_red = editor.fold_bookmark_summary_at(20).unwrap();
    assert!(!summary_red.is_unbookmarked);
    assert_eq!(summary_red.color, BookmarkColor::Red);
    assert_eq!(summary_red.comment, "Header");

    let summary_gap1 = editor.fold_bookmark_summary_at(36).unwrap();
    assert!(summary_gap1.is_unbookmarked);
}

#[test]
fn test_bookmark_visibility_hide_unbookmarked_navigation() {
    use crate::core::bookmark::{BookmarkColor, BookmarkItem};
    let content = vec![0u8; 100];
    let mut editor = create_editor_with_content(&content);

    for bm in [
        BookmarkItem::new(16, 16, BookmarkColor::Green, "Green1"),
        BookmarkItem::new(64, 16, BookmarkColor::Green, "Green2"),
    ] {
        editor.bookmarks_mut().add(bm);
    }

    editor.bookmarks_mut().set_hide_unbookmarked(true);

    // Initial cursor at offset 16 (first byte of first visible bookmark)
    editor.go_to_offset(16, false);
    assert_eq!(editor.cursor.offset, 16);

    // Move down: should skip unbookmarked gap [32..64) and land on 64 (start of next bookmark)
    editor.move_down();
    assert_eq!(editor.cursor.offset, 64);

    // Move up: should skip back to 16
    editor.move_up();
    assert_eq!(editor.cursor.offset, 16);
}

#[test]
fn test_unfold_single_bookmark_when_color_hidden() {
    use crate::core::bookmark::{BookmarkColor, BookmarkItem};
    let content = vec![0u8; 100];
    let mut editor = create_editor_with_content(&content);

    // Add 3 Red bookmarks: [10..20), [40..50), [70..80)
    let bm1 = BookmarkItem::new(10, 10, BookmarkColor::Red, "Red 1");
    let bm2 = BookmarkItem::new(40, 10, BookmarkColor::Red, "Red 2");
    let bm3 = BookmarkItem::new(70, 10, BookmarkColor::Red, "Red 3");
    let id1 = editor.bookmarks_mut().add(bm1);
    let id2 = editor.bookmarks_mut().add(bm2);
    let id3 = editor.bookmarks_mut().add(bm3);

    // Hide all Red bookmarks
    editor.bookmarks_mut().hide_color(BookmarkColor::Red);
    assert!(editor.is_folded(10));
    assert!(editor.is_folded(40));
    assert!(editor.is_folded(70));

    // Click/unfold ONLY the middle bookmark [40..50)
    let changed = editor.unfold_bookmark_at(45);
    assert!(changed);

    // Now: [10..20) and [70..80) must remain folded!
    // [40..50) must be unfolded (visible)!
    assert!(editor.is_folded(10));
    assert!(!editor.is_folded(40));
    assert!(!editor.is_folded(45));
    assert!(editor.is_folded(70));

    assert!(editor.bookmarks().is_id_hidden(&id1));
    assert!(!editor.bookmarks().is_id_hidden(&id2));
    assert!(editor.bookmarks().is_id_hidden(&id3));

    // Click/unfold first bookmark [10..20)
    editor.unfold_bookmark_at(10);
    assert!(!editor.is_folded(10));
    assert!(!editor.is_folded(40));
    assert!(editor.is_folded(70));
}

#[test]
fn test_adjacent_consecutive_bookmarks_do_not_merge() {
    use crate::core::bookmark::{BookmarkColor, BookmarkItem};
    let content = vec![0u8; 100];
    let mut editor = create_editor_with_content(&content);

    // Add 3 consecutive bookmarks without gaps:
    // BM1: Red   [10..20)
    // BM2: Green [20..30)
    // BM3: Red   [30..40)
    for bm in [
        BookmarkItem::new(10, 10, BookmarkColor::Red, "Red1"),
        BookmarkItem::new(20, 10, BookmarkColor::Green, "Green"),
        BookmarkItem::new(30, 10, BookmarkColor::Red, "Red2"),
    ] {
        editor.bookmarks_mut().add(bm);
    }

    // Scenario A: Hide only Red bookmarks (Green is visible)
    editor.bookmarks_mut().hide_color(BookmarkColor::Red);
    let folds_red = editor.computed_folded_regions();
    assert_eq!(folds_red.len(), 2);
    assert_eq!(folds_red.get(&10), Some(&20));
    assert_eq!(folds_red.get(&30), Some(&40));
    assert!(!editor.is_folded(25)); // Green is visible between them

    // Scenario B: Hide BOTH Red and Green bookmarks
    editor.bookmarks_mut().hide_color(BookmarkColor::Green);
    let folds_all = editor.computed_folded_regions();
    // Each adjacent bookmark must remain its own distinct fold row!
    assert_eq!(folds_all.len(), 3);
    assert_eq!(folds_all.get(&10), Some(&20));
    assert_eq!(folds_all.get(&20), Some(&30));
    assert_eq!(folds_all.get(&30), Some(&40));

    let sum1 = editor.fold_bookmark_summary_at(10).unwrap();
    assert_eq!(sum1.color, BookmarkColor::Red);
    assert_eq!(sum1.comment, "Red1");

    let sum2 = editor.fold_bookmark_summary_at(20).unwrap();
    assert_eq!(sum2.color, BookmarkColor::Green);
    assert_eq!(sum2.comment, "Green");

    let sum3 = editor.fold_bookmark_summary_at(30).unwrap();
    assert_eq!(sum3.color, BookmarkColor::Red);
    assert_eq!(sum3.comment, "Red2");
}

#[test]
fn test_fill_selection_single_byte_and_undo() {
    use crate::core::fill::FillPattern;

    let mut editor = create_editor_with_content(b"Hello, World!");
    let range = 7..12; // "World"
    editor.set_selection_range(range.clone());
    assert_eq!(editor.selection_range(), Some(range.clone()));

    let pattern = FillPattern::SingleByte(b'X');
    let bytes = pattern.generate(range.len());
    assert!(editor.replace_range(range.clone(), bytes));
    editor.set_selection_range(range.clone());

    let doc = editor.document.read().unwrap();
    assert_eq!(doc.buffer.data(), b"Hello, XXXXX!");
    drop(doc);

    // Test Undo (Ctrl+Z)
    assert!(editor.undo());
    let doc = editor.document.read().unwrap();
    assert_eq!(doc.buffer.data(), b"Hello, World!");
    drop(doc);

    // Test Redo
    assert!(editor.redo());
    let doc = editor.document.read().unwrap();
    assert_eq!(doc.buffer.data(), b"Hello, XXXXX!");
}

#[test]
fn test_fill_selection_pattern_and_undo() {
    use crate::core::fill::FillPattern;

    let mut editor = create_editor_with_content(&[0u8; 16]);
    let range = 4..12;
    editor.set_selection_range(range.clone());

    let pattern = FillPattern::Pattern(vec![0xAA, 0xBB]);
    let bytes = pattern.generate(range.len());
    assert!(editor.replace_range(range.clone(), bytes));

    let doc = editor.document.read().unwrap();
    assert_eq!(doc.buffer.data(), &[0, 0, 0, 0, 0xAA, 0xBB, 0xAA, 0xBB, 0xAA, 0xBB, 0xAA, 0xBB, 0, 0, 0, 0]);
    drop(doc);

    // Undo
    assert!(editor.undo());
    let doc = editor.document.read().unwrap();
    assert_eq!(doc.buffer.data(), &[0u8; 16]);
}

#[test]
fn test_fill_selection_sequential_multi_width_and_undo() {
    use crate::core::fill::{FillPattern, SequentialWidth};
    use crate::core::radix::ByteOrder;

    let mut editor = create_editor_with_content(&[0u8; 16]);
    let range = 0..8;

    // 2-byte LE sequential
    let pattern_u16 = FillPattern::Sequential {
        width: SequentialWidth::U16,
        start: 1,
        step: 1,
        byte_order: ByteOrder::LittleEndian,
    };
    let bytes = pattern_u16.generate(range.len());
    assert!(editor.replace_range(range.clone(), bytes));

    let doc = editor.document.read().unwrap();
    assert_eq!(&doc.buffer.data()[..8], &[0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00]);
    drop(doc);

    // 4-byte BE sequential over same range
    let pattern_u32 = FillPattern::Sequential {
        width: SequentialWidth::U32,
        start: 0x12345678,
        step: 1,
        byte_order: ByteOrder::BigEndian,
    };
    let bytes = pattern_u32.generate(range.len());
    assert!(editor.replace_range(range.clone(), bytes));

    let doc = editor.document.read().unwrap();
    assert_eq!(&doc.buffer.data()[..8], &[0x12, 0x34, 0x56, 0x78, 0x12, 0x34, 0x56, 0x79]);
    drop(doc);

    // Undo 4-byte BE -> returns to 2-byte LE
    assert!(editor.undo());
    let doc = editor.document.read().unwrap();
    assert_eq!(&doc.buffer.data()[..8], &[0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00]);
    drop(doc);

    // Undo 2-byte LE -> returns to all zeros
    assert!(editor.undo());
    let doc = editor.document.read().unwrap();
    assert_eq!(doc.buffer.data(), &[0u8; 16]);
}

#[test]
fn test_single_hex_digit_commit_cursor_advance() {
    let mut editor = create_editor_with_content(&[0x00, 0x00, 0x00]);
    assert_eq!(editor.cursor.offset, 0);

    // Commit single digit at offset 0 keeping cursor at 0
    assert!(editor.replace_range_with_cursor(0..1, vec![0x01], 0));
    assert_eq!(editor.cursor.offset, 0);

    // Subsequent move right advances cursor to 1 (not 2)
    let total = editor.total_size();
    let next = crate::core::radix::next_visual_byte(editor.cursor.offset, total, editor.options.group_size, editor.options.is_big_endian);
    editor.set_cursor_offset_exact(next);
    assert_eq!(editor.cursor.offset, 1);
}

#[test]
fn test_eof_cursor_movement_and_append_and_backspace() {
    let mut editor = create_editor_with_content(b"abc");
    assert_eq!(editor.cursor.offset, 0);

    // Navigate to EOF (offset 3)
    editor.move_right(); // 1
    editor.move_right(); // 2
    editor.move_right(); // 3 (EOF)
    assert_eq!(editor.cursor.offset, 3);

    // Moving right at EOF stays at EOF
    editor.move_right();
    assert_eq!(editor.cursor.offset, 3);

    // Append 1 byte at EOF: replacing range 3..3 with [0x64] ('d') and cursor_after = 4
    assert!(editor.replace_range_with_cursor(3..3, vec![b'd'], 4));
    assert_eq!(editor.document.read().unwrap().buffer.data(), b"abcd");
    assert_eq!(editor.cursor.offset, 4);

    // Backspace at EOF: deletes the preceding byte (offset 3) and moves cursor to 3
    assert!(editor.delete_backward());
    assert_eq!(editor.document.read().unwrap().buffer.data(), b"abc");
    assert_eq!(editor.cursor.offset, 3);

    // Move left returns to offset 2 (last valid byte)
    editor.move_left();
    assert_eq!(editor.cursor.offset, 2);
}

#[test]
fn test_eof_cursor_line_navigation_on_full_row() {
    // 16-byte buffer with bytes_per_row = 16
    let mut editor = create_editor_with_content(&[0u8; 16]);
    assert_eq!(editor.total_size(), 16);
    assert_eq!(editor.line_starts(), vec![0]);

    // Move cursor down from line 0 pos 0 to virtual EOF line pos 0 (offset 16)
    editor.move_down();
    assert_eq!(editor.cursor.offset, 16);

    // Moving down from EOF stays at EOF
    editor.move_down();
    assert_eq!(editor.cursor.offset, 16);

    // Moving up from EOF returns to line 0 pos 0
    editor.move_up();
    assert_eq!(editor.cursor.offset, 0);

    // Go to end lands at 16 (EOF)
    editor.go_to_end();
    assert_eq!(editor.cursor.offset, 16);

    // Move left from EOF lands at offset 15
    editor.move_left();
    assert_eq!(editor.cursor.offset, 15);
}
