use crate::core::editor::Editor;
use crate::core::format::{format_size_friendly, format_with_commas};
use crate::core::radix::{DisplayRadix, HexCase};
use crate::core::structure::{
    FieldValue, ParseResult, ParsedField, StructureYamlIncludeOffsets, StructureYamlShaThreshold, YamlExportOptions, format_parse_result_as_text,
    format_parse_result_as_yaml_with_options,
};
use crate::ui::appearance::Appearance;
use crate::ui::icon::IconName;
use crate::ui::views::hex_view::paint::DescriptionPalette;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu};
use gpui_kit::component::table::{Column, DataTable, TableDelegate, TableEvent, TableState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, StyledExt as _, WindowExt as _, button::ButtonVariants as _, h_flex, toolbar::Toolbar, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::*;
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

actions!(
    struct_tree,
    [MoveUp, MoveDown, MoveTop, MoveBottom, PageUp, PageDown, ToggleExpand, Expand, Collapse, Dismiss]
);

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = struct_tree)]
pub struct CopyFieldValue {
    pub value: String,
}

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = struct_tree)]
pub struct CopyFieldName {
    pub name: String,
}

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = struct_tree)]
pub struct CopyFieldOffset {
    pub offset: String,
}

const CONTEXT: &str = "StructTreeView";
const TREE_INDICATOR_WIDTH: f32 = 12.0;
const TREE_INDENT_WIDTH: f32 = 14.0;
const COLUMN_GAP: f32 = 4.0;
const FIELD_COLUMN_WIDTH: f32 = 150.0;
const ADDRESS_COLUMN_WIDTH: f32 = 78.0;
const TYPE_COLUMN_WIDTH: f32 = 108.0;
const SIZE_COLUMN_WIDTH: f32 = 58.0;
const VALUE_COLUMN_WIDTH: f32 = 180.0;
const AUTOFIT_HORIZONTAL_PADDING: f32 = 16.0;
const AUTOFIT_MIN_WIDTH: f32 = 52.0;
const AUTOFIT_MAX_WIDTH: f32 = 360.0;
const AUTOFIT_CHAR_WIDTH: f32 = 7.2;
const AUTOFIT_WIDE_CHAR_WIDTH: f32 = 13.0;
const AUTOFIT_MAX_TEXT_CHARS: usize = 128;

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

fn path_from_string_file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".to_string())
}

fn default_yaml_file_name(definition_id: &str) -> String {
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

fn estimated_text_width(text: &str) -> f32 {
    text.chars()
        .take(AUTOFIT_MAX_TEXT_CHARS)
        .map(|character| if character.is_ascii() { AUTOFIT_CHAR_WIDTH } else { AUTOFIT_WIDE_CHAR_WIDTH })
        .sum()
}

fn value_text(parsed_field: &ParsedField, radix: DisplayRadix, hex_case: HexCase) -> String {
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

fn resolve_field_at_path<'a>(parse_result: &'a Option<std::sync::Arc<ParseResult>>, path: &[usize]) -> Option<&'a ParsedField> {
    let res = parse_result.as_ref()?;
    let (&first, rest) = path.split_first()?;
    let mut target_field = res.fields.get(first)?;
    for &idx in rest {
        target_field = target_field.children.get(idx)?;
    }
    Some(target_field)
}

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Dismiss, Some("StructTreeView")),
        KeyBinding::new("escape", Dismiss, Some("StructTreeView > DataTable")),
        KeyBinding::new("up", MoveUp, Some("StructTreeView && !Input")),
        KeyBinding::new("down", MoveDown, Some("StructTreeView && !Input")),
        KeyBinding::new("k", MoveUp, Some("StructTreeView && !Input")),
        KeyBinding::new("j", MoveDown, Some("StructTreeView && !Input")),
        KeyBinding::new("home", MoveTop, Some("StructTreeView && !Input")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-home", MoveTop, Some("StructTreeView && !Input")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-home", MoveTop, Some("StructTreeView && !Input")),
        KeyBinding::new("end", MoveBottom, Some("StructTreeView && !Input")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-end", MoveBottom, Some("StructTreeView && !Input")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-end", MoveBottom, Some("StructTreeView && !Input")),
        KeyBinding::new("pageup", PageUp, Some("StructTreeView && !Input")),
        KeyBinding::new("pagedown", PageDown, Some("StructTreeView && !Input")),
        KeyBinding::new("space", ToggleExpand, Some("StructTreeView && !Input")),
        KeyBinding::new("enter", ToggleExpand, Some("StructTreeView && !Input")),
        KeyBinding::new("right", Expand, Some("StructTreeView && !Input")),
        KeyBinding::new("left", Collapse, Some("StructTreeView && !Input")),
        KeyBinding::new("l", Expand, Some("StructTreeView && !Input")),
        KeyBinding::new("h", Collapse, Some("StructTreeView && !Input")),
        // When DataTable has focus within StructTreeView, override navigation for tree actions
        KeyBinding::new("space", ToggleExpand, Some("StructTreeView > DataTable")),
        KeyBinding::new("enter", ToggleExpand, Some("StructTreeView > DataTable")),
        KeyBinding::new("right", Expand, Some("StructTreeView > DataTable")),
        KeyBinding::new("left", Collapse, Some("StructTreeView > DataTable")),
        KeyBinding::new("l", Expand, Some("StructTreeView > DataTable")),
        KeyBinding::new("h", Collapse, Some("StructTreeView > DataTable")),
    ]);
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
enum StructureExportStatus {
    #[default]
    Idle,
    Copying,
    Exporting,
    Success(String),
    Error(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructTreeViewEvent {
    NavigateTo { offset: usize, size: usize },
    FocusEditor,
}

pub struct StructTreeTableDelegate {
    panel: WeakEntity<StructTreeView>,
    flattened_fields: Vec<FlattenedField>,
    parse_result: Option<std::sync::Arc<ParseResult>>,
    value_radix: DisplayRadix,
    selected_index: Option<usize>,
    visible_columns: Vec<StructColumn>,
    column_widths: HashMap<StructColumn, Pixels>,
    focus_handle: FocusHandle,
}

impl StructTreeTableDelegate {
    fn field_at_path<'a>(&'a self, path: &[usize]) -> Option<&'a ParsedField> {
        resolve_field_at_path(&self.parse_result, path)
    }
}

impl TableDelegate for StructTreeTableDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.visible_columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.flattened_fields.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        let Some(col) = self.visible_columns.get(col_ix) else {
            return Column::default();
        };

        let width = self.column_widths.get(col).copied().unwrap_or_else(|| col.default_width());
        Column::new(col.key(), col.name()).width(width).min_width(col.min_width()).resizable(true)
    }

    fn fit_column(&mut self, col_ix: usize, _window: &mut Window, _cx: &mut Context<TableState<Self>>) -> Option<Pixels> {
        let col = self.visible_columns.get(col_ix)?;
        let mut max_width = estimated_text_width(col.name());
        if *col == StructColumn::Field {
            max_width += TREE_INDICATOR_WIDTH + COLUMN_GAP;
        }

        let hex_case = _cx.try_global::<HexCase>().copied().unwrap_or_default();
        for field in self.flattened_fields.iter().take(128) {
            let Some(parsed_field) = self.field_at_path(&field.path) else {
                continue;
            };

            let text = match col {
                StructColumn::Field => parsed_field.id.clone(),
                StructColumn::Address => hex_case.format_prefixed(parsed_field.offset),
                StructColumn::Type => {
                    if parsed_field.field_type.is_empty() {
                        if field.has_children { "struct".to_string() } else { "value".to_string() }
                    } else {
                        parsed_field.field_type.clone()
                    }
                }
                StructColumn::Size => format!("{} B", format_with_commas(field.size)),
                StructColumn::Value => value_text(parsed_field, self.value_radix, hex_case),
            };
            let mut text_width = estimated_text_width(&text);
            if *col == StructColumn::Field {
                text_width += field.depth as f32 * TREE_INDENT_WIDTH + TREE_INDICATOR_WIDTH + COLUMN_GAP;
            }
            max_width = max_width.max(text_width);
        }

        let width = (max_width + AUTOFIT_HORIZONTAL_PADDING).clamp(AUTOFIT_MIN_WIDTH, AUTOFIT_MAX_WIDTH);
        self.column_widths.insert(*col, px(width));
        Some(px(width))
    }

    fn render_th(&mut self, col_ix: usize, _window: &mut Window, _cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        let col = self.visible_columns.get(col_ix);
        let name = col.map(|c| c.name()).unwrap_or_default();
        let is_address_visible = self.visible_columns.contains(&StructColumn::Address);
        let is_type_visible = self.visible_columns.contains(&StructColumn::Type);
        let is_size_visible = self.visible_columns.contains(&StructColumn::Size);
        let is_value_visible = self.visible_columns.contains(&StructColumn::Value);
        let value_radix = self.value_radix;
        let context_focus = self.focus_handle.clone();

        let header_content = if col == Some(&StructColumn::Field) {
            h_flex()
                .size_full()
                .items_center()
                .gap_1()
                .child(div().flex_shrink_0().w(px(TREE_INDICATOR_WIDTH)).h(px(TREE_INDICATOR_WIDTH)))
                .child(div().flex_1().min_w_0().truncate().whitespace_nowrap().child(name))
                .into_any_element()
        } else {
            div().size_full().child(name).into_any_element()
        };

        div().size_full().child(header_content).context_menu(move |menu, window, cx| {
            menu.action_context(context_focus.clone())
                .label("Visible columns")
                .separator()
                .menu_with_check("Address", is_address_visible, Box::new(crate::actions::ToggleStructureAddressColumn))
                .menu_with_check("Type", is_type_visible, Box::new(crate::actions::ToggleStructureTypeColumn))
                .menu_with_check("Size", is_size_visible, Box::new(crate::actions::ToggleStructureSizeColumn))
                .menu_with_check("Value", is_value_visible, Box::new(crate::actions::ToggleStructureValueColumn))
                .separator()
                .submenu("Value format", window, cx, move |menu, _window, _cx| {
                    menu.menu_with_check(
                        "Hexadecimal (16)",
                        value_radix == DisplayRadix::Hexadecimal,
                        Box::new(crate::actions::SetRadixHex),
                    )
                    .menu_with_check("Decimal (10)", value_radix == DisplayRadix::Decimal, Box::new(crate::actions::SetRadixDec))
                    .menu_with_check("Octal (8)", value_radix == DisplayRadix::Octal, Box::new(crate::actions::SetRadixOct))
                    .menu_with_check("Binary (2)", value_radix == DisplayRadix::Binary, Box::new(crate::actions::SetRadixBin))
                })
        })
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, window: &mut Window, cx: &mut Context<TableState<Self>>) -> PopupMenu {
        let Some(field) = self.flattened_fields.get(row_ix) else {
            return menu;
        };
        let Some(parsed_field) = self.field_at_path(&field.path) else {
            return menu;
        };
        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
        let field_id = parsed_field.id.clone();
        let field_offset = parsed_field.offset;
        let field_value = value_text(parsed_field, self.value_radix, hex_case);
        let value_radix = self.value_radix;
        let offset_hex = hex_case.format_prefixed_pad(field_offset, 8);
        let context_focus = self.focus_handle.clone();

        let value_format_submenu = PopupMenu::build(window, cx, move |menu, _window, _cx| {
            menu.menu_with_check(
                "Hexadecimal (16)",
                value_radix == DisplayRadix::Hexadecimal,
                Box::new(crate::actions::SetRadixHex),
            )
            .menu_with_check("Decimal (10)", value_radix == DisplayRadix::Decimal, Box::new(crate::actions::SetRadixDec))
            .menu_with_check("Octal (8)", value_radix == DisplayRadix::Octal, Box::new(crate::actions::SetRadixOct))
            .menu_with_check("Binary (2)", value_radix == DisplayRadix::Binary, Box::new(crate::actions::SetRadixBin))
        });

        menu.action_context(context_focus)
            .menu_with_icon(
                format!("Go to Offset ({})", offset_hex),
                IconName::ChevronRight,
                Box::new(crate::actions::NavigateToOffset {
                    target: offset_hex.clone(),
                    target_file: None,
                }),
            )
            .separator()
            .menu_with_icon(
                format!("Copy Value ({})", field_value),
                IconName::Copy,
                Box::new(CopyFieldValue { value: field_value }),
            )
            .menu_with_icon(
                format!("Copy Field Name ({})", field_id),
                IconName::Copy,
                Box::new(CopyFieldName { name: field_id }),
            )
            .menu_with_icon(
                format!("Copy Offset ({})", offset_hex),
                IconName::Hash,
                Box::new(CopyFieldOffset { offset: offset_hex }),
            )
            .separator()
            .item(gpui_kit::component::menu::PopupMenuItem::submenu("Value format", value_format_submenu))
            .separator()
            .menu_with_icon("Copy Structure Analysis", IconName::Copy, Box::new(crate::actions::CopyStructureResult))
            .menu_with_icon(
                "Export Structure as YAML",
                IconName::HardDriveDownload,
                Box::new(crate::actions::ExportStructureYaml),
            )
            .separator()
            .menu_with_icon("Expand All", IconName::ChevronDown, Box::new(crate::actions::ExpandAllStructure))
            .menu_with_icon("Collapse All", IconName::ChevronUp, Box::new(crate::actions::CollapseAllStructure))
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, _window: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        let Some(field) = self.flattened_fields.get(row_ix) else {
            return div().into_any_element();
        };
        let Some(parsed_field) = self.field_at_path(&field.path) else {
            return div().into_any_element();
        };
        let Some(col) = self.visible_columns.get(col_ix) else {
            return div().into_any_element();
        };

        let theme = cx.theme();
        let palette = DescriptionPalette::from_theme(theme);
        let font_family = cx.global::<Appearance>().font_family.clone();
        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
        let is_selected = self.selected_index == Some(row_ix);

        match col {
            StructColumn::Field => {
                let id = SharedString::from(parsed_field.id.clone());
                let chevron_symbol = if field.has_children {
                    if field.is_collapsed { "▶" } else { "▼" }
                } else {
                    " "
                };
                let has_children = field.has_children;
                let depth = field.depth;
                let panel = self.panel.clone();

                h_flex()
                    .items_center()
                    .gap_1()
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .w(px(TREE_INDENT_WIDTH * depth as f32 + TREE_INDICATOR_WIDTH))
                            .h(px(TREE_INDICATOR_WIDTH))
                            .items_center()
                            .justify_end()
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .w(px(TREE_INDICATOR_WIDTH))
                                    .h(px(TREE_INDICATOR_WIDTH))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .when(has_children, |this| {
                                        this.cursor_pointer().on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                                            cx.stop_propagation();
                                            if let Some(panel) = panel.upgrade() {
                                                cx.defer(move |cx| {
                                                    panel.update(cx, |this, cx| {
                                                        this.toggle_collapse_at(row_ix, cx);
                                                    });
                                                });
                                            }
                                        })
                                    })
                                    .child(chevron_symbol),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .whitespace_nowrap()
                            .text_sm()
                            .font_medium()
                            .text_color(palette.field_id)
                            .child(id),
                    )
                    .into_any_element()
            }
            StructColumn::Address => {
                let field_offset = parsed_field.offset;
                div()
                    .text_xs()
                    .font_family(font_family)
                    .text_color(if is_selected { theme.accent_foreground } else { theme.muted_foreground })
                    .truncate()
                    .whitespace_nowrap()
                    .child(hex_case.format_prefixed(field_offset))
                    .into_any_element()
            }
            StructColumn::Type => {
                let type_name = if parsed_field.field_type.is_empty() {
                    if field.has_children { "struct" } else { "value" }
                } else {
                    parsed_field.field_type.as_str()
                };
                let type_label = SharedString::from(type_name.to_string());
                div()
                    .text_xs()
                    .font_family(font_family)
                    .text_color(palette.type_name)
                    .truncate()
                    .whitespace_nowrap()
                    .child(type_label)
                    .into_any_element()
            }
            StructColumn::Size => div()
                .text_xs()
                .font_family(font_family)
                .text_color(palette.number_value)
                .truncate()
                .whitespace_nowrap()
                .child(format!("{} B", format_with_commas(field.size)))
                .into_any_element(),
            StructColumn::Value => match categorize_field_value(parsed_field, self.value_radix, hex_case) {
                StructValueStyle::StructSummary(count) => {
                    let text = format!("{} child{}", count, if count == 1 { "" } else { "ren" });
                    div()
                        .text_xs()
                        .font_family(font_family)
                        .text_color(theme.muted_foreground)
                        .truncate()
                        .whitespace_nowrap()
                        .child(text)
                        .into_any_element()
                }
                StructValueStyle::Enum { value, label } => h_flex()
                    .items_center()
                    .text_xs()
                    .font_family(font_family)
                    .truncate()
                    .whitespace_nowrap()
                    .min_w_0()
                    .child(div().text_color(palette.number_value).child(value))
                    .child(div().text_color(palette.punctuation).child(" ("))
                    .child(div().text_color(palette.enum_label).child(label))
                    .child(div().text_color(palette.punctuation).child(")"))
                    .into_any_element(),
                StructValueStyle::Number(val_str) => div()
                    .text_xs()
                    .font_family(font_family)
                    .text_color(palette.number_value)
                    .truncate()
                    .whitespace_nowrap()
                    .child(val_str)
                    .into_any_element(),
                StructValueStyle::String(val_str) => div()
                    .text_xs()
                    .font_family(font_family)
                    .text_color(palette.string_value)
                    .truncate()
                    .whitespace_nowrap()
                    .child(val_str)
                    .into_any_element(),
                StructValueStyle::Bytes(val_str) => div()
                    .text_xs()
                    .font_family(font_family)
                    .text_color(palette.bytes_value)
                    .truncate()
                    .whitespace_nowrap()
                    .child(val_str)
                    .into_any_element(),
            },
        }
    }
}

pub struct StructTreeView {
    pub parse_result: Option<std::sync::Arc<ParseResult>>,
    pub flattened_fields: Vec<FlattenedField>,
    pub expanded_paths: HashSet<Vec<usize>>,
    pub editor: Option<Entity<Editor>>,
    pub table_state: Entity<TableState<StructTreeTableDelegate>>,
    pub focus_handle: FocusHandle,
    pub selected_index: Option<usize>,
    pub recent_definition_paths: Vec<PathBuf>,
    visible_columns: Vec<StructColumn>,
    column_widths: HashMap<StructColumn, Pixels>,
    value_radix: DisplayRadix,
    last_parse_id: Option<(String, usize, usize, usize, usize, bool)>,
    last_selection_cursor: Option<usize>,
    last_selection_scan_len: usize,
    export_status: StructureExportStatus,
    export_request_id: u64,
    _table_subscription: Option<Subscription>,
    _editor_subscription: Option<Subscription>,
}

impl EventEmitter<StructTreeViewEvent> for StructTreeView {}

impl StructTreeView {
    pub fn new(
        parse_result: Option<std::sync::Arc<crate::core::structure::types::ParseResult>>,
        editor: Option<Entity<Editor>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let expanded_paths = HashSet::new();
        let mut flattened = Vec::new();
        if let Some(ref res) = parse_result {
            Self::flatten_fields(res.fields.iter(), 0, &[], &expanded_paths, &mut flattened, 0);
        }
        let focus_handle = cx.focus_handle();
        let visible_columns = vec![
            StructColumn::Field,
            StructColumn::Address,
            StructColumn::Type,
            StructColumn::Size,
            StructColumn::Value,
        ];
        let delegate = StructTreeTableDelegate {
            panel: cx.entity().downgrade(),
            flattened_fields: flattened.clone(),
            parse_result: parse_result.clone(),
            value_radix: DisplayRadix::Decimal,
            selected_index: None,
            visible_columns: visible_columns.clone(),
            column_widths: HashMap::new(),
            focus_handle: focus_handle.clone(),
        };
        let table_state = cx.new(|cx| {
            TableState::new(delegate, window, cx)
                .row_selectable(true)
                .col_selectable(false)
                .col_resizable(true)
                .col_movable(false)
        });

        let table_subscription = cx.subscribe_in(&table_state, window, |this, _, event: &TableEvent, _window, cx| match event {
            TableEvent::SelectRow(row_ix) => {
                let row_ix = *row_ix;
                if this.selected_index != Some(row_ix) {
                    this.select_item(row_ix, cx);
                }
            }
            TableEvent::DoubleClickedRow(row_ix) => {
                let row_ix = *row_ix;
                this.select_item(row_ix, cx);
                if let Some(field) = this.flattened_fields.get(row_ix)
                    && field.has_children
                {
                    this.toggle_collapse_at(row_ix, cx);
                }
            }
            TableEvent::ColumnWidthsChanged(widths) => {
                for (ix, &width) in widths.iter().enumerate() {
                    if let Some(&col) = this.visible_columns.get(ix) {
                        this.column_widths.insert(col, width);
                    }
                }
            }
            _ => {}
        });

        let mut this = Self {
            parse_result,
            flattened_fields: flattened,
            expanded_paths,
            editor: editor.clone(),
            table_state,
            focus_handle,
            selected_index: None,
            recent_definition_paths: Vec::new(),
            visible_columns,
            column_widths: HashMap::new(),
            value_radix: DisplayRadix::Decimal,
            last_parse_id: None,
            last_selection_cursor: None,
            last_selection_scan_len: 0,
            export_status: StructureExportStatus::default(),
            export_request_id: 0,
            _table_subscription: Some(table_subscription),
            _editor_subscription: None,
        };

        if let Some(ed) = editor {
            this._editor_subscription = Some(cx.observe(&ed, |this, editor, cx| {
                this.sync_fields(&editor, cx);
            }));
            this.sync_fields(&ed, cx);
        }

        this
    }

    pub fn set_editor(&mut self, editor: Option<Entity<Editor>>, cx: &mut Context<Self>) {
        let same_editor = match (&self.editor, &editor) {
            (Some(current), Some(next)) => current.entity_id() == next.entity_id(),
            (None, None) => true,
            _ => false,
        };
        if same_editor {
            return;
        }

        self._editor_subscription = None;
        self.editor = editor.clone();

        if let Some(ed) = &editor {
            self._editor_subscription = Some(cx.observe(ed, |this, editor, cx| {
                this.sync_fields(&editor, cx);
            }));
            self.sync_fields(ed, cx);
        } else {
            self.last_parse_id = None;
            self.set_parse_result(None, cx);
        }
        cx.notify();
    }

    /// Updates the recent structure definition paths displayed in the empty state.
    pub fn set_definition_history(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        if self.recent_definition_paths == paths {
            return;
        }

        self.recent_definition_paths = paths.to_vec();
        cx.notify();
    }

    fn sync_fields(&mut self, editor: &Entity<Editor>, cx: &mut Context<Self>) {
        let (current_parse_id, cursor_offset) = {
            let editor_lock = editor.read(cx);
            let doc_version = editor_lock.document.read().ok().map(|d| d.history.version()).unwrap_or(0);
            let parse_id = editor_lock.parse_result().map(|r| {
                (
                    r.definition_id.clone(),
                    r.total_parsed_bytes,
                    r.fields.len(),
                    doc_version,
                    editor_lock.structure.generation,
                    editor_lock.structure.is_parsing(),
                )
            });
            (parse_id, editor_lock.cursor.offset)
        };

        if current_parse_id != self.last_parse_id {
            let parse_res = editor.read(cx).parse_result();
            let can_append = match (&self.last_parse_id, &current_parse_id, &self.parse_result) {
                (Some(previous), Some(current), Some(existing)) => {
                    previous.3 == current.3 && previous.4 == current.4 && current.2 >= previous.2 && existing.fields.len() == previous.2
                }
                _ => false,
            };
            if can_append {
                let previous_field_count = self.last_parse_id.as_ref().map(|parse_id| parse_id.2).unwrap_or(0);
                let old = std::mem::replace(&mut self.parse_result, parse_res);
                if let Some(old) = old {
                    Self::release_parse_result_in_background(old, cx);
                }
                if let Some(ref res) = self.parse_result
                    && current_parse_id.as_ref().map(|parse_id| parse_id.2).unwrap_or(0) > previous_field_count
                {
                    Self::flatten_fields(
                        res.fields.iter_from(previous_field_count),
                        0,
                        &[],
                        &self.expanded_paths,
                        &mut self.flattened_fields,
                        previous_field_count,
                    );
                }
                self.sync_table_state(cx);
                cx.notify();
            } else {
                self.set_parse_result(parse_res, cx);
            }
            self.last_parse_id = current_parse_id;
        }

        self.sync_selected_index_from_cursor(cursor_offset, cx);
    }

    fn sync_selected_index_from_cursor(&mut self, cursor_offset: usize, cx: &mut Context<Self>) {
        if self.flattened_fields.is_empty() {
            self.last_selection_cursor = Some(cursor_offset);
            self.last_selection_scan_len = 0;
            return;
        }

        // If current selected item already covers cursor_offset (or was just selected internally), keep it
        if let Some(curr_idx) = self.selected_index
            && let Some(field) = self.flattened_fields.get(curr_idx)
        {
            let end = field.offset + field.size;
            let matches = if field.size > 0 {
                cursor_offset >= field.offset && cursor_offset < end
            } else {
                cursor_offset == field.offset
            };
            if matches || self.last_selection_cursor == Some(cursor_offset) {
                self.last_selection_cursor = Some(cursor_offset);
                self.last_selection_scan_len = self.flattened_fields.len();
                return;
            }
        }

        let cursor_changed = self.last_selection_cursor != Some(cursor_offset);
        let scan_start = if !cursor_changed && self.selected_index.is_none() && self.last_selection_scan_len <= self.flattened_fields.len() {
            self.last_selection_scan_len
        } else {
            0
        };
        if scan_start == self.flattened_fields.len() {
            return;
        }

        let mut best_match: Option<(usize, bool, usize, usize)> = None; // (index, is_leaf, depth, size)
        for (i, field) in self.flattened_fields.iter().enumerate().skip(scan_start) {
            let end = field.offset + field.size;
            let matches = if field.size > 0 {
                cursor_offset >= field.offset && cursor_offset < end
            } else {
                cursor_offset == field.offset
            };

            if matches {
                let is_leaf = !field.has_children;
                match best_match {
                    None => {
                        best_match = Some((i, is_leaf, field.depth, field.size));
                    }
                    Some((_, best_is_leaf, best_depth, best_size)) => {
                        // Prefer leaf nodes over container nodes, then deeper nodes, then smaller size
                        if (is_leaf && !best_is_leaf)
                            || (is_leaf == best_is_leaf && field.depth > best_depth)
                            || (is_leaf == best_is_leaf && field.depth == best_depth && field.size < best_size)
                        {
                            best_match = Some((i, is_leaf, field.depth, field.size));
                        }
                    }
                }
            }
        }

        self.last_selection_cursor = Some(cursor_offset);
        self.last_selection_scan_len = self.flattened_fields.len();
        if let Some((idx, _, _, _)) = best_match
            && self.selected_index != Some(idx)
        {
            self.selected_index = Some(idx);
            let table_state = self.table_state.clone();
            cx.defer(move |cx| {
                table_state.update(cx, |table, cx| {
                    table.delegate_mut().selected_index = Some(idx);
                    if table.selected_row() != Some(idx) {
                        table.set_selected_row(idx, cx);
                    }
                });
            });
            cx.notify();
        } else if best_match.is_none() && self.selected_index.is_some() {
            self.selected_index = None;
            let table_state = self.table_state.clone();
            cx.defer(move |cx| {
                table_state.update(cx, |table, cx| {
                    table.delegate_mut().selected_index = None;
                    table.clear_selection(cx);
                });
            });
        }
    }

    pub fn set_parse_result(&mut self, parse_result: Option<std::sync::Arc<crate::core::structure::types::ParseResult>>, cx: &mut Context<Self>) {
        let old_expanded_paths = std::mem::take(&mut self.expanded_paths);
        let old_flattened_fields = std::mem::take(&mut self.flattened_fields);
        let old_parse_result = std::mem::replace(&mut self.parse_result, parse_result);
        self.export_status = StructureExportStatus::default();
        self.export_request_id = self.export_request_id.wrapping_add(1);

        if old_parse_result.is_some() || !old_flattened_fields.is_empty() || !old_expanded_paths.is_empty() {
            cx.background_executor()
                .spawn(async move {
                    drop((old_parse_result, old_flattened_fields, old_expanded_paths));
                })
                .detach();
        }
        self.rebuild_flattened();
        self.selected_index = None;
        self.last_selection_cursor = None;
        self.last_selection_scan_len = 0;
        self.sync_table_state(cx);
        cx.notify();
    }

    fn release_parse_result_in_background(result: std::sync::Arc<crate::core::structure::types::ParseResult>, cx: &mut Context<Self>) {
        cx.background_executor()
            .spawn(async move {
                drop(result);
            })
            .detach();
    }

    fn can_export_result(&self) -> bool {
        self.parse_result.as_ref().is_some_and(|result| !result.fields.is_empty())
    }

    fn export_is_busy(&self) -> bool {
        matches!(self.export_status, StructureExportStatus::Copying | StructureExportStatus::Exporting)
    }

    fn copy_structure_result(&mut self, _: &crate::actions::CopyStructureResult, window: &mut Window, cx: &mut Context<Self>) {
        if self.export_is_busy() || !self.can_export_result() {
            return;
        }

        let Some(parse_result) = self.parse_result.clone() else {
            return;
        };
        let request_id = self.export_request_id.wrapping_add(1);
        self.export_request_id = request_id;
        self.export_status = StructureExportStatus::Copying;
        cx.notify();

        let field_count = parse_result.fields.len();
        let task = cx.background_executor().spawn(async move { format_parse_result_as_text(&parse_result) });
        let view = cx.entity().clone();
        cx.spawn_in(window, async move |_, window| {
            let text = task.await;
            let _ = window.update(|window, cx| {
                let applied = view.update(cx, |this, cx| {
                    if this.export_request_id != request_id {
                        return false;
                    }
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    this.export_status = StructureExportStatus::Success(format!("Copied {field_count} root fields to clipboard"));
                    cx.notify();
                    true
                });
                if applied {
                    window.push_notification(
                        gpui_kit::component::notification::Notification::success("Structure analysis copied to clipboard"),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    fn copy_field_value(&mut self, action: &CopyFieldValue, _window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
    }

    fn copy_field_name(&mut self, action: &CopyFieldName, _window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.name.clone()));
    }

    fn copy_field_offset(&mut self, action: &CopyFieldOffset, _window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.offset.clone()));
    }

    fn export_structure_yaml(&mut self, _: &crate::actions::ExportStructureYaml, window: &mut Window, cx: &mut Context<Self>) {
        if self.export_is_busy() || !self.can_export_result() {
            return;
        }

        let Some(parse_result) = self.parse_result.clone() else {
            return;
        };
        let (parent_dir, target_file_name) = self
            .editor
            .as_ref()
            .and_then(|ed| {
                let doc = ed.read(cx).document.read().ok()?;
                let path = doc.path();
                let dir = path.parent().filter(|p| p.exists()).map(|p| p.to_path_buf());
                let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned());
                Some((dir, file_name))
            })
            .unwrap_or((None, None));

        let parent_dir = parent_dir.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        let default_file_name = if let Some(name) = target_file_name
            && !name.is_empty()
            && name != "Untitled"
            && name != "untitled"
        {
            format!("{name}.{}.yaml", parse_result.definition_id)
        } else {
            default_yaml_file_name(&parse_result.definition_id)
        };

        let prompt = cx.prompt_for_new_path(&parent_dir, Some(&default_file_name));

        let threshold = cx.global::<StructureYamlShaThreshold>().0;
        let include_offsets = cx.global::<StructureYamlIncludeOffsets>().0;
        let request_id = self.export_request_id.wrapping_add(1);
        self.export_request_id = request_id;
        self.export_status = StructureExportStatus::Exporting;
        cx.notify();

        let executor = cx.background_executor().clone();
        let view = cx.entity().clone();
        cx.spawn_in(window, async move |_, window| {
            let selected_path = prompt.await.ok().and_then(|result| result.ok()).flatten();
            let Some(mut path) = selected_path else {
                let _ = window.update(|_, cx| {
                    view.update(cx, |this, cx| {
                        if this.export_request_id == request_id {
                            this.export_status = StructureExportStatus::Idle;
                            cx.notify();
                        }
                    });
                });
                return;
            };

            let export_task = executor.spawn(async move {
                if path.extension().is_none() {
                    path.set_extension("yaml");
                }

                let options = YamlExportOptions {
                    binary_sha256_threshold: threshold,
                    include_offsets,
                };
                let yaml = format_parse_result_as_yaml_with_options(&parse_result, options).map_err(|error| error.to_string())?;
                std::fs::write(&path, yaml).map_err(|error| format!("{}: {error}", path.display()))?;
                Ok::<PathBuf, String>(path)
            });
            let result = export_task.await;

            let _ = window.update(|window, cx| {
                let applied = view.update(cx, |this, cx| {
                    if this.export_request_id != request_id {
                        return false;
                    }
                    match &result {
                        Ok(path) => {
                            this.export_status = StructureExportStatus::Success(format!("Exported YAML to {}", path.display()));
                        }
                        Err(error) => {
                            this.export_status = StructureExportStatus::Error(format!("YAML export failed: {error}"));
                        }
                    }
                    cx.notify();
                    true
                });

                if applied {
                    match &result {
                        Ok(path) => window.push_notification(crate::ui::notification::success(format!("Structure YAML exported to {}", path.display())), cx),
                        Err(error) => window.push_notification(crate::ui::notification::error(error.clone()), cx),
                    }
                }
            });
        })
        .detach();
    }

    fn on_action_copy_structure_result(&mut self, action: &crate::actions::CopyStructureResult, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_structure_result(action, window, cx);
    }

    fn on_action_export_structure_yaml(&mut self, action: &crate::actions::ExportStructureYaml, window: &mut Window, cx: &mut Context<Self>) {
        self.export_structure_yaml(action, window, cx);
    }

    fn rebuild_flattened(&mut self) {
        let mut flattened = Vec::new();
        if let Some(ref res) = self.parse_result {
            Self::flatten_fields(res.fields.iter(), 0, &[], &self.expanded_paths, &mut flattened, 0);
        }
        self.flattened_fields = flattened;
        self.last_selection_cursor = None;
        self.last_selection_scan_len = 0;
    }

    fn flatten_fields<'a, I>(
        fields: I,
        depth: usize,
        parent_path: &[usize],
        expanded_paths: &HashSet<Vec<usize>>,
        results: &mut Vec<FlattenedField>,
        index_offset: usize,
    ) where
        I: IntoIterator<Item = &'a ParsedField>,
    {
        struct Frame<I> {
            iter: I,
            depth: usize,
            parent_path: Vec<usize>,
            next_index: usize,
        }

        let mut stack: Vec<Frame<std::vec::IntoIter<&'a ParsedField>>> = vec![Frame {
            iter: fields.into_iter().collect::<Vec<_>>().into_iter(),
            depth,
            parent_path: parent_path.to_vec(),
            next_index: index_offset,
        }];

        while let Some(frame) = stack.last_mut() {
            let Some(field) = frame.iter.next() else {
                stack.pop();
                continue;
            };

            let mut current_path = frame.parent_path.clone();
            current_path.push(frame.next_index);
            frame.next_index += 1;

            let has_children = !field.children.is_empty();
            let is_collapsed = !expanded_paths.contains(&current_path);

            results.push(FlattenedField {
                path: current_path.clone(),
                offset: field.offset,
                size: field.size,
                depth: frame.depth,
                has_children,
                is_collapsed,
            });

            if has_children && !is_collapsed {
                let next_depth = frame.depth + 1;
                let children: Vec<&'a ParsedField> = field.children.iter().collect();
                stack.push(Frame {
                    iter: children.into_iter(),
                    depth: next_depth,
                    parent_path: current_path,
                    next_index: 0,
                });
            }
        }
    }

    pub fn field_at_path<'a>(&'a self, path: &[usize]) -> Option<&'a ParsedField> {
        resolve_field_at_path(&self.parse_result, path)
    }

    #[allow(dead_code)]
    pub fn get_field_at_path<'a>(&'a self, path: &[usize]) -> Option<&'a ParsedField> {
        self.field_at_path(path)
    }

    fn find_first_leaf_offset(field: &ParsedField) -> usize {
        let mut current = field;
        while let Some(first_child) = current.children.first() {
            current = first_child;
        }
        current.offset
    }

    fn select_item(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx >= self.flattened_fields.len() {
            return;
        }

        self.selected_index = Some(idx);
        let table_state = self.table_state.clone();
        cx.defer(move |cx| {
            table_state.update(cx, |table, cx| {
                table.delegate_mut().selected_index = Some(idx);
                if table.selected_row() != Some(idx) {
                    table.set_selected_row(idx, cx);
                }
            });
        });

        let item = &self.flattened_fields[idx];
        let offset = if item.size == 0 && item.has_children {
            if let Some(parsed_field) = self.field_at_path(&item.path) {
                Self::find_first_leaf_offset(parsed_field)
            } else {
                item.offset
            }
        } else {
            item.offset
        };

        let mut nav_offset = offset;
        self.last_selection_cursor = Some(offset);
        self.last_selection_scan_len = self.flattened_fields.len();

        if let Some(editor) = &self.editor {
            editor.update(cx, |editor, cx| {
                let total = editor.total_size();
                if total > 0 && item.size > 0 {
                    let start = item.offset.min(total.saturating_sub(1));
                    let end = (item.offset + item.size.saturating_sub(1)).min(total.saturating_sub(1));
                    editor.set_selection_range(start..end.saturating_add(1));
                    editor.cursor.offset = start;
                    nav_offset = start;
                } else {
                    let clamped = offset.min(total);
                    editor.set_cursor_offset_exact(clamped);
                    nav_offset = clamped;
                }
                cx.notify();
            });
        }

        self.last_selection_cursor = Some(nav_offset);
        self.last_selection_scan_len = self.flattened_fields.len();

        cx.emit(StructTreeViewEvent::NavigateTo {
            offset: nav_offset,
            size: item.size,
        });
        cx.notify();
    }

    fn toggle_collapse_at(&mut self, idx: usize, cx: &mut Context<Self>) {
        if let Some(field) = self.flattened_fields.get(idx) {
            if !field.has_children {
                return;
            }

            let path = field.path.clone();
            if self.expanded_paths.contains(&path) {
                self.expanded_paths.remove(&path);
            } else {
                self.expanded_paths.insert(path);
            }

            self.rebuild_flattened();

            // Maintain selection index if possible by path
            if idx < self.flattened_fields.len() {
                self.selected_index = Some(idx);
            } else if !self.flattened_fields.is_empty() {
                self.selected_index = Some(self.flattened_fields.len() - 1);
            }

            self.sync_table_state(cx);
            cx.notify();
        }
    }

    fn move_up(&mut self, _: &MoveUp, _window: &mut Window, cx: &mut Context<Self>) {
        if self.flattened_fields.is_empty() {
            return;
        }

        let next_idx = match self.selected_index {
            Some(idx) => idx.saturating_sub(1),
            None => 0,
        };

        self.select_item(next_idx, cx);
    }

    fn move_down(&mut self, _: &MoveDown, _window: &mut Window, cx: &mut Context<Self>) {
        if self.flattened_fields.is_empty() {
            return;
        }

        let last_idx = self.flattened_fields.len().saturating_sub(1);
        let next_idx = match self.selected_index {
            Some(idx) => (idx + 1).min(last_idx),
            None => 0,
        };

        self.select_item(next_idx, cx);
    }

    fn move_top(&mut self, _: &MoveTop, _window: &mut Window, cx: &mut Context<Self>) {
        if self.flattened_fields.is_empty() {
            return;
        }
        self.select_item(0, cx);
    }

    fn move_bottom(&mut self, _: &MoveBottom, _window: &mut Window, cx: &mut Context<Self>) {
        if self.flattened_fields.is_empty() {
            return;
        }
        let last_idx = self.flattened_fields.len().saturating_sub(1);
        self.select_item(last_idx, cx);
    }

    fn page_up(&mut self, _: &PageUp, _window: &mut Window, cx: &mut Context<Self>) {
        if self.flattened_fields.is_empty() {
            return;
        }
        let current = self.selected_index.unwrap_or(0);
        let next_idx = current.saturating_sub(10);
        self.select_item(next_idx, cx);
    }

    fn page_down(&mut self, _: &PageDown, _window: &mut Window, cx: &mut Context<Self>) {
        if self.flattened_fields.is_empty() {
            return;
        }
        let current = self.selected_index.unwrap_or(0);
        let last_idx = self.flattened_fields.len().saturating_sub(1);
        let next_idx = (current + 10).min(last_idx);
        self.select_item(next_idx, cx);
    }

    fn toggle_expand(&mut self, _: &ToggleExpand, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(idx) = self.selected_index {
            self.toggle_collapse_at(idx, cx);
        }
    }

    fn expand(&mut self, _: &Expand, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(idx) = self.selected_index
            && let Some(field) = self.flattened_fields.get(idx)
            && field.has_children
            && field.is_collapsed
        {
            self.toggle_collapse_at(idx, cx);
        }
    }

    fn collapse(&mut self, _: &Collapse, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(idx) = self.selected_index
            && let Some(field) = self.flattened_fields.get(idx)
        {
            if field.has_children && !field.is_collapsed {
                self.toggle_collapse_at(idx, cx);
            } else if field.depth > 0 {
                // Move to parent field
                let target_depth = field.depth - 1;
                for p_idx in (0..idx).rev() {
                    if self.flattened_fields[p_idx].depth == target_depth {
                        self.select_item(p_idx, cx);
                        break;
                    }
                }
            }
        }
    }

    pub fn collapse_all(&mut self, cx: &mut Context<Self>) {
        self.expanded_paths.clear();
        self.rebuild_flattened();
        self.selected_index = self.selected_index.map(|idx| idx.min(self.flattened_fields.len().saturating_sub(1)));
        self.sync_table_state(cx);
        cx.notify();
    }

    pub fn expand_all(&mut self, cx: &mut Context<Self>) {
        if let Some(ref res) = self.parse_result {
            fn collect_branches<'a>(fields: impl IntoIterator<Item = &'a ParsedField>, parent_path: Vec<usize>, out: &mut HashSet<Vec<usize>>) {
                for (idx, field) in fields.into_iter().enumerate() {
                    let mut path = parent_path.clone();
                    path.push(idx);
                    if !field.children.is_empty() {
                        out.insert(path.clone());
                        collect_branches(field.children.iter(), path, out);
                    }
                }
            }
            collect_branches(res.fields.iter(), Vec::new(), &mut self.expanded_paths);
        }
        self.rebuild_flattened();
        self.sync_table_state(cx);
        cx.notify();
    }

    fn dismiss(&mut self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(StructTreeViewEvent::FocusEditor);
        window.dispatch_action(Box::new(crate::actions::FocusHexView), cx);
    }

    fn on_action_expand_all(&mut self, _: &crate::actions::ExpandAllStructure, _window: &mut Window, cx: &mut Context<Self>) {
        self.expand_all(cx);
    }

    fn on_action_collapse_all(&mut self, _: &crate::actions::CollapseAllStructure, _window: &mut Window, cx: &mut Context<Self>) {
        self.collapse_all(cx);
    }

    fn toggle_column(&mut self, col: StructColumn, cx: &mut Context<Self>) {
        if let Some(pos) = self.visible_columns.iter().position(|&c| c == col) {
            self.visible_columns.remove(pos);
        } else {
            let all = [
                StructColumn::Field,
                StructColumn::Address,
                StructColumn::Type,
                StructColumn::Size,
                StructColumn::Value,
            ];
            let mut new_cols = Vec::new();
            for c in all {
                if self.visible_columns.contains(&c) || c == col {
                    new_cols.push(c);
                }
            }
            self.visible_columns = new_cols;
        }
        self.sync_table_state(cx);
        cx.notify();
    }

    fn on_action_toggle_structure_address_column(&mut self, _: &crate::actions::ToggleStructureAddressColumn, _window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_column(StructColumn::Address, cx);
    }

    fn on_action_toggle_structure_type_column(&mut self, _: &crate::actions::ToggleStructureTypeColumn, _window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_column(StructColumn::Type, cx);
    }

    fn on_action_toggle_structure_size_column(&mut self, _: &crate::actions::ToggleStructureSizeColumn, _window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_column(StructColumn::Size, cx);
    }

    fn on_action_toggle_structure_value_column(&mut self, _: &crate::actions::ToggleStructureValueColumn, _window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_column(StructColumn::Value, cx);
    }

    fn set_value_radix(&mut self, radix: DisplayRadix, cx: &mut Context<Self>) {
        if self.value_radix == radix {
            return;
        }

        self.value_radix = radix;
        self.sync_table_state(cx);
        cx.notify();
    }

    fn on_action_set_value_radix_hex(&mut self, _: &crate::actions::SetRadixHex, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_value_radix(DisplayRadix::Hexadecimal, cx);
    }

    fn on_action_set_value_radix_dec(&mut self, _: &crate::actions::SetRadixDec, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_value_radix(DisplayRadix::Decimal, cx);
    }

    fn on_action_set_value_radix_oct(&mut self, _: &crate::actions::SetRadixOct, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_value_radix(DisplayRadix::Octal, cx);
    }

    fn on_action_set_value_radix_bin(&mut self, _: &crate::actions::SetRadixBin, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_value_radix(DisplayRadix::Binary, cx);
    }

    fn sync_table_state(&mut self, cx: &mut Context<Self>) {
        let flattened_fields = self.flattened_fields.clone();
        let parse_result = self.parse_result.clone();
        let value_radix = self.value_radix;
        let selected_index = self.selected_index;
        let visible_columns = self.visible_columns.clone();
        let column_widths = self.column_widths.clone();
        let table_state = self.table_state.clone();

        cx.defer(move |cx| {
            table_state.update(cx, |table, cx| {
                let delegate = table.delegate_mut();
                delegate.flattened_fields = flattened_fields;
                delegate.parse_result = parse_result;
                delegate.value_radix = value_radix;
                delegate.selected_index = selected_index;
                delegate.visible_columns = visible_columns;
                delegate.column_widths = column_widths;
                table.refresh(cx);
                if let Some(idx) = selected_index {
                    if table.selected_row() != Some(idx) {
                        table.set_selected_row(idx, cx);
                    }
                } else {
                    table.clear_selection(cx);
                }
            });
        });
    }
}

impl Render for StructTreeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_parsing = self.editor.as_ref().is_some_and(|ed| ed.read(cx).structure.is_parsing());
        let is_empty = self.flattened_fields.is_empty() && self.parse_result.as_ref().is_none_or(|result| result.fields.is_empty());
        let show_parsing_placeholder = is_parsing && self.flattened_fields.is_empty();
        let is_table_focused = self.table_state.read(cx).focus_handle(cx).is_focused(window);
        let is_focused = self.focus_handle.is_focused(window) || is_table_focused;
        let theme = cx.theme();
        let has_active_editor = self.editor.is_some();
        let parse_progress = self.editor.as_ref().and_then(|editor| {
            let editor = editor.read(cx);
            if editor.structure.is_idle() {
                return None;
            }

            let total_bytes = if editor.structure.total_size > 0 {
                editor.structure.total_size
            } else {
                editor.total_size()
            };
            let progress = if total_bytes > 0 {
                (editor.structure.progress_offset as f32 / total_bytes as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            Some((progress, editor.structure.progress_offset, total_bytes, editor.structure.is_finalizing()))
        });

        let has_structure = self.parse_result.as_ref().is_some_and(|result| !result.fields.is_empty()) || !self.flattened_fields.is_empty();
        let export_is_busy = self.export_is_busy();
        let definition_badge = self.parse_result.as_ref().map(|result| {
            crate::ui::panels::panel_badge(result.definition_id.clone(), theme)
                .max_w(px(110.0))
                .truncate()
                .into_any_element()
        });

        let header_actions = if has_structure {
            Some(
                gpui_kit::component::button::Button::new("clear-structure-btn")
                    .ghost()
                    .icon(IconName::Eraser)
                    .with_size(gpui_kit::component::Size::XSmall)
                    .disabled(export_is_busy)
                    .tooltip("Clear Structure Definition")
                    .on_click(cx.listener(|_, _, window, cx| {
                        window.dispatch_action(Box::new(crate::actions::ClearStructureDefinition), cx);
                    }))
                    .into_any_element(),
            )
        } else if has_active_editor {
            Some(
                gpui_kit::component::button::Button::new("load-structure-header-btn")
                    .ghost()
                    .icon(IconName::FolderOpen)
                    .with_size(gpui_kit::component::Size::XSmall)
                    .tooltip(if cfg!(target_os = "macos") {
                        "Load Structure Definition (cmd-shift-s)"
                    } else {
                        "Load Structure Definition (ctrl-shift-s)"
                    })
                    .on_click(cx.listener(|_, _, window, cx| {
                        window.dispatch_action(Box::new(crate::actions::LoadStructureDefinition), cx);
                    }))
                    .into_any_element(),
            )
        } else {
            None
        };

        let header = crate::ui::panels::panel_header("STRUCTURE", is_focused, theme, definition_badge, header_actions);

        let structure_toolbar = if has_structure {
            Some(
                h_flex()
                    .w_full()
                    .justify_end()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        Toolbar::new("structure-toolbar")
                            .with_size(gpui_kit::component::Size::XSmall)
                            .child(
                                gpui_kit::component::button::Button::new("expand-all-struct-btn")
                                    .ghost()
                                    .icon(IconName::ListTree)
                                    .tooltip("Expand all fields")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.expand_all(cx);
                                    })),
                            )
                            .child(
                                gpui_kit::component::button::Button::new("collapse-all-struct-btn")
                                    .ghost()
                                    .icon(IconName::Minimize)
                                    .tooltip("Collapse all fields")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.collapse_all(cx);
                                    })),
                            ),
                    )
                    .into_any_element(),
            )
        } else {
            None
        };

        let export_feedback = match &self.export_status {
            StructureExportStatus::Idle => None,
            StructureExportStatus::Copying => Some((IconName::LoaderCircle, "Preparing structure text...".to_string(), theme.accent)),
            StructureExportStatus::Exporting => Some((IconName::LoaderCircle, "Preparing YAML export...".to_string(), theme.accent)),
            StructureExportStatus::Success(message) => Some((IconName::Check, message.clone(), theme.success)),
            StructureExportStatus::Error(message) => Some((IconName::TriangleAlert, message.clone(), theme.danger)),
        };

        crate::ui::panels::panel_container(is_focused, theme)
            .id("struct-tree-view")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::move_up))
            .on_action(cx.listener(Self::move_down))
            .on_action(cx.listener(Self::move_top))
            .on_action(cx.listener(Self::move_bottom))
            .on_action(cx.listener(Self::page_up))
            .on_action(cx.listener(Self::page_down))
            .on_action(cx.listener(Self::toggle_expand))
            .on_action(cx.listener(Self::expand))
            .on_action(cx.listener(Self::collapse))
            .on_action(cx.listener(Self::dismiss))
            .on_action(cx.listener(|this, _: &gpui_kit::base::actions::Cancel, window, cx| {
                this.dismiss(&Dismiss, window, cx);
            }))
            .on_action(cx.listener(Self::on_action_expand_all))
            .on_action(cx.listener(Self::on_action_collapse_all))
            .on_action(cx.listener(Self::on_action_toggle_structure_address_column))
            .on_action(cx.listener(Self::on_action_toggle_structure_type_column))
            .on_action(cx.listener(Self::on_action_toggle_structure_size_column))
            .on_action(cx.listener(Self::on_action_toggle_structure_value_column))
            .on_action(cx.listener(Self::on_action_set_value_radix_hex))
            .on_action(cx.listener(Self::on_action_set_value_radix_dec))
            .on_action(cx.listener(Self::on_action_set_value_radix_oct))
            .on_action(cx.listener(Self::on_action_set_value_radix_bin))
            .on_action(cx.listener(Self::copy_field_value))
            .on_action(cx.listener(Self::copy_field_name))
            .on_action(cx.listener(Self::copy_field_offset))
            .on_action(cx.listener(Self::on_action_copy_structure_result))
            .on_action(cx.listener(Self::on_action_export_structure_yaml))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.focus_handle.focus(window, cx);
                }),
            )
            .child(header)
            .when_some(structure_toolbar, |el, toolbar| el.child(toolbar))
            .when_some(export_feedback, |el, (icon, message, color)| {
                el.child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_1()
                        .px_3()
                        .py_1()
                        .bg(theme.secondary)
                        .text_xs()
                        .text_color(color)
                        .child(Icon::new(icon).size(px(13.0)))
                        .child(div().flex_1().truncate().child(message)),
                )
            })
            .when_some(parse_progress, |el, (progress, parsed_bytes, total_bytes, is_finalizing)| {
                el.child(
                    div()
                        .id("structure-parse-progress")
                        .h(px(3.0))
                        .w_full()
                        .overflow_hidden()
                        .bg(theme.progress_bar)
                        .child(
                            div()
                                .h_full()
                                .w(relative(progress))
                                .bg(if is_finalizing { theme.warning } else { theme.accent }),
                        ),
                )
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(if is_finalizing {
                            "Finalizing structure index..."
                        } else {
                            "Analyzing binary data..."
                        })
                        .child(format!("{} / {}", format_size_friendly(parsed_bytes), format_size_friendly(total_bytes))),
                )
            })
            .child(div().flex_1().min_h_0().w_full().overflow_hidden().child(if show_parsing_placeholder {
                crate::ui::panels::panel_empty_state(
                    IconName::LoaderCircle,
                    "Parsing Structure...",
                    Some("Analyzing binary data with Kaitai Struct..."),
                    None,
                    theme,
                )
                .into_any_element()
            } else if is_empty {
                let recent_section = if !self.recent_definition_paths.is_empty() {
                    let recent_buttons = self
                        .recent_definition_paths
                        .iter()
                        .enumerate()
                        .map(|(index, path)| {
                            let path = path.to_string_lossy().into_owned();
                            let label = path_from_string_file_name(&path);
                            let load_path = path.clone();
                            let remove_path = path.clone();

                            h_flex()
                                .w_full()
                                .items_center()
                                .gap_1()
                                .child(
                                    h_flex()
                                        .id(("recent-definition-item", index))
                                        .flex_1()
                                        .min_w_0()
                                        .h_5()
                                        .items_center()
                                        .gap_1()
                                        .px_1()
                                        .cursor_pointer()
                                        .hover(|style| style.bg(theme.list_hover))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            if has_active_editor {
                                                this.focus_handle.focus(window, cx);
                                                window.dispatch_action(
                                                    Box::new(crate::actions::LoadStructureDefinitionFromHistory { path: load_path.clone() }),
                                                    cx,
                                                );
                                            }
                                        }))
                                        .child(Icon::new(IconName::FileCode).with_size(gpui_kit::component::Size::XSmall))
                                        .child(div().flex_1().min_w_0().text_xs().truncate().whitespace_nowrap().child(label)),
                                )
                                .child(
                                    gpui_kit::component::button::Button::new(SharedString::from(format!("remove-recent-definition-{index}")))
                                        .ghost()
                                        .icon(IconName::Close)
                                        .with_size(gpui_kit::component::Size::XSmall)
                                        .tooltip("Remove from recents")
                                        .on_click(cx.listener(move |_, _, window, cx| {
                                            window.dispatch_action(
                                                Box::new(crate::actions::RemoveStructureDefinitionFromHistory { path: remove_path.clone() }),
                                                cx,
                                            );
                                        })),
                                )
                                .into_any_element()
                        })
                        .collect::<Vec<_>>();

                    Some(
                        h_flex()
                            .w_full()
                            .justify_start()
                            .child(
                                v_flex()
                                    .w(relative(0.95))
                                    .mt_4()
                                    .pt_3()
                                    .border_t_1()
                                    .border_color(theme.border)
                                    .items_start()
                                    .gap_1()
                                    .child(div().px_1().text_xs().font_semibold().text_color(theme.muted_foreground).child("Recents"))
                                    .children(recent_buttons),
                            )
                            .into_any_element(),
                    )
                } else {
                    None
                };

                if !has_active_editor {
                    crate::ui::panels::panel_empty_state(
                        IconName::File,
                        "No File Open",
                        Some("Open a binary file before loading a structure definition"),
                        recent_section,
                        theme,
                    )
                    .into_any_element()
                } else {
                    let load_btn = gpui_kit::component::button::Button::new("load-ksy-btn")
                        .label("Load Definition...")
                        .primary()
                        .with_size(gpui_kit::component::Size::Small)
                        .on_click(cx.listener(|_, _, window, cx| {
                            window.dispatch_action(Box::new(crate::actions::LoadStructureDefinition), cx);
                        }))
                        .into_any_element();

                    let mut load_actions = v_flex().w_full().items_center().child(load_btn);
                    if let Some(recent_section) = recent_section {
                        load_actions = load_actions.child(recent_section);
                    }

                    crate::ui::panels::panel_empty_state(
                        IconName::Braces,
                        "No Structure Loaded",
                        Some("Open a Kaitai Struct (.ksy) YAML file to inspect binary fields"),
                        Some(load_actions.into_any_element()),
                        theme,
                    )
                    .into_any_element()
                }
            } else {
                div()
                    .size_full()
                    .child(DataTable::new(&self.table_state).bordered(false))
                    .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        if event.keystroke.key == "escape" {
                            this.dismiss(&Dismiss, window, cx);
                            cx.stop_propagation();
                        }
                    }))
                    .into_any_element()
            }))
    }
}

impl Focusable for StructTreeView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AUTOFIT_CHAR_WIDTH, AUTOFIT_WIDE_CHAR_WIDTH, StructColumn, StructTreeView, StructValueStyle, categorize_field_value, default_yaml_file_name,
        estimated_text_width, resolve_field_at_path, value_text,
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
}
