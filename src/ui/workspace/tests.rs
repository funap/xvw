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
fn test_resolve_effective_item_prefers_current_active() {
    use super::resolve_effective_item;

    let res = resolve_effective_item(Some(10), Some(20), |_| true, || Some(30));
    assert_eq!(res, Some(10));
}

#[test]
fn test_resolve_effective_item_falls_back_to_last_active_when_scratchpad_focused() {
    use super::resolve_effective_item;

    // When scratchpad has focus (current_active is None), fallback to last active if open
    let res = resolve_effective_item(None, Some(20), |&id| id == 20, || Some(30));
    assert_eq!(res, Some(20));
}

#[test]
fn test_resolve_effective_item_ignores_closed_last_active() {
    use super::resolve_effective_item;

    // If last active was closed (!is_open), fallback to any open editor
    let res = resolve_effective_item(None, Some(20), |&id| id != 20, || Some(30));
    assert_eq!(res, Some(30));
}

#[test]
fn test_resolve_effective_item_returns_none_when_no_editors_open() {
    use super::resolve_effective_item;

    let res = resolve_effective_item::<usize>(None, None, |_| false, || None);
    assert_eq!(res, None);
}

#[test]
fn test_toggle_right_panel_action_exists() {
    let action = crate::actions::ToggleRightPanel;
    assert_eq!(action, crate::actions::ToggleRightPanel);
}
