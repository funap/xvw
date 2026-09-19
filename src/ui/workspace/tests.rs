use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use super::dialog_flow::{deduplicate_dirty_documents, format_unsaved_changes_prompt};
use crate::core::buffer::Buffer;
use crate::core::document::Document;

#[test]
fn test_format_unsaved_changes_prompt_single_file() {
    let (detail, buttons) = format_unsaved_changes_prompt(1, Some("test.bin"));
    assert_eq!(detail, "Save changes to test.bin before closing?");
    assert_eq!(buttons, &["Save", "Don't Save", "Cancel"]);
}

#[test]
fn test_format_unsaved_changes_prompt_multiple_files() {
    let (detail, buttons) = format_unsaved_changes_prompt(3, Some("test.bin"));
    assert_eq!(detail, "You have 3 files with unsaved changes. Save changes before closing?");
    assert_eq!(buttons, &["Save All", "Don't Save", "Cancel"]);
}

#[test]
fn test_format_unsaved_changes_prompt_empty_fallback() {
    let (detail, buttons) = format_unsaved_changes_prompt(0, None);
    assert_eq!(detail, "Save changes to document before closing?");
    assert_eq!(buttons, &["Save", "Don't Save", "Cancel"]);
}

#[test]
fn test_deduplicate_dirty_documents_same_arc() {
    let doc = Arc::new(RwLock::new(Document::new(PathBuf::from("a.bin"), Buffer::new(vec![0x01]))));
    let list = vec![(doc.clone(), "a.bin".to_string()), (doc.clone(), "a.bin".to_string())];
    let deduplicated = deduplicate_dirty_documents(list);
    assert_eq!(deduplicated.len(), 1);
}

#[test]
fn test_deduplicate_dirty_documents_same_path() {
    let doc1 = Arc::new(RwLock::new(Document::new(PathBuf::from("b.bin"), Buffer::new(vec![0x01]))));
    let doc2 = Arc::new(RwLock::new(Document::new(PathBuf::from("b.bin"), Buffer::new(vec![0x02]))));
    let list = vec![(doc1, "b.bin".to_string()), (doc2, "b.bin".to_string())];
    let deduplicated = deduplicate_dirty_documents(list);
    assert_eq!(deduplicated.len(), 1);
}

#[test]
fn test_deduplicate_dirty_documents_different_paths() {
    let doc1 = Arc::new(RwLock::new(Document::new(PathBuf::from("a.bin"), Buffer::new(vec![0x01]))));
    let doc2 = Arc::new(RwLock::new(Document::new(PathBuf::from("b.bin"), Buffer::new(vec![0x02]))));
    let list = vec![(doc1, "a.bin".to_string()), (doc2, "b.bin".to_string())];
    let deduplicated = deduplicate_dirty_documents(list);
    assert_eq!(deduplicated.len(), 2);
}

#[test]
fn test_new_empty_file_document_properties() {
    let title = format!("Untitled-{}.bin", 1);
    let path = PathBuf::from(title);
    let data = vec![0u8; 0];
    let buffer = Buffer::new(data);
    let doc = Document::new(path.clone(), buffer);
    assert_eq!(doc.path, path);
    assert_eq!(doc.buffer.len(), 0);
    assert!(!doc.is_dirty());
    assert!(!doc.is_read_only());
}

#[test]
fn test_notification_item_state_timer_logic() {
    use super::NotificationItemState;

    let mut state = NotificationItemState::new();
    assert!(!state.is_pinned);
    assert!(!state.is_hovered);
    assert!(state.timer.is_none());
    assert!(state.should_run_timer(), "Initial notification should start dismiss timer");

    // When user hovers over notification
    state.is_hovered = true;
    assert!(!state.should_run_timer(), "Hovered notification must not run dismiss timer");

    // When cursor leaves notification
    state.is_hovered = false;
    assert!(state.should_run_timer(), "When cursor leaves, dismiss timer should run");

    // When user clicks/pins the notification
    state.is_pinned = true;
    assert!(!state.should_run_timer(), "Pinned notification must not run dismiss timer");

    // Hovering while pinned
    state.is_hovered = true;
    assert!(!state.should_run_timer());

    // Cursor leaves pinned notification
    state.is_hovered = false;
    assert!(
        !state.should_run_timer(),
        "Pinned notification must never run dismiss timer even after cursor leaves"
    );
}
