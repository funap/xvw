use super::StructTreeView;
use super::model::{
    AUTOFIT_CHAR_WIDTH, AUTOFIT_WIDE_CHAR_WIDTH, StructColumn, StructValueStyle, categorize_field_value, default_yaml_file_name, estimated_text_width,
    resolve_field_at_path, value_text,
};
use crate::core::color::RgbaColor;
use crate::core::format::HexCase;
use crate::core::radix::DisplayRadix;
use crate::core::structure::types::FieldValue;
use crate::core::structure::{ParseResult, ParsedField};
use std::collections::HashSet;

#[test]
fn test_struct_column_properties() {
    assert_eq!(StructColumn::Field.key(), "field");
    assert_eq!(StructColumn::Field.name(), "Field");
    assert_eq!(StructColumn::Address.key(), "address");
    assert_eq!(StructColumn::Address.name(), "Address");
    assert_eq!(StructColumn::Type.key(), "type");
    assert_eq!(StructColumn::Type.name(), "Type");
    assert_eq!(StructColumn::Size.key(), "size");
    assert_eq!(StructColumn::Size.name(), "Size");
    assert_eq!(StructColumn::Value.key(), "value");
    assert_eq!(StructColumn::Value.name(), "Value");
}

#[test]
fn test_estimated_text_width() {
    let ascii_width = estimated_text_width("abc");
    assert!((ascii_width - (3.0 * AUTOFIT_CHAR_WIDTH)).abs() < 0.001);

    let wide_width = estimated_text_width("あ");
    assert!((wide_width - AUTOFIT_WIDE_CHAR_WIDTH).abs() < 0.001);
}

#[test]
fn test_value_text_formatting() {
    let field_int = ParsedField {
        id: "magic".to_string(),
        field_type: "u4".to_string(),
        offset: 0,
        size: 4,
        value: FieldValue::U32(255),
        color: RgbaColor::default(),
        description: None,
        enum_label: None,
        is_instance: false,
        children: Vec::new(),
    };

    assert_eq!(value_text(&field_int, DisplayRadix::Decimal, HexCase::Lower), "255");
    assert_eq!(value_text(&field_int, DisplayRadix::Hexadecimal, HexCase::Lower), "0xff");
    assert_eq!(value_text(&field_int, DisplayRadix::Hexadecimal, HexCase::Upper), "0xFF");
    assert_eq!(value_text(&field_int, DisplayRadix::Octal, HexCase::Lower), "0o377");
    assert_eq!(value_text(&field_int, DisplayRadix::Binary, HexCase::Lower), "0b11111111");

    let field_with_enum = ParsedField {
        id: "kind".to_string(),
        field_type: "u1".to_string(),
        offset: 4,
        size: 1,
        value: FieldValue::U8(1),
        color: RgbaColor::default(),
        description: None,
        enum_label: Some("HEADER".to_string()),
        is_instance: false,
        children: Vec::new(),
    };

    assert_eq!(value_text(&field_with_enum, DisplayRadix::Decimal, HexCase::Lower), "1 (HEADER)");

    let field_struct = ParsedField {
        id: "header".to_string(),
        field_type: String::new(),
        offset: 0,
        size: 5,
        value: FieldValue::Struct,
        color: RgbaColor::default(),
        description: None,
        enum_label: None,
        is_instance: false,
        children: vec![field_int, field_with_enum],
    };

    assert_eq!(value_text(&field_struct, DisplayRadix::Decimal, HexCase::Lower), "2 children");
}

#[test]
fn test_categorize_field_value() {
    let field_int = ParsedField {
        id: "magic".to_string(),
        field_type: "u4".to_string(),
        offset: 0,
        size: 4,
        value: FieldValue::U32(255),
        color: RgbaColor::default(),
        description: None,
        enum_label: None,
        is_instance: false,
        children: Vec::new(),
    };
    assert_eq!(
        categorize_field_value(&field_int, DisplayRadix::Hexadecimal, HexCase::Lower),
        StructValueStyle::Number("0xff".to_string())
    );
    assert_eq!(
        categorize_field_value(&field_int, DisplayRadix::Hexadecimal, HexCase::Upper),
        StructValueStyle::Number("0xFF".to_string())
    );

    let field_str = ParsedField {
        id: "tag".to_string(),
        field_type: "str".to_string(),
        offset: 0,
        size: 4,
        value: FieldValue::String("PNG".to_string()),
        color: RgbaColor::default(),
        description: None,
        enum_label: None,
        is_instance: false,
        children: Vec::new(),
    };
    assert_eq!(
        categorize_field_value(&field_str, DisplayRadix::Decimal, HexCase::Lower),
        StructValueStyle::String("\"PNG\"".to_string())
    );

    let field_bytes = ParsedField {
        id: "buf".to_string(),
        field_type: "bytes".to_string(),
        offset: 0,
        size: 4,
        value: FieldValue::Bytes(vec![0; 4]),
        color: RgbaColor::default(),
        description: None,
        enum_label: None,
        is_instance: false,
        children: Vec::new(),
    };
    assert_eq!(
        categorize_field_value(&field_bytes, DisplayRadix::Decimal, HexCase::Lower),
        StructValueStyle::Bytes("[4 bytes]".to_string())
    );

    let field_enum = ParsedField {
        id: "kind".to_string(),
        field_type: "u1".to_string(),
        offset: 4,
        size: 1,
        value: FieldValue::U8(1),
        color: RgbaColor::default(),
        description: None,
        enum_label: Some("HEADER".to_string()),
        is_instance: false,
        children: Vec::new(),
    };
    assert_eq!(
        categorize_field_value(&field_enum, DisplayRadix::Decimal, HexCase::Lower),
        StructValueStyle::Enum {
            value: "1".to_string(),
            label: "HEADER".to_string(),
        }
    );

    let field_struct = ParsedField {
        id: "root".to_string(),
        field_type: String::new(),
        offset: 0,
        size: 5,
        value: FieldValue::Struct,
        color: RgbaColor::default(),
        description: None,
        enum_label: None,
        is_instance: false,
        children: vec![field_int, field_enum],
    };
    assert_eq!(
        categorize_field_value(&field_struct, DisplayRadix::Decimal, HexCase::Lower),
        StructValueStyle::StructSummary(2)
    );
}

#[test]
fn test_default_yaml_file_name() {
    assert_eq!(default_yaml_file_name("png"), "png.yaml");
    assert_eq!(default_yaml_file_name("zip.yaml"), "zip.yaml");
    assert_eq!(default_yaml_file_name("bad/name"), "bad_name.yaml");
    assert_eq!(default_yaml_file_name(""), "structure.yaml");
}

#[test]
fn test_resolve_field_at_path() {
    let field_a = ParsedField {
        id: "a".to_string(),
        field_type: "u1".to_string(),
        offset: 0,
        size: 1,
        value: FieldValue::U8(10),
        color: RgbaColor::default(),
        description: None,
        enum_label: None,
        is_instance: false,
        children: Vec::new(),
    };
    let field_b = ParsedField {
        id: "b".to_string(),
        field_type: "u1".to_string(),
        offset: 1,
        size: 1,
        value: FieldValue::U8(20),
        color: RgbaColor::default(),
        description: None,
        enum_label: None,
        is_instance: false,
        children: Vec::new(),
    };
    let field_parent = ParsedField {
        id: "parent".to_string(),
        field_type: String::new(),
        offset: 0,
        size: 2,
        value: FieldValue::Struct,
        color: RgbaColor::default(),
        description: None,
        enum_label: None,
        is_instance: false,
        children: vec![field_a, field_b],
    };

    let result = std::sync::Arc::new(ParseResult::new("test".to_string(), vec![field_parent], 2, Vec::new()));

    let opt_res = Some(result);
    assert_eq!(resolve_field_at_path(&opt_res, &[0]).map(|f| f.id.as_str()), Some("parent"));
    assert_eq!(resolve_field_at_path(&opt_res, &[0, 0]).map(|f| f.id.as_str()), Some("a"));
    assert_eq!(resolve_field_at_path(&opt_res, &[0, 1]).map(|f| f.id.as_str()), Some("b"));
    assert!(resolve_field_at_path(&opt_res, &[0, 2]).is_none());
    assert!(resolve_field_at_path(&opt_res, &[1]).is_none());
    assert!(resolve_field_at_path(&opt_res, &[]).is_none());
}

#[test]
fn test_flatten_fields_hierarchy() {
    let field_child = ParsedField {
        id: "child".to_string(),
        field_type: "u1".to_string(),
        offset: 0,
        size: 1,
        value: FieldValue::U8(1),
        color: RgbaColor::default(),
        description: None,
        enum_label: None,
        is_instance: false,
        children: Vec::new(),
    };
    let field_parent = ParsedField {
        id: "root".to_string(),
        field_type: String::new(),
        offset: 0,
        size: 1,
        value: FieldValue::Struct,
        color: RgbaColor::default(),
        description: None,
        enum_label: None,
        is_instance: false,
        children: vec![field_child],
    };

    // When collapsed (not in expanded_paths)
    let expanded = HashSet::new();
    let mut flattened = Vec::new();
    StructTreeView::flatten_fields(std::iter::once(&field_parent), 0, &[], &expanded, &mut flattened, 0);
    assert_eq!(flattened.len(), 1);
    assert!(flattened[0].has_children);
    assert!(flattened[0].is_collapsed);
    assert_eq!(flattened[0].depth, 0);

    // When expanded
    let mut expanded = HashSet::new();
    expanded.insert(vec![0]);
    let mut flattened = Vec::new();
    StructTreeView::flatten_fields(std::iter::once(&field_parent), 0, &[], &expanded, &mut flattened, 0);
    assert_eq!(flattened.len(), 2);
    assert_eq!(flattened[0].path, vec![0]);
    assert!(!flattened[0].is_collapsed);
    assert_eq!(flattened[1].path, vec![0, 0]);
    assert_eq!(flattened[1].depth, 1);
    assert!(!flattened[1].has_children);
}

#[test]
fn test_go_to_offset_action_target_format() {
    let field_offset = 0x12ABusize;
    let offset_hex = format!("0x{:08X}", field_offset);
    assert_eq!(offset_hex, "0x000012AB");

    let action = crate::actions::NavigateToOffset {
        target: offset_hex,
        target_file: None,
    };
    assert_eq!(action.target, "0x000012AB");
    assert!(action.target_file.is_none());
}

#[test]
fn test_struct_tree_view_event_focus_editor() {
    let event = super::StructTreeViewEvent::FocusEditor;
    assert_eq!(event, super::StructTreeViewEvent::FocusEditor);

    let parent_ctx = gpui_kit::KeyContext::try_from("StructTreeView").unwrap();
    let child_ctx = gpui_kit::KeyContext::try_from("DataTable").unwrap();
    let contexts = vec![parent_ctx, child_ctx];

    let pred_desc = gpui_kit::KeyBindingContextPredicate::parse("StructTreeView > DataTable").unwrap();
    assert!(pred_desc.eval(&contexts));
    assert_eq!(pred_desc.depth_of(&contexts), Some(2));
}
