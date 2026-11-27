use crate::core::radix::{DisplayRadix, HexCase};
use crate::core::structure::{FieldValue, ParseResult, ParsedField};
use gpui_kit::{Pixels, px};
use std::path::Path;

pub(super) const TREE_INDICATOR_WIDTH: f32 = 12.0;
pub(super) const TREE_INDENT_WIDTH: f32 = 14.0;
pub(super) const COLUMN_GAP: f32 = 4.0;
pub(super) const FIELD_COLUMN_WIDTH: f32 = 150.0;
pub(super) const ADDRESS_COLUMN_WIDTH: f32 = 78.0;
pub(super) const TYPE_COLUMN_WIDTH: f32 = 108.0;
pub(super) const SIZE_COLUMN_WIDTH: f32 = 58.0;
pub(super) const VALUE_COLUMN_WIDTH: f32 = 180.0;
pub(super) const AUTOFIT_HORIZONTAL_PADDING: f32 = 16.0;
pub(super) const AUTOFIT_MIN_WIDTH: f32 = 52.0;
pub(super) const AUTOFIT_MAX_WIDTH: f32 = 360.0;
pub(super) const AUTOFIT_CHAR_WIDTH: f32 = 7.2;
pub(super) const AUTOFIT_WIDE_CHAR_WIDTH: f32 = 13.0;
pub(super) const AUTOFIT_MAX_TEXT_CHARS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StructColumn {
    Field,
    Address,
    Type,
    Size,
    Value,
}

impl StructColumn {
    pub fn key(&self) -> &'static str {
        match self {
            Self::Field => "field",
            Self::Address => "address",
            Self::Type => "type",
            Self::Size => "size",
            Self::Value => "value",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Field => "Field",
            Self::Address => "Address",
            Self::Type => "Type",
            Self::Size => "Size",
            Self::Value => "Value",
        }
    }

    pub fn default_width(&self) -> Pixels {
        match self {
            Self::Field => px(FIELD_COLUMN_WIDTH),
            Self::Address => px(ADDRESS_COLUMN_WIDTH),
            Self::Type => px(TYPE_COLUMN_WIDTH),
            Self::Size => px(SIZE_COLUMN_WIDTH),
            Self::Value => px(VALUE_COLUMN_WIDTH),
        }
    }

    pub fn min_width(&self) -> Pixels {
        match self {
            Self::Field => px(60.0),
            Self::Address => px(50.0),
            Self::Type => px(50.0),
            Self::Size => px(40.0),
            Self::Value => px(60.0),
        }
    }
}

pub(super) fn path_from_string_file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".to_string())
}

pub(super) fn default_yaml_file_name(definition_id: &str) -> String {
    let mut name: String = definition_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() || name == "." || name == ".." {
        name = "structure".to_string();
    }
    if !name.ends_with(".yaml") && !name.ends_with(".yml") {
        name.push_str(".yaml");
    }
    name
}

pub(super) fn estimated_text_width(text: &str) -> f32 {
    text.chars()
        .take(AUTOFIT_MAX_TEXT_CHARS)
        .map(|character| if character.is_ascii() { AUTOFIT_CHAR_WIDTH } else { AUTOFIT_WIDE_CHAR_WIDTH })
        .sum()
}

pub(super) fn value_text(parsed_field: &ParsedField, radix: DisplayRadix, hex_case: HexCase) -> String {
    if parsed_field.is_struct() {
        format!(
            "{} child{}",
            parsed_field.children.len(),
            if parsed_field.children.len() == 1 { "" } else { "ren" }
        )
    } else if let Some(label) = &parsed_field.enum_label {
        format!("{} ({})", parsed_field.value.format_with_radix_and_case(radix, hex_case), label)
    } else {
        parsed_field.value.format_with_radix_and_case(radix, hex_case)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StructValueStyle {
    StructSummary(usize),
    Enum { value: String, label: String },
    Number(String),
    String(String),
    Bytes(String),
}

pub(crate) fn categorize_field_value(parsed_field: &ParsedField, radix: DisplayRadix, hex_case: HexCase) -> StructValueStyle {
    if parsed_field.is_struct() {
        StructValueStyle::StructSummary(parsed_field.children.len())
    } else if let Some(label) = &parsed_field.enum_label {
        StructValueStyle::Enum {
            value: parsed_field.value.format_with_radix_and_case(radix, hex_case),
            label: label.clone(),
        }
    } else {
        let val_str = parsed_field.value.format_with_radix_and_case(radix, hex_case);
        match &parsed_field.value {
            FieldValue::String(_) => StructValueStyle::String(val_str),
            FieldValue::Bytes(_) => StructValueStyle::Bytes(val_str),
            _ => StructValueStyle::Number(val_str),
        }
    }
}

pub(super) fn resolve_field_at_path<'a>(parse_result: &'a Option<std::sync::Arc<ParseResult>>, path: &[usize]) -> Option<&'a ParsedField> {
    let res = parse_result.as_ref()?;
    let (&first, rest) = path.split_first()?;
    let mut target_field = res.fields.get(first)?;
    for &idx in rest {
        target_field = target_field.children.get(idx)?;
    }
    Some(target_field)
}

#[derive(Clone)]
pub struct FlattenedField {
    pub path: Vec<usize>,
    pub offset: usize,
    pub size: usize,
    pub depth: usize,
    pub has_children: bool,
    pub is_collapsed: bool,
}

#[derive(Clone, Default)]
pub(super) enum StructureExportStatus {
    #[default]
    Idle,
    Copying,
    Exporting,
    Success(String),
    Error(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructTreePanelEvent {
    NavigateTo { offset: usize, size: usize },
    FocusEditor,
}
