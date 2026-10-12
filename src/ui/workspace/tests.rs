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

#[test]
fn test_active_file_name_extraction_from_document_path() {
    let doc = Document::new(PathBuf::from("/path/to/firmware.bin"), Buffer::new(vec![]));
    let filename = doc.path().file_name().and_then(|n| n.to_str()).filter(|s| !s.trim().is_empty());
    assert_eq!(filename, Some("firmware.bin"));

    let content = crate::ui::views::scratchpad_view::default_content_for(filename);
    assert_eq!(content, "# firmware.bin\n");
    assert_eq!(crate::service::ScratchService::extract_title(&content), Some("firmware.bin".to_string()));
}

#[test]
fn test_active_file_name_extraction_empty_or_none_fallback() {
    let empty_path = PathBuf::new();
    let filename = empty_path.file_name().and_then(|n| n.to_str()).filter(|s| !s.trim().is_empty());
    assert_eq!(filename, None);

    let content = crate::ui::views::scratchpad_view::default_content_for(filename);
    assert_eq!(content, "# Scratchpad\n");
    assert_eq!(crate::service::ScratchService::extract_title(&content), Some("Scratchpad".to_string()));
}

#[test]
fn test_active_file_path_frontmatter_content() {
    let doc = Document::new(PathBuf::from("/path/to/firmware.bin"), Buffer::new(vec![]));
    let path = doc.path();
    let content = crate::ui::views::scratchpad_view::default_content_for_target(Some(path), Some("Firmware Bootloader Notes"));
    assert_eq!(content, "---\ntarget_file: /path/to/firmware.bin\n---\n# Firmware Bootloader Notes\n");
    assert_eq!(
        crate::service::ScratchService::extract_target_file(&content),
        Some(PathBuf::from("/path/to/firmware.bin"))
    );
    assert_eq!(
        crate::service::ScratchService::extract_title(&content),
        Some("Firmware Bootloader Notes".to_string())
    );

    // Without custom title
    let content_default = crate::ui::views::scratchpad_view::default_content_for_target(Some(path), None);
    assert_eq!(content_default, "---\ntarget_file: /path/to/firmware.bin\n---\n# firmware.bin\n");
    assert_eq!(
        crate::service::ScratchService::extract_target_file(&content_default),
        Some(PathBuf::from("/path/to/firmware.bin"))
    );
}

#[test]
fn test_navigate_to_offset_action_with_target_file() {
    let action = crate::actions::NavigateToOffset {
        target: "0x1040".to_string(),
        target_file: Some(PathBuf::from("/path/to/firmware.bin")),
    };
    assert_eq!(action.target, "0x1040");
    assert_eq!(action.target_file, Some(PathBuf::from("/path/to/firmware.bin")));

    // Test serde round-trip
    let json = serde_json::to_string(&action).expect("must serialize");
    let deserialized: crate::actions::NavigateToOffset = serde_json::from_str(&json).expect("must deserialize");
    assert_eq!(deserialized, action);

    // Test backward compatibility without target_file
    let json_legacy = r#"{"target":"0x2000"}"#;
    let legacy_action: crate::actions::NavigateToOffset = serde_json::from_str(json_legacy).expect("must deserialize legacy action");
    assert_eq!(legacy_action.target, "0x2000");
    assert_eq!(legacy_action.target_file, None);
}
